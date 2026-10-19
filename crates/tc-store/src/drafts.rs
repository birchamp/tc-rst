//! Drafts (`docs/STORAGE.md` §5.14): unsaved editor text in `.tc/drafts.json`, written with
//! `atomic_write` at most once a second while text is unsaved, by a thread of its own beside
//! the actor, so a draft write never delays a save (APP-DESIGN R22). A draft is best effort:
//! after a crash the file holds the old or the new list (crash row D1).
//!
//! The file holds two kinds of entry (APP-DESIGN §6.6, C1b). A *current* entry is the newest
//! draft of a field this session typed; a save of the field removes it. An *offered* entry is
//! a text an open offered, from the last session's drafts or taken in from the page's buffer
//! (`offer`): it has a number, and only *Keep* or *Discard* of that number removes it, written
//! before they answer, so a save of the field, the field's typing since or a reload never
//! takes it.
//!
//! At open (§6.1 O12) an entry whose text the record holds (the value its token stands for,
//! or any head now) goes, and so does a current entry of the last session that the session's
//! own later save superseded; the others become offered. *Keep* saves an entry with its tokens
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
use tc_journal::{BookId, DeviceId, RecordKey, ShownToken, TokenKey, WallTime};

/// At most one write a second.
const EVERY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    field: serde_json::Value,
    shown: Vec<String>,
    text: String,
    at: i64,
    /// An open offered it (APP-DESIGN §6.6): it stays until the person keeps or discards it by
    /// its number, and neither the field's typing since nor a save of the field replaces it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    offered: bool,
    /// The server run whose page typed it, as the app names its runs: the page's own buffer
    /// entry of that run, when newer, holds the same editor's newer text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run: Option<String>,
}

/// A draft offered at open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedDraft {
    pub id: u64,
    pub draft: Draft,
    /// The run that typed it, when the app named one.
    pub run: Option<String>,
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
    Put(Box<Draft>, WallTime, Option<String>),
    /// The current entries of these fields go: a save of the field succeeded.
    Remove(Vec<String>),
    /// An offered entry, written before the answer, which says whether it was.
    Offer(u64, Box<Entry>, mpsc::Sender<bool>),
    /// An offered entry goes, written before the answer.
    Unstore(u64, mpsc::Sender<bool>),
    Flush(mpsc::Sender<()>),
    Stop,
}

/// The draft writer of one open project (see the module documentation).
pub(crate) struct DraftWriter {
    tx: Mutex<Option<Sender<Msg>>>,
    offered: Arc<Mutex<Vec<(OfferedDraft, String)>>>,
    next_id: AtomicU64,
    thread: Mutex<Option<JoinHandle<()>>>,
    /// The app's server run whose page sends this open's drafts.
    run: Mutex<Option<String>>,
}

struct Writer {
    vfs: Arc<dyn Vfs>,
    folder: Folder,
    /// The newest draft of each field this session typed.
    current: BTreeMap<String, Entry>,
    /// The texts an open offered, by number, until they are kept or discarded.
    offered: BTreeMap<u64, Entry>,
    dirty: bool,
    last: Option<Instant>,
    hooks: Arc<Hooks>,
    counters: Arc<Counters>,
}

impl Writer {
    /// Writes the list when it changed; whether the file now holds it.
    fn write(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        self.hooks.wait(Point::BeforeDraftWrite);
        let list: Vec<&Entry> = self.offered.values().chain(self.current.values()).collect();
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
        written
    }
}

/// Reads `.tc/drafts.json`.
fn read_entries(vfs: &dyn Vfs, folder: &Folder) -> Vec<Entry> {
    vfs.read(&folder.path(crate::layout::DRAFTS))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<Entry>>(&bytes).ok())
        .unwrap_or_default()
}

