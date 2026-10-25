//! The count rows of B8 and D2: the Community Checking preview of a book, and an export of one,
//! read that book's records and nothing of the rest of the project, on Psalms and ten times it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tc_api::Reply;
use tc_api::export::{Book, Exported, PrintItem, Producer, RunExport, Subject};
use tc_api::ids::{DownloadId, RequestId};
use tc_api::jobs::{JobOutcome, JobState};
use tc_app::jobs::JobRegistry;

use super::corpus::store_corpora;
use super::corpus::{PSALMS, corpora};
use super::harness::{Backing, Cost, Scale, cost, costs_on_both, growth, opened, scale};

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

/// The server's downloads: a temporary folder, each file left where the app wrote it.
struct Downloads(tempfile::TempDir);

impl tc_app::DownloadFiles for Downloads {
    fn dir(&self) -> PathBuf {
        self.0.path().to_owned()
    }

    fn add(&self, _: &Path, _: &str, _: &'static str) -> Result<DownloadId, String> {
        Ok(DownloadId::parse(&format!("{:032x}", 1)).unwrap())
    }
}

/// The screen session open and the downloads folder given, before the export.
async fn ready_to_export(s: &Scale) {
    opened(s).await;
    s.app
        .provide_downloads(Arc::new(Downloads(tempfile::tempdir().unwrap())));
}

/// Exports Psalms as aligned USFM and as plain USFM, each job to its end.
async fn export_psalms(s: &Scale) {
    for (n, producer) in [(1, Producer::AlignedUsfm), (2, Producer::PlainUsfm)] {
        let Reply::Ok(job) = s
            .app
            .request(&RunExport {
                request: RequestId::from_bytes([n; 16]),
                at: s.scope(),
                producer,
                subject: Subject::Book { book: PSALMS },
                print: None,
            })
            .await
            .unwrap()
        else {
            panic!("export/run")
        };
        let mut watch = s.app.jobs().watch(job).unwrap();
        let state = tokio::time::timeout(Duration::from_secs(60), async {
            watch.wait_for(JobRegistry::is_ended).await.unwrap().clone()
        })
        .await
        .expect("hung: the export ends");
        assert!(
            matches!(
                state,
                JobState::Done(JobOutcome::ExportReady(Exported { .. }))
            ),
            "{state:?}"
        );
    }
}

/// What an export's read of Psalms costs the fold: the book's USFM parts, then each verse
/// record and the alignment record of each verse with text. The larger corpus's Psalms is
/// another text, with other merges.
fn export_reads(project: &tc_app::backend::memory::MemoryProject) -> tc_journal::fold::Counters {
    use tc_journal::record::VerseValue;
    use tc_journal::{Record, RecordKey};

    let fold = &project.fold;
    let keys: Vec<_> = fold
        .chapters_of(PSALMS)
        .into_iter()
        .flat_map(|c| fold.chapter_keys(PSALMS, c).verses)
        .collect();
    let before = fold.counters();
    fold.book_parts(PSALMS).unwrap();
    for key in &keys {
        if let Some(Record::Verse {
            chapter,
            verse,
            value: VerseValue::Text { from, .. },
            ..
        }) = fold.projected(key)
        {
            let (chapter, verse) = from.map_or((chapter, verse), |f| (f.chapter, f.verse));
            fold.projected(&RecordKey::Alignment(PSALMS, chapter, verse));
        }
    }
    fold.counters().since(&before)
}

/// The two checkpoints `export_psalms` runs first, alone: on the store, a checkpoint with
/// nothing to commit still reads the project folder, which is tc-store's checkpoint budget
/// (PERFORMANCE "Owners"), not the export's.
async fn checkpoint_twice(s: &Scale) {
    let session = s.app.read(&s.scope()).unwrap();
    for _ in 0..2 {
        session
            .handle()
            .checkpoint(tc_app::backend::CheckpointReason::BeforeExport)
            .await;
    }
}

/// What `export_psalms` costs on Psalms and on ten times it over `backing`, the store's file
/// work of its checkpoints taken out.
async fn export_costs(backing: Backing) -> (Cost, Cost) {
    let (memory, disk) = (corpora(), store_corpora());
    let mut out = Vec::new();
    for (project, on_disk) in [
        (&memory.psalms, &disk.psalms),
        (&memory.larger, &disk.larger),
    ] {
        let s = scale(backing, project, on_disk).await;
        ready_to_export(&s).await;
        let checkpoints = cost(&s, checkpoint_twice).await;
        let mut export = cost(&s, export_psalms).await;
        export.store = export.store.since(&checkpoints.store);
        out.push(export);
    }
    (out[0], out[1])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_export_run_touches_one_book() {
    let corpora = corpora();
    let expected = [export_reads(&corpora.psalms), export_reads(&corpora.larger)];
    let mut on_memory: Option<Cost> = None;
    for backing in [Backing::Memory, Backing::Store] {
        let (small, large) = export_costs(backing).await;
        // The records projected and the values decoded (on the store, read) follow the book's
        // own merges, checked exactly below.
        let mut grew = growth(&small, &large);
        grew.retain(|g| {
            !g.starts_with("fold values_decoded")
                && !g.starts_with("fold values_read")
                && !g.starts_with("fold records_projected")
        });
        assert!(
            grew.is_empty(),
            "an export of a book grows with the project on {backing:?}:\n{}",
            grew.join("\n")
        );
        // Two exports, each reading the book once; the values past the book's own are the
        // project's few settings, the same on both.
        let mut beyond = Vec::new();
        for (cost, reads) in [small, large].iter().zip(&expected) {
            assert_eq!(
                cost.fold.records_projected,
                2 * reads.records_projected,
                "{backing:?} {cost:?}"
            );
            let values = match backing {
                Backing::Memory => cost.fold.values_decoded,
                Backing::Store => cost.fold.values_read,
            };
            beyond.push(values - 2 * reads.values_decoded);
            assert_eq!(cost.app.book_usfm, 2, "one book's parts per export");
            assert_eq!(
                cost.app.book_resource_loads, 1,
                "the original-language book, loaded once"
            );
        }
        assert_eq!(beyond[0], beyond[1], "{backing:?}");
        match backing {
            Backing::Memory => on_memory = Some(small),
            Backing::Store => {
                assert_eq!(Some(small.app), on_memory.map(|m| m.app), "the same reads");
                assert_eq!(small.store.commands, 0, "an export writes nothing");
                assert_eq!(small.store.files_opened, 0, "{small:?}");
            }
        }
    }
}
