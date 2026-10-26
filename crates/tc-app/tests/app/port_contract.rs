//! What every sync port must do (UI-2, INV-ONE-PORT-PER-JOB), driven through the app's public
//! API: post `door43/sync` or `door43/open`, follow the job, read the project list and the
//! project's card. The scripted port runs it here; E1a adds the real port's invocation.

use tc_api::ApiError;
use tc_api::door43::{OpenDoor43, ProjectName, RepoSource, SyncProject};
use tc_api::endpoint::NetKind;
use tc_api::error::Flow;
use tc_api::exchange::{ExchangeFile, Export, HandEditOutcome, HandEdited, Receive, ReceiveNew};
use tc_api::home::{HomeView, List};
use tc_api::ids::{JobId, ProjectRef};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::refusal::{ExchangeRefusal, OpenDoor43Refusal, Refusal, SyncFailure, SyncRefusal};
use tc_api::test::ScriptedSync;
use tc_app::App;
use tc_app::ports::scripted::ScriptedReceive;
use tc_door43_fake::seeded::PARITY_USER;

use super::common::{folder, post, rid, until};
use super::sync::{World, card_with_counts, internet_on, script, shared, sign_in};

const SAMPLE: &str = "sample_burrito";

/// A port under the contract: how a world is made, and how each case's outcome is arranged.
pub(super) trait SyncPortFixture {
    /// The shared project's world, the internet off and nobody signed in.
    async fn world() -> World;
    /// Whether a successful open writes the project it answers. The scripted port answers a
    /// project the scenario already holds; a real port writes a new one, which the contract
    /// then checks was not listed before.
    const OPEN_WRITES: bool;
    /// The next sync of the sample fails before anything is taken in; the job's error.
    async fn sync_fails_before_intake(w: &World) -> ApiError;
    /// The next open from Door43 succeeds; the project it adds.
    async fn open_succeeds(w: &World) -> ProjectRef;
    /// The next open from Door43 is refused; the job's error.
    async fn open_refused(w: &World) -> ApiError;
    /// How many syncs and opens reached the port.
    fn calls(w: &World) -> usize;
}

pub(super) struct Scripted;

impl SyncPortFixture for Scripted {
    const OPEN_WRITES: bool = false;

    async fn world() -> World {
        shared().await
    }

    async fn sync_fails_before_intake(w: &World) -> ApiError {
        let failure = SyncFailure::BeforeIntake(SyncRefusal::Unreachable);
        script(&w.t.app, 90, ScriptedSync::Failed(failure.clone())).await;
        ApiError::Refused(Refusal::Sync(failure))
    }

    async fn open_succeeds(w: &World) -> ProjectRef {
        let project = folder(SAMPLE);
        w.t.app.script_open_door43(Ok(project.clone()));
        project
    }

    async fn open_refused(w: &World) -> ApiError {
        let refused = ApiError::Refused(Refusal::OpenDoor43(OpenDoor43Refusal::NoJournal));
        w.t.app.script_open_door43(Err(refused.clone()));
        refused
    }

    fn calls(w: &World) -> usize {
        w.t.app
            .scripted_sync_port()
            .expect("the scripted port")
            .calls()
    }
}

pub(super) fn sync(n: u8) -> SyncProject {
    SyncProject {
        request: rid(n),
        project: folder(SAMPLE),
    }
}

pub(super) fn open(n: u8, name: &str) -> OpenDoor43 {
    OpenDoor43 {
        request: rid(n),
        repo: RepoSource::new(PARITY_USER, "es-419_tit_equipo").unwrap(),
        name: ProjectName::parse(name).unwrap(),
    }
}

pub(super) async fn ended(app: &App, job: JobId) -> JobState {
    let mut state = None;
    until("the job ends", || {
        state = app.jobs().view(job).map(|v| v.state);
        matches!(
            state,
            Some(JobState::Done(_) | JobState::Failed(_) | JobState::Cancelled)
        )
    })
    .await;
    state.unwrap()
}

pub(super) async fn listed(app: &App) -> Vec<ProjectRef> {
    let view: HomeView = post(app, &List {}).await.unwrap();
    view.cards.into_iter().map(|c| c.project).collect()
}

