//! Drafts (`docs/STORAGE.md` §5.14): unsaved editor text in `.tc/drafts.json`, written with
//! `atomic_write` at most once a second while text is unsaved, by a thread of its own beside
//! the actor, so a draft write never delays a save (APP-DESIGN R22). A draft is best effort:
//! after a crash the file holds the old or the new list (crash row D1).
//!
//! At open (§6.1 O12) a draft whose text differs from the value its token stands for, and
//! from what the record holds now, is offered. *Keep* saves it with its tokens
//! (`Command::KeepDraft`), so a record that changed meanwhile becomes a conflict, never a
//! revert. A draft whose token this installation cannot verify any more (a new installation
//! secret) is offered as text, and *Keep* writes it with no parents (R20).

use crate::atomic::{SyncDir, atomic_create_dirs, atomic_write};
use crate::clock::Clock;
use crate::counters::Counters;
use crate::hooks::{Hooks, Point};
use crate::layout::{Folder, TC};
use crate::vfs::Vfs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tc_journal::command::{Draft, DraftField};
use tc_journal::fold::FoldState;
use tc_journal::record::{CheckValue, Record, VerseValue};
use tc_journal::{BookId, RecordKey, ShownToken, TokenKey, WallTime};

/// At most one write a second.
const EVERY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    field: serde_json::Value,
    shown: Vec<String>,
    text: String,
    at: i64,
}

/// A draft offered at open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedDraft {
    pub id: u64,
    pub draft: Draft,
    /// Every token verifies under this installation's key; otherwise *Keep* writes the text
    /// with no parents (R20).
    pub verified: bool,
    pub at: WallTime,
}

fn field_json(field: &DraftField) -> serde_json::Value {
    use serde_json::json;
    match field {
        DraftField::Words => json!("words"),
        DraftField::Layout => json!("layout"),
        DraftField::Section { book } => json!({ "section": { "book": book.code() } }),
        DraftField::Frame => json!("frame"),
        DraftField::StoryRef => json!("storyRef"),
        DraftField::CheckComment => json!("checkComment"),
        DraftField::Alignment => json!("alignment"),
        DraftField::UserComment { target } => {
            let target = tc_journal::canon::encode_comment_target(target).to_canonical_string();
            let target: serde_json::Value =
                serde_json::from_str(&target).unwrap_or(serde_json::Value::Null);
            json!({ "userComment": { "target": target } })
        }
    }
}

fn field_of(json: &serde_json::Value) -> Option<DraftField> {
    if let Some(name) = json.as_str() {
        return Some(match name {
            "words" => DraftField::Words,
            "layout" => DraftField::Layout,
            "frame" => DraftField::Frame,
            "storyRef" => DraftField::StoryRef,
            "checkComment" => DraftField::CheckComment,
            "alignment" => DraftField::Alignment,
            _ => return None,
        });
    }
    if let Some(book) = json.pointer("/section/book").and_then(|b| b.as_str()) {
        return Some(DraftField::Section {
            book: book.parse::<BookId>().ok()?,
        });
    }
    let target = json.pointer("/userComment/target")?;
    let target = tc_journal::json::parse(target.to_string().as_bytes()).ok()?;
    Some(DraftField::UserComment {
        target: tc_journal::canon::decode_comment_target(&target)?,
    })
}

/// The identity of a draft: one per field and record (a section's by its first record, a
/// new comment's by its target).
pub(crate) fn draft_key(field: &DraftField, keys: &[RecordKey]) -> String {
    let mut out = field_json(field).to_string();
    for key in keys.iter().take(1) {
        out.push('|');
        out.push_str(
            &tc_journal::canon::encode_key(key)
                .map(|j| j.to_canonical_string())
                .unwrap_or_default(),
        );
    }
    out
}

fn key_of(draft: &Draft) -> String {
    let keys: Vec<RecordKey> = draft.shown.iter().map(|t| t.key().clone()).collect();
    draft_key(&draft.field, &keys)
}

enum Msg {
    Put(Box<Draft>, WallTime),
    Remove(Vec<String>),
    Flush(mpsc::Sender<()>),
    Stop,
}

/// The draft writer of one open project (see the module documentation).
pub(crate) struct DraftWriter {
    tx: Mutex<Option<Sender<Msg>>>,
    offered: Arc<Mutex<Vec<(OfferedDraft, String)>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct Writer {
    vfs: Arc<dyn Vfs>,
    folder: Folder,
    entries: BTreeMap<String, Entry>,
    dirty: bool,
    last: Option<Instant>,
    hooks: Arc<Hooks>,
    counters: Arc<Counters>,
}

impl Writer {
    fn write(&mut self) {
        if !self.dirty {
            return;
        }
        self.hooks.wait(Point::BeforeDraftWrite);
        let list: Vec<&Entry> = self.entries.values().collect();
        let bytes = serde_json::to_vec(&list).unwrap_or_default();
        let dir = self.folder.path(TC);
        let written = atomic_create_dirs(&*self.vfs, &dir).is_ok()
            && atomic_write(&*self.vfs, &dir, "drafts.json", &bytes, SyncDir::Now).is_ok();
        if written {
            self.counters.drafts_written.fetch_add(1, Ordering::Relaxed);
        }
        // Best effort: a failed write keeps the list in memory for the next one.
        self.dirty = !written;
        self.last = Some(Instant::now());
    }
}

/// Reads `.tc/drafts.json`.
fn read_entries(vfs: &dyn Vfs, folder: &Folder) -> Vec<Entry> {
    vfs.read(&folder.path(crate::layout::DRAFTS))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<Entry>>(&bytes).ok())
        .unwrap_or_default()
}

