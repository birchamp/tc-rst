//! The `test` service (APP-DESIGN §5.8, AD35): hooks that exist only in test builds, behind
//! the feature `test-hooks`. A release build has none of them.
//!
//! - **Crash points** (`test/crash`) end the app the next time a write reaches one. The
//!   points live in the generic write path (`dispatch`), because only that path knows where
//!   "before the change is durable" and "before the reply" are, so every write has them.
//! - **Holds** (`test/hold`, `test/release`) stop the next operation that reaches a point
//!   until it is released. On the store (C1a) each point also holds tc-store's matching point
//!   (`save` its durability point, `bookRead`, `checkpoint`, `materialize`, `draftWrite`) until
//!   it is released, and `test/quiet` counts a store thread waiting there.
//! - **`test/saveVerse`** saves a verse through the real seam and store (unit C0), so the
//!   contract cases that run through tc-app go through dispatch, the session's reply cache and
//!   the store before C2's `editor/saveVerse` exists. C2 moves them there and deletes it.
//! - **Test fields** (`test/fieldStore`, `test/fieldLog`) are a write that goes through the
//!   real reply cache and records every text it stores, so the UI's save machine can be
//!   proved before any real save exists. They are durable (`test-hooks/fields.json`, written
//!   atomically before the reply), because the reload tests end the process with
//!   `test/crash` and then read what was stored. Each store rewrites that one small file, so
//!   its cost grows with the stores of that app: a property test gives each case a fresh
//!   app-data folder.
//! - **Test drafts** (`test/fieldDraft`) keep the newest draft of a test field in the same
//!   file, as `editor/draft` will keep a verse's in `.tc/drafts.json`, and change neither its
//!   value nor its stores. A draft waits at its hold point before it takes the lane, so a held
//!   draft never delays a store (R22); it takes the lane only for the write of the file that
//!   drafts and stores share.
//!
//! The fields' file is written as every app-data file is: by `tc_store::atomic_write` (C1a).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hmac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tc_api::endpoint::Kind;
use tc_api::ids::{ConflictRef, RecordRef, ViewSeq};
use tc_api::shown::{PersonLabel, Who};
use tc_api::test::{
    Crash, CrashPoint, Echo, EchoView, FieldDraft, FieldLog, FieldLogView, FieldStore, Hold,
    HoldPoint, Keychain, KeychainHolds, KeychainMode, KeychainView, LastEdit, MAX_ECHO, Quiet,
    QuietView, Release, Reset, SaveVerse, StoredDraft, StoredText, TokenBase,
};
use tc_api::write::{Ack, ConflictBrief, SavedValue, Stored, TokenUpdate};
use tc_api::{ALL, ApiError, NotSaved, Shown, ShownToken};
use tokio::sync::Notify;
use unicode_normalization::UnicodeNormalization;

use crate::account::{MemKeychain, MemKeychainMode};
use crate::app::{App, lock};
use crate::backend::StoreCommand;
use crate::dispatch::Handle;
use crate::replies::Outcome;
use crate::wire;

/// The exit code of a test crash. tc-server's `CRASH_EXIT_CODE` is the same value, and the
/// join of the two asserts it.
pub const CRASH_EXIT_CODE: i32 = 86;

/// Proof that an [`Ender`] ran. Only [`RecordingEnder`] returns one; [`ExitProcess`] never
/// returns.
#[derive(Debug)]
pub struct Ended(());

/// Ends the process at a crash point.
pub trait Ender: Send + Sync + 'static {
    fn end(&self, at: CrashPoint) -> Ended;
}

/// `std::process::exit(CRASH_EXIT_CODE)`: no flush and no checkpoint, like a kill.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExitProcess;

impl Ender for ExitProcess {
    fn end(&self, _at: CrashPoint) -> Ended {
        std::process::exit(CRASH_EXIT_CODE)
    }
}

/// For tests: records each point, and the app marks itself dead, so every later call
/// answers [`Answer::Ended`](crate::Answer::Ended) as a process that is gone would. The test
/// then starts a new app on the same app-data folder, as the harness restarts `tc4`.
#[derive(Debug, Default)]
pub struct RecordingEnder {
    ended: Mutex<Vec<CrashPoint>>,
}

