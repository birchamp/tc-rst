//! The operation reports (APP-DESIGN §4.1, §4.9): the last 50 operations that finished,
//! with what they found, kept for the Feedback dialog ("Ask for help") and the developer
//! Inspector. tC4 keeps the same record per store operation (`Report`,
//! `tC4:src/data/journal/runtime.ts:259-272`), and the dialog attaches the report of the
//! operation that was refused (`reportToAttach`, `tC4:src/data/feedback.ts:10-14`).
//!
//! The prefs writer keeps the log in `reports.json` (unit C1a, `prefs::read_reports`), so the
//! last 50 reports survive a relaunch: after a crash, "Ask for help" still attaches what went
//! wrong. tC4 keeps its reports in memory only (`store.lastReport`), a declared difference
//! (APP-DESIGN §4.9). A report holds facts the operation chose to record; a Door43 token is
//! never one of them (`tc_door43::Token` prints redacted).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tc_api::ids::{ReportId, WallMs};
use tokio::sync::Notify;

/// What kind of operation a report is about. tC4's ten, then the ones this app adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReportOp {
    Open,
    Checkpoint,
    Seed,
    Reconcile,
    PinComplete,
    Export,
    Import,
    Share,
    Sync,
    OpenDoor43,
    /// The UI's panic hook (`app/clientError`).
    ClientError,
    /// A bug in the server: a handler that panicked, or a reply that could not be written.
    Internal,
    /// An app-data file that could not be written or read (§4.9).
    PrefsWrite,
    /// Which projects folder this launch uses, and why (Q-4).
    ProjectsFolder,
    /// The kept Door43 sign-in could not be read at start.
    Keychain,
    /// The machine's resource store at start: what an interrupted install left and could not
    /// be removed (§4.9).
    Resources,
    /// A project's close that failed; the session is gone all the same.
    Close,
    /// A staging folder that an interrupted create or copy left, or one whose name does not say
    /// its work, which tC4 has none of and tc-store's start removed (STORAGE §6.4 N1). A
    /// removed import's or join's is `Import` or `OpenDoor43`, as tC4 reports them.
    StagingRemoved,
}

/// In `reports.json`: `{"type":"ok"}`, `{"type":"refused","value":{"code":…,"key":…}}` or
/// `{"type":"failed","value":{"reason":…}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReportOutcome {
    Ok,
    /// A refusal with a code and the catalog key of its sentence.
    Refused {
        code: Option<String>,
        key: String,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationReport {
    pub id: ReportId,
    pub op: ReportOp,
    pub outcome: ReportOutcome,
    pub facts: Map<String, Value>,
    pub started_at: WallMs,
    pub ended_at: WallMs,
}

struct LogState {
    /// The next report's id. Ids are never reused, not even after `clear` or a relaunch, so a
    /// report named by a dialog that is still open never turns into another one.
    next_id: u64,
    kept: VecDeque<OperationReport>,
    /// One more for every add and clear: what the prefs writer compares with the version
    /// `reports.json` holds.
    version: u64,
}

#[derive(Clone)]
pub struct ReportLog {
    inner: Arc<Mutex<LogState>>,
    /// Told after every add and clear; the prefs writer waits on it.
    changed: Arc<Notify>,
}

impl Default for ReportLog {
    fn default() -> Self {
        Self::new()
    }
}

impl ReportLog {
    pub const KEPT: usize = 50;

    pub fn new() -> Self {
        Self::with_kept(Vec::new(), 0)
    }

    /// A log that goes on from the reports an earlier launch kept (oldest first): the newest
    /// 50 are kept, and the next id is one more than the highest of them and of `last_id` (the
    /// highest id the file named, an entry this build cannot read included), so no id is given
    /// twice. Its version is 0, as a new log's is: what `reports.json` holds.
    pub fn with_kept(kept: Vec<OperationReport>, last_id: u64) -> Self {
        let next_id = kept
            .iter()
            .map(|r| r.id.get())
            .chain([last_id])
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut kept = VecDeque::from(kept);
        while kept.len() > Self::KEPT {
            kept.pop_front();
        }
        Self {
            inner: Arc::new(Mutex::new(LogState {
                next_id,
                kept,
                version: 0,
            })),
            changed: Arc::new(Notify::new()),
        }
    }

    fn state(&self) -> MutexGuard<'_, LogState> {
        // Adding a report cannot panic half way, so the state is whole after a poisoning.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn add(
        &self,
        op: ReportOp,
        outcome: ReportOutcome,
        facts: Map<String, Value>,
        started_at: WallMs,
        ended_at: WallMs,
    ) -> ReportId {
        let mut state = self.state();
        let id = ReportId::new(state.next_id);
        state.next_id += 1;
        state.kept.push_back(OperationReport {
            id,
            op,
            outcome,
            facts,
            started_at,
            ended_at,
        });
        while state.kept.len() > Self::KEPT {
            state.kept.pop_front();
        }
        state.version += 1;
        drop(state);
        self.changed.notify_one();
        id
    }

    pub fn get(&self, id: ReportId) -> Option<OperationReport> {
        self.state().kept.iter().find(|r| r.id == id).cloned()
    }

    /// tC4's `reportToAttach`: the report of the refused operation when "Ask for help" names
    /// one and it is still kept, else the newest report.
    pub fn to_attach(&self, failed: Option<ReportId>) -> Option<OperationReport> {
        let state = self.state();
        failed
            .and_then(|id| state.kept.iter().find(|r| r.id == id))
            .or_else(|| state.kept.back())
            .cloned()
    }

    /// The kept reports, oldest first.
    pub fn all(&self) -> Vec<OperationReport> {
        self.state().kept.iter().cloned().collect()
    }

    pub fn clear(&self) {
        let mut state = self.state();
        state.kept.clear();
        state.version += 1;
        drop(state);
        self.changed.notify_one();
    }

    /// One more for every add and clear since the log was made.
    pub fn version(&self) -> u64 {
        self.state().version
    }

    /// The kept reports, oldest first, with the version they are.
    pub fn versioned(&self) -> (u64, Vec<OperationReport>) {
        let state = self.state();
        (state.version, state.kept.iter().cloned().collect())
    }

    /// What is told after every add and clear (one stored wake for any number of changes).
    pub fn notifier(&self) -> Arc<Notify> {
        self.changed.clone()
    }
}

/// `facts` for `serde_json::json!({...})` object literals.
pub fn facts(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        // A caller writes a literal object; anything else is no facts.
        _ => Map::new(),
    }
}
