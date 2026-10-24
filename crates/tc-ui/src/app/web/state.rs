//! The server's state that events carry whole (APP-DESIGN 6.1, plan D3).
//!
//! `app/hello` seeds what it holds (the internet switch, the account, the startup state, the
//! open sessions, the saved language), events update all of it, and a `Resync` reads `hello`
//! again. Jobs come from events and are read again with `jobs/get` after a gap. The last
//! checkpoint outcome, the files a materialize cannot write and the aligner save states come
//! from events alone: no read restores them after a gap until the units that emit them make
//! them readable (D1, B6, section 5.4), so a `Resync` keeps them.
//!
//! These are signals of their own, not entries of the query cache, because they arrive whole in
//! frames and must exist before any screen holds them: a cache entry with no holder is
//! forgotten. Every signal is reference counted, so a late completion never meets a disposed
//! one.

use std::collections::{BTreeMap, HashMap};

use leptos::prelude::*;
use tc_api::account::AccountView;
use tc_api::align::AlignSave;
use tc_api::app::{FailedCheckpoint, HelloView, OpenSession};
use tc_api::event::{CheckpointOutcome, SessionMode, StartupState};
use tc_api::ids::{AlignerId, JobId, LocaleId, Scope, ServerInstance};
use tc_api::jobs::JobView;

use crate::app::effects::{forget_run, retain_open};

#[derive(Clone)]
pub struct ServerState {
    pub instance: ArcRwSignal<ServerInstance>,
    pub internet: ArcRwSignal<bool>,
    pub account: ArcRwSignal<AccountView>,
    pub startup: ArcRwSignal<StartupState>,
    pub sessions: ArcRwSignal<Vec<OpenSession>>,
    /// The app language the server has saved (`hello.prefs.appLocale`). The language dialog
    /// rolls back to it; the shown language can differ while a choice is only previewed.
    pub saved_locale: ArcRwSignal<LocaleId>,
    pub jobs: ArcRwSignal<BTreeMap<JobId, JobView>>,
    pub checkpoints: ArcRwSignal<HashMap<Scope, CheckpointOutcome>>,
    /// INV-A-FAILED-CHECKPOINT-IS-SHOWN-UNTIL-RETRIED (D1): the projects whose last checkpoint
    /// failed, from `hello` and kept by the checkpoint events, so Home shows "Checkpoint not
    /// saved" after a reload too.
    pub failed: ArcRwSignal<Vec<FailedCheckpoint>>,
    /// The files a materialize cannot write, by open ("can't update TIT.usfm…").
    pub blocked: ArcRwSignal<HashMap<Scope, Vec<String>>>,
    pub align_saves: ArcRwSignal<HashMap<AlignerId, AlignSave>>,
    /// The server started again since this page loaded: its opens are gone.
    pub restarted: ArcRwSignal<bool>,
}

impl ServerState {
    pub fn from_hello(hello: &HelloView) -> Self {
        Self {
            instance: ArcRwSignal::new(hello.instance),
            internet: ArcRwSignal::new(hello.internet),
            account: ArcRwSignal::new(hello.account.clone()),
            startup: ArcRwSignal::new(hello.startup.clone()),
            sessions: ArcRwSignal::new(hello.sessions.clone()),
            saved_locale: ArcRwSignal::new(hello.prefs.app_locale),
            jobs: ArcRwSignal::new(BTreeMap::new()),
            checkpoints: ArcRwSignal::new(failed_of_open(hello)),
            failed: ArcRwSignal::new(hello.checkpoints.clone()),
            blocked: ArcRwSignal::new(HashMap::new()),
            align_saves: ArcRwSignal::new(HashMap::new()),
            restarted: ArcRwSignal::new(false),
        }
    }

    /// Replaces what `hello` holds. Not the shown language, which may be a preview, and not
    /// what only events carry: those stay, except what belonged to an open that `hello` no
    /// longer lists.
    pub(super) fn reseed(&self, hello: &HelloView) {
        self.instance.set(hello.instance);
        self.internet.set(hello.internet);
        self.account.set(hello.account.clone());
        self.startup.set(hello.startup.clone());
        self.sessions.set(hello.sessions.clone());
        self.checkpoints.update(|map| {
            retain_open(map, &hello.sessions);
            map.extend(failed_of_open(hello));
        });
        self.failed.set(hello.checkpoints.clone());
        self.blocked.update(|map| retain_open(map, &hello.sessions));
        self.saved_locale.set(hello.prefs.app_locale);
    }

    /// The server started again: it is another run, which numbers its jobs, aligners and opens
    /// from the start. What an event filled and a number of the old run keys goes
    /// ([`forget_run`]); `hello` follows and replaces the rest.
    ///
    /// Every field is named, so that a new one cannot be left out without the compiler asking.
    pub(super) fn restarted(&self, run: ServerInstance) {
        let Self {
            instance,
            internet: _,
            account: _,
            startup: _,
            sessions: _,
            saved_locale: _,
            jobs,
            checkpoints,
            failed,
            blocked,
            align_saves,
            restarted,
        } = self;
        instance.set(run);
        restarted.set(true);
        // `hello` of the new run lists what failed in it.
        failed.set(Vec::new());
        jobs.update(|jobs| {
            checkpoints.update(|checkpoints| {
                blocked.update(|blocked| {
                    align_saves.update(|saves| forget_run(jobs, checkpoints, blocked, saves));
                });
            });
        });
    }

    /// A checkpoint of `at` ended: a failure stays listed until a later one of the project
    /// ends otherwise.
    pub(super) fn checkpoint_ended(&self, at: &Scope, outcome: &CheckpointOutcome) {
        self.failed.update(|failed| {
            failed.retain(|f| f.project != at.project);
            if let CheckpointOutcome::Failed { reason } = outcome {
                failed.push(FailedCheckpoint {
                    project: at.project.clone(),
                    reason: reason.clone(),
                });
            }
        });
    }

    pub(super) fn session_mode(&self, at: &Scope, mode: SessionMode) {
        self.sessions.update(|sessions| {
            for session in sessions.iter_mut().filter(|s| s.at == *at) {
                session.mode = mode;
            }
        });
    }

    /// The open ended: its session, its checkpoint and its blocked files go.
    pub(super) fn session_closed(&self, at: &Scope) {
        self.sessions
            .update(|sessions| sessions.retain(|s| s.at != *at));
        self.checkpoints.update(|map| {
            map.remove(at);
        });
        self.blocked.update(|map| {
            map.remove(at);
        });
    }
}

/// The open sessions whose project's last checkpoint failed, as the save indicator reads them.
fn failed_of_open(hello: &HelloView) -> HashMap<Scope, CheckpointOutcome> {
    hello
        .sessions
        .iter()
        .filter_map(|session| {
            let failed = hello
                .checkpoints
                .iter()
                .find(|f| f.project == session.at.project)?;
            Some((
                session.at.clone(),
                CheckpointOutcome::Failed {
                    reason: failed.reason.clone(),
                },
            ))
        })
        .collect()
}
