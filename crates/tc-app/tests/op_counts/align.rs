//! The count rows of B6 (§8.2), on Psalms and on the corpus ten times larger: opening one
//! verse in the aligner and editing it touch that verse, whatever the size of the project.

use tc_api::Reply;
use tc_api::align::{AlignEdit, AlignerView, BankClick, Edit, Open, OpenVerse, Opened};
use tc_api::ids::RequestId;
use tc_app::App;

use super::corpus::{PSALMS, corpora};
use super::harness::{OnStore, Scale, costs_what_it_touches, opened};

async fn ok<E: tc_api::Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

fn request(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

/// Verse 1 of a chapter whose records are the same in both corpora.
fn first_verse() -> OpenVerse {
    OpenVerse {
        chapter: corpora().chapter,
        verse: 1,
    }
}

async fn open_verse(s: &Scale, at: OpenVerse, n: u8) -> AlignerView {
    match ok(
        &s.app,
        &Open {
            at: s.scope(),
            request: request(n),
            book: PSALMS,
            verse: Some(at),
        },
    )
    .await
    {
        Opened::Ready(view) => *view,
        other => panic!("{other:?}"),
    }
}

/// Opening a verse reads that verse's record and its alignment, the pins and the numbering,
/// and loads the book's resources once; the project's size changes none of it. The first
/// verse costs the cut of the Hebrew Bible, the second does not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_open_touches_one_verse() {
    let ((small, _), _) = costs_what_it_touches(
        "opening one verse in the aligner",
        OnStore::AppCounts,
        opened,
        async |s: &Scale| {
            let at = first_verse();
            let view = open_verse(s, at, 1).await;
            assert!(!view.cards.is_empty());
        },
    )
    .await;
    assert_eq!(small.app.book_resource_loads, 1, "{small:?}");
    // The verse, its alignment, the pins (five groups), the versification and the two
    // tokens: a handful of records, not the book's.
    assert!(small.app.records_read <= 24, "{small:?}");
}

/// A click, a placement and a Mark valid read nothing of the project: the session holds the
/// verse.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_edit_touches_one_verse() {
    let ((small, large), _) = costs_what_it_touches(
        "editing one verse in the aligner",
        OnStore::Everything,
        async |s: &Scale| {
            opened(s).await;
            let at = first_verse();
            open_verse(s, at, 1).await;
        },
        async |s: &Scale| {
            let at = first_verse();
            let view = open_verse(s, at, 1).await;
            let clicked = ok(
                &s.app,
                &Edit {
                    at: s.scope(),
                    request: request(2),
                    aligner: view.aligner,
                    seq: view.seq,
                    edit: AlignEdit::ClickBank {
                        word: view.bank[0].clone(),
                        click: BankClick::Plain,
                    },
                },
            )
            .await;
            assert_eq!(clicked.seq, view.seq + 1);
        },
    )
    .await;
    // The session holds the verse: a click reads nothing of the project.
    assert_eq!(small.app.records_read, 0, "{small:?}");
    assert_eq!(small.app.book_resource_loads, 0, "{small:?}");
    assert_eq!(large.app.records_read, 0, "{large:?}");
}

/// The rail reads each verse of the book and its alignment once, and nothing of the rest of the
/// project: its cost is the book's, whatever the history behind it (PERF rule 1).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn align_index_costs_the_book() {
    let ((small, large), _) = costs_what_it_touches(
        "the aligner's rail",
        OnStore::AppCounts,
        opened,
        async |s: &Scale| {
            let rail = ok(
                &s.app,
                &tc_api::align::Index {
                    at: s.scope(),
                    book: PSALMS,
                },
            )
            .await;
            assert!(rail.items.len() > 2_000, "{}", rail.items.len());
        },
    )
    .await;
    assert_eq!(small.app.book_resource_loads, 0, "{small:?}");
    // Two records for each of the book's verses (its text, its alignment), and no more.
    assert!(small.app.records_read <= 2 * 2_461 + 8, "{small:?}");
    assert_eq!(small.app.records_read, large.app.records_read);
}
