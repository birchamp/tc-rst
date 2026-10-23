//! The count row of B11 (§8.2, `conflict_tab_costs_its_conflicts`): a review tab costs the
//! conflicts it lists, never the project. The corpora hold the conflicts their devices left
//! unresolved, ten times as many in the larger one, so the read is held to a bound per listed
//! conflict and to one walk of the conflict list; a verse's History costs that verse's versions
//! on both corpora.

use tc_api::conflicts::{History, List, ReviewEntries, ReviewTab, ReviewView};
use tc_app::wire::scope::record_ref;

use super::corpus::{PSALMS, corpora};
use super::harness::{Backing, costs_on_both, opened};
use crate::common::post;

/// What one listed conflict may read: its review token, at most one per record of a
/// passage group, and nothing of the rest of the project.
const READS_PER_CONFLICT: u64 = 4;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflict_tab_costs_its_conflicts() {
    for backing in [Backing::Memory, Backing::Store] {
        let listed = std::sync::Mutex::new(Vec::new());
        let (small, large) = costs_on_both(backing, opened, async |s| {
            for tab in ReviewTab::ALL {
                let view: ReviewView = post(&s.app, &List { at: s.scope(), tab }).await.unwrap();
                let n = match view.entries {
                    ReviewEntries::Conflicts(e) => e.len(),
                    ReviewEntries::Detached(e) => e.len(),
                    ReviewEntries::Notices(e) => e.len(),
                };
                listed.lock().unwrap().push(n as u64);
            }
        })
        .await;
        let listed = listed.lock().unwrap().clone();
        let (on_small, on_large) = listed.split_at(ReviewTab::ALL.len());
        let conflicts = |counts: &[u64]| counts[..4].iter().sum::<u64>();
        assert!(
            conflicts(on_small) > 0 && conflicts(on_large) > conflicts(on_small),
            "{backing:?}: the corpora hold conflicts, ten times as many in the larger one: {listed:?}"
        );
        for (cost, counts, scale) in [(small, on_small, "Psalms"), (large, on_large, "ten times")] {
            let entries: u64 = counts.iter().sum();
            let app = cost.app;
            assert_eq!(
                app.conflict_walks, 4,
                "{backing:?} {scale}: one walk of the list for each conflict tab"
            );
            assert!(
                app.records_read <= READS_PER_CONFLICT * entries.max(1),
                "{backing:?} {scale}: {} records read for {entries} entries",
                app.records_read
            );
            for (name, n) in [
                ("chapters_projected", app.chapters_projected),
                ("book_usfm", app.book_usfm),
                ("books_rendered", app.books_rendered),
                ("files_rendered", app.files_rendered),
                ("items_derived", app.items_derived),
            ] {
                assert_eq!(n, 0, "{backing:?} {scale}: {name}");
            }
        }
    }
}

/// A History lists every version of its record, so it costs those versions: one value decoded
/// and at most two records read per version listed, on both corpora, whatever else the
/// project holds. Naming the people costs one profile per person of the project, as the sync
/// result's names do: "Teammate n" numbers every unnamed person, so it reads them all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_verses_history_costs_its_versions() {
    let key = || {
        tc_journal::RecordKey::Verse(
            PSALMS,
            tc_journal::ChapterNo::new(corpora().chapter).unwrap(),
            tc_journal::VerseNo::new(1).unwrap(),
        )
    };
    for backing in [Backing::Memory, Backing::Store] {
        let people = std::sync::Mutex::new(Vec::new());
        let listed = std::sync::Mutex::new(Vec::new());
        let before = async |s: &super::harness::Scale| {
            opened(s).await;
            let read = s.app.read(&s.scope()).unwrap().read();
            let n = read
                .person_labels(tc_journal::DeviceId::local([0; 15]))
                .len() as u64;
            people.lock().unwrap().push(n);
        };
        let (small, large) = costs_on_both(backing, before, async |s| {
            let view = post(
                &s.app,
                &History {
                    at: s.scope(),
                    record: record_ref(&key()).unwrap(),
                },
            )
            .await
            .unwrap();
            listed.lock().unwrap().push(view.versions.len() as u64);
        })
        .await;
        let people = people.lock().unwrap().clone();
        let listed: Vec<(u64, u64)> = listed.lock().unwrap().iter().copied().zip(people).collect();
        for (cost, (n, people)) in [(small, listed[0]), (large, listed[1])] {
            assert!(n > 0, "{backing:?}: the verse has versions");
            assert!(
                cost.fold.values_decoded <= n + people,
                "{backing:?}: {} values decoded for {n} versions and {people} people",
                cost.fold.values_decoded
            );
            assert!(
                cost.app.records_read <= 2 * n + 2,
                "{backing:?}: {} records read for {n} versions",
                cost.app.records_read
            );
            assert_eq!(cost.app.conflict_walks, 0);
            assert_eq!(cost.app.chapters_projected, 0);
        }
    }
}

