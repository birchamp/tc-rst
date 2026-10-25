//! Sessions (unit B1b, APP-DESIGN §4.3, AD38, A-8, A-25): one open per folder, epochs that are
//! never used again, the close at Home with tC4's leave checkpoint, and a reply cache per
//! session. Every case runs through `App`, over a counting store around the memory backend.

use std::sync::Arc;
use std::time::Duration;

use tc_api::app::SessionRole;
use tc_api::event::{
    Aspect, ChangeCause, ChangeScope, ChapterRef, CheckpointOutcome, Event, SessionEvent,
    SessionMode,
};
use tc_api::ids::{Epoch, ProjectRef, Scope};
use tc_api::net::Set;
use tc_api::prefs::{DraftUnit, SetDraftUnit};
use tc_api::shown::{PersonLabel, Who};
use tc_api::test::Reset;
use tc_api::write::ReadOnlyReason;
use tc_api::{ApiError, NotSaved};
use tc_app::backend::memory::{MemoryBackend, MemoryProject, Scripted};
use tc_app::backend::{CheckpointReason, OpenFailure, StoreCommand};
use tc_app::events::{Next, Subscription};
use tc_app::reports::{ReportOp, ReportOutcome};
use tc_journal::fold::ProjectKind;
use tc_journal::{BookId, ProjectId};
use tempfile::TempDir;

use super::common::{Parts, TestApp, folder, open_progress, post, rid, sample_bible, tit};
use crate::counting_store::CountingStore;

struct Fixture {
    _dir: TempDir,
    t: TestApp,
    store: Arc<CountingStore<MemoryBackend>>,
}

impl Fixture {
    fn backend(&self) -> &MemoryBackend {
        self.store.inner()
    }

    fn sessions(&self) -> &tc_app::session::SessionRegistry {
        self.t.app.sessions()
    }

    async fn open(&self, name: &str) -> Scope {
        self.sessions()
            .open_screen(&folder(name), open_progress())
            .await
            .unwrap()
            .at
    }

    async fn settled(&self, name: &str) {
        tokio::time::timeout(
            Duration::from_secs(60),
            self.sessions().settled(&folder(name)),
        )
        .await
        .expect("hung: the folder settles");
    }

    fn events(&self) -> Subscription {
        let bus = self.t.app.events();
        bus.subscribe(bus.head())
    }
}

fn empty(n: u8) -> MemoryProject {
    MemoryProject::empty(ProjectId::from_bytes([n; 16]), ProjectKind::Bible)
}

/// An app over a counting store holding `projects`.
async fn started(projects: Vec<(&str, MemoryProject)>) -> Fixture {
    let backend = MemoryBackend::new();
    for (name, project) in projects {
        backend.add(folder(name), project).unwrap();
    }
    let store = Arc::new(CountingStore::new(backend));
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_with(dir.path(), &Parts::new(), |_, deps| {
        deps.store = tc_app::StoreChoice::Given(store.clone());
    })
    .await;
    Fixture {
        _dir: dir,
        t,
        store,
    }
}

async fn one_project() -> Fixture {
    started(vec![("kr_ulb", empty(1))]).await
}

fn draft_unit(at: &Scope, id: u8, unit: DraftUnit) -> SetDraftUnit {
    SetDraftUnit {
        at: at.clone(),
        request: rid(id),
        unit,
    }
}

fn later(at: &Scope) -> Scope {
    Scope {
        project: at.project.clone(),
        epoch: Epoch::new(at.epoch.get() + 1),
    }
}

const STALE: ApiError = ApiError::NotSaved(NotSaved::StaleScope);

/// The session events of `at`, in order, until `last` holds of one.
async fn events_of(
    sub: &mut Subscription,
    at: &Scope,
    last: impl Fn(&SessionEvent) -> bool,
) -> Vec<SessionEvent> {
    let mut out = Vec::new();
    loop {
        let next = tokio::time::timeout(Duration::from_secs(60), sub.next())
            .await
            .expect("hung: the event comes");
        let Next::Frame(frame) = next else {
            panic!("{next:?}")
        };
        if let Event::Session { at: scope, event } = frame.event
            && &scope == at
        {
            let done = last(&event);
            out.push(event);
            if done {
                return out;
            }
        }
    }
}

// ---- The row's epoch refusal tests ---------------------------------------------------------