/// The text a record holds now in the field of a draft.
fn field_text(field: &DraftField, record: Option<&Record>) -> Option<String> {
    match (field, record?) {
        (
            DraftField::Words,
            Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            },
        ) => Some(words.clone()),
        (
            DraftField::Layout,
            Record::Verse {
                value: VerseValue::Text { layout, .. },
                ..
            },
        ) => Some(layout.clone()),
        (DraftField::Frame, Record::Frame { value, .. }) => Some(value.clone()),
        (DraftField::StoryRef, Record::StoryRef { value, .. }) => {
            Some(value.clone().unwrap_or_default())
        }
        (
            DraftField::CheckComment,
            Record::Check {
                value: CheckValue::Decision(d),
                ..
            },
        ) => Some(d.comment.clone().unwrap_or_default()),
        (DraftField::UserComment { .. }, Record::Comment { value, .. }) => Some(value.text.clone()),
        (DraftField::Alignment, Record::Alignment { value, .. }) => Some(
            tc_journal::command::AlignmentEdit {
                alignments: value.alignments.clone(),
                word_bank: value.word_bank.clone(),
                done: value.done,
            }
            .to_json(),
        ),
        _ => None,
    }
}

/// The drafts of the last session to offer (O12): those whose text differs from the value
/// their token stands for and from what the record holds now. The others are dropped.
fn to_offer(
    entries: Vec<Entry>,
    fold: &FoldState,
    signing: &TokenKey,
) -> Vec<(Draft, bool, WallTime, String)> {
    let mut out = Vec::new();
    for entry in entries {
        let Some(field) = field_of(&entry.field) else {
            continue;
        };
        let mut verified = true;
        let mut shown = Vec::new();
        for text in &entry.shown {
            match ShownToken::parse(signing, text) {
                Ok(t) => shown.push(t),
                Err(_) => match ShownToken::parse_unverified(text) {
                    Ok(t) => {
                        verified = false;
                        shown.push(t);
                    }
                    Err(_) => continue,
                },
            }
        }
        let draft = Draft {
            field: field.clone(),
            shown,
            text: entry.text.clone(),
        };
        let key = key_of(&draft);
        let offer = match (&field, draft.shown.first()) {
            (DraftField::Section { .. }, _) => true,
            (DraftField::UserComment { .. }, None) => true,
            (_, Some(token)) => {
                let then = token
                    .heads()
                    .first()
                    .and_then(|h| fold.body(h))
                    .map(|b| b.record);
                let now = fold.projected(token.key());
                let differs = |r: Option<&Record>| {
                    field_text(&field, r).as_deref() != Some(entry.text.as_str())
                };
                differs(then.as_ref()) && differs(now.as_ref())
            }
            (_, None) => false,
        };
        if offer {
            out.push((draft, verified, WallTime(entry.at), key));
        }
    }
    out
}