macro_rules! sync_port_contract {
    ($name:ident, $fixture:ty) => {
        mod $name {
            use super::*;

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_sync_that_fails_before_intake_changes_nothing() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                internet_on(app).await;
                sign_in(app).await;
                let before = card_with_counts(app).await;
                let error = <$fixture>::sync_fails_before_intake(&w).await;
                let job = post(app, &sync(1)).await.unwrap();
                assert_eq!(ended(app, job).await, JobState::Failed(error));
                assert_eq!(
                    card_with_counts(app).await,
                    before,
                    "the project is as it was"
                );
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn an_open_from_door43_answers_a_project_the_list_holds() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                internet_on(app).await;
                let before = listed(app).await;
                let project = <$fixture>::open_succeeds(&w).await;
                if <$fixture>::OPEN_WRITES {
                    assert!(!before.contains(&project), "{project:?} is new");
                }
                let job = post(app, &open(2, "contract-open")).await.unwrap();
                match ended(app, job).await {
                    JobState::Done(tc_api::jobs::JobOutcome::Created { project: added }) => {
                        assert_eq!(added, project);
                        assert!(listed(app).await.contains(&added), "{added:?} is listed");
                    }
                    other => panic!("{other:?}"),
                }
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_refused_open_from_door43_adds_no_project() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                internet_on(app).await;
                let before = listed(app).await;
                let error = <$fixture>::open_refused(&w).await;
                let job = post(app, &open(4, "contract-refused")).await.unwrap();
                assert_eq!(ended(app, job).await, JobState::Failed(error));
                assert_eq!(listed(app).await, before, "no project was added");
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn with_the_internet_off_the_port_is_never_called() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                <$fixture>::sync_fails_before_intake(&w).await;
                <$fixture>::open_refused(&w).await;
                assert_eq!(
                    post(app, &sync(5)).await,
                    Err(ApiError::NeedsInternet(NetKind::Sync))
                );
                assert_eq!(
                    post(app, &open(6, "contract-offline")).await,
                    Err(ApiError::NeedsInternet(NetKind::Open))
                );
                assert_eq!(<$fixture>::calls(&w), 0, "nothing reached the port");
            }
        }
    };
}

sync_port_contract!(scripted, Scripted);
// E1a: sync_port_contract!(door43, Door43Port);

/// An exchange port under the contract (E2): how each case's outcome is arranged. A real
/// port writes and reads files on this computer; the scripted one answers what it was told.
pub(super) trait ExchangePortFixture {
    type World;
    async fn world() -> Self::World;
    fn app(w: &Self::World) -> &App;
    /// Whether the port writes a real exchange file the contract can look at.
    const WRITES_FILES: bool;
    /// The changes the hand-edit case's file carries, and the verse conflicts they make.
    const RECEIVED: u32;
    const VERSE_CONFLICTS: u32;
    /// The next export of the sample fails and leaves no file; the job's error when the
    /// fixture knows it exactly.
    async fn export_fails(w: &Self::World) -> Option<ApiError>;
    /// The next export of the sample ends verified; the folder it writes into.
    async fn export_succeeds(w: &Self::World) -> String;
    /// A receive of this file into the sample fails before anything is taken in; the job's
    /// error when the fixture knows it exactly.
    async fn receive_fails(w: &Self::World) -> (ExchangeFile, Option<ApiError>);
    /// A receive of this file brings `RECEIVED` changes and a book file someone edited by
    /// hand inside the zip; the path of that file inside the zip.
    async fn receive_with_a_hand_edit(w: &Self::World) -> (ExchangeFile, String);
    /// The next receive into the sample waits until `release_receive`.
    async fn hold_receive(w: &Self::World);
    async fn release_receive(w: &Self::World);
    /// A receive of this file as the new project `name` succeeds; the project it adds.
    async fn receive_new_succeeds(w: &Self::World, name: &str) -> (ExchangeFile, ProjectRef);
}

pub(super) struct ScriptedExchange;

impl ExchangePortFixture for ScriptedExchange {
    type World = World;
    const WRITES_FILES: bool = false;
    const RECEIVED: u32 = 2;
    const VERSE_CONFLICTS: u32 = 1;

    async fn world() -> World {
        shared().await
    }

    fn app(w: &World) -> &App {
        &w.t.app
    }

