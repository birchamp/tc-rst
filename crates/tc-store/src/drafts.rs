//! Drafts (`docs/STORAGE.md` §5.14): unsaved editor text in `.tc/drafts.json`, written with
//! `atomic_write` at most once a second while text is unsaved, by a thread of its own beside
//! the actor, so a draft write never delays a save (APP-DESIGN R22). A draft is best effort:
//! after a crash the file holds the old or the new list (crash row D1).
//!
//! The file holds two kinds of entry (APP-DESIGN §6.6, C1b). A *current* entry is the newest
//! draft of a field this session typed; a save removes it only if it predates submission or
//! holds the saved text (R26); *Keep* does not. An
//! *offered* entry is a text an open offered, from the last session's drafts or taken in from
//! the page's buffer (`offer`): it has a number, and only *Keep* of its text or *Discard* of
//! its number removes it, *Discard* written before it answers, so a save of the field, the
//! field's typing since or a reload never takes it. Each entry records the app's run that
//! typed it.
//!
//! At open (§6.1 O12) an entry whose text the record holds now (any head), or, for one no open
//! offered yet, the value its token stands for, goes; an entry of the opening run itself stays
//! its current entry; the others become offered. Whether a later save superseded one is the
//! app's to judge, with the page's buffer and the shell's mirror. *Keep* saves an entry with its
//! tokens (`Command::KeepDraft`), so a record that changed meanwhile becomes a conflict, never
//! a revert. A draft whose token this installation cannot verify any more (a new installation
//! secret) is offered as text, and *Keep* writes it with no parents (R20).

use crate::atomic::{SyncDir, atomic_create_dirs, atomic_write};
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
    /// An open offered it (APP-DESIGN §6.6): it stays until the person keeps or discards it by
    /// its number, and neither the field's typing since nor a save of the field replaces it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    offered: bool,
    /// The server run whose page typed it, as the app names its runs: the page's own buffer
    /// entry of that run, when newer, holds the same editor's newer text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run: Option<String>,
    /// Local ordering against command submission; WallTime belongs to the page and can repeat.
    #[serde(skip)]
    put: Option<Instant>,
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
    Put(Box<Draft>, WallTime, Option<String>, Instant),
    /// A successful save may remove an older or equal current entry, never newer work.
    Remove(Vec<SavedField>, Instant),
    /// An offered entry, written before the answer, which says whether it was.
    Offer(u64, Box<Entry>, mpsc::Sender<bool>),
    /// An offered entry goes, written before the answer.
    Unstore(u64, mpsc::Sender<bool>),
    Cancel(Box<Draft>, WallTime, mpsc::Sender<bool>),
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
    run: Option<String>,
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

/// Whether an entry of `drafts.json` is still unsaved text to offer at open (O12), and its
/// draft and whether its tokens verify. An entry goes when the record holds its text: any head
/// now, or, for one no open offered yet, the value its token stands for. Whether a later save
/// of its session superseded it is the app's to judge with the page's buffer (APP-DESIGN §6.6),
/// which saw that session end.
fn judge(entry: &Entry, fold: &FoldState, signing: &TokenKey) -> Option<(Draft, bool)> {
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
            // An offered entry was judged against its own version already: one that holds it
            // was typed back after a save it no longer names (APP-DESIGN §6.6).
            let saved = (!entry.offered && holds(then.as_ref()))
                || heads.iter().any(|b| holds(Some(&b.record)));
            !saved
        }
        (_, None) => false,
    };
    offer.then_some((draft, verified))
}

