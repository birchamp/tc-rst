//! The conflict review (unit B11) through `App::request`, on the parity `shared` scenario
//! written to the store: two teammates' versions of Titus 1:1 arrive by a scripted sync, the
//! Text tab lists them as tC4's review does (#679), labelled as the sync's toast labels them
//! (#680), Keep this version writes one resolution (J18's end state), a second send of one
//! request gets the first reply (A-27), and the verse's History lists every version (#78).

use tc_api::conflicts::{
    ConflictKind, History, HistoryVersion, KeepChoice, List, MadeBy, Resolve, ReviewEntries,
    ReviewPlace, ReviewTab, ReviewView, Version, VersionCause, VersionValue,
};
use tc_api::editor::{Chapters, SaveVerse};
use tc_api::event::Event;
use tc_api::home::Card;
use tc_api::ids::{ChangeRef, Scope};
use tc_api::session::Close;
use tc_api::shown::{Known, PersonLabel, Who};
use tc_api::test::{ScriptedSync, TeammatesEdit};
use tc_api::write::Stored;
use tc_app::App;
use tc_app::events::Next;
use tc_app::wire::token::shown;
use tc_core::BookId;

use super::common::{folder, open_progress, post, rid, tit};
use super::sync::{TWO_TEAMMATES, internet_on, script, shared, sign_in, synced, teammate};
use super::understand::opened;
use crate::store_world::START_MS;

const ONE: &str = "Pablo, versión del compañero 1.";
const TWO: &str = "Pablo, versión del compañero 2.";

/// The shared project after two teammates changed Titus 1:1 from one version, open at Titus,
/// with this computer's clock at the store's.
async fn two_versions() -> (super::sync::World, Scope) {
    synced_with(TWO_TEAMMATES()).await
}

/// The shared project after a sync that `outcome` scripts, open at Titus.
async fn synced_with(outcome: ScriptedSync) -> (super::sync::World, Scope) {
    let w = shared().await;
    w.t.clock.set(START_MS);
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    script(app, 1, outcome).await;
    let synced = synced(app, 2).await;
    assert_eq!(synced.from, vec![teammate(1), teammate(2)]);
    let at = opened(app).await.session.at;
    (w, at)
}

async fn tab(app: &App, at: &Scope, tab: ReviewTab) -> ReviewView {
    post(
        app,
        &List {
            at: at.clone(),
            tab,
        },
    )
    .await
    .unwrap()
}

fn texts(versions: &[Version]) -> Vec<(PersonLabel, String, bool)> {
    versions
        .iter()
        .map(|v| {
            let MadeBy::Person(by) = &v.by else {
                panic!("{v:?}")
            };
            let VersionValue::Text(text) = &v.value else {
                panic!("{v:?}")
            };
            (by.clone(), text.clone(), v.shown)
        })
        .collect()
}

fn counts(view: &ReviewView) -> Vec<(ReviewTab, u32)> {
    view.tabs.iter().map(|t| (t.tab, t.count)).collect()
}

