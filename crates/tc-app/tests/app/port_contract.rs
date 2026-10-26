//! What every sync port must do (UI-2, INV-ONE-PORT-PER-JOB), driven through the app's public
//! API: post `door43/sync` or `door43/open`, follow the job, read the project list and the
//! project's card. The scripted port runs it here; E1a adds the real port's invocation.

use tc_api::ApiError;
use tc_api::account::WebUrl;
use tc_api::door43::{
    OpenDoor43, ProjectName, RepoName, RepoSource, ShareOnDoor43, ShareOwner, SharedRepo, SyncProject,
};
use tc_api::editor::SaveVerse;
use tc_api::endpoint::NetKind;
use tc_api::error::Flow;
use tc_api::exchange::{ExchangeFile, Export, HandEditOutcome, HandEdited, Receive, ReceiveNew};
use tc_api::home::{HomeView, List};
use tc_api::ids::RequestId;
use tc_api::ids::{JobId, ProjectRef};
use tc_api::jobs::{JobOutcome, JobState};
use tc_api::refusal::{ExchangeRefusal, OpenDoor43Refusal, Refusal, SyncFailure, SyncRefusal};
use tc_api::session::Close;
use tc_api::test::ScriptedSync;
use tc_api::write::Stored;
use tc_app::App;
use tc_app::ports::scripted::ScriptedReceive;
use tc_app::wire::token::shown;
use tc_door43_fake::seeded::PARITY_USER;
use tc_journal::Record;
use tc_journal::record::VerseValue;

use super::common::{folder, open_progress, post, rid, tit, until};
use super::sync::{
    World, card_with_counts, internet_on, script, shared, shared_with, sign_in, synced, world_of,
};
use std::sync::Arc;
use tc_door43_fake::{Fault, FaultRule, RepoSeed};

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
    /// The computer that opens from Door43: this one for the scripted port; for a real port a
    /// second one, since a computer that has the project opens no second copy (ruling H14).
    async fn opener(w: &World) -> Opener;
    async fn open_succeeds(w: &World) -> ProjectRef;
    /// The next open from Door43 is refused; the job's error.
    async fn open_refused(w: &World) -> ApiError;
    /// How many syncs and opens reached the port.
    fn calls(w: &World) -> usize;
    /// The next sync of `project` succeeds; `n` makes the arrangement's request id.
    async fn sync_succeeds(w: &World, project: &ProjectRef, n: u8);
    /// The next sync waits, once it reached the port, until [`Self::release_sync`].
    async fn hold_next_sync(w: &World);
    fn release_sync(w: &World);
    /// Whether Door43's copy of the sample holds `words` at Titus 1:1; `None` for a port with
    /// no Door43 behind it.
    async fn door43_holds(w: &World, words: &str) -> Option<bool>;
    /// A world whose sample is not on Door43 yet, for a first share.
    async fn share_world() -> World;
    /// The next share of `project` as `name` succeeds; the repository it ends with.
    async fn share_succeeds(w: &World, project: &ProjectRef, name: &str) -> SharedRepo;
    /// Whether Door43's `name`, opened on this computer as a new project, holds `words` at
    /// Titus 1:1; `None` for a port with no Door43 behind it.
    async fn shared_holds(w: &World, name: &str, words: &str) -> Option<bool>;
}

/// A project of the `shared` scenario that tells stories (#761).
const STORIES: &str = "sample_burrito_obs";

async fn script_synced(w: &World, n: u8, project: &ProjectRef) {
    post(
        &w.t.app,
        &tc_api::test::Script {
            request: rid(n),
            next: tc_api::test::Scripted::Sync {
                project: project.clone(),
                outcome: ScriptedSync::Synced {
                    teammates: None,
                    joined: None,
                    restored: 0,
                    kinds: Vec::new(),
                },
            },
        },
    )
    .await
    .unwrap();
}

/// Titus 1:1 of `project`, opened for the screens, saved as `words`; then closed.
async fn save_titus_1_1(app: &App, project: &ProjectRef, n: u8, words: &str) {
    save_titus(app, project, 1, n, words).await;
}

/// Titus 1:`verse` of `project`, opened for the screens, saved as `words`; then closed.
async fn save_titus(app: &App, project: &ProjectRef, verse: u16, n: u8, words: &str) {
    let at = save_titus_open(app, project, verse, n, words).await;
    post(
        app,
        &Close {
            at,
            request: rid(n.wrapping_add(1)),
        },
    )
    .await
    .unwrap();
}