impl DraftWriter {
    /// Starts the writer of a writable project. The last session's drafts that are still
    /// unsaved are offered and marked so in the file, written before the open returns: from
    /// then on only *Keep* or *Discard* removes them.
    #[expect(
        clippy::too_many_arguments,
        reason = "the open's parts the writer needs, each from its own step of the open"
    )]
    pub(crate) fn start(
        vfs: Arc<dyn Vfs>,
        folder: Folder,
        fold: &FoldState,
        signing: &TokenKey,
        run: Option<String>,
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
        let mut current = BTreeMap::new();
        for mut entry in entries {
            // The same run's page typed it (a reopen in the same run): it holds that typing,
            // so the entry stays its current draft, and a later run's open judges it.
            if !entry.offered && entry.run.is_some() && entry.run == run {
                if let Some(field) = field_of(&entry.field) {
                    let keys: Vec<RecordKey> = entry
                        .shown
                        .iter()
                        .filter_map(|t| ShownToken::parse_unverified(t).ok())
                        .map(|t| t.key().clone())
                        .collect();
                    current.insert(draft_key(&field, &keys), entry);
                }
                continue;
            }
            let Some((draft, verified)) = judge(&entry, fold, signing) else {
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
            run,
        };
        if read_only {
            return writer;
        }
        let (tx, rx) = mpsc::channel::<Msg>();
        let mut state = Writer {
            vfs,
            folder,
            current,
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
                        Ok(Msg::Put(draft, at, run, put)) => {
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
                                    put: Some(put),
                                },
                            );
                            state.dirty = true;
                        }
                        Ok(Msg::Remove(fields, submitted)) => {
                            for field in fields {
                                let remove = state.current.get(&field.key).is_some_and(|entry| {
                                    entry.put.is_none_or(|put| put < submitted)
                                        || field
                                            .text
                                            .as_ref()
                                            .is_some_and(|text| same_text(&entry.text, text))
                                });
                                if remove {
                                    state.dirty |= state.current.remove(&field.key).is_some();
                                }
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
                        Ok(Msg::Cancel(draft, at, done)) => {
                            let key = key_of(&draft);
                            let shown: Vec<String> =
                                draft.shown.iter().map(ShownToken::to_text).collect();
                            let matches = state.current.get(&key).is_some_and(|entry| {
                                entry.at == at.0 && entry.text == draft.text && entry.shown == shown
                            });
                            if matches {
                                state.dirty |= state.current.remove(&key).is_some();
                            }
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

    /// The field's current draft, with the page's time of its text: the open compares it with
    /// the page's buffer entries, which carry that clock (APP-DESIGN §6.6).
    pub(crate) fn put(&self, draft: Draft, at: WallTime) {
        self.send(Msg::Put(
            Box::new(draft),
            at,
            self.run.clone(),
            Instant::now(),
        ));
    }

    /// Cancel removes only the exact current entry the page discarded, never newer typing
    /// or an offer. The page finishes its earlier draft requests before issuing this.
    pub(crate) fn cancel(&self, draft: Draft, at: WallTime) -> bool {
        let (done, finished) = mpsc::channel();
        self.send(Msg::Cancel(Box::new(draft), at, done)) && finished.recv().unwrap_or(false)
    }

    /// The save cannot erase a newer differing draft sent while it waited (R26). Offered
    /// entries stay until the person answers them.
    pub(crate) fn saved(&self, fields: Vec<SavedField>, submitted: Instant) {
        self.send(Msg::Remove(fields, submitted));
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
        // Only an entry an earlier offer took in holds the text for good: one of a run goes
        // once this one is written.
        if let Some((same, _)) = offered
            .iter()
            .find(|(o, k)| *k == key && o.run.is_none() && same_text(&o.draft.text, &draft.text))
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
            put: None,
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

/// The current field a successful save may clear, with the text the committed fold holds.
pub(crate) struct SavedField {
    key: String,
    text: Option<String>,
}

/// Computed after commit: a newer differing current draft stays even before the save's Ack.
pub(crate) fn cleared_by(
    command: &tc_journal::command::Command,
    fold: &FoldState,
) -> Vec<SavedField> {
    use tc_journal::command::Command;
    let one = |field: DraftField, token: &ShownToken| {
        vec![SavedField {
            key: draft_key(&field, &[token.key().clone()]),
            text: field_text(&field, fold.projected(token.key()).as_ref()),
        }]
    };
    match command {
        Command::SaveVerse { shown, .. } => one(DraftField::Words, shown),
        Command::SaveLayout { shown, .. } => one(DraftField::Layout, shown),
        Command::SaveSection { book, shown, .. } => {
            let keys: Vec<RecordKey> = shown.iter().map(|(t, _)| t.key().clone()).collect();
            vec![SavedField {
                key: draft_key(&DraftField::Section { book: *book }, &keys),
                // The command has parsed values, not the section editor's raw buffer.
                text: None,
            }]
        }
        Command::SaveFrame { shown, .. } => {
            let field = match shown.key() {
                RecordKey::StoryRef(_) => DraftField::StoryRef,
                _ => DraftField::Frame,
            };
            one(field, shown)
        }
        // A mark or a bookmark saves no comment: the comment typed before it is still unsaved.
        Command::SaveDecision { shown, value, .. }
            if value
                .changes
                .iter()
                .any(|c| matches!(c, tc_journal::command::DecisionChange::Comment(_))) =>
        {
            one(DraftField::CheckComment, shown)
        }
        Command::SaveAlignment { shown, .. } => one(DraftField::Alignment, shown),
        Command::ReviseComment { shown, .. } => match fold.projected(shown.key()) {
            Some(Record::Comment { value, .. }) => one(
                DraftField::UserComment {
                    target: value.target,
                },
                shown,
            ),
            _ => Vec::new(),
        },
        Command::AddComment { target, text } => vec![SavedField {
            key: draft_key(
                &DraftField::UserComment {
                    target: target.clone(),
                },
                &[],
            ),
            text: Some(text.clone()),
        }],
        // *Keep* answers an offered entry (`DraftWriter::kept`); the field's current draft is
        // the page's typing since, which it still holds.
        Command::KeepDraft { .. } => Vec::new(),
        _ => Vec::new(),
    }
}
