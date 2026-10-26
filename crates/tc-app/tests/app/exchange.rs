//! The release exchange port (E2, `tc_app::ports::usb::UsbExchange`) under the port contract:
//! an app on this computer's files, and a second computer (its own tc-store) that saves into
//! a copy of the project and writes a real exchange file for it.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tc_api::ApiError;
use tc_api::exchange::ExchangeFile;
use tc_api::ids::{ProjectRef, RequestId};
use tc_api::test::{Hold, HoldPoint, Release};
use tc_app::App;
use tc_app::StoreChoice;
use tc_app::config::ProjectsFolder;
use tc_door43_fake::FakeDoor43;
use tc_journal::command::Command;
use tc_journal::{BookId, ChapterNo, RecordKey, VerseNo};

use super::common::{Parts, TestApp, post};
use super::port_contract::ExchangePortFixture;
use super::store::{OnDisk, files_on_disk, make_project};
use crate::store_world::assert_valid_burrito_files;

const SAMPLE: &str = "sample_burrito";

pub(super) struct UsbWorld {
    pub(super) t: TestApp,
    disk: OnDisk,
    _fake: FakeDoor43,
}

pub(super) struct Usb;

impl ExchangePortFixture for Usb {
    type World = UsbWorld;
    const WRITES_FILES: bool = true;
    const RECEIVED: u32 = 2;
    const VERSE_CONFLICTS: u32 = 0;

    async fn world() -> UsbWorld {
        let fake = FakeDoor43::start().await.unwrap();
        tc_door43_fake::seeded::parity_account(&fake).await.unwrap();
        let disk = OnDisk::new();
        make_project(&disk.app_data(), &disk.projects, SAMPLE);
        let projects = disk.projects.clone();
        let url = fake.url().to_owned();
        let t = TestApp::start_with(disk.dir.path(), &Parts::new(), move |config, deps| {
            config.projects = ProjectsFolder::Given(projects);
            config.door43 = url.parse().unwrap();
            deps.store = StoreChoice::OnDisk;
            deps.exchange = Arc::new(tc_app::ports::usb::UsbExchange);
        })
        .await;
        UsbWorld {
            t,
            disk,
            _fake: fake,
        }
    }

    fn app(w: &UsbWorld) -> &App {
        &w.t.app
    }

    async fn export_fails(_w: &UsbWorld) -> Option<ApiError> {
        // The contract's destination is a folder that does not exist: nothing is written.
        None
    }

    async fn export_succeeds(w: &UsbWorld) -> String {
        let stick = w.disk.dir.path().join("stick");
        std::fs::create_dir_all(&stick).unwrap();
        stick.to_string_lossy().into_owned()
    }

    async fn receive_fails(w: &UsbWorld) -> (ExchangeFile, Option<ApiError>) {
        let damaged = w.disk.dir.path().join("damaged.zip");
        std::fs::write(&damaged, b"not a zip at all").unwrap();
        (
            ExchangeFile::Path(damaged.to_string_lossy().into_owned()),
            None,
        )
    }

    async fn receive_with_a_hand_edit(w: &UsbWorld) -> (ExchangeFile, String) {
        let exported = juans_file(w).await;
        let edited = w.disk.dir.path().join("edited.zip");
        rewrite_entry(&exported, &edited, "ingredients/TIT.usfm", |text| {
            text.replace(
                "Pablo, como Juan lo tiene",
                "Pablo, como alguien lo escribió",
            )
        });
        (
            ExchangeFile::Path(edited.to_string_lossy().into_owned()),
            "ingredients/TIT.usfm".to_owned(),
        )
    }

    async fn hold_receive(w: &UsbWorld) {
        post(
            &w.t.app,
            &Hold {
                request: RequestId::from_bytes([0xE2; 16]),
                at: HoldPoint::Checkpoint,
            },
        )
        .await
        .unwrap();
    }

    async fn receive_new_succeeds(w: &UsbWorld, name: &str) -> (ExchangeFile, ProjectRef) {
        let exported = juans_file(w).await;
        (
            ExchangeFile::Path(exported.to_string_lossy().into_owned()),
            ProjectRef::parse(name).unwrap(),
        )
    }

    async fn release_receive(w: &UsbWorld) {
        post(
            &w.t.app,
            &Release {
                request: RequestId::from_bytes([0xE3; 16]),
                at: HoldPoint::Checkpoint,
            },
        )
        .await
        .unwrap();
    }
}