    async fn export_fails(w: &World) -> Option<ApiError> {
        exchange_port(w).script_export(folder(SAMPLE), Err(ExchangeRefusal::VerifyFailed));
        Some(ApiError::Refused(Refusal::Exchange(
            ExchangeRefusal::VerifyFailed,
        )))
    }

    async fn export_succeeds(w: &World) -> String {
        exchange_port(w).script_export(
            folder(SAMPLE),
            Ok("/media/stick/tit-2026-10-25-ana.zip".to_owned()),
        );
        "/media/stick".to_owned()
    }

    async fn receive_fails(w: &World) -> (ExchangeFile, Option<ApiError>) {
        exchange_port(w).script_receive(
            folder(SAMPLE),
            ScriptedReceive::Failed(ExchangeRefusal::Damaged),
        );
        (
            ExchangeFile::Path("/media/stick/damaged.zip".into()),
            Some(ApiError::Refused(Refusal::Exchange(
                ExchangeRefusal::Damaged,
            ))),
        )
    }

    async fn receive_with_a_hand_edit(w: &World) -> (ExchangeFile, String) {
        let path = "ingredients/TIT.usfm".to_owned();
        exchange_port(w).script_receive(
            folder(SAMPLE),
            ScriptedReceive::Received {
                changes: super::sync::TWO_TEAMMATES(),
                hand_edited: vec![HandEdited {
                    path: path.clone(),
                    outcome: HandEditOutcome::NotTakenIn,
                }],
            },
        );
        (
            ExchangeFile::Path("/media/stick/tit-2026-10-25-maria.zip".into()),
            path,
        )
    }

    async fn hold_receive(w: &World) {
        exchange_port(w).hold_receive(folder(SAMPLE));
    }

    async fn release_receive(w: &World) {
        exchange_port(w).release_receive(&folder(SAMPLE));
    }

    async fn receive_new_succeeds(w: &World, _name: &str) -> (ExchangeFile, ProjectRef) {
        // The scripted port writes nothing: it answers a project the scenario holds.
        let project = folder(SAMPLE);
        exchange_port(w).script_receive_new(Ok(project.clone()));
        (
            ExchangeFile::Path("/media/stick/tit-2026-10-25-maria.zip".into()),
            project,
        )
    }
}

fn exchange_port(w: &World) -> &tc_app::ports::scripted::ScriptedExchangePort {
    w.t.app
        .scripted_exchange_port()
        .expect("the scripted exchange port")
}

pub(super) fn export(n: u8, destination: &str) -> Export {
    Export {
        request: rid(n),
        project: folder(SAMPLE),
        destination: destination.to_owned(),
    }
}

pub(super) fn receive_new(n: u8, file: ExchangeFile, name: &str) -> ReceiveNew {
    ReceiveNew {
        request: rid(n),
        file,
        name: ProjectName::parse(name).unwrap(),
    }
}

pub(super) fn receive(n: u8, file: ExchangeFile) -> Receive {
    Receive {
        request: rid(n),
        project: folder(SAMPLE),
        file,
    }
}