impl RecordingEnder {
    pub fn ended(&self) -> Vec<CrashPoint> {
        lock(&self.ended).clone()
    }
}

impl Ender for RecordingEnder {
    fn end(&self, at: CrashPoint) -> Ended {
        lock(&self.ended).push(at);
        Ended(())
    }
}

/// The hooks that arm or release something. A crash never fires on them, and none can be
/// the `path` of an armed crash: a crash that fired on the request that armed it, or on its
/// own reset, would end every test at once.
const ARMING_HOOKS: [&str; 5] = [
    "test/crash",
    "test/hold",
    "test/release",
    "test/keychain",
    "test/reset",
];

fn is_arming_hook(path: &str) -> bool {
    ARMING_HOOKS.contains(&path)
}

pub(crate) struct TestHooks {
    /// Set when an ender fired: the app answers nothing and writes nothing after it (a real
    /// process would be gone, with every task it had).
    dead: AtomicBool,
    /// One armed crash; a new one replaces it.
    crash: Mutex<Option<Crash>>,
    holds: Holds,
    fields: TestFields,
    ender: Arc<dyn Ender>,
    /// The app's keychain, when the test set it (`test/keychain`).
    keychain: Option<Arc<MemKeychain>>,
}

impl TestHooks {
    pub(crate) fn new(
        app_data: &Path,
        ender: Arc<dyn Ender>,
        keychain: Option<Arc<MemKeychain>>,
        store: Option<Arc<tc_store::Hooks>>,
    ) -> Result<Self, String> {
        Ok(Self {
            dead: AtomicBool::new(false),
            crash: Mutex::new(None),
            holds: Holds {
                points: Mutex::default(),
                store,
                armed: Mutex::default(),
            },
            fields: TestFields::load(app_data)?,
            ender,
            keychain,
        })
    }

    fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    /// The end of the app: nothing is held any more, so a held operation finishes (as
    /// ended) instead of waiting for a release that will not come.
    fn die(&self, at: CrashPoint) {
        self.dead.store(true, Ordering::SeqCst);
        self.holds.release_all();
        // The proof only says the ender ran.
        let _ended = self.ender.end(at);
    }

    /// Takes the armed crash if it is for `at` and `path`.
    fn take_crash(&self, at: CrashPoint, path: &str) -> bool {
        if is_arming_hook(path) {
            return false;
        }
        let mut armed = lock(&self.crash);
        let fires = armed
            .as_ref()
            .is_some_and(|c| c.at == at && c.path.as_deref().is_none_or(|p| p == path));
        if fires {
            *armed = None;
        }
        fires
    }
}

impl App {
    fn hooks(&self) -> Result<&TestHooks, ApiError> {
        self.inner
            .hooks
            .as_ref()
            .ok_or_else(|| ApiError::BadRequest {
                field: "path".to_owned(),
                reason: "the test hooks are off".to_owned(),
            })
    }

    pub(crate) fn is_dead(&self) -> bool {
        self.inner.hooks.as_ref().is_some_and(TestHooks::is_dead)
    }

    /// Ends the app if a crash is armed for this write at `point`.
    pub(crate) fn crash_at(&self, point: CrashPoint, path: &str) -> Option<Outcome> {
        let hooks = self.inner.hooks.as_ref()?;
        hooks.take_crash(point, path).then(|| {
            hooks.die(point);
            Outcome::Ended
        })
    }

    /// Whether the server must end the process once it has sent this write's reply. The
    /// app stays alive: only the server knows when the body was sent.
    pub(crate) fn crash_after_reply(&self, path: &str) -> bool {
        self.inner
            .hooks
            .as_ref()
            .is_some_and(|hooks| hooks.take_crash(CrashPoint::AfterReply, path))
    }

    /// Where an operation of the app passes `at`: it waits there while the point is armed.
    pub(crate) async fn hold_at(&self, at: HoldPoint) {
        if let Some(hooks) = self.inner.hooks.as_ref() {
            hooks.holds.reach(at).await;
        }
    }

    /// The operations held now, for tests that wait until one has reached its hold.
    pub fn holding(&self) -> usize {
        self.inner.hooks.as_ref().map_or(0, |h| h.holds.holding())
    }
}

// --- Holds -------------------------------------------------------------------------------