/// Juan's computer: its own tc-store over a copy of the sample, two verses saved, exported.
async fn juans_file(w: &UsbWorld) -> PathBuf {
    juans_file_with(w, |_| {}).await
}

/// As [`juans_file`], with `before_open` run on the copied folder first, as another program
/// would put a file there.
async fn juans_file_with(w: &UsbWorld, before_open: impl FnOnce(&Path)) -> PathBuf {
    let dir = w.disk.dir.path();
    let projects = dir.join("projects-juan");
    std::fs::create_dir_all(&projects).unwrap();
    let platform = tc_store::FakePlatform {
        host: "juan".to_owned(),
        machine: "machine-of-juan".to_owned(),
        ..tc_store::FakePlatform::default()
    };
    let config = tc_store::StoreConfig {
        app_data: dir.join("app-juan"),
        projects: projects.clone(),
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(platform),
        clock: Arc::new(tc_store::SystemClock),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    };
    let store = tc_store::Store::start(config).unwrap();
    store.wait_cleanup();
    let folder = projects.join(SAMPLE);
    copy_dir(&w.disk.projects.join(SAMPLE), &folder);
    before_open(&folder);
    let handle = store
        .open(&folder, &tc_store::OpenOptions::default())
        .unwrap();
    handle.wait_idle();
    save(&handle, 1, 1, "Pablo, como Juan lo tiene");
    save(&handle, 1, 2, "en esperanza, como Juan lo tiene");
    let stick = dir.join("juans-stick");
    std::fs::create_dir_all(&stick).unwrap();
    let exported = tc_sync::exchange::export(&handle, &stick, "juan")
        .await
        .unwrap();
    handle.close().unwrap();
    // P5/INV-VALID-BURRITO on Juan's folder, which no FaultFs watch sees.
    assert_valid_burrito_files(&files_on_disk(&folder));
    exported
}

fn save(handle: &tc_store::ProjectHandle, c: u16, v: u16, words: &str) {
    let key = RecordKey::Verse(
        BookId::Titus,
        ChapterNo::new(c).unwrap(),
        VerseNo::new(v).unwrap(),
    );
    let shown = handle.mint(&key, None).unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: words.to_owned(),
        })
        .wait()
        .unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// `to` is `from` with the entry `name` rewritten by `edit`: what another program's edit
/// inside the exchange file looks like.
fn rewrite_entry(from: &Path, to: &Path, name: &str, edit: impl Fn(&str) -> String) {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(from).unwrap()).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(to).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let entry_name = entry.name().to_owned();
        if entry.is_dir() {
            out.add_directory(entry_name.as_str(), options).unwrap();
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if entry_name == name {
            bytes = edit(&String::from_utf8(bytes).unwrap()).into_bytes();
        }
        out.start_file(entry_name.as_str(), options).unwrap();
        out.write_all(&bytes).unwrap();
    }
    out.finish().unwrap();
}

/// #705: a recording a teammate put into the project folder with another program arrives
/// byte for byte through an exchange file (STORAGE §10.2 R5, carried files as X built them).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_teammates_audio_file_arrives_byte_for_byte() {
    use tc_api::jobs::{JobOutcome, JobState};

    use super::common::{folder, rid};
    use super::port_contract::ended;

    let w = Usb::world().await;
    let wav: Vec<u8> = (0..4096u32).map(|i| (i * 7 % 251) as u8).collect();
    let exported = juans_file_with(&w, |folder| {
        let audio = folder.join("ingredients/audio");
        std::fs::create_dir_all(&audio).unwrap();
        std::fs::write(audio.join("jon_01.wav"), &wav).unwrap();
    })
    .await;
    let job = post(
        &w.t.app,
        &tc_api::exchange::Receive {
            request: rid(10),
            project: folder(SAMPLE),
            file: ExchangeFile::Path(exported.to_string_lossy().into_owned()),
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        ended(&w.t.app, job).await,
        JobState::Done(JobOutcome::Received(_))
    ));
    let arrived = w
        .disk
        .projects
        .join(SAMPLE)
        .join("ingredients/audio/jon_01.wav");
    assert_eq!(std::fs::read(&arrived).unwrap(), wav, "INV-CARRIED-BYTES");
}
