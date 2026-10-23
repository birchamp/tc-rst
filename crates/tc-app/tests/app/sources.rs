//! Project settings from Home and Get source texts with its downloads (unit B9a), through
//! `App::request` on the parity sample: the settings-contract's read part (tC4
//! `e2e/settings-contract.spec.ts`), the packages, a package's rows from the fake Door43's
//! catalog, and the `Download` job, whose cancel leaves no install and no part-downloaded file.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tc_api::ApiError;
use tc_api::endpoint::NetKind;
use tc_api::ids::{JobId, ProjectRef, Scope};
use tc_api::jobs::{Cancel, JobOutcome, JobState};
use tc_api::net::Set;
use tc_api::refusal::{OpenRefusal, Refusal};
use tc_api::resources::{
    Catalog, Download, FromInstalls, GatewayRef, PackageRole, Sources, SourcesView,
};
use tc_api::session::{Open, OpenUnit};
use tc_api::settings::{SettingsView, ViewFromHome};
use tc_api::shown::Direction;
use tc_api::{Endpoint, Reply};
use tc_app::backend::memory::MemoryBackend;
use tc_app::backend::scenario::install_resources_into;
use tc_app::backend::scenario_store::ScenarioStore;
use tc_app::config::ProjectsFolder;
use tc_app::{App, StoreChoice};
use tc_core::BookId;
use tc_door43_fake::{CommitSeed, FakeDoor43, Fault, FaultRule, ReleaseKind, RepoSeed, SeedFile};
use tc_resources::ResourceStore;
use tc_store::FakePlatform;
use tc_store::fault::FaultFs;

use super::common::{Parts, TestApp, folder, rid};
use super::scenario::scenario;
use crate::store_world::{APP, PROJECTS, backend_over};

const SAMPLE: &str = "sample_burrito";

/// The sample on the memory backend with the scenario's resources installed, its Door43 the
/// given fake.
async fn start(door43: Option<&FakeDoor43>) -> (tempfile::TempDir, TestApp) {
    let state = scenario("sample");
    let (backend, _) = MemoryBackend::from_scenario(state).unwrap();
    let dir = tempfile::tempdir().unwrap();
    install_resources_into(&dir.path().join("app-data"), &state.resources().unwrap()).unwrap();
    let url = door43.map(|f| f.url().to_owned());
    let t = TestApp::start_with(dir.path(), &Parts::new(), |config, deps| {
        deps.store = StoreChoice::Given(Arc::new(backend));
        if let Some(url) = url {
            config.door43 = url.parse().unwrap();
        }
    })
    .await;
    (dir, t)
}

async fn ok<E: Endpoint>(app: &App, req: &E) -> E::Ok {
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => ok,
        Reply::Err(e) => panic!("{}: {e:?}", E::PATH),
    }
}

async fn err<E: Endpoint>(app: &App, req: &E) -> ApiError
where
    E::Ok: std::fmt::Debug,
{
    match app.request(req).await.unwrap() {
        Reply::Ok(ok) => panic!("{}: {ok:?}", E::PATH),
        Reply::Err(e) => e,
    }
}

async fn ended(app: &App, job: JobId) -> JobState {
    let mut watch = app.jobs().watch(job).unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        watch
            .wait_for(tc_app::jobs::JobRegistry::is_ended)
            .await
            .unwrap()
            .clone()
    })
    .await
    .expect("the job ends")
}

async fn open_sample(app: &App) -> Scope {
    let job = ok(
        app,
        &Open {
            request: rid(40),
            project: folder(SAMPLE),
            unit: Some(OpenUnit::Book(BookId::Titus)),
        },
    )
    .await;
    match ended(app, job).await {
        JobState::Done(JobOutcome::Opened(opened)) => opened.session.at,
        other => panic!("{other:?}"),
    }
}

async fn internet(app: &App, on: bool, id: u8) {
    ok(
        app,
        &Set {
            request: rid(id),
            on,
        },
    )
    .await;
}