enum HoldState {
    /// The next operation to reach the point waits.
    Armed(Arc<Notify>),
    /// An operation waits now.
    Holding(Arc<Notify>),
}

struct Holds {
    points: Mutex<HashMap<HoldPoint, HoldState>>,
    /// The store's own hold points, when the projects are on the store.
    store: Option<Arc<tc_store::Hooks>>,
    /// The store points armed now, each with how often a store thread had reached it held
    /// when it was armed: a thread waits there now when the count has grown since.
    armed: Mutex<HashMap<HoldPoint, u64>>,
}

/// tc-store's point for each of the app's (C1a); none for the open, which the app holds
/// before it asks the store (B3).
fn store_point(at: HoldPoint) -> Option<tc_store::Point> {
    use tc_store::Point;
    match at {
        HoldPoint::Save => Some(Point::BeforeDurablePending),
        HoldPoint::BookRead => Some(Point::BeforeBookRead),
        HoldPoint::Checkpoint => Some(Point::BeforeCheckpoint),
        HoldPoint::Materialize => Some(Point::BeforeMaterialize),
        HoldPoint::DraftWrite => Some(Point::BeforeDraftWrite),
        HoldPoint::Open => None,
    }
}

impl Holds {
    /// Arms a point. A point that holds an operation stays as it is: replacing it would
    /// leave that operation with nothing to release it.
    fn hold(&self, at: HoldPoint) {
        lock(&self.points)
            .entry(at)
            .or_insert_with(|| HoldState::Armed(Arc::new(Notify::new())));
        if let (Some(store), Some(point)) = (&self.store, store_point(at)) {
            lock(&self.armed)
                .entry(at)
                .or_insert_with(|| store.reached(point));
            store.hold(point);
        }
    }

    /// Where an operation passes a hold point: waits there if the point is armed.
    async fn reach(&self, at: HoldPoint) {
        let wait = {
            let mut points = lock(&self.points);
            match points.get(&at) {
                Some(HoldState::Armed(notify)) => {
                    let notify = Arc::clone(notify);
                    points.insert(at, HoldState::Holding(Arc::clone(&notify)));
                    Some(notify)
                }
                _ => None,
            }
        };
        if let Some(notify) = wait {
            // `notify_one` keeps a permit when the release comes before this is polled.
            notify.notified().await;
        }
    }

    fn release(&self, at: HoldPoint) {
        if let Some(HoldState::Holding(notify)) = lock(&self.points).remove(&at) {
            notify.notify_one();
        }
        if let (Some(store), Some(point)) = (&self.store, store_point(at)) {
            lock(&self.armed).remove(&at);
            store.release(point);
        }
    }

    fn release_all(&self) {
        let released: Vec<HoldState> = lock(&self.points).drain().map(|(_, state)| state).collect();
        for state in released {
            if let HoldState::Holding(notify) = state {
                notify.notify_one();
            }
        }
        if let Some(store) = &self.store {
            let armed: Vec<HoldPoint> = lock(&self.armed).drain().map(|(at, _)| at).collect();
            for point in armed.into_iter().filter_map(store_point) {
                store.release(point);
            }
        }
    }

    fn holding(&self) -> usize {
        let app = lock(&self.points)
            .values()
            .filter(|state| matches!(state, HoldState::Holding(_)))
            .count();
        let store = self.store.as_ref().map_or(0, |store| {
            lock(&self.armed)
                .iter()
                .filter(|(at, then)| {
                    store_point(**at).is_some_and(|point| store.reached(point) > **then)
                })
                .count()
        });
        app + store
    }
}

// --- The atomic file write ---------------------------------------------------------------

/// Writes `dir/name` through `tc_store::atomic_write`, so a reader sees the old bytes or the
/// new ones, never a part. Bytes in place whose folder flush failed count as written: the test
/// fields need the bytes there, not the flush.
fn write_file_atomically(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let fs = tc_store::StdFs;
    tc_store::atomic_create_dirs(&fs, dir).map_err(|e| e.to_string())?;
    match tc_store::atomic_write(&fs, dir, name, bytes, tc_store::SyncDir::Now) {
        Ok(()) | Err(tc_store::WriteError::WrittenNotDurable(_)) => Ok(()),
        Err(error @ tc_store::WriteError::NotWritten(_)) => Err(error.to_string()),
    }
}

