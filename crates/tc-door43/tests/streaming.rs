//! Downloads stream to a file in the caller's folder, report their bytes, stop between
//! chunks on a cancel or when the internet switch goes off, and keep nothing unless they
//! succeed.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use common::{client, client_with, left_in, repo, sha, tag};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_door43::{
    Door43Client, Door43Error, DownloadTo, DownloadWatch, DownloadedZip, InternetSwitch, Limits,
    UnreachableCause, VerifiedArchive, VerifiedSbExport,
};
use tc_door43_fake::{FakeDoor43, Fault, FaultRule};

/// The fake holds a download after this many bytes ([`Fault::HoldBody`]).
const HELD_AT: u64 = 1000;

/// What a watch does at its first report.
#[derive(Default)]
enum AtFirstReport {
    #[default]
    Nothing,
    Cancel,
    SwitchOff(InternetSwitch),
}

/// Records every report, and cancels when told to.
#[derive(Default)]
struct Recorder {
    reports: Mutex<Vec<(u64, Option<u64>)>>,
    cancel: AtomicBool,
    at_first_report: AtFirstReport,
}

impl Recorder {
    fn new(at_first_report: AtFirstReport) -> Arc<Self> {
        Arc::new(Self {
            at_first_report,
            ..Self::default()
        })
    }

    fn reports(&self) -> Vec<(u64, Option<u64>)> {
        self.reports.lock().unwrap().clone()
    }

    fn most_received(&self) -> u64 {
        self.reports().iter().map(|r| r.0).max().unwrap_or(0)
    }

