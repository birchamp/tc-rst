//! The count rows of B3 (§8.2), on the memory backend and on the store: opening a book reads
//! that book's records and nothing of the rest of the project, and a book's resources are
//! read once for every chapter and read asked of it.

use tc_api::Reply;
use tc_api::editor::{Book, Chapters, Sources};
use tc_api::helps::Passage;
use tc_app::App;

use super::corpus::{PSALMS, corpora};
use super::harness::{OnStore, Scale, costs_what_it_touches, opened};

async fn ok<E: tc_api::Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

/// The records `editor/book` reads of Psalms in a corpus: each verse record of each chapter,
/// each comment of the book, the pins and the helps setting are not read by it.
fn book_records(project: &tc_app::backend::memory::MemoryProject) -> u64 {
    let fold = &project.fold;
    let verses: usize = fold
        .chapters_of(PSALMS)
        .into_iter()
        .map(|c| fold.chapter_keys(PSALMS, c).verses.len())
        .sum();
    let comments = fold.comments_of(PSALMS).len();
    // A comment is read for its value and for when it was written.
    u64::try_from(verses + 2 * comments).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_book_touches_one_book() {
    let ((small, large), (store, _)) = costs_what_it_touches(
        "opening a book",
        OnStore::Everything,
        opened,
        async |s: &Scale| {
            let book = ok(
                &s.app,
                &Book {
                    at: s.scope(),
                    book: PSALMS,
                },
            )
            .await;
            assert!(book.chapters.len() >= 150, "{}", book.chapters.len());
        },
    )
    .await;
    let corpora = corpora();
    assert_eq!(
        small.app.records_read,
        book_records(&corpora.psalms),
        "{small:?}"
    );
    assert_eq!(
        large.app.records_read,
        book_records(&corpora.larger),
        "{large:?}"
    );
    assert_eq!(
        small.app.book_usfm, 0,
        "the book is read by its records, never projected"
    );
    assert_eq!(
        small.app.book_resource_loads, 0,
        "the book's text needs no resource"
    );
    assert_eq!(store.app, small.app, "the same reads on the store");
    assert_eq!(store.store.files_opened, 0, "{store:?}");
}

/// The records `editor/chapters` reads of Psalms: each verse's value, its conflict state and
/// its token.
fn translation_records(project: &tc_app::backend::memory::MemoryProject) -> u64 {
    let fold = &project.fold;
    let verses: usize = fold
        .chapters_of(PSALMS)
        .into_iter()
        .map(|c| fold.chapter_keys(PSALMS, c).verses.len())
        .sum();
    u64::try_from(3 * verses).unwrap()
}

/// Translate's text of a book (B4a): one read of that book's verse records, whatever the
/// project holds, so a chapter switch reads nothing (AD16). On the store a verse in conflict
/// costs one more value read, its token's candidate: a cost of the book's conflicts, which the
/// larger corpus's Psalms has one more of, never of the project's size.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn translate_text_touches_one_book() {
    let marked = std::sync::Mutex::new(Vec::new());
    let ((small, large), (store, store_large)) = costs_what_it_touches(
        "Translate's text of a book",
        OnStore::AppCounts,
        opened,
        async |s: &Scale| {
            let view = ok(
                &s.app,
                &Chapters {
                    at: s.scope(),
                    book: PSALMS,
                },
            )
            .await;
            assert!(view.chapters.len() >= 150, "{}", view.chapters.len());
            let n = view
                .chapters
                .iter()
                .flat_map(|c| c.verses.iter())
                .filter(|v| v.words.marker.is_some())
                .count();
            marked.lock().unwrap().push(u64::try_from(n).unwrap());
        },
    )
    .await;
    let corpora = corpora();
    assert_eq!(
        small.app.records_read,
        translation_records(&corpora.psalms),
        "{small:?}"
    );
    assert_eq!(
        large.app.records_read,
        translation_records(&corpora.larger),
        "{large:?}"
    );
    assert_eq!(
        small.app.book_usfm, 0,
        "read by its records, never projected"
    );
    assert_eq!(small.app.book_resource_loads, 0);
    assert_eq!(store.app, small.app, "the same reads on the store");
    assert_eq!(store.store.files_opened, 0, "{store:?}");
    let marked = marked.lock().unwrap().clone();
    let [_, _, store_small_marked, store_large_marked] = marked[..] else {
        panic!("{marked:?}")
    };
    assert_eq!(
        store_large.fold.values_read - store.fold.values_read,
        store_large_marked - store_small_marked,
        "only the conflicts' tokens read more"
    );
    let mut same = store_large.fold;
    same.values_read = store.fold.values_read;
    assert_eq!(same, store.fold, "nothing else of the fold grows");
}

/// On the store the row holds the app's counts: the one load reads the five pin groups and
/// the helps setting, whose values tc-store reads from the packs that hold them, and a pack's
/// framing is the store's own budget (tc-store's `tests/store/opcount.rs`). Every later read
/// of the book compares their versions, which are in memory, and reads no value.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn helps_load_one_book() {
    let chapter = corpora().chapter;
    let ((small, _), (store, _)) = costs_what_it_touches(
        "a book's helps and panes",
        OnStore::AppCounts,
        opened,
        async |s: &Scale| {
            let first = s.app.counters().book_resource_loads;
            let passage = Passage {
                at: s.scope(),
                book: PSALMS,
                chapter,
            };
            let sources = Sources {
                at: s.scope(),
                book: PSALMS,
                chapter,
            };
            // The screen asks for the panes and the helps at once, as Understand mounts.
            tokio::join!(ok(&s.app, &passage), ok(&s.app, &sources));
            assert_eq!(s.app.counters().book_resource_loads - first, 1, "one load");
            // Every later read of the book, another chapter's too, reads nothing again.
            let again = s.app.counters().book_resource_loads;
            ok(&s.app, &passage).await;
            ok(
                &s.app,
                &Passage {
                    chapter: chapter + 1,
                    ..passage.clone()
                },
            )
            .await;
            ok(&s.app, &sources).await;
            assert_eq!(s.app.counters().book_resource_loads, again, "then none");
        },
    )
    .await;
    assert_eq!(small.app.book_resource_loads, 1);
    assert_eq!(store.app.book_resource_loads, 1);
}