/// tC4's review of `sync-ok-teammates` (#679, #680): one verse, its versions newest first, each
/// with the label the toast gave its teammate and its date, the newest the one the grid shows;
/// every other tab is empty, so the review is tC4's one tab.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_tab_lists_each_version_with_its_teammate_and_date_newest_first() {
    let (w, at) = two_versions().await;
    let view = tab(&w.t.app, &at, ReviewTab::Text).await;
    assert_eq!(
        counts(&view),
        vec![
            (ReviewTab::Text, 1),
            (ReviewTab::Alignments, 0),
            (ReviewTab::Checks, 0),
            (ReviewTab::Settings, 0),
            (ReviewTab::Detached, 0),
            (ReviewTab::Notices, 0),
        ]
    );
    let ReviewEntries::Conflicts(entries) = view.entries else {
        panic!("{:?}", view.entries)
    };
    let [entry] = entries.as_slice() else {
        panic!("{entries:?}")
    };
    assert_eq!(entry.kind, ConflictKind::VerseText);
    assert_eq!(
        entry.place,
        ReviewPlace::Verse {
            book: BookId::Titus,
            chapter: 1,
            verse: 1
        }
    );
    assert_eq!(
        texts(&entry.versions),
        vec![
            (teammate(2), TWO.to_owned(), true),
            (teammate(1), ONE.to_owned(), false),
        ]
    );
    assert!(entry.versions[0].at > entry.versions[1].at);
    assert!(entry.versions.iter().all(|v| !v.clock_ahead));
    assert_eq!(entry.review.len(), 1, "one review token for the verse");
    assert!(!entry.incomplete_history);
    // The other tabs list nothing.
    for other in [
        ReviewTab::Alignments,
        ReviewTab::Checks,
        ReviewTab::Settings,
    ] {
        let view = tab(&w.t.app, &at, other).await;
        assert_eq!(
            view.entries,
            ReviewEntries::Conflicts(Vec::new()),
            "{other:?}"
        );
    }
    let detached = tab(&w.t.app, &at, ReviewTab::Detached).await;
    assert_eq!(detached.entries, ReviewEntries::Detached(Vec::new()));
    let notices = tab(&w.t.app, &at, ReviewTab::Notices).await;
    assert_eq!(notices.entries, ReviewEntries::Notices(Vec::new()));
}

/// STORAGE §8.5: a version dated more than a day after this computer's clock says that the
/// clock it was made under may be wrong; one dated before it does not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_version_dated_after_this_computers_clock_says_the_clock_may_be_wrong() {
    let (w, at) = two_versions().await;
    let teammates = || async {
        let view = tab(&w.t.app, &at, ReviewTab::Text).await;
        let ReviewEntries::Conflicts(entries) = view.entries else {
            panic!()
        };
        entries[0]
            .versions
            .iter()
            .map(|v| v.clock_ahead)
            .collect::<Vec<_>>()
    };
    assert_eq!(teammates().await, vec![false, false]);
    // A month before the teammates' versions.
    w.t.clock.set(START_MS - 45 * 24 * 60 * 60 * 1000);
    assert_eq!(teammates().await, vec![true, true]);
}

/// J18 (#679): Keep this version writes one resolution over every version the review showed.
/// The review then lists nothing, the grid shows the kept text with no marker, Home counts no
/// conflict, and a second send of the same Keep gets the first reply and writes nothing more
/// (A-27).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn keep_this_version_resolves_with_review_tokens_and_a_second_send_gets_the_first_reply() {
    let (w, at) = two_versions().await;
    let app = &w.t.app;
    let view = tab(app, &at, ReviewTab::Text).await;
    let ReviewEntries::Conflicts(entries) = view.entries else {
        panic!()
    };
    let entry = &entries[0];
    let keep = Resolve {
        at: at.clone(),
        request: rid(60),
        conflict: entry.conflict.clone(),
        review: entry.review.clone(),
        choice: KeepChoice::Candidate {
            change: entry.versions[1].change.clone(),
        },
    };
    let first = post(app, &keep).await.unwrap();
    assert_eq!(first.stored, Stored::Written);
    let again = post(app, &keep).await.unwrap();
    assert_eq!(again, first, "a second send gets the first reply");

    let after = tab(app, &at, ReviewTab::Text).await;
    assert_eq!(after.entries, ReviewEntries::Conflicts(Vec::new()));
    assert_eq!(after.tabs[0].count, 0);
    let text = post(
        app,
        &Chapters {
            at: at.clone(),
            book: BookId::Titus,
        },
    )
    .await
    .unwrap();
    assert_eq!(text.conflicts, 0);
    let verse = text.chapters[0]
        .verses
        .iter()
        .find(|v| v.key.as_str() == "1")
        .unwrap();
    assert_eq!(verse.text, ONE);
    assert_eq!(verse.words.marker, None);

    let history = history(app, &at, &entry.record).await;
    let kept: Vec<&HistoryVersion> = history
        .iter()
        .filter(|v| v.cause == VersionCause::Kept)
        .collect();
    assert_eq!(kept.len(), 1, "one resolution, however often Keep was sent");

    // Home reads a card at its visit and again only when told (`HomeCard`), as the UI does:
    // the read at the visit may come before the leave's checkpoint writes the conflicts file.
    let card = |app: &App| {
        let app = app.clone();
        async move {
            post(
                &app,
                &Card {
                    project: folder("sample_burrito"),
                },
            )
            .await
            .unwrap()
        }
    };
    let mut events = app.events().subscribe(app.events().head());
    post(
        app,
        &Close {
            at,
            request: rid(61),
        },
    )
    .await
    .unwrap();
    let mut read = card(app).await;
    let told = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while read.conflicts != Known::Known(0) {
            loop {
                match events.next().await {
                    Next::Frame(frame)
                        if frame.event
                            == (Event::HomeCard {
                                project: folder("sample_burrito"),
                            }) =>
                    {
                        break;
                    }
                    Next::Frame(_) => {}
                    other => panic!("{other:?}"),
                }
            }
            read = card(app).await;
        }
    })
    .await;
    assert!(
        told.is_ok(),
        "Home is told to read the card again and counts no conflict once the verse is kept: {:?}",
        read.conflicts
    );
}

