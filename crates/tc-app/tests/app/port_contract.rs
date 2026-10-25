//! What every sync port must do (UI-2, INV-ONE-PORT-PER-JOB), driven through the app's public
//! API: post `door43/sync` or `door43/open`, follow the job, read the project list and the
//! project's card. The scripted port runs it here; E1a adds the real port's invocation.

use tc_api::ApiError;
use tc_api::door43::{OpenDoor43, ProjectName, RepoSource, SyncProject};
use tc_api::endpoint::NetKind;
use tc_api::error::Flow;
use tc_api::exchange::{ExchangeFile, Export, HandEditOutcome, HandEdited, Receive};
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