macro_rules! exchange_port_contract {
    ($name:ident, $fixture:ty) => {
        mod $name {
            use super::*;

            /// INV-REMOVE-THE-STICK-ONLY-AFTER-VERIFY: the job ends verified, naming the file,
            /// or fails with no file left; the project is as it was either way.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn an_export_ends_verified_or_deleted() {
                let w = <$fixture>::world().await;
                let app = <$fixture>::app(&w);
                let before = card_with_counts(app).await;
                let error = <$fixture>::export_fails(&w).await;
                let job = post(app, &export(1, "/nowhere/stick")).await.unwrap();
                match ended(app, job).await {
                    JobState::Failed(got) => {
                        assert!(
                            matches!(got, ApiError::Refused(Refusal::Exchange(_))),
                            "{got:?}"
                        );
                        if let Some(error) = error {
                            assert_eq!(got, error);
                        }
                    }
                    other => panic!("{other:?}"),
                }
                assert_eq!(
                    card_with_counts(app).await,
                    before,
                    "the project is as it was"
                );
                let destination = <$fixture>::export_succeeds(&w).await;
                let job = post(app, &export(2, &destination)).await.unwrap();
                match ended(app, job).await {
                    JobState::Done(JobOutcome::Exported(view)) => {
                        assert!(view.path.starts_with(&destination), "{view:?}");
                        if <$fixture>::WRITES_FILES {
                            assert!(std::path::Path::new(&view.path).is_file(), "{view:?}");
                        }
                    }
                    other => panic!("{other:?}"),
                }
            }

            /// INV-RECEIVE-IS-WHOLE-OR-ABSENT: a receive that fails leaves the project as it
            /// was; one that completes shows what it brought.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_receive_is_whole_or_absent() {
                let w = <$fixture>::world().await;
                let app = <$fixture>::app(&w);
                let before = card_with_counts(app).await;
                let (file, error) = <$fixture>::receive_fails(&w).await;
                let job = post(app, &receive(3, file)).await.unwrap();
                match ended(app, job).await {
                    JobState::Failed(got) => {
                        assert!(
                            matches!(got, ApiError::Refused(Refusal::Exchange(_))),
                            "{got:?}"
                        );
                        if let Some(error) = error {
                            assert_eq!(got, error);
                        }
                    }
                    other => panic!("{other:?}"),
                }
                assert_eq!(card_with_counts(app).await, before, "nothing was taken in");
                let (file, _) = <$fixture>::receive_with_a_hand_edit(&w).await;
                let job = post(app, &receive(4, file)).await.unwrap();
                match ended(app, job).await {
                    JobState::Done(JobOutcome::Received(view)) => {
                        assert_eq!(view.received, <$fixture>::RECEIVED, "{view:?}");
                        assert_eq!(
                            view.verse_conflicts,
                            <$fixture>::VERSE_CONFLICTS,
                            "{view:?}"
                        );
                    }
                    other => panic!("{other:?}"),
                }
            }

            /// INV-NEVER-BESIDE-A-SYNC: while a receive of a project runs, its sync and a
            /// second exchange of it are refused as busy.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_receive_waits_for_a_sync_of_its_project() {
                let w = <$fixture>::world().await;
                let app = <$fixture>::app(&w);
                internet_on(app).await;
                sign_in(app).await;
                <$fixture>::hold_receive(&w).await;
                let (file, _) = <$fixture>::receive_with_a_hand_edit(&w).await;
                let job = post(app, &receive(5, file)).await.unwrap();
                assert_eq!(
                    post(app, &sync(6)).await,
                    Err(ApiError::Busy(Flow::ExchangeReceive)),
                    "INV-NEVER-BESIDE-A-SYNC"
                );
                assert_eq!(
                    post(app, &export(7, "/nowhere/stick")).await,
                    Err(ApiError::Busy(Flow::ExchangeReceive))
                );
                <$fixture>::release_receive(&w).await;
                assert!(matches!(ended(app, job).await, JobState::Done(_)));
                // The flow is free again once the job ended.
                let destination = <$fixture>::export_succeeds(&w).await;
                let job = post(app, &export(8, &destination)).await.unwrap();
                assert!(matches!(ended(app, job).await, JobState::Done(_)));
            }

            /// E2 step 4b: a file for a project this computer lacks becomes that project, and a
            /// name in use is refused before anything is written.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_newcomers_receive_creates_the_project() {
                let w = <$fixture>::world().await;
                let app = <$fixture>::app(&w);
                let name = "tito_de_juan";
                let (file, project) = <$fixture>::receive_new_succeeds(&w, name).await;
                let taken = post(app, &receive_new(10, file.clone(), SAMPLE)).await;
                assert_eq!(
                    taken,
                    Err(ApiError::Refused(Refusal::Exchange(
                        ExchangeRefusal::NameExists
                    ))),
                    "the sample's name is in use"
                );
                let job = post(app, &receive_new(11, file, name)).await.unwrap();
                match ended(app, job).await {
                    JobState::Done(JobOutcome::Created { project: added }) => {
                        assert_eq!(added, project);
                        assert!(listed(app).await.contains(&added), "{added:?} is listed");
                    }
                    other => panic!("{other:?}"),
                }
            }

            /// INV-RECEIVE-TAKES-ONLY-PACKS: a file edited by hand inside the zip is listed as
            /// not taken in, and its text reaches no verse.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_receive_takes_in_only_the_files_packs() {
                let w = <$fixture>::world().await;
                let app = <$fixture>::app(&w);
                let (file, path) = <$fixture>::receive_with_a_hand_edit(&w).await;
                let job = post(app, &receive(9, file)).await.unwrap();
                match ended(app, job).await {
                    JobState::Done(JobOutcome::Received(view)) => {
                        assert_eq!(
                            view.hand_edited,
                            vec![HandEdited {
                                path,
                                outcome: HandEditOutcome::NotTakenIn
                            }]
                        );
                        assert_eq!(
                            view.received,
                            <$fixture>::RECEIVED,
                            "only the packs' changes: {view:?}"
                        );
                    }
                    other => panic!("{other:?}"),
                }
            }
        }
    };
}