/// Titus 1:`verse` of `project` saved as `words`, the project left open for the screens: the
/// save is in its journal and not yet in a commit.
async fn save_titus_open(
    app: &App,
    project: &ProjectRef,
    verse: u16,
    n: u8,
    words: &str,
) -> tc_api::ids::Scope {
    let at = app
        .sessions()
        .open_screen(project, open_progress())
        .await
        .unwrap()
        .at;
    let minted = app
        .sessions()
        .reader(&at)
        .unwrap()
        .read()
        .mint(&tit(1, verse), None)
        .unwrap();
    let ack = post(
        app,
        &SaveVerse {
            at: at.clone(),
            request: rid(n),
            shown: shown(&minted).unwrap(),
            words: words.to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(ack.stored, Stored::Written);
    at
}

/// Titus 1:1's words in `project`, as the project holds them now.
async fn titus_1_1(app: &App, project: &ProjectRef) -> Option<String> {
    let at = app
        .sessions()
        .open_screen(project, open_progress())
        .await
        .unwrap()
        .at;
    let record = app
        .sessions()
        .reader(&at)
        .unwrap()
        .read()
        .projected(&tit(1, 1));
    post(
        app,
        &Close {
            at,
            request: RequestId::from_bytes([0xC7; 16]),
        },
    )
    .await
    .unwrap();
    match record {
        Some(Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        }) => Some(words),
        _ => None,
    }
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

    async fn opener(w: &World) -> Opener {
        Opener {
            app: w.t.app.clone(),
            _computer: None,
        }
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

    async fn sync_succeeds(w: &World, project: &ProjectRef, n: u8) {
        script_synced(w, n, project).await;
    }

    async fn hold_next_sync(w: &World) {
        w.t.app
            .scripted_sync_port()
            .expect("the scripted port")
            .hold_next();
    }

    fn release_sync(w: &World) {
        w.t.app
            .scripted_sync_port()
            .expect("the scripted port")
            .release();
    }

    async fn door43_holds(_w: &World, _words: &str) -> Option<bool> {
        None
    }

    async fn share_world() -> World {
        world_of("sample", None).await
    }

    async fn share_succeeds(w: &World, _project: &ProjectRef, name: &str) -> SharedRepo {
        let repo = SharedRepo {
            repo: RepoSource::new(PARITY_USER, name).unwrap(),
            url: WebUrl::parse(&format!("https://git.door43.org/{PARITY_USER}/{name}")).unwrap(),
        };
        w.t.app.script_share(Ok(repo.clone()));
        repo
    }

    async fn shared_holds(_w: &World, _name: &str, _words: &str) -> Option<bool> {
        None
    }
}

pub(super) fn share(n: u8, project: &ProjectRef, name: &str) -> ShareOnDoor43 {
    ShareOnDoor43 {
        request: rid(n),
        project: project.clone(),
        owner: ShareOwner::Mine,
        name: RepoName::parse(name).unwrap(),
    }
}

/// Door43's `repo` of the parity user opened here as a new project.
async fn open_shared(app: &App, n: u8, repo: &str) -> JobState {
    let job = post(
        app,
        &OpenDoor43 {
            request: rid(n),
            repo: RepoSource::new(PARITY_USER, repo).unwrap(),
        },
    )
    .await
    .unwrap();
    ended(app, job).await
}

pub(super) fn sync(n: u8) -> SyncProject {
    SyncProject {
        request: rid(n),
        project: folder(SAMPLE),
    }
}

pub(super) fn open(n: u8) -> OpenDoor43 {
    OpenDoor43 {
        request: rid(n),
        repo: RepoSource::new(PARITY_USER, "es-419_tit_equipo").unwrap(),
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
                sign_in(app).await;
                let project = <$fixture>::open_succeeds(&w).await;
                let opener = <$fixture>::opener(&w).await;
                let other = &opener.app;
                let before = listed(other).await;
                if <$fixture>::OPEN_WRITES {
                    assert!(!before.contains(&project), "{project:?} is new");
                }
                let job = post(other, &open(2)).await.unwrap();
                match ended(other, job).await {
                    JobState::Done(tc_api::jobs::JobOutcome::Created { project: added }) => {
                        assert_eq!(added, project);
                        assert!(listed(other).await.contains(&added), "{added:?} is listed");
                    }
                    other => panic!("{other:?}"),
                }
            }

            /// #677: a project opened from Door43 on another computer holds what Door43's main
            /// holds, the last sync's text included.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn open_from_door43_equals_main() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                internet_on(app).await;
                sign_in(app).await;
                let words = "Pablo, siervo, como Door43 lo tiene";
                save_titus_1_1(app, &folder(SAMPLE), 50, words).await;
                <$fixture>::sync_succeeds(&w, &folder(SAMPLE), 51).await;
                let job = post(app, &sync(52)).await.unwrap();
                assert!(
                    matches!(ended(app, job).await, JobState::Done(_)),
                    "the sync took the text to Door43"
                );
                if let Some(holds) = <$fixture>::door43_holds(&w, words).await {
                    assert!(holds, "the project opened from Door43 holds main's text");
                }
            }

            /// INV-SHARE-PUSHES-WHAT-THE-TRANSLATOR-SAW: a save made just before Share is in
            /// the main Door43 holds after it.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn share_pushes_the_checkpointed_main() {
                let w = <$fixture>::share_world().await;
                let app = &w.t.app;
                internet_on(app).await;
                sign_in(app).await;
                let project = folder(SAMPLE);
                // The translator shares from Home with the project still open in the screens.
                let at =
                    save_titus_open(app, &project, 1, 30, "Pablo, siervo, antes de compartir")
                        .await;
                let expected = <$fixture>::share_succeeds(&w, &project, "contract-share").await;
                let job = post(app, &share(31, &project, "contract-share")).await.unwrap();
                assert_eq!(
                    ended(app, job).await,
                    JobState::Done(tc_api::jobs::JobOutcome::Shared(expected))
                );
                post(app, &Close { at, request: rid(33) }).await.unwrap();
                if let Some(holds) = <$fixture>::shared_holds(
                    &w,
                    "contract-share",
                    "Pablo, siervo, antes de compartir",
                )
                .await
                {
                    assert!(
                        holds,
                        "INV-SHARE-PUSHES-WHAT-THE-TRANSLATOR-SAW: Door43's main lacks the save made before Share"
                    );
                }
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_refused_open_from_door43_adds_no_project() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                internet_on(app).await;
                sign_in(app).await;
                let opener = <$fixture>::opener(&w).await;
                let other = &opener.app;
                let before = listed(other).await;
                let error = <$fixture>::open_refused(&w).await;
                let job = post(other, &open(4)).await.unwrap();
                assert_eq!(ended(other, job).await, JobState::Failed(error));
                assert_eq!(listed(other).await, before, "no project was added");
            }

            /// INV-A-SYNC-NEVER-LOSES-A-SAVE: a save made while a sync runs is on disk after
            /// the sync, and the next sync takes it to Door43.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_save_during_a_sync_is_kept_and_pushed_next() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                internet_on(app).await;
                sign_in(app).await;
                let sample = folder(SAMPLE);
                <$fixture>::sync_succeeds(&w, &sample, 92).await;
                <$fixture>::hold_next_sync(&w).await;
                let calls = <$fixture>::calls(&w);
                let job = post(app, &sync(10)).await.unwrap();
                until("the sync reached the port", || {
                    <$fixture>::calls(&w) > calls
                })
                .await;
                const DURING: &str = "Pablo, siervo de Dios, escrito mientras se sincroniza";
                save_titus_1_1(app, &sample, 11, DURING).await;
                <$fixture>::release_sync(&w);
                assert!(
                    matches!(ended(app, job).await, JobState::Done(_)),
                    "the sync ends"
                );
                assert_eq!(
                    titus_1_1(app, &sample).await.as_deref(),
                    Some(DURING),
                    "INV-A-SYNC-NEVER-LOSES-A-SAVE: the save made during the sync is kept"
                );
                <$fixture>::sync_succeeds(&w, &sample, 93).await;
                let next = post(app, &sync(13)).await.unwrap();
                assert!(matches!(ended(app, next).await, JobState::Done(_)));
                if let Some(held) = <$fixture>::door43_holds(&w, DURING).await {
                    assert!(
                        held,
                        "INV-A-SYNC-NEVER-LOSES-A-SAVE: the next sync took the save to Door43"
                    );
                }
            }

            /// #761: a story project syncs as a Bible does.
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn a_story_project_syncs_like_a_bible() {
                let w = <$fixture>::world().await;
                let app = &w.t.app;
                internet_on(app).await;
                sign_in(app).await;
                let stories = folder(STORIES);
                <$fixture>::sync_succeeds(&w, &stories, 94).await;
                let job = post(
                    app,
                    &SyncProject {
                        request: rid(14),
                        project: stories,
                    },
                )
                .await
                .unwrap();
                assert!(
                    matches!(
                        ended(app, job).await,
                        JobState::Done(tc_api::jobs::JobOutcome::Synced(_))
                    ),
                    "the story project synced"
                );
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
                    post(app, &open(6)).await,
                    Err(ApiError::NeedsInternet(NetKind::Open))
                );
                assert_eq!(<$fixture>::calls(&w), 0, "nothing reached the port");
            }
        }
    };
}