#[tokio::test]
async fn a_write_after_going_home_is_stale_scope_and_writes_nothing() {
    let f = one_project().await;
    let at = f.open("kr_ulb").await;
    f.sessions().leave_screen(&at).unwrap();
    // At once: the checkpoint and the close may still run behind it.
    assert_eq!(
        post(&f.t.app, &draft_unit(&at, 1, DraftUnit::Verse)).await,
        Err(STALE)
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(f.t.files.writes().is_empty(), "{:?}", f.t.files.writes());
    // A second leave of the same scope is stale too.
    assert!(f.sessions().leave_screen(&at).is_err());
}

#[tokio::test]
async fn a_write_with_the_epoch_of_an_earlier_open_is_stale_after_a_reopen() {
    let f = one_project().await;
    let first = f.open("kr_ulb").await;
    f.sessions().leave_screen(&first).unwrap();
    f.settled("kr_ulb").await;
    let second = f.open("kr_ulb").await;
    assert!(second.epoch > first.epoch);
    assert_eq!(
        post(&f.t.app, &draft_unit(&first, 1, DraftUnit::Verse)).await,
        Err(STALE)
    );
    assert_eq!(
        post(&f.t.app, &draft_unit(&second, 2, DraftUnit::Verse)).await,
        Ok(())
    );
}

#[tokio::test]
async fn a_query_with_a_stale_scope_names_the_current_epoch() {
    let f = one_project().await;
    let first = f.open("kr_ulb").await;
    f.sessions().leave_screen(&first).unwrap();
    f.settled("kr_ulb").await;
    let second = f.open("kr_ulb").await;
    assert_eq!(
        f.t.app.read(&first).err(),
        Some(ApiError::StaleScope {
            current: Some(second.epoch)
        })
    );
    assert_eq!(f.t.app.read(&second).unwrap().scope(), &second);
    f.sessions().leave_screen(&second).unwrap();
    f.settled("kr_ulb").await;
    assert_eq!(
        f.t.app.read(&second).err(),
        Some(ApiError::StaleScope { current: None })
    );
}

#[tokio::test]
async fn a_stale_scope_refusal_is_never_kept_under_its_request_id() {
    let f = one_project().await;
    let at = f.open("kr_ulb").await;
    assert_eq!(
        post(&f.t.app, &draft_unit(&later(&at), 7, DraftUnit::Verse)).await,
        Err(STALE)
    );
    // The same id, with another body, at the open scope: it runs, it is no reuse.
    assert_eq!(
        post(&f.t.app, &draft_unit(&at, 7, DraftUnit::Section)).await,
        Ok(())
    );
}

#[tokio::test]
async fn epochs_are_never_reused_across_reopens_and_resets() {
    let f = one_project().await;
    let first = f.open("kr_ulb").await;
    post(&f.t.app, &Reset { request: rid(1) }).await.unwrap();
    assert!(f.sessions().open_sessions().is_empty());
    let second = f.open("kr_ulb").await;
    assert!(second.epoch > first.epoch, "{second:?} after {first:?}");
    f.sessions().leave_screen(&second).unwrap();
    f.settled("kr_ulb").await;
    let third = f.open("kr_ulb").await;
    assert!(third.epoch > second.epoch);
}

// ---- The reply cache per session ------------------------------------------------------------

#[tokio::test]
async fn a_retry_of_a_project_write_is_answered_from_its_sessions_cache() {
    let f = one_project().await;
    let at = f.open("kr_ulb").await;
    assert_eq!(
        post(&f.t.app, &draft_unit(&at, 3, DraftUnit::Verse)).await,
        Ok(())
    );
    assert_eq!(
        post(&f.t.app, &draft_unit(&at, 3, DraftUnit::Section)).await,
        Err(ApiError::RequestReused)
    );
    assert_eq!(
        post(&f.t.app, &draft_unit(&at, 3, DraftUnit::Verse)).await,
        Ok(())
    );
}

#[tokio::test]
async fn the_sessions_cache_goes_with_its_session() {
    let f = one_project().await;
    let first = f.open("kr_ulb").await;
    assert_eq!(
        post(&f.t.app, &draft_unit(&first, 4, DraftUnit::Verse)).await,
        Ok(())
    );
    f.sessions().leave_screen(&first).unwrap();
    f.settled("kr_ulb").await;
    let second = f.open("kr_ulb").await;
    assert_eq!(
        post(&f.t.app, &draft_unit(&second, 4, DraftUnit::Section)).await,
        Ok(())
    );
}

#[tokio::test]
async fn writes_with_no_scope_still_use_the_app_wide_cache() {
    let f = one_project().await;
    let set = |on| Set {
        request: rid(9),
        on,
    };
    assert!(post(&f.t.app, &set(true)).await.is_ok());
    assert_eq!(
        post(&f.t.app, &set(false)).await,
        Err(ApiError::RequestReused)
    );
    assert!(f.sessions().open_sessions().is_empty());
}

// ---- Users, the close at Home and its checkpoint ---------------------------------------------

#[tokio::test]
async fn a_session_stays_open_while_one_of_its_writes_runs() {
    let f = one_project().await;
    let at = f.open("kr_ulb").await;
    let lease = f.sessions().writer(&at).unwrap();
    f.sessions().leave_screen(&at).unwrap();
    // The leave checkpoint ends; the write still holds the session open.
    f.settled("kr_ulb").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(f.store.checkpoints().len(), 1);
    assert_eq!(
        f.store.closes(&folder("kr_ulb")),
        0,
        "the write holds the session"
    );
    assert!(f.sessions().busy());
    assert_eq!(lease.session().scope(), &at);
    drop(lease);
    f.settled("kr_ulb").await;
    assert_eq!(f.store.closes(&folder("kr_ulb")), 1);
    assert!(!f.sessions().busy());
}

#[tokio::test]
async fn going_home_runs_the_leave_checkpoint_then_closes() {
    let f = one_project().await;
    let mut sub = f.events();
    let at = f.open("kr_ulb").await;
    f.sessions().leave_screen(&at).unwrap();
    let events = events_of(&mut sub, &at, |e| matches!(e, SessionEvent::Closed)).await;
    assert_eq!(
        events,
        [
            SessionEvent::Opened,
            SessionEvent::Checkpoint(CheckpointOutcome::Unchanged),
            SessionEvent::Closed
        ]
    );
    f.settled("kr_ulb").await;
    assert_eq!(
        f.store.checkpoints(),
        [(folder("kr_ulb"), CheckpointReason::LeavingProject)]
    );
    assert_eq!(f.store.closes(&folder("kr_ulb")), 1);
    assert!(
        super::common::hello_value(&f.t.app).await["sessions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn going_home_returns_before_the_leave_checkpoint_ends() {
    let f = one_project().await;
    f.store.hold_checkpoint();
    let at = f.open("kr_ulb").await;
    f.sessions().leave_screen(&at).unwrap();
    let settled = tokio::time::timeout(
        Duration::from_millis(100),
        f.sessions().settled(&folder("kr_ulb")),
    )
    .await;
    assert!(settled.is_err(), "the checkpoint is still running");
    assert!(f.sessions().busy());
    assert!(
        f.sessions().open_sessions().is_empty(),
        "the screen left at once"
    );
    f.store.release_checkpoint();
    f.settled("kr_ulb").await;
    assert_eq!(f.store.closes(&folder("kr_ulb")), 1);
}

#[tokio::test]
async fn an_open_during_the_leave_checkpoint_waits_for_it_and_gets_a_new_epoch() {
    let f = one_project().await;
    f.store.hold_checkpoint();
    let first = f.open("kr_ulb").await;
    f.sessions().leave_screen(&first).unwrap();
    let sessions = f.sessions().clone();
    let reopen = tokio::spawn(async move {
        sessions
            .open_screen(&folder("kr_ulb"), open_progress())
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!reopen.is_finished(), "the open waits for the checkpoint");
    assert_eq!(f.store.opens(&folder("kr_ulb")), 1);
    f.store.release_checkpoint();
    let second = tokio::time::timeout(Duration::from_secs(60), reopen)
        .await
        .expect("hung: the reopen after the checkpoint")
        .unwrap()
        .unwrap()
        .at;
    assert!(second.epoch > first.epoch);
    assert_eq!(f.store.opens(&folder("kr_ulb")), 2);
    assert_eq!(f.store.closes(&folder("kr_ulb")), 1);
    assert_eq!(
        f.t.app.read(&first).err(),
        Some(ApiError::StaleScope {
            current: Some(second.epoch)
        })
    );
}

#[tokio::test]
async fn a_side_session_outlives_going_home_and_closes_when_its_holder_leaves() {
    let f = one_project().await;
    let at = f.open("kr_ulb").await;
    let side = f
        .sessions()
        .open_side(&folder("kr_ulb"), open_progress())
        .await
        .unwrap();
    assert_eq!(side.scope(), &at);
    f.sessions().leave_screen(&at).unwrap();
    f.settled("kr_ulb").await;
    assert_eq!(f.store.checkpoints().len(), 1);
    assert_eq!(f.store.closes(&folder("kr_ulb")), 0);
    let open = f.sessions().open_sessions();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].at, at, "the same epoch");
    assert_eq!(open[0].role, SessionRole::Side);
    // The person is Home: the screen's requests are stale, whoever holds the session.
    assert_eq!(
        post(&f.t.app, &draft_unit(&at, 1, DraftUnit::Verse)).await,
        Err(STALE)
    );
    drop(side);
    f.settled("kr_ulb").await;
    assert_eq!(f.store.closes(&folder("kr_ulb")), 1);
    assert!(f.sessions().open_sessions().is_empty());
}

#[tokio::test]
async fn opening_a_second_project_leaves_the_first_screen_session() {
    let f = started(vec![("first", empty(1)), ("second", empty(2))]).await;
    let a = f.open("first").await;
    let b = f.open("second").await;
    f.settled("first").await;
    assert_eq!(
        f.store.checkpoints(),
        [(folder("first"), CheckpointReason::LeavingProject)]
    );
    assert_eq!(f.store.closes(&folder("first")), 1);
    let open = f.sessions().open_sessions();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].at, b);
    assert_eq!(open[0].role, SessionRole::Screen);
    assert!(f.t.app.read(&a).is_err());
}