// --- Test fields -------------------------------------------------------------------------

const FIELDS_FILE: &str = "fields.json";
const FIELDS_SCHEMA: u32 = 1;
const MAX_FIELD_NAME: usize = 200;
const TAG_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FieldsFile {
    schema_version: u32,
    /// 64 hex digits: the key of the tokens, kept so they verify after a restart, as the
    /// store's installation secret does.
    key: String,
    fields: BTreeMap<String, Vec<StoredText>>,
    /// The newest draft of each field and how many came. A file written before drafts
    /// existed has none.
    #[serde(default)]
    drafts: BTreeMap<String, KeptDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeptDraft {
    draft: StoredDraft,
    count: u32,
}

/// What a token says: the field and the version of it that was shown.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenPayload {
    field: String,
    version: u64,
}

pub(crate) struct TestFields {
    dir: PathBuf,
    key: hmac::Key,
    /// Stores go one at a time, as the actor runs commands. Held across the hold point and the
    /// durable write.
    lane: tokio::sync::Mutex<()>,
    /// What `test/fieldLog` reads. Changed only by the holder of the lane, so a read never
    /// waits behind a held store.
    state: Mutex<FieldsFile>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

impl TestFields {
    /// Reads `<app_data>/test-hooks/fields.json`, or makes it with a new key. A file the
    /// hooks cannot read stops the start: it is never overwritten.
    fn load(app_data: &Path) -> Result<Self, String> {
        let dir = app_data.join("test-hooks");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(FIELDS_FILE);
        let file = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<FieldsFile>(&bytes)
                .map_err(|e| format!("{}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut key = [0u8; 32];
                SystemRandom::new()
                    .fill(&mut key)
                    .map_err(|_| "this computer gave no random bytes".to_owned())?;
                let file = FieldsFile {
                    schema_version: FIELDS_SCHEMA,
                    key: hex(&key),
                    fields: BTreeMap::new(),
                    drafts: BTreeMap::new(),
                };
                let bytes = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
                write_file_atomically(&dir, FIELDS_FILE, &bytes)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                file
            }
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        if file.schema_version != FIELDS_SCHEMA {
            return Err(format!(
                "{}: schema version {} is not {FIELDS_SCHEMA}",
                path.display(),
                file.schema_version
            ));
        }
        let key = unhex(&file.key)
            .filter(|key| key.len() == 32)
            .ok_or_else(|| format!("{}: the key is not 64 hex digits", path.display()))?;
        Ok(Self {
            dir,
            key: hmac::Key::new(hmac::HMAC_SHA256, &key),
            lane: tokio::sync::Mutex::new(()),
            state: Mutex::new(file),
        })
    }

    /// The token of version `version` of `field`: the payload and its tag, base64url.
    fn mint(&self, field: &str, version: u64) -> Result<ShownToken, String> {
        let mut bytes = serde_json::to_vec(&json!({"field": field, "version": version}))
            .map_err(|e| e.to_string())?;
        let tag = hmac::sign(&self.key, &bytes);
        bytes.extend_from_slice(tag.as_ref());
        ShownToken::from_body(&URL_SAFE_NO_PAD.encode(bytes)).map_err(|e| e.to_string())
    }

    /// The version a token names, if it is one of this field's own.
    fn read_token(&self, field: &str, token: &ShownToken) -> Option<u64> {
        let bytes = URL_SAFE_NO_PAD.decode(token.body()).ok()?;
        let split = bytes.len().checked_sub(TAG_LEN)?;
        let (payload, tag) = bytes.split_at(split);
        // `verify` needs the whole tag and compares in constant time.
        hmac::verify(&self.key, payload, tag).ok()?;
        let payload: TokenPayload = serde_json::from_slice(payload).ok()?;
        (payload.field == field).then_some(payload.version)
    }

    /// The text of a version: the text stored to make it, and "" before the first store.
    fn value_of(stores: &[StoredText], version: u64) -> String {
        usize::try_from(version)
            .ok()
            .and_then(|v| v.checked_sub(1))
            .and_then(|i| stores.get(i))
            .map(|stored| stored.text.clone())
            .unwrap_or_default()
    }

    fn record_of(field: &str) -> Result<RecordRef, String> {
        let text = serde_json::to_string(&json!({"field": field, "kind": "testField"}))
            .map_err(|e| e.to_string())?;
        RecordRef::parse(&text).map_err(|e| e.to_string())
    }

    fn conflict_of(field: &str, version: u64) -> Result<ConflictRef, String> {
        let mut context = digest::Context::new(&digest::SHA256);
        context.update(field.as_bytes());
        context.update(&[0]);
        context.update(version.to_string().as_bytes());
        ConflictRef::parse(&hex(context.finish().as_ref())).map_err(|e| e.to_string())
    }

    fn check_name(name: &str) -> Result<(), ApiError> {
        if name.is_empty() || name.len() > MAX_FIELD_NAME || name.chars().any(char::is_control) {
            return Err(ApiError::BadRequest {
                field: "field".to_owned(),
                reason: format!("a name of 1 to {MAX_FIELD_NAME} bytes with no control character"),
            });
        }
        Ok(())
    }

    /// Writes `file` atomically on the blocking pool: an fsync on a runtime worker would stall
    /// every other request.
    async fn write_file(&self, file: &FieldsFile) -> Result<(), NotSaved> {
        let bytes = serde_json::to_vec_pretty(file).map_err(|e| NotSaved::Disk(e.to_string()))?;
        let dir = self.dir.clone();
        match tokio::task::spawn_blocking(move || write_file_atomically(&dir, FIELDS_FILE, &bytes))
            .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(NotSaved::Disk(e)),
            Err(stopped) => Err(NotSaved::Disk(crate::jobs::stop_reason(stopped))),
        }
    }