/// The real port on the fake Door43: the sample is connected to an empty repository of the
/// parity user's, which a first sync fills.
pub(super) struct Door43;

const REPO: &str = "es-419_tit_equipo";
const STORY_REPO: &str = "es-419_obs_equipo";

fn story_repo() -> RepoSeed {
    RepoSeed {
        owner: PARITY_USER.to_owned(),
        name: STORY_REPO.to_owned(),
        subject: "Open Bible Stories".to_owned(),
        language: "es-419".to_owned(),
        topics: Vec::new(),
        private: false,
        commits: Vec::new(),
    }
}

fn repo_lookup_fails(w: &World) -> ApiError {
    w.fake.inject(FaultRule::once(
        "GET",
        &format!("/api/v1/repos/{PARITY_USER}/{REPO}"),
        Fault::Status {
            status: 404,
            message: "Not Found".to_owned(),
        },
    ));
    ApiError::Refused(Refusal::Sync(SyncFailure::BeforeIntake(
        SyncRefusal::RemoteMissing,
    )))
}

impl SyncPortFixture for Door43 {
    const OPEN_WRITES: bool = true;

    async fn world() -> World {
        let w = shared_with(Some(Arc::new(tc_app::ports::door43::Door43Sync))).await;
        w.fake
            .add_repo(RepoSeed {
                owner: PARITY_USER.to_owned(),
                name: REPO.to_owned(),
                subject: "Bible".to_owned(),
                language: "es-419".to_owned(),
                topics: Vec::new(),
                private: false,
                commits: Vec::new(),
            })
            .await
            .unwrap();
        w.t.app
            // The fake's seeded repositories' default branch.
            .connect_door43(&folder(SAMPLE), PARITY_USER, REPO, "master")
            .await
            .unwrap();
        w.fake.add_repo(story_repo()).await.unwrap();
        w.t.app
            .connect_door43(&folder(STORIES), PARITY_USER, STORY_REPO, "master")
            .await
            .unwrap();
        w.fake.clear_requests();
        w
    }