fn gateway(language: &str, owner: &str) -> GatewayRef {
    GatewayRef {
        language: language.into(),
        owner: owner.into(),
    }
}

fn settings_from_home(project: &str) -> ViewFromHome {
    ViewFromHome {
        project: ProjectRef::parse(project).unwrap(),
    }
}

fn option_keys(view: &SettingsView) -> Vec<String> {
    match &view.gateway.options {
        FromInstalls::Listed(options) => options.iter().map(|o| o.gateway.key()).collect(),
        FromInstalls::Failed { reason } => panic!("{reason}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_from_home_show_the_sample_as_tc4_does() {
    // The baseline screen `project-settings`: left to right, the first font, "This project
    // checks in Español (Latinoamérica) · es-419_gl." from the pins, and English, the one
    // package installed that covers its books. The language name is the metadata's, where
    // tC4 shows an empty field for a project its settings file names none in (#832.2, a
    // declared difference).
    let (_dir, t) = start(None).await;
    let view = ok(&t.app, &settings_from_home(SAMPLE)).await;
    assert_eq!(view.language_name, "Español (Latinoamérica)");
    assert_eq!(view.direction, Direction::Ltr);
    assert_eq!(view.font, None);
    let current = view.gateway.current.clone().unwrap();
    assert_eq!(
        current.autonym, "Español (Latinoamérica)",
        "#579: the autonym"
    );
    assert_eq!(current.gateway, gateway("es-419", "es-419_gl"));
    assert_eq!(option_keys(&view), ["en::unfoldingWord"]);
    assert!(
        t.app.sessions().open_sessions().is_empty(),
        "Settings from Home opens nothing (AD14)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_from_home_of_a_folder_that_is_gone_say_so() {
    let (_dir, t) = start(None).await;
    assert_eq!(
        err(&t.app, &settings_from_home("no_such_project")).await,
        ApiError::Refused(Refusal::Open(OpenRefusal::NotAProject))
    );
}

/// The sample written to disk on tc-store's in-memory file system, under an app over it.
async fn on_the_store(dir: &Path) -> (FaultFs, TestApp) {
    let fs = FaultFs::new();
    let backend = backend_over(&fs, APP, FakePlatform::default(), Arc::default());
    let (store, _) = ScenarioStore::write(backend, scenario("sample")).unwrap();
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let t = TestApp::start_with(dir, &Parts::new(), |config, deps| {
        config.projects = ProjectsFolder::Given(projects.clone());
        deps.store = StoreChoice::Given(Arc::new(store));
    })
    .await;
    (fs, t)
}

fn checking_file(name: &str) -> std::path::PathBuf {
    Path::new(PROJECTS)
        .join(SAMPLE)
        .join("ingredients/checking")
        .join(name)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_from_home_read_what_the_project_files_hold() {
    // settings-contract, "a restart shows the saved choices" and "an existing project that
    // carries the obsolete checkingLanguage keeps it, and the field has no effect": the dialog
    // shows the file's direction, font and language name, and the pins, never the old field,
    // name the checking language. The direction is the one every screen shows: the metadata's
    // `scriptDirection` when it names one (here `ltr`), the file's only when it does not
    // (INV-TEXT-DIRECTION-HAS-ONE-SOURCE, tC4 #832.1).
    let dir = tempfile::tempdir().unwrap();
    let (fs, t) = on_the_store(dir.path()).await;
    let settings = r#"{"schemaVersion":1,"checkingLanguage":"en","textDirection":"rtl",
        "textFont":"Charis SIL","languageName":"Español"}"#;
    fs.put(&checking_file("settings.json"), settings.as_bytes(), None);
    let view = ok(&t.app, &settings_from_home(SAMPLE)).await;
    assert_eq!(view.direction, Direction::Ltr);
    assert_eq!(view.font.as_deref(), Some("Charis SIL"));
    assert_eq!(view.language_name, "Español");
    let current = view.gateway.current.unwrap();
    assert_eq!(current.gateway, gateway("es-419", "es-419_gl"));
    assert_eq!(current.autonym, "Español (Latinoamérica)");
    assert_eq!(
        fs.peek(&checking_file("settings.json")).unwrap(),
        settings.as_bytes(),
        "nothing is written"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settings_file_another_program_holds_is_stated_not_defaulted() {
    // tC4's D30 sweep: defaults shown over an unread file would let a save write them over the
    // project's real direction and font.
    let dir = tempfile::tempdir().unwrap();
    let (fs, t) = on_the_store(dir.path()).await;
    fs.set_unreadable(&checking_file("settings.json"), true);
    match err(&t.app, &settings_from_home(SAMPLE)).await {
        ApiError::Refused(Refusal::Open(OpenRefusal::Unreadable { reason })) => {
            assert!(reason.contains("settings.json"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    fs.set_unreadable(&checking_file("settings.json"), false);
    ok(&t.app, &settings_from_home(SAMPLE)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pins_another_program_saved_broken_are_stated_not_no_checking_language() {
    let dir = tempfile::tempdir().unwrap();
    let (fs, t) = on_the_store(dir.path()).await;
    fs.put(
        &checking_file("resources.json"),
        br#"{"schemaVersion":2, "languageSets": {"#,
        None,
    );
    match err(&t.app, &settings_from_home(SAMPLE)).await {
        ApiError::Refused(Refusal::Open(OpenRefusal::Unreadable { reason })) => {
            assert!(reason.contains("resources.json"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_step_lists_tc4s_packages_with_the_projects_checking_language() {
    let (_dir, t) = start(None).await;
    let at = open_sample(&t.app).await;
    let view: SourcesView = ok(&t.app, &Sources { at }).await;
    assert_eq!(view.gateways.len(), 18);
    let english = &view.gateways[0].package;
    assert_eq!(english.gateway.key(), "en::unfoldingWord");
    assert_eq!(
        (&*english.seen.tn, &*english.seen.tw, &*english.seen.ta),
        ("v89", "v89", "v89")
    );
    let urdu = &view.gateways[17].package;
    assert_eq!(urdu.autonym, "उर्दू", "#701: named in Devanagari");
    assert_eq!(urdu.direction, Direction::Ltr);
    assert!(view.gateways.iter().all(|g| g.installed == 0));
    assert_eq!(view.current, Some(gateway("es-419", "es-419_gl")));
    assert!(view.pinned);
    let FromInstalls::Listed(checkable) = view.checkable else {
        panic!("{:?}", view.checkable)
    };
    assert_eq!(checkable, [gateway("en", "unfoldingWord")]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_lists_what_the_catalog_offers_only_with_the_internet_on() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let (_dir, t) = start(Some(&fake)).await;
    let at = open_sample(&t.app).await;
    let catalog = Catalog {
        at,
        gateway: gateway("en", "unfoldingWord"),
        book: BookId::Titus,
    };
    assert_eq!(
        err(&t.app, &catalog).await,
        ApiError::NeedsInternet(NetKind::Sources),
        "nothing is sent while the internet is off"
    );
    assert!(fake.requests().is_empty());
    internet(&t.app, true, 1).await;
    let rows = ok(&t.app, &catalog).await.rows;
    let mut found: Vec<(&str, PackageRole)> =
        rows.iter().map(|r| (r.repo.as_str(), r.role)).collect();
    found.sort_unstable_by_key(|(repo, _)| *repo);
    assert_eq!(
        found,
        [
            ("en_tn", PackageRole::Notes),
            ("en_twl", PackageRole::WordLinks),
            ("en_ult", PackageRole::Text),
        ]
    );
}

const HELD_AT: usize = 1000;
/// A repository the sample does not pin, under a registry organization: `en_tn` v86's files,
/// whose export names `en_tn` (an export's owner is never compared).
const OTHER_OWNER: &str = "Door43-Catalog";

async fn add_unpinned_notes(fake: &FakeDoor43) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4/test-fixtures/resources/en_tn@v86");
    let file = |name: &str| std::fs::read(dir.join(name)).unwrap();
    fake.add_repo(RepoSeed {
        owner: OTHER_OWNER.into(),
        name: "en_tn".into(),
        subject: "TSV Translation Notes".into(),
        language: "en".into(),
        topics: vec!["tc-ready".into()],
        private: false,
        commits: vec![CommitSeed {
            message: "Release v86".into(),
            files: vec![
                SeedFile::regular("metadata.json", file("metadata.json")),
                SeedFile::regular("ingredients/TIT.tsv", file("TIT.tsv")),
                SeedFile::regular("ingredients/JON.tsv", file("JON.tsv")),
            ],
            tag: Some("v86".into()),
            release: Some(ReleaseKind::Published),
        }],
    })
    .await
    .unwrap();
}

fn download(at: Scope) -> Download {
    Download {
        request: rid(60),
        at,
        gateway: gateway("ml", OTHER_OWNER),
        book: BookId::Titus,
        repos: vec!["en_tn".into()],
    }
}

fn installs_of(app_data: &Path, owner: &str) -> usize {
    let store = ResourceStore::open(app_data.join(tc_app::RESOURCES_FOLDER)).unwrap();
    store
        .installed_dcs_pins()
        .unwrap()
        .iter()
        .filter(|p| p.repo.repo().owner.as_str() == owner)
        .count()
}

fn partial_files(app_data: &Path) -> usize {
    std::fs::read_dir(app_data.join(tc_app::RESOURCES_FOLDER).join(".partial"))
        .map(|d| d.count())
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_installs_the_latest_release_and_counts_the_package() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    add_unpinned_notes(&fake).await;
    let (_dir, t) = start(Some(&fake)).await;
    let at = open_sample(&t.app).await;
    internet(&t.app, true, 1).await;
    let job = ok(&t.app, &download(at.clone())).await;
    assert_eq!(
        ended(&t.app, job).await,
        JobState::Done(JobOutcome::Finished)
    );
    assert_eq!(installs_of(&t.app_data, OTHER_OWNER), 1);
    let view = ok(&t.app, &Sources { at: at.clone() }).await;
    let malayalam = view
        .gateways
        .iter()
        .find(|g| g.package.gateway == gateway("ml", OTHER_OWNER))
        .unwrap();
    assert_eq!(malayalam.installed, 1, "tC4's `n installed`");
    // A second download finds it here and fetches nothing.
    let before = fake.requests().len();
    let again = ok(
        &t.app,
        &Download {
            request: rid(61),
            ..download(at)
        },
    )
    .await;
    assert_eq!(
        ended(&t.app, again).await,
        JobState::Done(JobOutcome::Finished)
    );
    assert_eq!(fake.requests().len(), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_download_leaves_no_install_and_no_partial_file() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    add_unpinned_notes(&fake).await;
    fake.inject(FaultRule::once(
        "GET",
        &format!("/{OTHER_OWNER}/en_tn/sb/"),
        Fault::HoldBody { after: HELD_AT },
    ));
    let (_dir, t) = start(Some(&fake)).await;
    let at = open_sample(&t.app).await;
    internet(&t.app, true, 1).await;
    let job = ok(&t.app, &download(at)).await;
    // The job reports the bytes that arrived before the fake held the rest.
    let started = std::time::Instant::now();
    loop {
        if let Some(JobState::Running(progress)) = t.app.jobs().view(job).map(|v| v.state)
            && progress.done >= HELD_AT as u64
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "no bytes arrived"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ok(
        &t.app,
        &Cancel {
            request: rid(62),
            job,
        },
    )
    .await;
    assert_eq!(ended(&t.app, job).await, JobState::Cancelled);
    assert_eq!(installs_of(&t.app_data, OTHER_OWNER), 0);
    assert_eq!(partial_files(&t.app_data), 0);
}