async fn history(app: &App, at: &Scope, record: &tc_api::ids::RecordRef) -> Vec<HistoryVersion> {
    let view = post(
        app,
        &History {
            at: at.clone(),
            record: record.clone(),
        },
    )
    .await
    .unwrap();
    view.versions
}

/// #78: a verse's History lists every version, newest first, each with who made it, when
/// and how: the kept version, the two teammates' and the version they both started from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verse_history_lists_every_version_with_person_and_time() {
    let (w, at) = two_versions().await;
    let app = &w.t.app;
    let view = tab(app, &at, ReviewTab::Text).await;
    let ReviewEntries::Conflicts(entries) = view.entries else {
        panic!()
    };
    let entry = &entries[0];
    let before = history(app, &at, &entry.record).await;
    assert!(before.len() >= 3, "{before:?}");
    let newest: Vec<(MadeBy, VersionCause)> = before
        .iter()
        .take(2)
        .map(|v| (v.version.by.clone(), v.cause))
        .collect();
    let person = |n| MadeBy::Person(teammate(n));
    assert_eq!(
        newest,
        vec![
            (person(2), VersionCause::Edited),
            (person(1), VersionCause::Edited)
        ]
    );
    assert!(
        before
            .windows(2)
            .all(|w| w[0].version.at >= w[1].version.at)
    );
    assert_eq!(
        before.iter().filter(|v| v.version.shown).count(),
        1,
        "the version the grid shows"
    );
    let keep = Resolve {
        at: at.clone(),
        request: rid(62),
        conflict: entry.conflict.clone(),
        review: entry.review.clone(),
        choice: KeepChoice::Candidate {
            change: entry.versions[0].change.clone(),
        },
    };
    post(app, &keep).await.unwrap();
    let after = history(app, &at, &entry.record).await;
    assert_eq!(after.len(), before.len() + 1);
    let top = &after[0];
    assert_eq!(top.cause, VersionCause::Kept);
    assert_eq!(
        top.version.by,
        MadeBy::Person(PersonLabel {
            who: Who::Mine,
            computer: None
        })
    );
    assert_eq!(top.version.value, VersionValue::Text(TWO.to_owned()));
    assert!(top.version.shown);
    let changes: Vec<&ChangeRef> = after.iter().map(|v| &v.version.change).collect();
    let mut unique = changes.clone();
    unique.dedup();
    assert_eq!(unique.len(), changes.len(), "each version once");
}

