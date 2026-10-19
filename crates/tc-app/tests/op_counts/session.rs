//! The count rows of B1b, on the memory backend and on the store (C1a): opening a project,
//! taking snapshots, reading a chapter and a token's round trip cost what they touch, and the
//! harness fails an action that costs the project.

use tc_api::app::Hello;
use tc_app::counters::AppCounts;
use tc_app::wire::token::{edit_token, shown};
use tc_journal::fold::Counters;

use super::corpus::{PSALMS, corpora};
use super::harness::{
    Backing, OnStore, Scale, costs_on_both, costs_what_it_touches, growth, nothing, opened,
};

/// The row's test: an open makes no counted read and walks nothing, at any size (§8.2; there
/// is no project-wide index, A-21). tc-journal counts no work for `record_keys`, so no count
/// sees a key listing; the lint holds that half (`lint.rs`). On the store the open folds the
/// project, which is the store's budget (tc-store's `tests/store/opcount.rs`): there the row
/// holds the app's counts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_lists_no_key() {
    let ((small, large), (store_small, store_large)) =
        costs_what_it_touches("an open", OnStore::AppCounts, nothing, async |s: &Scale| {
            s.open().await;
            s.app.request(&Hello {}).await.unwrap();
        })
        .await;
    for cost in [small, large] {
        assert_eq!(cost.app, AppCounts::default(), "{cost:?}");
        assert_eq!(cost.fold, Counters::default(), "{cost:?}");
    }
    for cost in [store_small, store_large] {
        assert_eq!(cost.app, AppCounts::default(), "{cost:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_costs_nothing_at_any_size() {
    let (memory, store) = costs_what_it_touches(
        "a snapshot",
        OnStore::Everything,
        opened,
        async |s: &Scale| {
            let session = s.app.read(&s.scope()).unwrap();
            let mut kept = Vec::new();
            for _ in 0..1_000 {
                let read = session.read();
                kept.push(read.clone());
                kept.push(read);
            }
        },
    )
    .await;
    for cost in [memory.0, memory.1, store.0, store.1] {
        assert_eq!(cost.app, AppCounts::default());
        assert_eq!(cost.fold, Counters::default());
        assert_eq!(cost.store, Default::default(), "a snapshot touches no file");
    }
}

/// What the editor reads of a chapter: its keys (R1), each verse's state and token, and the
/// chapter's USFM.
async fn read_chapter(s: &Scale, chapter: u16) {
    let session = s.app.read(&s.scope()).unwrap();
    let read = session.read();
    let keys = read.chapter_keys(PSALMS, chapter);
    for key in keys.front.iter().chain(&keys.verses) {
        read.key_state(key);
        read.mint(key, None).unwrap();
    }
    assert!(read.chapter_usfm(PSALMS, chapter).is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chapter_read_costs_one_chapter() {
    let chapter = corpora().chapter;
    let ((small, _), (store, _)) = costs_what_it_touches(
        "a chapter read",
        OnStore::Everything,
        opened,
        async |s: &Scale| read_chapter(s, chapter).await,
    )
    .await;
    // The row measures something, the same on both backends; the store reads no file for it.
    assert!(small.app.keys_probed > 0, "{small:?}");
    assert!(small.app.records_read > 0, "{small:?}");
    assert_eq!(small.app.chapters_projected, 1);
    assert_eq!(store.app, small.app);
    assert_eq!(store.store.files_opened, 0, "{store:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_token_round_trip_costs_one_record() {
    let chapter = corpora().chapter;
    let ((small, _), (store, _)) = costs_what_it_touches(
        "a token's round trip",
        OnStore::Everything,
        opened,
        async |s: &Scale| {
            let session = s.app.read(&s.scope()).unwrap();
            let read = session.read();
            let key = read.chapter_keys(PSALMS, chapter).verses[0].clone();
            let token = shown(&read.mint(&key, None).unwrap()).unwrap();
            let back = edit_token(session.handle().as_ref(), &token).unwrap();
            assert_eq!(back.key(), &key);
        },
    )
    .await;
    // The range read finds the key, the token is one record read, and parsing it reads none.
    assert_eq!(small.app.records_read, 1);
    assert!(small.app.keys_probed > 0);
    assert_eq!(store.app, small.app);
}

/// The planted case: an action that reads every chapter of every book costs the project, and
/// the harness says so. Without it, a harness that compared nothing would pass every row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_harness_fails_an_action_that_costs_the_project() {
    for backing in [Backing::Memory, Backing::Store] {
        let (small, large) = costs_on_both(backing, opened, async |s: &Scale| {
            let read = s.app.read(&s.scope()).unwrap().read();
            for book in read.projected_books() {
                for chapter in read.chapters_of(book) {
                    read.chapter_keys(book, chapter);
                }
            }
        })
        .await;
        let grew = growth(&small, &large);
        assert!(
            grew.iter().any(|g| g.starts_with("app keys_probed")),
            "{backing:?}: {grew:?}"
        );
    }
}