    async fn sync_fails_before_intake(w: &World) -> ApiError {
        repo_lookup_fails(w)
    }

    async fn opener(w: &World) -> Opener {
        let b = computer_b(w).await;
        Opener {
            app: b.0.app.clone(),
            _computer: Some(b),
        }
    }

    async fn open_succeeds(w: &World) -> ProjectRef {
        // The repository holds the sample once a first sync pushed it.
        synced(&w.t.app, 91).await;
        folder(REPO)
    }

    async fn open_refused(w: &World) -> ApiError {
        repo_lookup_fails(w)
    }

    fn calls(w: &World) -> usize {
        w.fake.requests().len()
    }

    async fn sync_succeeds(_w: &World, _project: &ProjectRef, _n: u8) {}

    async fn hold_next_sync(w: &World) {
        w.fake.inject(FaultRule::once(
            "GET",
            &format!("/api/v1/repos/{PARITY_USER}/{REPO}"),
            Fault::Hold,
        ));
    }

    fn release_sync(w: &World) {
        w.fake.release_held();
    }

    async fn door43_holds(w: &World, words: &str) -> Option<bool> {
        // Door43's copy, opened as a new project, as a teammate's computer gets it.
        let (b, _dir) = computer_b(w).await;
        let app = &b.app;
        let job = post(app, &open(20)).await.unwrap();
        let JobState::Done(tc_api::jobs::JobOutcome::Created { project }) = ended(app, job).await
        else {
            return Some(false);
        };
        Some(titus_1_1(app, &project).await.as_deref() == Some(words))
    }

    async fn share_world() -> World {
        let w = world_of("sample", Some(Arc::new(tc_app::ports::door43::Door43Sync))).await;
        w.fake.clear_requests();
        w
    }