exchange_port_contract!(scripted_exchange, ScriptedExchange);
exchange_port_contract!(usb, crate::app::exchange::Usb);

/// A script sent to an app whose sync port is not the scripted one is refused, so a test can
/// never pass on a script that went nowhere.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scripting_a_port_that_is_not_scripted_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let t = super::common::TestApp::start_in(dir.path(), |_, deps| {
        deps.sync = std::sync::Arc::new(tc_app::ports::UnavailableSync);
    })
    .await;
    let answer = post(
        &t.app,
        &tc_api::test::Script {
            request: rid(7),
            next: tc_api::test::Scripted::Sync {
                project: folder(SAMPLE),
                outcome: ScriptedSync::Failed(SyncFailure::BeforeIntake(SyncRefusal::Unreachable)),
            },
        },
    )
    .await;
    assert!(
        matches!(answer, Err(ApiError::BadRequest { .. })),
        "{answer:?}"
    );
}

/// What every import port must do (F1), driven through the app's public API on a store world:
/// post `import/review`, `import/run` and `import/updateBook`, follow the jobs, read the project
/// list. The checks before a port is called are the app's own, so both ports meet them.
mod import_ports {
    use std::sync::Arc;

    use tc_api::ApiError;
    use tc_api::import::{
        FoundBook, ImportDetails, ImportFile, ImportFound, Review, UpdateReport, VersionSlot,
    };
    use tc_api::jobs::{JobOutcome, JobState};
    use tc_api::refusal::{ImportRefusal, Refusal};
    use tc_app::StoreChoice;
    use tc_app::ports::scripted::ScriptedImportPort;
    use tc_core::BookId;
    use tc_store::hooks::Point;

    use super::super::common::{Parts, TestApp, post};
    use super::super::import::{aligned_titus, open_sample, run, tc3_titus, update, upload};
    use super::{ended, listed};
    use crate::store_world::{StoreWorld, store_world};

    pub(super) struct ImportWorld {
        pub dir: tempfile::TempDir,
        pub t: TestApp,
        pub world: StoreWorld,
        pub scripted: Option<Arc<ScriptedImportPort>>,
        pub zip: ImportFile,
    }