fn same_text(a: &str, b: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    a == b || a.nfc().eq(b.nfc())
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

/// Whether this folder's device saved the record over the version `token` names: a change of
/// the device on the way from the record's heads back to that version.
fn own_save_since(fold: &FoldState, token: &ShownToken, device: DeviceId) -> bool {
    let base = token.heads();
    let mut seen = std::collections::BTreeSet::new();
    let mut stack = fold.heads(token.key());
    while let Some(change) = stack.pop() {
        if base.contains(&change) || !seen.insert(change) {
            continue;
        }
        let Some(body) = fold.body(&change) else {
            continue;
        };
        if body.device == device {
            return true;
        }
        stack.extend(body.parents);
    }
    false
}

/// Whether an entry of `drafts.json` is still unsaved text to offer at open (O12), and its
/// draft and whether its tokens verify. An entry goes when the record holds its text (the
/// value its token stands for, or any head now). An entry the last session wrote but no open
/// offered goes when the device saved the record over the version the entry was typed over:
/// that session's editor sent the save, so the save holds the entry's text or newer typing,
/// and typing newer than the save is in the page's buffer, which keeps every keystroke. An
/// entry an open offered is never taken for superseded: the person typed after it without
/// seeing it.
fn judge(
    entry: &Entry,
    fold: &FoldState,
    signing: &TokenKey,
    device: DeviceId,
) -> Option<(Draft, bool)> {
    let field = field_of(&entry.field)?;
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
    let offer = match (&field, draft.shown.first()) {
        (DraftField::Section { .. }, _) => true,
        (DraftField::UserComment { .. }, None) => true,
        (_, Some(token)) => {
            let holds = |record: Option<&Record>| {
                field_text(&field, record).is_some_and(|text| same_text(&text, &entry.text))
            };
            let then = token
                .heads()
                .first()
                .and_then(|h| fold.body(h))
                .map(|b| b.record);
            let heads: Vec<_> = fold
                .heads(token.key())
                .iter()
                .filter_map(|h| fold.body(h))
                .collect();
            let saved = holds(then.as_ref()) || heads.iter().any(|b| holds(Some(&b.record)));
            let superseded = !entry.offered && verified && own_save_since(fold, token, device);
            !saved && !superseded
        }
        (_, None) => false,
    };
    offer.then_some((draft, verified))
}

impl DraftWriter {
    /// Starts the writer of a writable project. The last session's drafts that are still
    /// unsaved are offered and marked so in the file, written before the open returns, so no
    /// save of this session can be taken for one that superseded them.
    #[expect(
        clippy::too_many_arguments,
        reason = "the open's parts the writer needs, each from its own step of the open"
    )]
    pub(crate) fn start(
        vfs: Arc<dyn Vfs>,
        folder: Folder,
        fold: &FoldState,
        signing: &TokenKey,
        device: DeviceId,
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
        let next_id = AtomicU64::new(1);
        let mut changed = false;
        let mut offered = Vec::new();
        let mut kept = BTreeMap::new();
        for mut entry in entries {
            let Some((draft, verified)) = judge(&entry, fold, signing, device) else {
                changed = true;
                continue;
            };
            changed |= !entry.offered;
            entry.offered = true;
            let id = next_id.fetch_add(1, Ordering::Relaxed);
            let key = key_of(&draft);
            offered.push((
                OfferedDraft {
                    id,
                    draft,
                    run: entry.run.clone(),
                    verified,
                    at: WallTime(entry.at),
                },
                key,
            ));
            kept.insert(id, entry);
        }
        let writer = DraftWriter {
            tx: Mutex::new(None),
            offered: Arc::new(Mutex::new(offered)),
            next_id,
            thread: Mutex::new(None),
            run: Mutex::new(None),
        };
        if read_only {
            return writer;
        }
        let (tx, rx) = mpsc::channel::<Msg>();
        let mut state = Writer {
            vfs,
            folder,
            current: BTreeMap::new(),
            offered: kept,
            dirty: changed,
            last: None,
            hooks,
            counters,
        };
        state.write();
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
                        Ok(Msg::Put(draft, at, run)) => {
                            let key = key_of(&draft);
                            state.current.insert(
                                key,
                                Entry {
                                    field: field_json(&draft.field),
                                    shown: draft.shown.iter().map(ShownToken::to_text).collect(),
                                    text: draft.text,
                                    at: at.0,
                                    offered: false,
                                    run,
                                },
                            );
                            state.dirty = true;
                        }
                        Ok(Msg::Remove(keys)) => {
                            for key in keys {
                                state.dirty |= state.current.remove(&key).is_some();
                            }
                        }
                        Ok(Msg::Offer(id, entry, done)) => {
                            state.offered.insert(id, *entry);
                            state.dirty = true;
                            let _ = done.send(state.write());
                        }
                        Ok(Msg::Unstore(id, done)) => {
                            state.dirty |= state.offered.remove(&id).is_some();
                            let _ = done.send(state.write());
                        }
                        Ok(Msg::Flush(done)) => {
                            state.write();
                            let _ = done.send(());
                        }
                        Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                            state.write();
                            return;
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            state.write();
                        }
                    }
                }
            })
            .ok();
        *writer.tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        *writer.thread.lock().unwrap_or_else(|e| e.into_inner()) = thread;
        writer
    }

    fn send(&self, msg: Msg) -> bool {
        match &*self.tx.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(tx) => tx.send(msg).is_ok(),
            None => false,
        }
    }

    /// The field's current draft. Drafts are not journal data, so their time is the system
    /// clock's.
    pub(crate) fn put(&self, draft: Draft) {
        let run = self.run.lock().unwrap_or_else(|e| e.into_inner()).clone();
        self.send(Msg::Put(
            Box::new(draft),
            crate::clock::SystemClock.now(),
            run,
        ));
    }

    /// Names the app's server run whose page sends this open's drafts.
    pub(crate) fn name_run(&self, run: String) {
        *self.run.lock().unwrap_or_else(|e| e.into_inner()) = Some(run);
    }

    /// A save of these drafts' fields succeeded: their current entries go. Offered entries
    /// stay until the person answers them.
    pub(crate) fn saved(&self, keys: Vec<String>) {
        self.send(Msg::Remove(keys));
    }

    /// *Keep* of `draft` succeeded: the offered entries of its field with its text go, and
    /// the field's other offered texts stay. A crash before the file is written leaves them,
    /// and the next open drops them, since the record holds their text.
    pub(crate) fn kept(&self, draft: &Draft) {
        let key = key_of(draft);
        let ids: Vec<u64> = {
            let mut offered = self.offered.lock().unwrap_or_else(|e| e.into_inner());
            let ids: Vec<u64> = offered
                .iter()
                .filter(|(o, k)| *k == key && same_text(&o.draft.text, &draft.text))
                .map(|(o, _)| o.id)
                .collect();
            offered.retain(|(o, _)| !ids.contains(&o.id));
            ids
        };
        for id in ids {
            let (tx, _rx) = mpsc::channel();
            self.send(Msg::Unstore(id, tx));
        }
    }

    pub(crate) fn offered(&self) -> Vec<OfferedDraft> {
        self.offered
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(o, _)| o.clone())
            .collect()
    }

    /// Takes `draft` in as an offered entry, written before this returns its number; the same
    /// text of the same field already offered keeps its own. `None` when the file could not be
    /// written (a read-only open writes nothing): the text stays where it is.
    pub(crate) fn offer(&self, draft: Draft, verified: bool, at: WallTime) -> Option<u64> {
        let key = key_of(&draft);
        let mut offered = self.offered.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((same, _)) = offered
            .iter()
            .find(|(o, k)| *k == key && same_text(&o.draft.text, &draft.text))
        {
            return Some(same.id);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let entry = Entry {
            field: field_json(&draft.field),
            shown: draft.shown.iter().map(ShownToken::to_text).collect(),
            text: draft.text.clone(),
            at: at.0,
            offered: true,
            run: None,
        };
        let (tx, rx) = mpsc::channel();
        if !self.send(Msg::Offer(id, Box::new(entry), tx)) {
            return None;
        }
        if !rx.recv_timeout(Duration::from_secs(30)).unwrap_or(false) {
            return None;
        }
        offered.push((
            OfferedDraft {
                id,
                draft,
                run: None,
                verified,
                at,
            },
            key,
        ));
        Some(id)
    }

    /// An offered entry goes, written before this returns, so a discarded text never comes
    /// back after a crash. Whether the file holds the change.
    pub(crate) fn discard(&self, id: u64) -> bool {
        {
            let mut offered = self.offered.lock().unwrap_or_else(|e| e.into_inner());
            offered.retain(|(o, _)| o.id != id);
        }
        let (tx, rx) = mpsc::channel();
        self.send(Msg::Unstore(id, tx)) && rx.recv_timeout(Duration::from_secs(30)).unwrap_or(false)
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