    async fn store(&self, app: &App, hooks: &TestHooks, req: FieldStore) -> Result<Ack, ApiError> {
        let internal = |reason: String| {
            app.internal(
                "a test field could not be stored",
                crate::reports::facts(json!({"reason": reason})),
            )
        };
        let name = req.field.as_str();
        Self::check_name(name)?;
        let named = self
            .read_token(name, &req.shown)
            .ok_or(ApiError::NotSaved(NotSaved::BadToken))?;

        let _lane = self.lane.lock().await;
        hooks.holds.reach(HoldPoint::Save).await;
        if hooks.is_dead() {
            // Never sent: `run_write` answers `Ended` for a dead app, as a gone process does.
            return Err(ApiError::NotSaved(NotSaved::Disk("tc4 ended".to_owned())));
        }

        let (stores, version) = {
            let state = lock(&self.state);
            let stores = state.fields.get(name).cloned().unwrap_or_default();
            let version = stores.len() as u64;
            (stores, version)
        };
        if named > version {
            return Err(ApiError::NotSaved(NotSaved::BadToken));
        }
        let record = Self::record_of(name).map_err(internal)?;
        let text: String = req.text.nfc().collect();

        let named_value = Self::value_of(&stores, named);
        if text == named_value {
            // Nothing changes. The field stays clean on the token it already had: answering
            // with the newest token would make its next save replace a newer version
            // without a conflict (STORAGE:1446-1447).
            return Ok(Ack {
                view: ViewSeq::new(version),
                stored: Stored::Unchanged,
                tokens: vec![TokenUpdate {
                    record: record.clone(),
                    token: self.mint(name, named).map_err(internal)?,
                }],
                new_conflicts: Vec::new(),
                saved: vec![SavedValue {
                    record,
                    text: named_value,
                }],
                rows: Vec::new(),
                disk_warning: None,
            });
        }

        let stale = named < version;
        let mut file = lock(&self.state).clone();
        file.fields
            .entry(name.to_owned())
            .or_default()
            .push(StoredText {
                request: req.request,
                text: text.clone(),
                base: if stale {
                    TokenBase::Stale
                } else {
                    TokenBase::Current
                },
            });
        self.write_file(&file).await.map_err(ApiError::NotSaved)?;
        *lock(&self.state) = file;

        let new_conflicts = if stale {
            vec![ConflictBrief {
                conflict: Self::conflict_of(name, version + 1).map_err(internal)?,
                record: record.clone(),
                by: PersonLabel {
                    who: Who::Mine,
                    computer: None,
                },
            }]
        } else {
            Vec::new()
        };
        Ok(Ack {
            view: ViewSeq::new(version + 1),
            stored: Stored::Written,
            tokens: vec![TokenUpdate {
                record: record.clone(),
                token: self.mint(name, version + 1).map_err(internal)?,
            }],
            new_conflicts,
            saved: vec![SavedValue { record, text }],
            rows: Vec::new(),
            disk_warning: None,
        })
    }