#[tokio::test]
async fn a_refused_open_reaches_every_caller_and_leaves_nothing_open() {
    let f = one_project().await;
    let nowhere = folder("nowhere");
    f.store.hold_open(&nowhere);
    let screen = {
        let sessions = f.sessions().clone();
        let nowhere = nowhere.clone();
        tokio::spawn(async move { sessions.open_screen(&nowhere, open_progress()).await })
    };
    super::common::until("the first open reaches the store", || {
        f.store.opens(&nowhere) == 1
    })
    .await;
    let mut side = {
        let sessions = f.sessions().clone();
        let nowhere = nowhere.clone();
        Box::pin(async move {
            sessions
                .open_side(&nowhere, open_progress())
                .await
                .map(|lease| lease.scope().clone())
        })
    };
    assert!(
        crate::store_world::poll_once(&mut side).await.is_none(),
        "the side open joins the held one"
    );
    f.store.release_open(&nowhere);
    assert_eq!(screen.await.unwrap().err(), Some(OpenFailure::NotAProject));
    assert_eq!(side.await.err(), Some(OpenFailure::NotAProject));
    assert_eq!(f.store.opens(&nowhere), 1, "one open, shared");
    let reports: Vec<_> =
        f.t.app
            .reports()
            .all()
            .into_iter()
            .filter(|r| r.op == ReportOp::Open)
            .collect();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].outcome,
        ReportOutcome::Refused {
            code: Some("open.not-a-project".to_owned()),
            key: "refusal.open.not-a-project".to_owned()
        }
    );
    assert!(f.sessions().open_sessions().is_empty());
    assert!(!f.sessions().busy());
    // No epoch went to the failure.
    assert_eq!(f.open("kr_ulb").await.epoch, Epoch::new(1));
}

