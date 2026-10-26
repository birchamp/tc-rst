//! A watched install (the app's download job): the download reports its bytes, a cancel or
//! the internet switch stops it, and a stopped install leaves nothing in the store.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{nothing_installed, release, rig};
use tc_door43::{Door43Error, DownloadWatch, InternetSwitch};
use tc_door43_fake::{Fault, FaultRule};
use tc_resources::{
    InstallError, Installed, Packaging, ResourcePin, ResourceStore, install_dcs,
    install_dcs_watched,
};

const HELD_AT: u64 = 1000;
const EXPORT: &str = "/unfoldingWord/en_tn/sb/";
const ARCHIVE: &str = "/api/v1/repos/unfoldingWord/en_ult/archive/";

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

/// Starts a watched install of `pin` as `packaging`.
fn install_task(
    rig: &common::Rig,
    pin: &ResourcePin,
    packaging: Packaging,
    watch: Arc<Recorder>,
) -> tokio::task::JoinHandle<Result<Installed, InstallError>> {
    let (store, client, pin) = (rig.store.clone(), rig.client.clone(), pin.clone());
    tokio::spawn(
        async move { install_dcs_watched(&store, &client, &pin, packaging, &*watch).await },
    )
}

/// The pins and packagings each test runs: a burrito export, and a commit's archive.
async fn cases(rig: &common::Rig) -> [(ResourcePin, Packaging, &'static str); 2] {
    [
        (
            release(rig, "unfoldingWord/en_tn", "v86").await,
            Packaging::BurritoExport,
            EXPORT,
        ),
        (
            release(rig, "unfoldingWord/en_ult", "v89").await,
            Packaging::Archive,
            ARCHIVE,
        ),
    ]
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

fn no_partial_files(store: &ResourceStore) {
    let partial = store.root().join(".partial");
    let left = std::fs::read_dir(&partial).map(|d| d.count()).unwrap_or(0);
    assert_eq!(left, 0, "nothing may stay in {}", partial.display());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watched_install_reports_every_byte_of_its_download() {
    let rig = rig().await;
    for (pin, packaging, path) in cases(&rig).await {
        rig.fake.inject(hold(path));
        let watch = Recorder::new(AtFirstReport::Nothing);
        let task = install_task(&rig, &pin, packaging, watch.clone());
        watch.wait_for(HELD_AT).await;
        assert_eq!(
            watch.most_received(),
            HELD_AT,
            "{path}: the fake holds the rest"
        );
        rig.fake.release_held();
        let installed = task.await.unwrap().unwrap();
        installed.verify().unwrap();

        let reports = watch.reports();
        let (size, announced) = *reports.last().unwrap();
        assert_eq!(announced, Some(size), "{path}: {reports:?}");
        assert!(
            reports.windows(2).all(|w| w[0].0 < w[1].0),
            "{path}: {reports:?}"
        );
        assert!(reports.len() >= 2, "{path}: {reports:?}");
        no_partial_files(&rig.store);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_install_installs_nothing_and_can_run_again() {
    let rig = rig().await;
    for (pin, packaging, path) in cases(&rig).await {
        rig.fake.inject(hold(path));
        let watch = Recorder::new(AtFirstReport::Cancel);
        let task = install_task(&rig, &pin, packaging, watch.clone());
        watch.wait_for(HELD_AT).await;
        rig.fake.release_held();
        let err = task.await.unwrap().unwrap_err();
        assert!(
            matches!(err, InstallError::Door43(Door43Error::Cancelled { .. })),
            "{path}: {err}"
        );
        assert_eq!(watch.most_received(), HELD_AT, "{path}");
        nothing_installed(&rig.store);
    }
    // Nothing a cancel left behind stands in the way of the next try.
    for (pin, packaging, _) in cases(&rig).await {
        let watch = Recorder::new(AtFirstReport::Nothing);
        install_task(&rig, &pin, packaging, watch)
            .await
            .unwrap()
            .unwrap()
            .verify()
            .unwrap();
    }
    no_partial_files(&rig.store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_internet_switch_stops_an_install_at_the_next_chunk() {
    let rig = rig().await;
    for (pin, packaging, path) in cases(&rig).await {
        rig.internet.turn_on();
        rig.fake.inject(hold(path));
        let watch = Recorder::new(AtFirstReport::SwitchOff(rig.internet.clone()));
        let task = install_task(&rig, &pin, packaging, watch.clone());
        watch.wait_for(HELD_AT).await;
        rig.fake.release_held();
        let err = task.await.unwrap().unwrap_err();
        assert!(
            matches!(err, InstallError::Door43(Door43Error::Offline { .. })),
            "{path}: {err}"
        );
        assert_eq!(watch.most_received(), HELD_AT, "{path}");
        nothing_installed(&rig.store);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_install_dropped_part_way_installs_nothing() {
    let rig = rig().await;
    for (pin, packaging, path) in cases(&rig).await {
        rig.fake.inject(hold(path));
        let watch = Recorder::new(AtFirstReport::Nothing);
        let task = install_task(&rig, &pin, packaging, watch.clone());
        watch.wait_for(HELD_AT).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled(), "{path}");
        nothing_installed(&rig.store);
        rig.fake.release_held();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unwatched_install_leaves_no_download_behind() {
    let rig = rig().await;
    for (pin, _, _) in cases(&rig).await {
        install_dcs(&rig.store, &rig.client, &pin).await.unwrap();
    }
    let lexicon = common::commit_pin(&rig, "uW", "en_ugl", "HEAD").await;
    install_dcs(&rig.store, &rig.client, &lexicon)
        .await
        .unwrap();
    assert_eq!(rig.store.installed().unwrap().installs.len(), 3);
    no_partial_files(&rig.store);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_crash_part_way_leaves_the_download_in_partial_for_the_next_open_to_clear() {
    let rig = rig().await;
    let root = rig.store.root().to_owned();
    for (pin, packaging, path) in cases(&rig).await {
        rig.fake.inject(hold(path));
        let watch = Recorder::new(AtFirstReport::Nothing);
        let mut install = Box::pin(install_dcs_watched(
            &rig.store,
            &rig.client,
            &pin,
            packaging,
            &*watch,
        ));
        tokio::select! {
            () = watch.wait_for(HELD_AT) => {}
            result = &mut install => panic!("{path}: the held install ended: {result:?}"),
        }
        // A crash runs no destructor, so the part-written download stays where it is.
        std::mem::forget(install);
        let partial = root.join(".partial");
        let left: Vec<_> = std::fs::read_dir(&partial)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            left.len(),
            1,
            "{path}: the download is in .partial/: {left:?}"
        );

        let reopened = ResourceStore::open(&root).unwrap();
        nothing_installed(&reopened);
    }
}