    async fn share_succeeds(w: &World, _project: &ProjectRef, name: &str) -> SharedRepo {
        SharedRepo {
            repo: RepoSource::new(PARITY_USER, name).unwrap(),
            url: WebUrl::parse(&format!("{}/{PARITY_USER}/{name}", w.fake.url())).unwrap(),
        }
    }

    async fn shared_holds(w: &World, name: &str, words: &str) -> Option<bool> {
        let (b, _dir) = computer_b(w).await;
        let app = &b.app;
        let JobState::Done(tc_api::jobs::JobOutcome::Created { project }) =
            open_shared(app, 32, name).await
        else {
            return Some(false);
        };
        Some(titus_1_1(app, &project).await.as_deref() == Some(words))
    }
}

sync_port_contract!(scripted, Scripted);
sync_port_contract!(door43, Door43);

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
        deps.sync = std::sync::Arc::new(tc_app::ports::door43::Door43Sync);
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

/// INV-NO-REQUEST-WITHOUT-CONSENT: on the real port, nothing reaches Door43 at launch or in a
/// minute of idle, even with the internet question answered.
#[tokio::test(start_paused = true)]
async fn no_door43_request_at_launch_or_60_s_idle() {
    let w = <Door43 as SyncPortFixture>::world().await;
    let app = &w.t.app;
    internet_on(app).await;
    // Paused time: a minute passes at once, and every timer due in it fires; then real time,
    // so a request one of them started reaches the fake.
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    tokio::time::resume();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let heard = w.fake.requests();
    assert!(
        heard.is_empty(),
        "INV-NO-REQUEST-WITHOUT-CONSENT: Door43 heard {heard:?}"
    );
}

/// #724: a sync whose branch lookup fails is a failed sync, with nothing changed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_branch_lookup_is_a_failed_sync() {
    let w = <Door43 as SyncPortFixture>::world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    let before = card_with_counts(app).await;
    w.fake.inject(FaultRule::once(
        "GET",
        &format!("/api/v1/repos/{PARITY_USER}/{REPO}"),
        Fault::Status {
            status: 500,
            message: "Internal Server Error".to_owned(),
        },
    ));
    let job = post(app, &sync(30)).await.unwrap();
    match ended(app, job).await {
        JobState::Failed(ApiError::Refused(Refusal::Sync(SyncFailure::BeforeIntake(_)))) => {}
        other => panic!("a failed lookup is a failed sync: {other:?}"),
    }
    assert_eq!(card_with_counts(app).await, before, "nothing changed");
}

/// A second computer (B) on the same fake Door43: its own app data and device, the parity
/// user signed in, the sample opened from Door43 as `titus-b`.
struct Teammate {
    t: super::common::TestApp,
    project: ProjectRef,
    _dir: tempfile::TempDir,
}

/// The computer that opens from Door43 in the contract, kept while its app is used.
pub(super) struct Opener {
    app: tc_app::App,
    _computer: Option<(super::common::TestApp, tempfile::TempDir)>,
}

/// A second computer (B) on the same fake Door43: its own app data, device and empty projects
/// folder, the internet on and the parity user signed in.
async fn computer_b(w: &World) -> (super::common::TestApp, tempfile::TempDir) {
    use tc_app::StoreChoice;
    use tc_app::config::ProjectsFolder;
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let (backend, watch) = crate::store_world::backend_over(
        &tc_store::fault::FaultFs::new(),
        crate::store_world::APP,
        tc_store::FakePlatform {
            host: "computer-b".to_owned(),
            machine: "machine-of-b".to_owned(),
            ..tc_store::FakePlatform::default()
        },
        Arc::default(),
    );
    let url = w.fake.url().to_owned();
    let t = super::common::TestApp::start_with(
        dir.path(),
        &super::common::Parts::new(),
        |config, deps| {
            config.projects = ProjectsFolder::Given(projects.clone());
            config.door43 = url.parse().unwrap();
            deps.store = StoreChoice::Given(Arc::new(backend));
            deps.sync = Arc::new(tc_app::ports::door43::Door43Sync);
        },
    )
    .await;
    let mut t = t;
    t.watch = Some(watch);
    internet_on(&t.app).await;
    sign_in(&t.app).await;
    (t, dir)
}

async fn teammate_b(w: &World) -> Teammate {
    let (t, dir) = computer_b(w).await;
    let job = post(&t.app, &open(40)).await.unwrap();
    let JobState::Done(tc_api::jobs::JobOutcome::Created { project }) = ended(&t.app, job).await
    else {
        panic!("B opens the sample from Door43");
    };
    Teammate {
        t,
        project,
        _dir: dir,
    }
}