    async fn world_with(scripted: Option<Arc<ScriptedImportPort>>) -> ImportWorld {
        let world = store_world();
        let store = world.store();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("app-data/uploads")).unwrap();
        super::super::import::install_notes(dir.path());
        let port: Arc<dyn tc_app::ports::ImportPort> = match &scripted {
            Some(port) => port.clone(),
            None => Arc::new(tc_app::import::TcImport),
        };
        let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
            deps.store = StoreChoice::Given(store);
            deps.import = port;
        })
        .await;
        let zip = upload(dir.path(), 9, "cfm_fbt_tit_book.zip", &tc3_titus());
        ImportWorld {
            dir,
            t,
            world,
            scripted,
            zip,
        }
    }

    fn found() -> ImportFound {
        ImportFound {
            details: ImportDetails {
                name: "cfm_fbt".into(),
                code: "cfm".into(),
            },
            licenses: vec!["CC BY-SA 4.0".into()],
            contributors: Vec::new(),
            books: vec![FoundBook {
                book: BookId::Titus,
                chapters: 3,
                verses: 46,
                drafted: 46,
                aligned: 0,
                decisions: 5,
                not_kept: Vec::new(),
            }],
            slots: vec![
                VersionSlot::TranslationNotes,
                VersionSlot::OriginalNewTestament,
            ],
        }
    }

    fn details(name: &str) -> ImportDetails {
        ImportDetails {
            name: name.into(),
            code: "cfm".into(),
        }
    }

    /// A port under the contract: how a world is made, and how each case's outcome is arranged.
    pub(super) trait ImportPortFixture {
        /// Whether a successful run writes the project it answers. The scripted port answers a
        /// project the world holds already.
        const RUN_WRITES: bool;
        async fn world() -> ImportWorld;
        /// The next review reads the zip.
        fn review_reads(w: &ImportWorld);
        /// The next run makes a project named `name`; the project it answers.
        fn run_succeeds(w: &ImportWorld, name: &str) -> tc_api::ids::ProjectRef;
        /// The next run fails while it writes; the job's error. [`Self::heal`] ends it.
        fn run_fails(w: &ImportWorld) -> ApiError;
        fn heal(w: &ImportWorld);
        /// The next run stops before its end until [`Self::release`]; `held` waits for it.
        fn hold_run(w: &ImportWorld);
        async fn held(w: &ImportWorld);
        fn release(w: &ImportWorld);
        /// The next Update a book of Titus succeeds.
        fn update_succeeds(w: &ImportWorld);
    }

    pub(super) struct Scripted;

    impl Scripted {
        fn port(w: &ImportWorld) -> &ScriptedImportPort {
            w.scripted.as_deref().expect("the scripted port")
        }
    }

    impl ImportPortFixture for Scripted {
        const RUN_WRITES: bool = false;

        async fn world() -> ImportWorld {
            world_with(Some(Arc::new(ScriptedImportPort::default()))).await
        }

        fn review_reads(w: &ImportWorld) {
            Self::port(w).script_review(Ok(found()));
        }

        fn run_succeeds(w: &ImportWorld, _name: &str) -> tc_api::ids::ProjectRef {
            let project = w.world.bible.clone();
            Self::port(w).script_run(Ok(project.clone()));
            project
        }

        fn run_fails(w: &ImportWorld) -> ApiError {
            let error = ApiError::Refused(Refusal::Import(ImportRefusal::WriteFailed));
            Self::port(w).script_run(Err(error.clone()));
            error
        }

        fn heal(_w: &ImportWorld) {}

        fn hold_run(w: &ImportWorld) {
            Self::port(w).script_run_until_cancelled();
        }

        async fn held(_w: &ImportWorld) {}

        fn release(_w: &ImportWorld) {}

        fn update_succeeds(w: &ImportWorld) {
            Self::port(w).script_update(Ok(UpdateReport {
                book: BookId::Titus,
                imported: 41,
                to_review: 5,
                left_as_it_was: Vec::new(),
                markup_kept: Vec::new(),
                book_added: false,
            }));
        }
    }

    pub(super) struct Real;

    impl ImportPortFixture for Real {
        const RUN_WRITES: bool = true;

        async fn world() -> ImportWorld {
            world_with(None).await
        }

        fn review_reads(_w: &ImportWorld) {}

        fn run_succeeds(_w: &ImportWorld, name: &str) -> tc_api::ids::ProjectRef {
            tc_api::ids::ProjectRef::parse(name).unwrap()
        }

        fn run_fails(w: &ImportWorld) -> ApiError {
            w.world
                .fs
                .fail_writes(Some(std::io::ErrorKind::StorageFull));
            ApiError::Refused(Refusal::Import(ImportRefusal::WriteFailed))
        }

        fn heal(w: &ImportWorld) {
            w.world.fs.fail_writes(None);
        }

        fn hold_run(w: &ImportWorld) {
            w.world.hooks.hold(Point::BeforeStagingRename);
        }

        async fn held(w: &ImportWorld) {
            let hooks = w.world.hooks.clone();
            tokio::task::spawn_blocking(move || hooks.wait_reached(Point::BeforeStagingRename, 1))
                .await
                .unwrap();
        }

        fn release(w: &ImportWorld) {
            w.world.hooks.release(Point::BeforeStagingRename);
        }

        fn update_succeeds(_w: &ImportWorld) {}
    }

    macro_rules! import_port_contract {
        ($name:ident, $fixture:ty) => {
            mod $name {
                use super::*;

                #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
                async fn an_import_makes_one_project_or_none() {
                    let w = <$fixture>::world().await;
                    let app = &w.t.app;
                    let before = listed(app).await;
                    let error = <$fixture>::run_fails(&w);
                    let job = post(app, &run(1, &w.zip, details("contract_one")))
                        .await
                        .unwrap();
                    assert_eq!(ended(app, job).await, JobState::Failed(error));
                    <$fixture>::heal(&w);
                    assert_eq!(listed(app).await, before, "a failed run adds no project");

                    let project = <$fixture>::run_succeeds(&w, "contract_one");
                    if <$fixture>::RUN_WRITES {
                        assert!(!before.contains(&project), "{project:?} is new");
                    }
                    let job = post(app, &run(2, &w.zip, details("contract_one")))
                        .await
                        .unwrap();
                    assert_eq!(
                        ended(app, job).await,
                        JobState::Done(JobOutcome::Created {
                            project: project.clone()
                        })
                    );
                    let after = listed(app).await;
                    assert!(after.contains(&project), "{project:?} is listed");
                    let added = after.iter().filter(|p| !before.contains(p)).count();
                    assert_eq!(
                        added,
                        usize::from(<$fixture>::RUN_WRITES),
                        "one project, or none"
                    );
                }

                #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
                async fn a_cancel_leaves_nothing() {
                    let w = <$fixture>::world().await;
                    let app = &w.t.app;
                    let before = listed(app).await;
                    <$fixture>::hold_run(&w);
                    let job = post(app, &run(3, &w.zip, details("contract_cancel")))
                        .await
                        .unwrap();
                    <$fixture>::held(&w).await;
                    assert_eq!(listed(app).await, before, "nothing is listed while it runs");
                    app.jobs().cancel(job);
                    <$fixture>::release(&w);
                    match ended(app, job).await {
                        JobState::Cancelled => {
                            assert_eq!(listed(app).await, before, "a cancel leaves nothing")
                        }
                        // The real create was past its last check: it placed the whole project.
                        JobState::Done(JobOutcome::Created { project }) => {
                            assert!(<$fixture>::RUN_WRITES);
                            assert!(listed(app).await.contains(&project));
                        }
                        other => panic!("{other:?}"),
                    }
                }

                #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
                async fn a_name_in_use_is_refused_on_review() {
                    let w = <$fixture>::world().await;
                    let app = &w.t.app;
                    <$fixture>::review_reads(&w);
                    let in_use = details(w.world.bible.as_str());
                    let review = post(
                        app,
                        &Review {
                            files: vec![w.zip.clone()],
                            details: Some(in_use.clone()),
                            use_installed: None,
                        },
                    )
                    .await
                    .unwrap();
                    assert_eq!(review.found.books[0].book, BookId::Titus);
                    assert_eq!(
                        review.refusal,
                        Some(Refusal::Import(ImportRefusal::NameExists))
                    );
                    let before = listed(app).await;
                    assert_eq!(
                        post(app, &run(4, &w.zip, in_use)).await,
                        Err(ApiError::Refused(Refusal::Import(
                            ImportRefusal::NameExists
                        )))
                    );
                    assert_eq!(listed(app).await, before);
                }

                #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
                async fn update_a_book_reports_and_refuses_another_book() {
                    let w = <$fixture>::world().await;
                    let app = &w.t.app;
                    let at = open_sample(&w.t).await;
                    let other = aligned_titus().replacen("\\id TIT", "\\id PHM", 1);
                    let file = upload(w.dir.path(), 10, "PHM.usfm", other.as_bytes());
                    assert!(matches!(
                        post(app, &update(5, &at, BookId::Titus, &file)).await,
                        Err(ApiError::Refused(Refusal::Import(
                            ImportRefusal::UpdateOtherBook { .. }
                        )))
                    ));
                    <$fixture>::update_succeeds(&w);
                    let file = upload(w.dir.path(), 11, "TIT.usfm", aligned_titus().as_bytes());
                    let job = post(app, &update(6, &at, BookId::Titus, &file))
                        .await
                        .unwrap();
                    match ended(app, job).await {
                        JobState::Done(JobOutcome::BookUpdated(report)) => {
                            assert_eq!(report.book, BookId::Titus);
                            assert!(report.imported > 0 && report.to_review > 0, "{report:?}");
                        }
                        other => panic!("{other:?}"),
                    }
                }
            }
        };
    }

    import_port_contract!(scripted, Scripted);
    import_port_contract!(tc_import, Real);
}