impl DraftWriter {
    /// Starts the writer of a writable project: offers the last session's drafts and keeps
    /// the ones it offers in the file until they are kept or discarded.
    pub(crate) fn start(
        vfs: Arc<dyn Vfs>,
        folder: Folder,
        fold: &FoldState,
        signing: &TokenKey,
        hooks: Arc<Hooks>,
        counters: Arc<Counters>,
        read_only: bool,
    ) -> Self {
        // A read-only open offers nothing (§5.14): keeping a draft would write, and the
        // computer that holds the project may still be writing the same drafts.
        let entries = if read_only {
            Vec::new()
        } else {
            read_entries(&*vfs, &folder)
        };
        let offered = to_offer(entries.clone(), fold, signing);
        let keep: BTreeMap<String, Entry> = entries
            .into_iter()
            .filter_map(|e| {
                let field = field_of(&e.field)?;
                let shown: Vec<RecordKey> = e
                    .shown
                    .iter()
                    .filter_map(|t| ShownToken::parse_unverified(t).ok())
                    .map(|t| t.key().clone())
                    .collect();
                let key = draft_key(&field, &shown);
                offered.iter().any(|o| o.3 == key).then_some((key, e))
            })
            .collect();
        let next_id = AtomicU64::new(1);
        let offered: Vec<(OfferedDraft, String)> = offered
            .into_iter()
            .map(|(draft, verified, at, key)| {
                (
                    OfferedDraft {
                        id: next_id.fetch_add(1, Ordering::Relaxed),
                        draft,
                        verified,
                        at,
                    },
                    key,
                )
            })
            .collect();
        let writer = DraftWriter {
            tx: Mutex::new(None),
            offered: Arc::new(Mutex::new(offered)),
            thread: Mutex::new(None),
        };
        if read_only {
            return writer;
        }
        let (tx, rx) = mpsc::channel::<Msg>();
        let mut state = Writer {
            vfs,
            folder,
            entries: keep,
            dirty: false,
            last: None,
            hooks,
            counters,
        };
        let thread = std::thread::Builder::new()
            .name("tc-store drafts".into())
            .spawn(move || {
                loop {
                    let wait = match (state.dirty, state.last) {
                        (true, Some(last)) => EVERY.saturating_sub(last.elapsed()),
                        (true, None) => Duration::ZERO,
                        (false, _) => Duration::from_secs(3600),
                    };
                    match rx.recv_timeout(wait) {
                        Ok(Msg::Put(draft, at)) => {
                            let key = key_of(&draft);
                            state.entries.insert(
                                key,
                                Entry {
                                    field: field_json(&draft.field),
                                    shown: draft.shown.iter().map(ShownToken::to_text).collect(),
                                    text: draft.text,
                                    at: at.0,
                                },
                            );
                            state.dirty = true;
                        }
                        Ok(Msg::Remove(keys)) => {
                            for key in keys {
                                state.dirty |= state.entries.remove(&key).is_some();
                            }
                        }
                        Ok(Msg::Flush(done)) => {
                            state.write();
                            let _ = done.send(());
                        }
                        Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                            state.write();
                            return;
                        }
                        Err(RecvTimeoutError::Timeout) => state.write(),
                    }
                }
            })
            .ok();
        *writer.tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        *writer.thread.lock().unwrap_or_else(|e| e.into_inner()) = thread;
        writer
    }

    fn send(&self, msg: Msg) {
        if let Some(tx) = &*self.tx.lock().unwrap_or_else(|e| e.into_inner()) {
            let _ = tx.send(msg);
        }
    }

    /// Drafts are not journal data, so their time is the system clock's.
    pub(crate) fn put(&self, draft: Draft) {
        self.send(Msg::Put(Box::new(draft), crate::clock::SystemClock.now()));
    }

    /// A save of these drafts' fields succeeded: their entries go.
    pub(crate) fn saved(&self, keys: Vec<String>) {
        {
            let mut offered = self.offered.lock().unwrap_or_else(|e| e.into_inner());
            offered.retain(|(_, key)| !keys.contains(key));
        }
        self.send(Msg::Remove(keys));
    }

    pub(crate) fn offered(&self) -> Vec<OfferedDraft> {
        self.offered
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(o, _)| o.clone())
            .collect()
    }

    pub(crate) fn discard(&self, id: u64) {
        let key = {
            let mut offered = self.offered.lock().unwrap_or_else(|e| e.into_inner());
            let at = offered.iter().position(|(o, _)| o.id == id);
            at.map(|at| offered.remove(at).1)
        };
        if let Some(key) = key {
            self.send(Msg::Remove(vec![key]));
        }
    }

    /// Writes what is due now.
    pub(crate) fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        self.send(Msg::Flush(tx));
        let _ = rx.recv_timeout(Duration::from_secs(30));
    }

    pub(crate) fn stop(&self) {
        self.send(Msg::Stop);
        self.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = thread.join();
        }
    }
}

/// The draft entries a successful command clears (§5.14: "A successful save of that field
/// removes its entry").
pub(crate) fn cleared_by(command: &tc_journal::command::Command, fold: &FoldState) -> Vec<String> {
    use tc_journal::command::Command;
    let one =
        |field: DraftField, token: &ShownToken| vec![draft_key(&field, &[token.key().clone()])];
    match command {
        Command::SaveVerse { shown, .. } => one(DraftField::Words, shown),
        Command::SaveLayout { shown, .. } => one(DraftField::Layout, shown),
        Command::SaveSection { book, shown, .. } => {
            let keys: Vec<RecordKey> = shown.iter().map(|(t, _)| t.key().clone()).collect();
            vec![draft_key(&DraftField::Section { book: *book }, &keys)]
        }
        Command::SaveFrame { shown, .. } => {
            let field = match shown.key() {
                RecordKey::StoryRef(_) => DraftField::StoryRef,
                _ => DraftField::Frame,
            };
            one(field, shown)
        }
        Command::SaveDecision { shown, .. } => one(DraftField::CheckComment, shown),
        Command::SaveAlignment { shown, .. } => one(DraftField::Alignment, shown),
        Command::ReviseComment { shown, .. } => match fold.projected(shown.key()) {
            Some(Record::Comment { value, .. }) => vec![draft_key(
                &DraftField::UserComment {
                    target: value.target,
                },
                &[shown.key().clone()],
            )],
            _ => Vec::new(),
        },
        Command::AddComment { target, .. } => vec![draft_key(
            &DraftField::UserComment {
                target: target.clone(),
            },
            &[],
        )],
        Command::KeepDraft { draft, .. } => vec![key_of(draft)],
        _ => Vec::new(),
    }
}