fn git_path(service: &str) -> String {
    format!("/{PARITY_USER}/{REPO}.git/git-{service}")
}

/// R13, INV-A-SYNC-NEVER-LOSES-A-SAVE: a connection cut after the teammates' work came in keeps
/// that work and says so; one cut before it changes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dropped_connection_keeps_received_work_and_says_so() {
    let w = <Door43 as SyncPortFixture>::world().await;
    let a = &w.t.app;
    internet_on(a).await;
    sign_in(a).await;
    synced(a, 41).await;
    let b = teammate_b(&w).await;
    const FROM_A: &str = "Pablo, siervo de Dios, como A lo escribio";
    save_titus_1_1(a, &folder(SAMPLE), 42, FROM_A).await;
    // Both computers sign in as the parity user, and a sign-in replaces the user's token: each
    // signs in again before it syncs.
    sign_in_again(a, 43).await;
    synced(a, 44).await;
    save_titus(
        &b.t.app,
        &b.project,
        2,
        45,
        "Lo que B escribio en el versiculo dos",
    )
    .await;
    let before_b = titus_1_1(&b.t.app, &b.project).await;

    sign_in_again(&b.t.app, 46).await;
    // Cut before intake: the fetch's pack stops part way.
    w.fake.inject(FaultRule::once(
        "POST",
        &git_path("upload-pack"),
        Fault::BreakOff { after: 0 },
    ));
    let job = post(
        &b.t.app,
        &SyncProject {
            request: rid(47),
            project: b.project.clone(),
        },
    )
    .await
    .unwrap();
    match ended(&b.t.app, job).await {
        JobState::Failed(ApiError::Refused(Refusal::Sync(SyncFailure::BeforeIntake(_)))) => {}
        other => panic!("a cut fetch fails before intake: {other:?}"),
    }
    assert_eq!(
        titus_1_1(&b.t.app, &b.project).await,
        before_b,
        "nothing changed"
    );

    // Cut after intake: the push's reply never arrives.
    w.fake.inject(FaultRule::once(
        "POST",
        &git_path("receive-pack"),
        Fault::BreakOff { after: 0 },
    ));
    let job = post(
        &b.t.app,
        &SyncProject {
            request: rid(48),
            project: b.project.clone(),
        },
    )
    .await
    .unwrap();
    let ended_b = ended(&b.t.app, job).await;
    assert_eq!(
        titus_1_1(&b.t.app, &b.project).await.as_deref(),
        Some(FROM_A),
        "INV-A-SYNC-NEVER-LOSES-A-SAVE: A's work came in and stays ({ended_b:?})"
    );
    match ended_b {
        JobState::Failed(ApiError::Refused(Refusal::Sync(SyncFailure::AfterIntake(_)))) => {}
        other => panic!(
            "INV-A-SYNC-NEVER-LOSES-A-SAVE: a push cut after intake says the work came in: {other:?}"
        ),
    }
}

/// A sign-in with its own request id, so it is not the reply of an earlier one.
async fn sign_in_again(app: &App, n: u8) {
    post(
        app,
        &tc_api::account::SignIn {
            request: rid(n),
            login: PARITY_USER.to_owned(),
            password: tc_door43_fake::seeded::PARITY_PASSWORD.to_owned(),
            stay: false,
        },
    )
    .await
    .unwrap();
}

/// Owner ruling of 2026-10-24: Door43's branch deleted by its owner; the next sync puts this
/// copy back and the toast says so, once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deleted_branch_is_put_back_and_the_toast_says_so_once() {
    let w = <Door43 as SyncPortFixture>::world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    synced(app, 50).await;
    let bare = git2::Repository::open(w.fake.git_dir(PARITY_USER, REPO)).unwrap();
    bare.find_reference("refs/heads/master")
        .unwrap()
        .delete()
        .unwrap();
    assert!(
        synced(app, 51).await.recreated,
        "the toast says the copy was put back"
    );
    assert!(
        bare.find_reference("refs/heads/master").is_ok(),
        "the branch is back"
    );
    assert!(!synced(app, 52).await.recreated, "said once");
}

