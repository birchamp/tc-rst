//! The count row of B8: the Community Checking preview of a book reads that book's verse
//! records once each and nothing of the rest of the project, on Psalms and ten times it.

use tc_api::Reply;
use tc_api::export::{Book, PrintItem};

use super::corpus::{PSALMS, corpora};
use super::harness::{Backing, Cost, Scale, costs_on_both, growth, opened};

/// The records the preview reads of Psalms, each verse record once; and the values the fold
/// decodes for them, one for each verse whose projection is one change (a merged value is
/// held decoded). The larger corpus's Psalms is another text, with other merges.
fn verse_reads(project: &tc_app::backend::memory::MemoryProject) -> (u64, u64) {
    let fold = &project.fold;
    let keys: Vec<_> = fold
        .chapters_of(PSALMS)
        .into_iter()
        .flat_map(|c| fold.chapter_keys(PSALMS, c).verses)
        .collect();
    let changes = keys
        .iter()
        .filter(|k| fold.projected_change(k).is_some())
        .count();
    (
        u64::try_from(keys.len()).unwrap(),
        u64::try_from(changes).unwrap(),
    )
}

async fn print_psalms(s: &Scale) {
    let Reply::Ok(print) = s
        .app
        .request(&Book {
            at: s.scope(),
            book: PSALMS,
        })
        .await
        .unwrap()
    else {
        panic!("export/book")
    };
    let chapters = print
        .items
        .iter()
        .filter(|i| matches!(i, PrintItem::Chapter(_)))
        .count();
    assert!(chapters >= 150, "{chapters}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn print_book_touches_one_book() {
    let corpora = corpora();
    let expected = [verse_reads(&corpora.psalms), verse_reads(&corpora.larger)];
    let mut on_memory: Option<Cost> = None;
    for backing in [Backing::Memory, Backing::Store] {
        let (small, large) = costs_on_both(backing, opened, print_psalms).await;
        // The values decoded (on the store, read) follow the book's own merges, checked
        // exactly below.
        let mut grew = growth(&small, &large);
        grew.retain(|g| {
            !g.starts_with("fold values_decoded") && !g.starts_with("fold values_read")
        });
        assert!(
            grew.is_empty(),
            "the preview of a book grows with the project on {backing:?}:\n{}",
            grew.join("\n")
        );
        for (cost, (records, decoded)) in [small, large].iter().zip(expected) {
            assert_eq!(cost.app.records_read, records, "{backing:?} {cost:?}");
            let values = match backing {
                Backing::Memory => cost.fold.values_decoded,
                Backing::Store => cost.fold.values_read,
            };
            assert_eq!(values, decoded, "{backing:?} {cost:?}");
            assert_eq!(
                cost.app.book_usfm, 0,
                "read by its records, never projected"
            );
            assert_eq!(cost.app.book_resource_loads, 0, "a print needs no resource");
        }
        match backing {
            Backing::Memory => on_memory = Some(small),
            Backing::Store => {
                assert_eq!(Some(small.app), on_memory.map(|m| m.app), "the same reads");
                assert_eq!(small.store.files_opened, 0, "{small:?}");
                assert_eq!(
                    small.store.commands, 0,
                    "reading the preview writes nothing"
                );
            }
        }
    }
}