#[tokio::test]
async fn a_failed_leave_checkpoint_still_closes_and_says_so() {
    let f = one_project().await;
    let mut sub = f.events();
    f.store.fail_checkpoint("the disk is full");
    let at = f.open("kr_ulb").await;
    f.sessions().leave_screen(&at).unwrap();
    let events = events_of(&mut sub, &at, |e| matches!(e, SessionEvent::Closed)).await;
    assert_eq!(
        events,
        [
            SessionEvent::Opened,
            SessionEvent::Checkpoint(CheckpointOutcome::Failed {
                reason: "the disk is full".to_owned(),
                code: None,
            }),
            SessionEvent::Closed
        ]
    );
    let failed: Vec<_> =
        f.t.app
            .reports()
            .all()
            .into_iter()
            .filter(|r| r.op == ReportOp::Checkpoint)
            .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].outcome,
        ReportOutcome::Failed {
            reason: "the disk is full".to_owned()
        }
    );
    f.settled("kr_ulb").await;
    assert_eq!(f.store.closes(&folder("kr_ulb")), 1);
}

#[tokio::test]
async fn quit_closes_every_session_after_its_checkpoint() {
    let f = started(vec![("first", empty(1)), ("second", empty(2))]).await;
    f.open("first").await;
    let side = f
        .sessions()
        .open_side(&folder("second"), open_progress())
        .await
        .unwrap();
    f.t.app.quit().await;
    assert_eq!(
        f.store.checkpoints(),
        [(folder("first"), CheckpointReason::LeavingProject)]
    );
    assert_eq!(f.store.closes(&folder("first")), 1);
    assert_eq!(f.store.closes(&folder("second")), 1);
    assert!(f.sessions().open_sessions().is_empty());
    // A lease let go by the quit changes nothing when its holder drops it later.
    drop(side);
    assert_eq!(f.store.closes(&folder("second")), 1);
}