/// Q7, D98: signing in is the consent, so the first confirmed sync writes the username and sends
/// it at once: right after that one sync, Door43's copy already names this computer's person.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_sync_sends_the_username_at_once() {
    let w = <Door43 as SyncPortFixture>::world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    let device = {
        let at = app
            .sessions()
            .open_screen(&folder(SAMPLE), open_progress())
            .await
            .unwrap()
            .at;
        let device = app.sessions().reader(&at).unwrap().handle().facts().device;
        post(
            app,
            &Close {
                at,
                request: rid(60),
            },
        )
        .await
        .unwrap();
        device
    };
    synced(app, 61).await;
    // Door43's copy, opened as a teammate's computer gets it.
    let (b, _dir) = computer_b(&w).await;
    let app = &b.app;
    let job = post(app, &open(62)).await.unwrap();
    let JobState::Done(tc_api::jobs::JobOutcome::Created { project }) = ended(app, job).await
    else {
        panic!("Door43's copy opens");
    };
    let at = app
        .sessions()
        .open_screen(&project, open_progress())
        .await
        .unwrap()
        .at;
    let profile = app
        .sessions()
        .reader(&at)
        .unwrap()
        .read()
        .projected(&tc_journal::RecordKey::DeviceProfile(device));
    let name = match profile {
        Some(Record::DeviceProfile { value, .. }) => {
            value.door43_username.map(|n| n.as_str().to_owned())
        }
        _ => None,
    };
    assert_eq!(
        name.as_deref(),
        Some(PARITY_USER),
        "Door43 has the name after one sync"
    );
}

/// tC4's sync-ok-restored: Door43 lost a teammate's change this computer already holds; the next
/// sync sends it again and the toast says so, counting the change once and no username record.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_door43_lost_is_sent_again_and_counted_once() {
    let w = <Door43 as SyncPortFixture>::world().await;
    let a = &w.t.app;
    internet_on(a).await;
    sign_in(a).await;
    synced(a, 70).await;
    let b = teammate_b(&w).await;
    let before = w.fake.resolve(PARITY_USER, REPO, "master").await.unwrap();
    save_titus(
        &b.t.app,
        &b.project,
        2,
        71,
        "Lo que B escribio, que Door43 perdera",
    )
    .await;
    sign_in_again(&b.t.app, 73).await;
    let job = post(
        &b.t.app,
        &SyncProject {
            request: rid(74),
            project: b.project.clone(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(ended(&b.t.app, job).await, JobState::Done(_)));
    sign_in_again(a, 75).await;
    let first = synced(a, 76).await;
    assert_eq!(
        first.received, 1,
        "B's verse, not B's username record: {first:?}"
    );
    w.fake
        .set_branch(PARITY_USER, REPO, "master", &before)
        .await
        .unwrap();
    let again = synced(a, 77).await;
    assert_eq!(
        again.restored, 1,
        "the lost change is sent again: {again:?}"
    );
}

/// Door43's requests that would write a repository: a create or a push.
fn writes(w: &World) -> Vec<String> {
    w.fake
        .requests()
        .into_iter()
        .map(|r| format!("{} {}", r.method, r.path))
        .filter(|r| r.starts_with("POST /api/v1/user/repos") || r.contains("git-receive-pack"))
        .collect()
}

/// tC4 RISKS #7: a repository of that name on Door43 (a tC3-era one included) is never pushed
/// over; the share is refused before any push and the project stays unshared.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn share_name_taken_on_door43_is_refused_before_any_push() {
    let w = Door43::share_world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    w.fake
        .add_repo(RepoSeed {
            owner: PARITY_USER.to_owned(),
            name: "tc3-era".to_owned(),
            subject: "Bible".to_owned(),
            language: "es-419".to_owned(),
            topics: Vec::new(),
            private: false,
            commits: Vec::new(),
        })
        .await
        .unwrap();
    w.fake.clear_requests();
    let job = post(app, &share(40, &folder(SAMPLE), "tc3-era"))
        .await
        .unwrap();
    assert_eq!(
        ended(app, job).await,
        JobState::Failed(ApiError::Refused(Refusal::Share(
            tc_api::refusal::ShareRefusal::NameExists
        )))
    );
    assert!(
        !writes(&w).iter().any(|r| r.contains("git-receive-pack")),
        "nothing was pushed: {:?}",
        writes(&w)
    );
}

