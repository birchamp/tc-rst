//! The count rows of B2: Home reads each project's derived files once per launch, off the
//! request, and never walks a project's history.

use std::time::Duration;

use tc_api::Reply;
use tc_api::home::{Card, List};
use tc_api::test::{Quiet, QuietView};

use tc_api::shown::Known;

use super::corpus::corpora;
use super::harness::{Scale, cost, costs_what_it_touches, scale};

/// One Home visit: the list, then each card's own read, as the cards do when they mount.
async fn visit(s: &Scale) {
    let Reply::Ok(_) = s.app.request(&List {}).await.unwrap() else {
        panic!("home/list")
    };
    let Reply::Ok(_) = s
        .app
        .request(&Card {
            project: s.folder.clone(),
        })
        .await
        .unwrap()
    else {
        panic!("home/card")
    };
    settle(s).await;
}

/// Until the summary jobs the visit queued are done.
async fn settle(s: &Scale) {
    for _ in 0..1_000 {
        let Reply::Ok(QuietView { busy }) = s.app.request(&Quiet {}).await.unwrap() else {
            panic!("test/quiet")
        };
        if !busy {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the app did not settle");
}

/// A second Home visit renders nothing: its summaries are kept under their stamps.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_home_visit_renders_nothing() {
    let (small, large) = costs_what_it_touches("a second Home visit", visit, visit).await;
    for cost in [small, large] {
        let app = cost.app;
        assert_eq!(
            (
                app.book_usfm,
                app.books_rendered,
                app.files_rendered,
                app.conflict_walks,
                app.keys_probed,
                app.summaries_computed
            ),
            (0, 0, 0, 0, 0, 0),
            "{cost:?}"
        );
        assert_eq!(cost.fold.changes_visited, 0, "{cost:?}");
        assert_eq!(cost.fold.records_listed, 0, "{cost:?}");
    }
}

/// The first visit renders each book once, in the background, and walks no history at any
/// size. Its book count grows with the books of a project by design, so it is not held to
/// the same cost on both corpora: it is held to one projection per book.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn home_walks_no_history() {
    let corpora = corpora();
    for project in [&corpora.psalms, &corpora.larger] {
        let s = scale(project).await;
        let cost = cost(&s, visit).await;
        let Reply::Ok(card) = s
            .app
            .request(&Card {
                project: s.folder.clone(),
            })
            .await
            .unwrap()
        else {
            panic!("home/card")
        };
        let Known::Known(books) = card.books else {
            panic!("books unknown")
        };
        assert!(books.iter().all(|b| matches!(b.drafted, Known::Known(_))));
        assert_eq!(cost.app.book_usfm, books.len() as u64, "{cost:?}");
        assert_eq!(cost.app.summaries_computed, 1, "{cost:?}");
        assert_eq!(cost.app.conflict_walks, 1, "{cost:?}");
        assert_eq!(cost.fold.changes_visited, 0, "{cost:?}");
        assert_eq!(cost.fold.records_listed, 0, "{cost:?}");
    }
}