/// A verse saved on this computer before a sync stands as a version of its own beside the two
/// teammates' (S4's `conflict-review-mine`): the scripted teammates edit Door43's copy, which
/// does not hold the save yet, as tC4's environment's teammates edit what they pulled.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_verse_saved_here_before_the_sync_is_a_version_of_its_own() {
    const MINE: &str = "Pablo, mi versión del versículo.";
    let w = shared().await;
    w.t.clock.set(START_MS);
    let app = &w.t.app;
    let at = app
        .sessions()
        .open_screen(&folder("sample_burrito"), open_progress())
        .await
        .unwrap()
        .at;
    let minted = app
        .sessions()
        .reader(&at)
        .unwrap()
        .read()
        .mint(&tit(1, 1), None)
        .unwrap();
    let ack = post(
        app,
        &SaveVerse {
            at: at.clone(),
            request: rid(70),
            shown: shown(&minted).unwrap(),
            words: MINE.to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
    post(
        app,
        &Close {
            at,
            request: rid(71),
        },
    )
    .await
    .unwrap();
    internet_on(app).await;
    sign_in(app).await;
    script(app, 1, TWO_TEAMMATES()).await;
    synced(app, 2).await;
    let at = opened(app).await.session.at;
    let ReviewEntries::Conflicts(entries) = tab(app, &at, ReviewTab::Text).await.entries else {
        panic!()
    };
    assert_eq!(entries.len(), 1);
    let versions = texts(&entries[0].versions);
    let mine: Vec<&str> = versions
        .iter()
        .filter(|(by, _, _)| by.who == Who::Mine)
        .map(|(_, text, _)| text.as_str())
        .collect();
    assert_eq!(versions.len(), 3, "{versions:?}");
    assert_eq!(mine, vec![MINE]);
}

/// Two teammates' versions of Titus 1:1 that differ only in a footnote look different in the
/// review and in History, each with its whole content as Translate's editor holds it, as
/// tC4's review shows `version.text.trim()`: otherwise Keep replaces the other footnote unseen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn versions_that_differ_only_in_a_footnote_look_different() {
    const FIRST: &str = "Pablo, siervo. \\f + \\ft nota del compañero\\f*";
    const SECOND: &str = "Pablo, siervo. \\f + \\ft otra nota\\f*";
    let (w, at) = synced_with(ScriptedSync::Synced {
        teammates: Some(TeammatesEdit {
            book: BookId::Titus,
            chapter: 1,
            verse: 1,
            texts: vec![FIRST.into(), SECOND.into()],
        }),
        restored: 0,
    })
    .await;
    let app = &w.t.app;
    let ReviewEntries::Conflicts(entries) = tab(app, &at, ReviewTab::Text).await.entries else {
        panic!()
    };
    let shown: Vec<String> = texts(&entries[0].versions)
        .into_iter()
        .map(|(_, text, _)| text)
        .collect();
    assert_eq!(shown, vec![SECOND.to_owned(), FIRST.to_owned()]);
    let history: Vec<VersionValue> = history(app, &at, &entries[0].record)
        .await
        .into_iter()
        .take(2)
        .map(|v| v.version.value)
        .collect();
    assert_eq!(
        history,
        vec![
            VersionValue::Text(SECOND.to_owned()),
            VersionValue::Text(FIRST.to_owned())
        ]
    );
}

/// One change of `device`, numbered `seq` in its sequence, in a batch of its own.
fn change_of(
    device: tc_journal::DeviceId,
    seq: u64,
    record: tc_journal::Record,
    parents: Vec<tc_journal::ChangeId>,
) -> tc_journal::Change {
    let mut batch = [0u8; 16];
    batch[..15].copy_from_slice(device.as_bytes().get(..15).unwrap());
    batch[15] = u8::try_from(seq).unwrap();
    tc_journal::Change::new(tc_journal::ChangeBody {
        v: 0,
        record,
        parents,
        device,
        seq: tc_journal::DeviceSeq(seq),
        at: tc_journal::WallTime(START_MS + i64::try_from(seq).unwrap() * 60_000),
        batch: tc_journal::BatchRef::new(tc_journal::BatchId::from_bytes(batch), 1).unwrap(),
        cause: tc_journal::Cause::Edit,
    })
    .unwrap()
}

/// `device` signed in to Door43 as `username`.
fn signed_in(device: tc_journal::DeviceId, seq: u64, username: &str) -> tc_journal::Change {
    let value = tc_journal::record::DeviceProfileValue {
        display_name: None,
        door43_username: Some(tc_journal::record::Door43Username::parse(username).unwrap()),
        legacy: None,
    };
    change_of(
        device,
        seq,
        tc_journal::Record::DeviceProfile { device, value },
        Vec::new(),
    )
}