/// C7a: *Restore this version* costs its record: one command, the record's own keys
/// updated, and nothing of the rest of the project, on both corpora.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restore_costs_its_record() {
    use tc_api::conflicts::{HistoryView, Restore};
    use tc_api::write::Stored;
    use tc_app::backend::memory::Scripted;

    let key = || {
        tc_journal::RecordKey::Verse(
            PSALMS,
            tc_journal::ChapterNo::new(corpora().chapter).unwrap(),
            tc_journal::VerseNo::new(1).unwrap(),
        )
    };
    let inputs = std::sync::Mutex::new(Vec::new());
    let (_, (stored, _)) = super::harness::costs_what_it_touches(
        "a restore from History",
        super::harness::OnStore::Everything,
        async |s: &super::harness::Scale| {
            opened(s).await;
            let view: HistoryView = post(
                &s.app,
                &History {
                    at: s.scope(),
                    record: record_ref(&key()).unwrap(),
                },
            )
            .await
            .unwrap();
            let newest = &view.versions[0].version.value;
            let from = view
                .versions
                .iter()
                .find(|v| &v.version.value != newest)
                .expect("the corpus saved the verse more than once")
                .version
                .change
                .clone();
            inputs.lock().unwrap().push((view.shown, from));
            if let Some(memory) = s.memory() {
                memory.script(&s.folder, Scripted::Written).unwrap();
            }
        },
        async |s: &super::harness::Scale| {
            let (shown, from) = inputs.lock().unwrap().pop().unwrap();
            let ack = post(
                &s.app,
                &Restore {
                    at: s.scope(),
                    request: crate::common::rid(1),
                    record: record_ref(&key()).unwrap(),
                    shown,
                    from,
                },
            )
            .await
            .unwrap();
            assert_eq!(ack.stored, Stored::Written);
        },
    )
    .await;
    assert!(stored.store.commands > 0, "{stored:?}");
    assert!(stored.fold.keys_updated > 0, "{stored:?}");
    assert_eq!(stored.app.conflict_walks, 0, "{stored:?}");
    assert_eq!(stored.app.chapters_projected, 0, "{stored:?}");
}

/// C7a: *Keep all of <person>'s* costs the conflicts it was given, ten times as many on the
/// larger corpus: one command, a bounded number of records read per conflict, the resolved
/// records' keys updated, and no walk of the list or projection of a chapter.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bulk_keep_costs_its_conflicts() {
    use tc_api::conflicts::{BulkConflict, MadeBy, ResolveBulk};
    use tc_api::write::Stored;
    use tc_app::backend::memory::Scripted;

    for backing in [Backing::Memory, Backing::Store] {
        let inputs = std::sync::Mutex::new(Vec::new());
        let listed = std::sync::Mutex::new(Vec::new());
        let (small, large) = costs_on_both(
            backing,
            async |s: &super::harness::Scale| {
                opened(s).await;
                let view: ReviewView = post(
                    &s.app,
                    &List {
                        at: s.scope(),
                        tab: ReviewTab::Text,
                    },
                )
                .await
                .unwrap();
                let ReviewEntries::Conflicts(entries) = view.entries else {
                    panic!()
                };
                let MadeBy::Person(label) = &entries[0].versions[0].by else {
                    panic!("{:?}", entries[0])
                };
                inputs.lock().unwrap().push((label.who.clone(), entries));
                if let Some(memory) = s.memory() {
                    memory.script(&s.folder, Scripted::Written).unwrap();
                }
            },
            async |s: &super::harness::Scale| {
                let (person, entries) = inputs.lock().unwrap().pop().unwrap();
                let kept = post(
                    &s.app,
                    &ResolveBulk {
                        at: s.scope(),
                        request: crate::common::rid(2),
                        person,
                        conflicts: entries
                            .iter()
                            .map(|e| BulkConflict {
                                conflict: e.conflict.clone(),
                                review: e.review.clone(),
                            })
                            .collect(),
                    },
                )
                .await
                .unwrap();
                assert_eq!(kept.ack.stored, Stored::Written);
                listed
                    .lock()
                    .unwrap()
                    .push((entries.len() as u64, kept.left.len() as u64));
            },
        )
        .await;
        let listed = listed.lock().unwrap().clone();
        for (cost, (n, left), scale) in [
            (small, listed[0], "Psalms"),
            (large, listed[1], "ten times"),
        ] {
            assert!(
                n > 0 && left < n,
                "{backing:?} {scale}: {n} conflicts, {left} left"
            );
            assert!(
                cost.app.records_read <= READS_PER_CONFLICT * n + 64,
                "{backing:?} {scale}: {} records read for {n} conflicts",
                cost.app.records_read
            );
            assert_eq!(cost.app.conflict_walks, 0, "{backing:?} {scale}");
            assert_eq!(cost.app.chapters_projected, 0, "{backing:?} {scale}");
            if backing == Backing::Store {
                assert!(cost.store.commands > 0, "{backing:?} {scale}");
                // One resolving change per record kept; a passage's records count as one
                // conflict each way.
                assert!(
                    0 < cost.fold.changes_indexed && cost.fold.changes_indexed <= 4 * (n - left),
                    "{backing:?} {scale}: {} changes for {} conflicts kept",
                    cost.fold.changes_indexed,
                    n - left
                );
            }
        }
        assert!(
            listed[1].0 > listed[0].0,
            "{backing:?}: ten times the conflicts: {listed:?}"
        );
    }
}
