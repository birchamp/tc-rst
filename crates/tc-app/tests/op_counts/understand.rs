//! The count rows of B3 (§8.2), on the memory backend and on the store: opening a book reads
//! that book's records and nothing of the rest of the project, and a book's resources are
//! read once for every chapter and read asked of it.

use tc_api::Reply;
use tc_api::editor::{Book, Sources};
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