/// STORAGE §8.5: versions from two computers that carry one person's label are told apart by
/// computer, in the review and in History. This computer and another signed in as the same
/// Door43 user ("ana", one person's two computers) are "Mine" on this computer and "ana" on
/// the other, never both "Mine", so only this computer's version is drawn as Mine; two
/// computers of one team account ("equipo") are numbered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn versions_of_two_computers_under_one_username_are_told_apart_by_computer() {
    use tc_api::shown::Computer;
    use tc_app::backend::memory::{MEMORY_DEVICE, MemoryBackend, teammates};
    use tc_journal::record::VerseValue;

    let state = super::scenario::scenario("sample");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let backend = std::sync::Arc::new(backend);
    let dir = tempfile::tempdir().unwrap();
    tc_app::backend::scenario::install_resources_into(
        &dir.path().join("app-data"),
        &state.resources().unwrap(),
    )
    .unwrap();
    let store = backend.clone();
    let t =
        super::common::TestApp::start_with(dir.path(), &super::common::Parts::new(), |_, deps| {
            deps.store = tc_app::StoreChoice::Given(store);
        })
        .await;
    let app = &t.app;
    let project = folder("sample_burrito");
    let me = tc_journal::DeviceId::local(MEMORY_DEVICE);
    let fold = backend.fold(&project).unwrap();
    let mut changes = backend
        .teammates_edit_verse(
            &project,
            &tit(1, 1),
            &["Pablo, uno.", "Pablo, dos.", "Pablo, tres."],
        )
        .unwrap();
    let mut mine = fold.projected(&tit(1, 1)).unwrap();
    let tc_journal::Record::Verse {
        value: VerseValue::Text { words, .. },
        ..
    } = &mut mine
    else {
        panic!("{mine:?}")
    };
    *words = "Pablo, en esta computadora.".to_owned();
    changes.push(change_of(me, 20, mine, fold.heads(&tit(1, 1))));
    changes.extend([
        signed_in(me, 21, "ana"),
        signed_in(teammates::device(1), 2, "ana"),
        signed_in(teammates::device(2), 2, "equipo"),
        signed_in(teammates::device(3), 2, "equipo"),
    ]);
    backend.teammate_edits(&project, changes).unwrap();
    assert_eq!(
        backend.fold(&project).unwrap().anomalies().count(),
        0,
        "no notice beside the conflict"
    );

    let at = super::understand::opened(app).await.session.at;
    let ReviewEntries::Conflicts(entries) = tab(app, &at, ReviewTab::Text).await.entries else {
        panic!()
    };
    let [entry] = entries.as_slice() else {
        panic!("{entries:?}")
    };
    let label = |who: Who, computer: Computer| PersonLabel {
        who,
        computer: Some(computer),
    };
    let ana = || Who::Username("ana".into());
    let equipo = || Who::Username("equipo".into());
    let expected = vec![
        (
            label(Who::Mine, Computer::This),
            "Pablo, en esta computadora.",
        ),
        (label(equipo(), Computer::Other(2)), "Pablo, tres."),
        (label(equipo(), Computer::Other(1)), "Pablo, dos."),
        (label(ana(), Computer::Other(1)), "Pablo, uno."),
    ];
    let listed: Vec<(PersonLabel, &str)> = entry
        .versions
        .iter()
        .map(|v| {
            let (MadeBy::Person(by), VersionValue::Text(text)) = (&v.by, &v.value) else {
                panic!("{v:?}")
            };
            (by.clone(), text.as_str())
        })
        .collect();
    assert_eq!(listed, expected);

    let history = history(app, &at, &entry.record).await;
    let newest: Vec<(MadeBy, VersionValue)> = history
        .iter()
        .take(4)
        .map(|v| (v.version.by.clone(), v.version.value.clone()))
        .collect();
    assert_eq!(
        newest,
        expected
            .into_iter()
            .map(|(by, text)| (MadeBy::Person(by), VersionValue::Text(text.to_owned())))
            .collect::<Vec<_>>()
    );
}