    /// Keeps `req` as the field's newest draft. The value and the stores do not change.
    async fn draft(&self, hooks: &TestHooks, req: FieldDraft) -> Result<(), ApiError> {
        let name = req.field.as_str();
        Self::check_name(name)?;
        let named = self
            .read_token(name, &req.shown)
            .ok_or(ApiError::NotSaved(NotSaved::BadToken))?;
        // Before the lane: a draft held here never holds a store back.
        hooks.holds.reach(HoldPoint::DraftWrite).await;
        let _lane = self.lane.lock().await;
        if hooks.is_dead() {
            return Err(ApiError::NotSaved(NotSaved::Disk("tc4 ended".to_owned())));
        }
        let mut file = lock(&self.state).clone();
        let version = file.fields.get(name).map_or(0, Vec::len) as u64;
        if named > version {
            return Err(ApiError::NotSaved(NotSaved::BadToken));
        }
        let draft = StoredDraft {
            request: req.request,
            text: req.text.nfc().collect(),
            base: if named < version {
                TokenBase::Stale
            } else {
                TokenBase::Current
            },
        };
        let count = file.drafts.get(name).map_or(0, |kept| kept.count);
        file.drafts.insert(
            name.to_owned(),
            KeptDraft {
                draft,
                count: count.saturating_add(1),
            },
        );
        self.write_file(&file).await.map_err(ApiError::NotSaved)?;
        *lock(&self.state) = file;
        Ok(())
    }

    fn log(&self, app: &App, field: &str) -> Result<FieldLogView, ApiError> {
        let (stores, kept) = {
            let state = lock(&self.state);
            (
                state.fields.get(field).cloned().unwrap_or_default(),
                state.drafts.get(field).cloned(),
            )
        };
        let version = stores.len() as u64;
        let token = self.mint(field, version).map_err(|reason| {
            app.internal(
                "a test field token could not be made",
                crate::reports::facts(json!({"reason": reason})),
            )
        })?;
        Ok(FieldLogView {
            current: Shown {
                value: Self::value_of(&stores, version),
                token,
                marker: None,
            },
            stores,
            draft: kept.as_ref().map(|kept| kept.draft.clone()),
            drafts: kept.map_or(0, |kept| kept.count),
        })
    }
}

// --- Handlers ----------------------------------------------------------------------------

impl Handle<Quiet> for App {
    /// Whether anything a capture would still see change is in flight. Pending preference
    /// timers do not count: they change nothing a screen shows.
    async fn handle(&self, _req: Quiet) -> Result<QuietView, ApiError> {
        let hooks = self.hooks()?;
        // `test/quiet` does not count itself (`App::call`), so any call counted is another.
        let busy = self.inner.calls.load(Ordering::SeqCst) > 0
            || self.inner.replies.running() > 0
            || self.inner.jobs.live() > 0
            || self.inner.sessions.busy()
            || hooks.holds.holding() > 0;
        Ok(QuietView { busy })
    }
}

impl Handle<Echo> for App {
    /// A reply of the size asked for, the same one tc-server's stand-in gives, so the loopback
    /// spike measures the same bytes before and after the join.
    async fn handle(&self, req: Echo) -> Result<EchoView, ApiError> {
        self.hooks()?;
        if req.size > MAX_ECHO {
            return Err(ApiError::BadRequest {
                field: "size".to_owned(),
                reason: format!("at most {MAX_ECHO}"),
            });
        }
        Ok(EchoView::of_size(req.size))
    }
}

impl Handle<Crash> for App {
    async fn handle(&self, req: Crash) -> Result<(), ApiError> {
        let hooks = self.hooks()?;
        // A crash that can never fire would make a crash test pass by doing nothing.
        if req.at == CrashPoint::DuringDraftWrite {
            return Err(ApiError::BadRequest {
                field: "at".to_owned(),
                reason: "no write reaches duringDraftWrite before drafts exist".to_owned(),
            });
        }
        if let Some(path) = &req.path {
            let writes = ALL.iter().any(|info| {
                info.path == path && matches!(info.kind, Kind::Command | Kind::StartJob)
            });
            if !writes || is_arming_hook(path) {
                return Err(ApiError::BadRequest {
                    field: "path".to_owned(),
                    reason: format!("{path} is not a write a crash can end"),
                });
            }
        }
        *lock(&hooks.crash) = Some(req);
        Ok(())
    }
}

impl Handle<Hold> for App {
    async fn handle(&self, req: Hold) -> Result<(), ApiError> {
        self.hooks()?.holds.hold(req.at);
        Ok(())
    }
}

impl Handle<Release> for App {
    async fn handle(&self, req: Release) -> Result<(), ApiError> {
        self.hooks()?.holds.release(req.at);
        Ok(())
    }
}

impl Handle<FieldStore> for App {
    async fn handle(&self, req: FieldStore) -> Result<Ack, ApiError> {
        let hooks = self.hooks()?;
        hooks.fields.store(self, hooks, req).await
    }
}

impl Handle<FieldDraft> for App {
    async fn handle(&self, req: FieldDraft) -> Result<(), ApiError> {
        let hooks = self.hooks()?;
        hooks.fields.draft(hooks, req).await
    }
}

impl Handle<SaveVerse> for App {
    /// The words over the verse the token shows, through the screen's session and the store,
    /// announced as this request's own save (AD37). No lock is held across the wait: the
    /// write's lease keeps the session open.
    async fn handle(&self, req: SaveVerse) -> Result<Ack, ApiError> {
        self.hooks()?;
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        let shown = wire::token::edit_token(handle.as_ref(), &req.shown)?;
        let me = handle.facts().device;
        let command =
            StoreCommand::save_verse(shown.clone(), req.words.clone()).requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => {
                wire::write::ack(&reply, &session.read(), &shown, &req.words, me).map_err(|e| {
                    self.internal(
                        "a save's reply could not be written",
                        crate::reports::facts(json!({"reason": e.to_string()})),
                    )
                })
            }
            Err(error) => Err(match wire::error::command_error(error) {
                Ok(api) => api,
                Err(unworded) => self.internal(
                    "the store refused a save no screen has words for yet",
                    crate::reports::facts(json!({"reason": unworded.0})),
                ),
            }),
        }
    }
}