#[tokio::test]
async fn test_reset_closes_every_session_and_puts_the_seed_back() {
    let f = started(vec![("sample_burrito", sample_bible())]).await;
    let project = folder("sample_burrito");
    f.open("sample_burrito").await;
    let changes = f
        .backend()
        .teammates_edit_verse(&project, &tit(1, 1), &["one", "two"])
        .unwrap();
    f.backend().teammate_edits(&project, changes).unwrap();
    assert_eq!(
        f.backend().fold(&project).unwrap().heads(&tit(1, 1)).len(),
        2
    );
    post(&f.t.app, &Reset { request: rid(1) }).await.unwrap();
    assert_eq!(f.store.closes(&project), 1);
    assert_eq!(
        f.backend().fold(&project).unwrap().heads(&tit(1, 1)).len(),
        1
    );
    assert!(
        super::common::hello_value(&f.t.app).await["sessions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn teammates_on_an_open_project_send_one_changed_event() {
    let f = started(vec![("sample_burrito", sample_bible())]).await;
    let project = folder("sample_burrito");
    let mut sub = f.events();
    let at = f.open("sample_burrito").await;
    let changes = f
        .backend()
        .teammates_edit_verse(&project, &tit(1, 1), &["one", "two"])
        .unwrap();
    f.backend().teammate_edits(&project, changes).unwrap();
    let events = events_of(&mut sub, &at, |e| matches!(e, SessionEvent::Changed { .. })).await;
    let teammate = |n| PersonLabel {
        who: Who::Teammate(n),
        computer: None,
    };
    let [
        SessionEvent::Opened,
        SessionEvent::Changed { view, scope, cause },
    ] = events.as_slice()
    else {
        panic!("{events:?}")
    };
    assert_eq!(view.get(), 2);
    assert_eq!(
        cause,
        &ChangeCause::Intake {
            from: vec![teammate(1), teammate(2)]
        }
    );
    let ChangeScope::Parts(parts) = scope else {
        panic!("{scope:?}")
    };
    assert_eq!(
        parts.chapters,
        [ChapterRef {
            book: BookId::Titus,
            chapter: 1
        }]
    );
    assert!(parts.books.is_empty());
    assert_eq!(parts.aspects, [Aspect::Conflicts]);
    assert_eq!(parts.records.len(), 1);
    assert_eq!(parts.records[0].by, Some(teammate(2)));
    assert_eq!(f.t.app.read(&at).unwrap().view().get(), 2);
    // Exactly one: nothing else for the scope follows.
    tokio::time::sleep(Duration::from_millis(30)).await;
    while let Some(next) = sub.try_next() {
        if let Next::Frame(frame) = next {
            assert!(
                !matches!(&frame.event, Event::Session { at: scope, .. } if scope == &at),
                "{frame:?}"
            );
        }
    }
}

#[tokio::test]
async fn hello_lists_the_open_sessions_with_their_roles_and_modes() {
    let f = started(vec![("b_side", sample_bible()), ("a_screen", empty(1))]).await;
    let mut sub = f.events();
    let screen = f.open("a_screen").await;
    let side = f
        .sessions()
        .open_side(&folder("b_side"), open_progress())
        .await
        .unwrap();
    f.backend()
        .script(
            &folder("b_side"),
            Scripted::ReadOnly(ReadOnlyReason::CloudSynced),
        )
        .unwrap();
    let handle = side.session().handle().clone();
    let token = handle.snapshot().mint(&tit(1, 1), None).unwrap();
    let refused = handle
        .submit(StoreCommand::save_verse(token, "x".to_owned()))
        .await;
    assert!(refused.is_err());
    let read_only = SessionMode::ReadOnly(ReadOnlyReason::CloudSynced);
    let events = events_of(&mut sub, side.scope(), |e| {
        matches!(e, SessionEvent::Mode(_))
    })
    .await;
    assert_eq!(events.last(), Some(&SessionEvent::Mode(read_only)));

    let hello = super::common::hello_value(&f.t.app).await;
    let sessions: Vec<tc_api::app::OpenSession> =
        serde_json::from_value(hello["sessions"].clone()).unwrap();
    let listed: Vec<(ProjectRef, SessionRole, SessionMode)> = sessions
        .iter()
        .map(|s| (s.at.project.clone(), s.role, s.mode))
        .collect();
    assert_eq!(
        listed,
        [
            (
                folder("a_screen"),
                SessionRole::Screen,
                SessionMode::Writable
            ),
            (folder("b_side"), SessionRole::Side, read_only),
        ]
    );
    assert_eq!(sessions[0].at, screen);
    assert_eq!(sessions[1].id.as_str(), sample_bible().id.to_string());
}

#[tokio::test]
async fn quit_during_an_open_closes_that_session_once_it_has_opened() {
    let f = one_project().await;
    let project = folder("kr_ulb");
    f.store.hold_open(&project);
    let opening = {
        let sessions = f.sessions().clone();
        let project = project.clone();
        tokio::spawn(async move { sessions.open_screen(&project, open_progress()).await })
    };
    super::common::until("the open reaches the store", || {
        f.store.opens(&project) == 1
    })
    .await;
    let quit = {
        let app = f.t.app.clone();
        tokio::spawn(async move { app.quit().await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!quit.is_finished(), "the quit waits for the open");
    f.store.release_open(&project);
    assert!(opening.await.unwrap().is_ok());
    tokio::time::timeout(Duration::from_secs(60), quit)
        .await
        .expect("hung: the quit ends")
        .unwrap();
    assert_eq!(f.store.closes(&project), 1);
    assert!(f.sessions().open_sessions().is_empty());
    assert!(!f.sessions().busy());
}

#[tokio::test]
async fn a_screen_open_waits_for_another_folders_screen_open_and_leaves_it() {
    let f = started(vec![("first", empty(1)), ("second", empty(2))]).await;
    f.store.hold_open(&folder("first"));
    let first = {
        let sessions = f.sessions().clone();
        tokio::spawn(async move {
            sessions
                .open_screen(&folder("first"), open_progress())
                .await
        })
    };
    super::common::until("the first open reaches the store", || {
        f.store.opens(&folder("first")) == 1
    })
    .await;
    let second = {
        let sessions = f.sessions().clone();
        tokio::spawn(async move {
            sessions
                .open_screen(&folder("second"), open_progress())
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        f.store.opens(&folder("second")),
        0,
        "it waits for the first open"
    );
    f.store.release_open(&folder("first"));
    let first = first.await.unwrap().unwrap().at;
    let second = second.await.unwrap().unwrap().at;
    f.settled("first").await;
    assert_eq!(f.store.closes(&folder("first")), 1);
    let open = f.sessions().open_sessions();
    assert_eq!(open.len(), 1, "one folder has the screen");
    assert_eq!(open[0].at, second);
    assert!(f.t.app.read(&first).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn two_screen_opens_of_two_folders_at_once_leave_one_screen() {
    let f = started(vec![("first", empty(1)), ("second", empty(2))]).await;
    for round in 0..64 {
        // Both opens leave the barrier together, each on its own worker thread.
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let open = |name: &'static str| {
            let sessions = f.sessions().clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait();
                sessions.open_screen(&folder(name), open_progress()).await
            })
        };
        let (first, second) = (open("first"), open("second"));
        let first = first.await.unwrap().unwrap().at;
        let second = second.await.unwrap().unwrap().at;
        f.settled("first").await;
        f.settled("second").await;
        let screens: Vec<_> = f
            .sessions()
            .open_sessions()
            .into_iter()
            .filter(|s| s.role == SessionRole::Screen)
            .collect();
        assert_eq!(screens.len(), 1, "round {round}: {screens:?}");
        assert!(screens[0].at == first || screens[0].at == second);
        let (left, kept) = if screens[0].at == first {
            (&second, &first)
        } else {
            (&first, &second)
        };
        assert!(f.t.app.read(left).is_err(), "round {round}");
        assert!(f.t.app.read(kept).is_ok(), "round {round}");
        f.sessions().leave_screen(kept).unwrap();
        f.settled("first").await;
        f.settled("second").await;
        assert!(f.sessions().open_sessions().is_empty(), "round {round}");
    }
}

#[tokio::test]
async fn a_cancelled_screen_open_leaves_the_screen_to_another_open_of_that_folder() {
    let f = one_project().await;
    let project = folder("kr_ulb");
    f.store.hold_open(&project);
    let spawn = || {
        let sessions = f.sessions().clone();
        let project = project.clone();
        tokio::spawn(async move { sessions.open_screen(&project, open_progress()).await })
    };
    let first = spawn();
    super::common::until("the open reaches the store", || {
        f.store.opens(&project) == 1
    })
    .await;
    let second = spawn();
    // The second open joins the first's and waits for it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    f.store.release_open(&project);
    let at = second.await.unwrap().unwrap().at;
    f.settled("kr_ulb").await;
    let open = f.sessions().open_sessions();
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0].at, at);
    assert_eq!(open[0].role, SessionRole::Screen);
    assert!(f.t.app.read(&at).is_ok());
    assert_eq!(f.store.opens(&project), 1);
    assert_eq!(f.store.closes(&project), 0);
    // The screen is the person's: going Home closes it once.
    f.sessions().leave_screen(&at).unwrap();
    f.settled("kr_ulb").await;
    assert_eq!(f.store.closes(&project), 1);
}

#[tokio::test]
async fn a_cancelled_screen_open_with_no_other_caller_closes_the_project() {
    let f = one_project().await;
    let project = folder("kr_ulb");
    f.store.hold_open(&project);
    let only = {
        let sessions = f.sessions().clone();
        let project = project.clone();
        tokio::spawn(async move { sessions.open_screen(&project, open_progress()).await })
    };
    super::common::until("the open reaches the store", || {
        f.store.opens(&project) == 1
    })
    .await;
    only.abort();
    assert!(only.await.unwrap_err().is_cancelled());
    f.store.release_open(&project);
    super::common::until("the project closes", || f.store.closes(&project) == 1).await;
    f.settled("kr_ulb").await;
    assert!(f.sessions().open_sessions().is_empty());
    assert!(!f.sessions().busy());
}

/// D1, H22 part 1: lost work the store keeps at a look while the project is open (found at
/// focus or by the watcher) raises no notice of its own, only the file's write. The pages
/// hear of the new kept copy then, without a reload; a write that changes no list tells
/// nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lost_work_kept_at_a_look_reaches_the_pages_without_a_reload() {
    use tc_api::session::{KeptCopy, KeptWhy, OutsideEdits};
    let f = one_project().await;
    let at = f.open("kr_ulb").await;
    let mut sub = f.events();
    let lost = OutsideEdits {
        waiting: Vec::new(),
        kept: vec![KeptCopy {
            file: "ingredients/TIT.usfm".into(),
            copy: ".tc/external/20261025T120000Z-ingredients%2FTIT.usfm".into(),
            why: KeptWhy::Lost,
        }],
    };
    let written = tc_app::backend::StoreEvent::Materialized {
        paths: vec!["ingredients/TIT.usfm".into()],
    };
    f.store.set_outside(lost.clone());
    f.store.emit(&written);
    let events = events_of(&mut sub, &at, |e| {
        matches!(e, SessionEvent::OutsideEdits(_))
    })
    .await;
    assert_eq!(events.last(), Some(&SessionEvent::OutsideEdits(lost)));
    // The same lists again: no event, so the next one the pages hear is the mode's.
    f.store.emit(&written);
    f.store
        .emit(&tc_app::backend::StoreEvent::Mode(SessionMode::Writable));
    let events = events_of(&mut sub, &at, |e| matches!(e, SessionEvent::Mode(_))).await;
    assert_eq!(events, [SessionEvent::Mode(SessionMode::Writable)]);
}