    /// Waits until `bytes` have been reported.
    async fn wait_for(&self, bytes: u64) {
        let start = Instant::now();
        while self.most_received() < bytes {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "{bytes} bytes never arrived: {:?}",
                self.reports()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

impl DownloadWatch for Recorder {
    fn received(&self, received: u64, announced: Option<u64>) {
        let mut reports = self.reports.lock().unwrap();
        if reports.is_empty() {
            match &self.at_first_report {
                AtFirstReport::Nothing => {}
                AtFirstReport::Cancel => self.cancel.store(true, Ordering::SeqCst),
                AtFirstReport::SwitchOff(switch) => switch.turn_off(),
            }
        }
        reports.push((received, announced));
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

type Task<T> = tokio::task::JoinHandle<Result<T, Door43Error>>;

/// Starts downloading the archive of `unfoldingWord/en_ult` v89 into `folder`.
async fn archive_task(
    fake: &FakeDoor43,
    client: Door43Client,
    folder: PathBuf,
    watch: Arc<Recorder>,
) -> Task<(DownloadedZip, VerifiedArchive)> {
    let commit = sha(&fake
        .resolve("unfoldingWord", "en_ult", "v89")
        .await
        .unwrap());
    tokio::spawn(async move {
        let to = DownloadTo::folder(&folder).watched_by(&*watch);
        client
            .download_archive_zip(&repo("unfoldingWord/en_ult"), &commit, to)
            .await
    })
}

/// Starts downloading the burrito export of `unfoldingWord/en_tn` v86 into `folder`.
async fn export_task(
    fake: &FakeDoor43,
    client: Door43Client,
    folder: PathBuf,
    watch: Arc<Recorder>,
) -> Task<VerifiedSbExport> {
    let commit = sha(&fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap());
    tokio::spawn(async move {
        let to = DownloadTo::folder(&folder).watched_by(&*watch);
        client
            .download_sb_export(&repo("unfoldingWord/en_tn"), &tag("v86"), &commit, to)
            .await
    })
}

fn hold(path: &str) -> FaultRule {
    FaultRule::once(
        "GET",
        path,
        Fault::HoldBody {
            after: HELD_AT as usize,
        },
    )
}

const ARCHIVE: &str = "/api/v1/repos/unfoldingWord/en_ult/archive/";
const EXPORT: &str = "/unfoldingWord/en_tn/sb/";

/// Reports grow with every chunk and end at the whole download.
fn assert_reports_grow_to(reports: &[(u64, Option<u64>)], size: u64) {
    assert!(!reports.is_empty());
    assert!(
        reports.windows(2).all(|w| w[0].0 < w[1].0),
        "the count only grows: {reports:?}"
    );
    assert_eq!(reports.last().unwrap().0, size, "{reports:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_chunk_reports_the_bytes_so_far_and_the_announced_size() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(hold(ARCHIVE));
    let watch = Recorder::new(AtFirstReport::Nothing);
    let task = archive_task(&fake, client(&fake), dl.path().to_owned(), watch.clone()).await;

    // The fake holds the rest back, so these are exactly the bytes before the hold.
    watch.wait_for(HELD_AT).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(watch.most_received(), HELD_AT, "{:?}", watch.reports());
    fake.release_held();
    let (zip, archive) = task.await.unwrap().unwrap();

    let reports = watch.reports();
    assert_reports_grow_to(&reports, zip.size());
    assert!(reports.iter().any(|r| r.0 == HELD_AT), "{reports:?}");
    assert!(
        reports.iter().all(|r| r.1 == Some(zip.size())),
        "every report carries the announced size: {reports:?}"
    );
    assert!(
        archive
            .files
            .iter()
            .any(|f| f.path == "ingredients/TIT.usfm")
    );

    // The zip is the one file in the caller's folder until it is dropped.
    assert_eq!(std::fs::metadata(zip.path()).unwrap().len(), zip.size());
    assert_eq!(zip.path().parent(), Some(dl.path()));
    assert_eq!(left_in(dl.path()).len(), 1);
    drop(zip);
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_with_no_announced_size_reports_none() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(FaultRule::once("GET", EXPORT, Fault::NoContentLength));
    fake.inject(hold(EXPORT));
    let watch = Recorder::new(AtFirstReport::Nothing);
    let task = export_task(&fake, client(&fake), dl.path().to_owned(), watch.clone()).await;
    watch.wait_for(HELD_AT).await;
    fake.release_held();
    let export = task.await.unwrap().unwrap();

    let reports = watch.reports();
    assert!(reports.len() >= 2, "{reports:?}");
    assert!(reports.iter().all(|r| r.1.is_none()), "{reports:?}");
    assert!(reports.windows(2).all(|w| w[0].0 < w[1].0), "{reports:?}");
    assert!(export.files.iter().any(|f| f.path == "ingredients/TIT.tsv"));
    assert_eq!(
        left_in(dl.path()),
        Vec::<String>::new(),
        "an export's zip is removed once it is checked"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_stops_at_the_next_chunk_and_keeps_nothing() {
    for path in [ARCHIVE, EXPORT] {
        let fake = FakeDoor43::start_seeded().await.unwrap();
        let dl = tempfile::tempdir().unwrap();
        fake.inject(hold(path));
        let watch = Recorder::new(AtFirstReport::Cancel);
        let folder = dl.path().to_owned();
        let result = if path == ARCHIVE {
            let task = archive_task(&fake, client(&fake), folder, watch.clone()).await;
            watch.wait_for(HELD_AT).await;
            fake.release_held();
            task.await.unwrap().map(drop)
        } else {
            let task = export_task(&fake, client(&fake), folder, watch.clone()).await;
            watch.wait_for(HELD_AT).await;
            fake.release_held();
            task.await.unwrap().map(drop)
        };
        assert!(
            matches!(&result, Err(Door43Error::Cancelled { route }) if route.starts_with("GET ")),
            "{path}: {result:?}"
        );
        assert_eq!(
            watch.most_received(),
            HELD_AT,
            "{path}: no chunk after the cancel is written or reported"
        );
        assert_eq!(left_in(dl.path()), Vec::<String>::new(), "{path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_internet_switch_stops_the_next_chunk_and_keeps_nothing() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(hold(EXPORT));
    let client = client(&fake);
    let watch = Recorder::new(AtFirstReport::SwitchOff(client.internet().clone()));
    let task = export_task(&fake, client, dl.path().to_owned(), watch.clone()).await;
    watch.wait_for(HELD_AT).await;
    fake.clear_requests();
    fake.release_held();
    let result = task.await.unwrap();
    assert!(
        matches!(&result, Err(Door43Error::Offline { route }) if route.contains("/sb/v86.zip")),
        "{result:?}"
    );
    assert_eq!(watch.most_received(), HELD_AT, "{:?}", watch.reports());
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
    assert_eq!(
        fake.requests(),
        [],
        "nothing is sent after the switch goes off"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_while_the_server_stalls_stops_within_the_check_interval() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(hold(EXPORT));
    // A read timeout far longer than the test: only the stop check can end the wait.
    let client = client_with(
        &fake,
        Limits {
            read_timeout: Duration::from_secs(120),
            ..Limits::default()
        },
    );
    let watch = Recorder::new(AtFirstReport::Nothing);
    let task = export_task(&fake, client, dl.path().to_owned(), watch.clone()).await;
    watch.wait_for(HELD_AT).await;
    watch.cancel.store(true, Ordering::SeqCst);
    let cancelled = Instant::now();
    let result = task.await.unwrap();
    let took = cancelled.elapsed();
    assert!(
        matches!(&result, Err(Door43Error::Cancelled { .. })),
        "{result:?}"
    );
    assert!(
        took < Duration::from_secs(2),
        "a cancel during a stall took {took:?}"
    );
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
    fake.release_held();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_after_the_last_chunk_still_keeps_nothing() {
    /// Cancels once the whole download has arrived.
    struct CancelAtEnd {
        seen: Mutex<Vec<u64>>,
        done: AtomicBool,
    }
    impl DownloadWatch for CancelAtEnd {
        fn received(&self, received: u64, announced: Option<u64>) {
            self.seen.lock().unwrap().push(received);
            if Some(received) == announced {
                self.done.store(true, Ordering::SeqCst);
            }
        }
        fn cancelled(&self) -> bool {
            self.done.load(Ordering::SeqCst)
        }
    }
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    let watch = CancelAtEnd {
        seen: Mutex::new(Vec::new()),
        done: AtomicBool::new(false),
    };
    let commit = sha(&fake
        .resolve("unfoldingWord", "en_ult", "v89")
        .await
        .unwrap());
    let to = DownloadTo::folder(dl.path()).watched_by(&watch);
    let result = client(&fake)
        .download_archive_zip(&repo("unfoldingWord/en_ult"), &commit, to)
        .await;
    assert!(watch.done.load(Ordering::SeqCst), "the whole zip arrived");
    assert!(
        matches!(&result, Err(Door43Error::Cancelled { .. })),
        "{result:?}"
    );
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
}

/// The commit lookup an archive download makes before it asks for the archive.
const COMMIT_LOOKUP: &str = "/api/v1/repos/unfoldingWord/en_ult/git/commits/";

/// Waits until the fake has received a request whose path starts with `prefix`.
async fn wait_for_request(fake: &FakeDoor43, prefix: &str) {
    let start = Instant::now();
    while !fake.requests().iter().any(|r| r.path.starts_with(prefix)) {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no request to {prefix}: {:?}",
            fake.requests()
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// How an archive download that stalls in its commit lookup ends when `stop` stops it,
/// and how long that took.
async fn stop_during_the_commit_lookup(
    stop: impl FnOnce(&Door43Client, &Recorder),
) -> (Result<(), Door43Error>, Duration) {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(FaultRule::once(
        "GET",
        COMMIT_LOOKUP,
        Fault::Delay(Duration::from_secs(10)),
    ));
    let client = client(&fake);
    let watch = Recorder::new(AtFirstReport::Nothing);
    let task = archive_task(&fake, client.clone(), dl.path().to_owned(), watch.clone()).await;
    wait_for_request(&fake, COMMIT_LOOKUP).await;
    stop(&client, &watch);
    let stopped = Instant::now();
    let result = task.await.unwrap().map(drop);
    let took = stopped.elapsed();
    assert!(
        !fake.requests().iter().any(|r| r.path.starts_with(ARCHIVE)),
        "the archive is never asked for: {:?}",
        fake.requests()
    );
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
    (result, took)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_while_the_commit_lookup_stalls_stops_within_the_check_interval() {
    let (result, took) =
        stop_during_the_commit_lookup(|_, watch| watch.cancel.store(true, Ordering::SeqCst)).await;
    assert!(
        matches!(&result, Err(Door43Error::Cancelled { route }) if route.contains("/archive/")),
        "{result:?}"
    );
    assert!(
        took < Duration::from_secs(2),
        "a cancel during the lookup took {took:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_internet_switch_stops_a_stalled_commit_lookup_within_the_check_interval() {
    let (result, took) =
        stop_during_the_commit_lookup(|client, _| client.internet().turn_off()).await;
    assert!(
        matches!(&result, Err(Door43Error::Offline { route }) if route.contains("/archive/")),
        "{result:?}"
    );
    assert!(
        took < Duration::from_secs(2),
        "turning the switch off during the lookup took {took:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_download_sends_nothing() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    let watch = Recorder::new(AtFirstReport::Nothing);
    watch.cancel.store(true, Ordering::SeqCst);
    let client = client(&fake);
    let ult = sha(&fake
        .resolve("unfoldingWord", "en_ult", "v89")
        .await
        .unwrap());
    let tn = sha(&fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap());
    let to = DownloadTo::folder(dl.path()).watched_by(&*watch);
    let archive = client
        .download_archive(&repo("unfoldingWord/en_ult"), &ult, to)
        .await;
    let export = client
        .download_sb_export(&repo("unfoldingWord/en_tn"), &tag("v86"), &tn, to)
        .await;
    assert!(
        matches!(archive, Err(Door43Error::Cancelled { .. })),
        "{archive:?}"
    );
    assert!(
        matches!(export, Err(Door43Error::Cancelled { .. })),
        "{export:?}"
    );
    assert_eq!(fake.requests(), []);
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_a_download_part_way_removes_its_file() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(hold(ARCHIVE));
    let watch = Recorder::new(AtFirstReport::Nothing);
    let task = archive_task(&fake, client(&fake), dl.path().to_owned(), watch.clone()).await;
    watch.wait_for(HELD_AT).await;
    assert_eq!(left_in(dl.path()).len(), 1, "the part written so far");
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
    fake.release_held();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transfer_that_breaks_off_keeps_nothing() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(FaultRule::once(
        "GET",
        EXPORT,
        Fault::BreakOff {
            after: HELD_AT as usize,
        },
    ));
    let watch = Recorder::new(AtFirstReport::Nothing);
    let result = export_task(&fake, client(&fake), dl.path().to_owned(), watch.clone())
        .await
        .await
        .unwrap();
    assert!(
        matches!(
            &result,
            Err(Door43Error::Unreachable {
                cause: UnreachableCause::Interrupted,
                ..
            })
        ),
        "{result:?}"
    );
    assert_eq!(
        watch.most_received(),
        HELD_AT,
        "the bytes before the break were written, and then removed"
    );
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_past_the_limit_with_no_announced_size_stops_there_and_keeps_nothing() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    fake.inject(FaultRule::once("GET", EXPORT, Fault::NoContentLength));
    let limit = 4096;
    let small = client_with(
        &fake,
        Limits {
            max_download_bytes: limit,
            ..Limits::default()
        },
    );
    let watch = Recorder::new(AtFirstReport::Nothing);
    let result = export_task(&fake, small, dl.path().to_owned(), watch.clone())
        .await
        .await
        .unwrap();
    assert!(
        matches!(result, Err(Door43Error::TooLarge { limit: 4096, .. })),
        "{result:?}"
    );
    assert!(watch.most_received() <= limit, "{:?}", watch.reports());
    assert_eq!(left_in(dl.path()), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_that_cannot_be_made_is_a_disk_error() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let dl = tempfile::tempdir().unwrap();
    let file = dl.path().join("a-file");
    std::fs::write(&file, b"not a folder").unwrap();
    let commit = sha(&fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap());
    let result = client(&fake)
        .download_sb_export(
            &repo("unfoldingWord/en_tn"),
            &tag("v86"),
            &commit,
            DownloadTo::folder(&file.join("downloads")),
        )
        .await;
    assert!(
        matches!(&result, Err(Door43Error::Disk { path, .. }) if path.starts_with(&file)),
        "{result:?}"
    );
    assert_eq!(left_in(dl.path()), ["a-file"]);
}