impl Handle<FieldLog> for App {
    async fn handle(&self, req: FieldLog) -> Result<FieldLogView, ApiError> {
        self.hooks()?.fields.log(self, &req.field)
    }
}

fn mode_of(mode: KeychainMode) -> MemKeychainMode {
    match mode {
        KeychainMode::Working => MemKeychainMode::Working,
        KeychainMode::None => MemKeychainMode::NoKeychain,
        KeychainMode::FailingKeep => MemKeychainMode::FailingKeep,
        KeychainMode::FailingForget => MemKeychainMode::FailingForget,
        KeychainMode::Unreadable => MemKeychainMode::Unreadable,
    }
}

fn mode_view(mode: MemKeychainMode) -> KeychainMode {
    match mode {
        MemKeychainMode::Working => KeychainMode::Working,
        MemKeychainMode::NoKeychain => KeychainMode::None,
        MemKeychainMode::FailingKeep => KeychainMode::FailingKeep,
        MemKeychainMode::FailingForget => KeychainMode::FailingForget,
        MemKeychainMode::Unreadable => KeychainMode::Unreadable,
    }
}

impl Handle<Keychain> for App {
    /// The test keychain's mode and contents; with `relaunch`, the account as a launch finds
    /// it, with the internet off as a launch has it. The journeys use it where tC4's use a fake
    /// keychain and a reload.
    async fn handle(&self, req: Keychain) -> Result<KeychainView, ApiError> {
        let hooks = self.hooks()?;
        let Some(keychain) = hooks.keychain.as_ref() else {
            return Err(ApiError::BadRequest {
                field: "keychain".to_owned(),
                reason: "this app has no test keychain".to_owned(),
            });
        };
        if let Some(mode) = req.mode {
            keychain.set_mode(mode_of(mode));
        }
        match req.holds {
            Some(KeychainHolds::Nothing) => keychain.clear(),
            Some(KeychainHolds::Token(text)) => {
                let token = tc_door43::Token::new(text).map_err(|e| ApiError::BadRequest {
                    field: "holds".to_owned(),
                    reason: e.to_string(),
                })?;
                keychain.seed(token);
            }
            None => {}
        }
        if req.relaunch {
            self.relaunch_account().await;
            self.set_internet(false);
        }
        let calls = keychain.calls();
        Ok(KeychainView {
            mode: mode_view(keychain.mode()),
            holds: keychain.holds(),
            keeps: calls.keeps,
            reads: calls.reads,
            forgets: calls.forgets,
        })
    }
}