/// INV-PRIVATE-OPEN-ASKS-SIGN-IN-ONCE: an open of a private repository without a sign-in asks
/// it once, and the open after the sign-in gets the project; a 401 or 404 with a sign-in is
/// the refusal, never a second ask.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_private_repo_signs_in_and_retries() {
    let w = Door43::share_world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    // A private repository that holds the sample: made empty, then filled by a sync.
    w.fake
        .add_repo(RepoSeed {
            owner: PARITY_USER.to_owned(),
            name: "privado".to_owned(),
            subject: "Bible".to_owned(),
            language: "es-419".to_owned(),
            topics: Vec::new(),
            private: true,
            commits: Vec::new(),
        })
        .await
        .unwrap();
    app.connect_door43(&folder(SAMPLE), PARITY_USER, "privado", "master")
        .await
        .unwrap();
    synced(app, 41).await;
    // Another computer, signed out, opens it.
    let (b, _dir) = computer_b(&w).await;
    let app = &b.app;
    post(app, &tc_api::account::SignOut { request: rid(42) })
        .await
        .unwrap();
    w.fake.clear_requests();
    let job = post(
        app,
        &OpenDoor43 {
            request: rid(43),
            repo: RepoSource::new(PARITY_USER, "privado").unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        ended(app, job).await,
        JobState::Failed(ApiError::NeedsSignIn(
            tc_api::error::SignInReason::NotSignedIn
        )),
        "without a sign-in Door43 hides the repository: the open asks for one"
    );
    assert!(
        w.fake
            .requests()
            .iter()
            .all(|r| r.authorization == tc_door43_fake::AuthSeen::None),
        "the first try carried no token"
    );
    sign_in_again(app, 47).await;
    let after = open_shared(app, 44, "privado").await;
    assert!(
        matches!(
            after,
            JobState::Done(tc_api::jobs::JobOutcome::Created { .. })
        ),
        "the open after the sign-in gets the project: {after:?}"
    );
    assert_eq!(
        open_shared(app, 45, "no-such-repo").await,
        JobState::Failed(ApiError::Refused(Refusal::Sync(SyncFailure::BeforeIntake(
            SyncRefusal::RemoteMissing
        )))),
        "INV-PRIVATE-OPEN-ASKS-SIGN-IN-ONCE: a 404 with a sign-in is the refusal, not a second ask"
    );
}

/// Ruling H14: a computer holds one copy of a project. An open of a repository this computer's
/// project syncs with makes no second copy and names that project, before Door43 is asked
/// anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_a_project_you_have_makes_no_copy_and_offers_yours() {
    let w = Door43::world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    let before = listed(app).await;
    w.fake.clear_requests();
    assert_eq!(
        post(app, &open(46)).await,
        Err(ApiError::Refused(Refusal::OpenDoor43(
            OpenDoor43Refusal::AlreadyHere {
                project: folder(SAMPLE)
            }
        )))
    );
    assert_eq!(listed(app).await, before, "no second copy");
    assert_eq!(w.fake.requests(), Vec::new(), "Door43 was asked nothing");
}

/// Ruling H14: an open asks for no name. The project keeps the name it has on Door43, in a
/// folder named for the repository (`open_folder`; its `-2` is tc-api's and the store test's).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_asks_no_name_and_keeps_the_teams_name() {
    let w = Door43::world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    synced(app, 47).await;
    let (b, _dir) = computer_b(&w).await;
    let JobState::Done(tc_api::jobs::JobOutcome::Created { project }) =
        ended(&b.app, post(&b.app, &open(48)).await.unwrap()).await
    else {
        panic!("Door43's copy opens");
    };
    assert_eq!(project, folder(REPO));
    let name_of = |home: &tc_api::home::HomeView, project: &ProjectRef| {
        home.cards
            .iter()
            .find(|c| &c.project == project)
            .map(|c| c.name.clone())
    };
    let here = post(app, &tc_api::home::List {}).await.unwrap();
    let there = post(&b.app, &tc_api::home::List {}).await.unwrap();
    assert!(name_of(&here, &folder(SAMPLE)).is_some());
    assert_eq!(
        name_of(&there, &project),
        name_of(&here, &folder(SAMPLE)),
        "one name for the whole team"
    );
}

/// A project already on Door43 is never shared again: a second tab's dialog, or a resend with
/// a changed name after a lost reply, would move it to another repository.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shared_project_is_never_shared_to_a_second_repository() {
    let w = Door43::world().await;
    let app = &w.t.app;
    internet_on(app).await;
    sign_in(app).await;
    assert_eq!(
        post(app, &share(60, &folder(SAMPLE), "a-second-repository")).await,
        Err(ApiError::Refused(Refusal::Share(
            tc_api::refusal::ShareRefusal::AlreadyShared
        )))
    );
    assert_eq!(
        writes(&w),
        Vec::<String>::new(),
        "nothing was created or pushed"
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