impl Handle<Reset> for App {
    /// The parity harness's `home()`: every session closed, the projects and the preferences
    /// and reports back to the seed, the crash disarmed, every hold released, the kept
    /// replies forgotten (a write still running is
    /// kept, so its retry waits for it), the account as a launch finds it with the test
    /// keychain working and empty (A8), and the internet off as a launch finds it (tC4's
    /// switch is page state that a reload turns off). The test fields are kept: tests name
    /// their own. The reset's own reply is not kept either (`dispatch::kept_reply`), so the
    /// next scenario's reset may carry the same id and still run.
    async fn handle(&self, _req: Reset) -> Result<(), ApiError> {
        let hooks = self.hooks()?;
        *lock(&hooks.crash) = None;
        // Holds go first: a held operation could keep a session from closing.
        hooks.holds.release_all();
        // Every session closes after its leave checkpoint, then every project goes back to
        // its seed. Epochs go on counting, so a page from before the reset is stale.
        self.inner.sessions.close_all().await;
        // On the store a reset writes projects again and flushes them: the blocking pool's.
        let store = self.inner.sessions.store().clone();
        let reset = tokio::task::spawn_blocking(move || store.reset())
            .await
            .unwrap_or_else(|e| Err(crate::backend::StoreError::Other(e.to_string())));
        if let Err(error) = reset {
            return Err(self.internal(
                "the projects could not be reset",
                crate::reports::facts(serde_json::json!({"reason": error.to_string()})),
            ));
        }
        // A launch computes every Home summary again.
        self.inner.home.reset();
        // A launch starts with no kept replies, so a request id may be used again.
        self.inner.replies.forget_settled();
        // The account as a launch finds it, on a computer whose keychain works and holds
        // nothing, so one captured screen's sign-in cannot reach the next.
        if let Some(keychain) = &hooks.keychain {
            keychain.reset();
        }
        self.relaunch_account().await;
        // The log after anything that reports, and before the prefs reset, which writes
        // `reports.json` as the log then holds it.
        self.inner.reports.clear();
        self.inner.prefs.reset().await;
        self.set_internet(false);
        Ok(())
    }
}

impl Handle<LastEdit> for App {
    /// Records a Resume record and the project's last use through the calls a save makes
    /// (C2), waits until they are written, then tells Home the projects changed (B2 D12).
    async fn handle(&self, req: LastEdit) -> Result<(), ApiError> {
        self.hooks()?;
        let folder = req.project.clone();
        let entry = self
            .inner
            .sessions
            .store()
            .list()
            .ok()
            .and_then(|listing| listing.projects.into_iter().find(|e| e.folder == folder))
            .ok_or_else(|| ApiError::BadRequest {
                field: "project".to_owned(),
                reason: "no such project".to_owned(),
            })?;
        let unit = match req.place {
            tc_api::home::ResumePlace::Book {
                book,
                chapter,
                verse,
            } => crate::prefs::EditUnit::Book {
                book,
                chapter,
                verse,
            },
            tc_api::home::ResumePlace::Story { story, frame } => {
                crate::prefs::EditUnit::Story { story, frame }
            }
        };
        let prefs = &self.inner.prefs;
        prefs.record_last_used(folder.clone(), req.at);
        prefs.record_last_edit(
            wire::scope::project_id(entry.id),
            crate::prefs::LastEdit {
                project: folder,
                unit,
                mode: req.mode,
                tool: None,
                snippet: req.snippet,
                at: req.at,
            },
        );
        prefs.flush().await;
        self.inner
            .bus
            .publish(tc_api::event::Event::ProjectsChanged);
        Ok(())
    }
}
