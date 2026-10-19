//! The draft machine (`tc_api::model::draft`, APP-DESIGN §6.6, C1b) as a model-based property:
//! random interleavings of typing, saves, replies, refusals, drafts, reloads, crashes and
//! relaunches, offers, keeps, discards, settling, a second writer and its copy, another
//! computer's lock and an unsafe projects folder, with an observation (a crash, a relaunch and
//! the offer) possible between any two steps.
//!
//! The world carries out the machine's actions on all four places: the page's buffer and the
//! shell's mirror through tc-ui's own functions (`buffer`, `offer::buffered_lines`,
//! `offer::settle`, `offer::copy`), and a model of tc-store's `drafts.json` (current and
//! offered entries, its open's judgement) and of a record's versions. The person's typing is
//! the truth the properties check:
//!
//! - **I1, no loss**: at a writable open, the last text of every run's typing of a field that
//!   reached a place that outlives a crash is stored, offered, or was answered.
//! - **I2, nothing stale or answered comes back**: every offered text is the unanswered last
//!   text of some run's typing. Without the shell's mirror, or when the storage refused a
//!   write, nothing tells an older text of a run from its last one, and an older one may be
//!   offered beside it, never instead of it.
//! - **I3**: a copy is never read-only because of the projects folder it was made in.
//! - **I4**: after "That computer is closed" the project is not read-only for that lock.
//! - **I5**: an offer during a run never settles the text of a field the page's save machine
//!   holds.
//!
//! Each of the nine findings of C1b's review can be put back ([`Seed`]); the last test shows
//! the property catches each one.

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestError, TestRng, TestRunner};
use serde_json::json;
use tc_api::buffer::{
    BufferEntry, BufferKey, BufferLine, FieldKey, FieldKind, FieldPlace, SentSave,
};
use tc_api::ids::{ProjectId, ProjectRef, RequestId, ServerInstance, WallMs};
use tc_api::model::draft::{
    self as machine, Action, Base, Candidate, Event, FieldDraft, Home, Live, Record, same_text,
};
use tc_api::session::{BufferSlot, StoredDraftId};
use tc_api::token::ShownToken;
use tc_ui::app::buffer::{self, MemStore};
use tc_ui::app::offer;

const FIELDS: usize = 2;
const TEXTS: [&str; 3] = ["x", "y", "z"];
const SEED_TEXT: &str = "s";
const ID: &str = "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50";

/// A finding of the review, put back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seed {
    /// 1: *Keep* of one text removes every offered text of the field.
    KeepTakesTheFieldsOtherTexts,
    /// 2: a text back at the version it was typed over is "saved" though its save landed.
    UndoneBeforeLanded,
    /// 3: the copy moves the page's items and leaves the mirror's entries behind.
    CopyLeavesTheMirror,
    /// 4: the copy goes into the projects folder that made the project read-only.
    CopyIntoTheUnsafeFolder,
    /// 5: an older `drafts.json` entry is offered over the session's own later save.
    StaleDraftOffered,
    /// 6: the open after "That computer is closed" joins the leaving read-only session.
    ReopenJoinsTheLeavingSession,
    /// 7: an open in the same run offers the fields the save machine holds.
    OwnFieldsOffered,
    /// 8: the settling as it was: a slot's mirror removal drops whatever its run holds, and the
    /// page writes nothing of its own again.
    MirrorRemovalIgnoresText,
    /// 9: *Discard* removes the field's draft typed after the offer.
    DiscardTakesTheFieldsDraft,
}

const SEEDS: [Seed; 9] = [
    Seed::KeepTakesTheFieldsOtherTexts,
    Seed::UndoneBeforeLanded,
    Seed::CopyLeavesTheMirror,
    Seed::CopyIntoTheUnsafeFolder,
    Seed::StaleDraftOffered,
    Seed::ReopenJoinsTheLeavingSession,
    Seed::OwnFieldsOffered,
    Seed::MirrorRemovalIgnoresText,
    Seed::DiscardTakesTheFieldsDraft,
];

#[derive(Debug, Clone)]
enum Op {
    Type(usize, usize),
    DraftTick(usize),
    /// The draft writer writes `drafts.json`.
    Flush,
    Send(usize),
    Land(usize),
    Ack(usize),
    Refuse(usize),
    Teammate(usize, usize),
    Reload,
    Crash,
    /// The app starts again and the page comes back at a new origin, its storage empty.
    CrashNewOrigin,
    /// The browser's storage refuses writes, or takes them again.
    StorageFull,
    StorageOk,
    Offer,
    Keep(usize, usize),
    Discard(usize, usize),
    Settle,
    SecondWriter,
    Copy,
    OtherComputer,
    ThatComputerClosed,
    Unsafe,
    Safe,
    Observe,
}

fn op() -> impl Strategy<Value = Op> {
    let f = 0..FIELDS;
    let t = 0..TEXTS.len();
    prop_oneof![
        8 => (f.clone(), t.clone()).prop_map(|(f, t)| Op::Type(f, t)),
        3 => f.clone().prop_map(Op::DraftTick),
        2 => Just(Op::Flush),
        4 => f.clone().prop_map(Op::Send),
        4 => f.clone().prop_map(Op::Land),
        3 => f.clone().prop_map(Op::Ack),
        1 => f.clone().prop_map(Op::Refuse),
        1 => (f.clone(), t).prop_map(|(f, t)| Op::Teammate(f, t)),
        1 => Just(Op::Reload),
        4 => Just(Op::Crash),
        1 => Just(Op::CrashNewOrigin),
        1 => Just(Op::StorageFull),
        1 => Just(Op::StorageOk),
        4 => Just(Op::Offer),
        3 => (f.clone(), 0..2usize).prop_map(|(f, i)| Op::Keep(f, i)),
        2 => (f, 0..2usize).prop_map(|(f, i)| Op::Discard(f, i)),
        3 => Just(Op::Settle),
        1 => Just(Op::SecondWriter),
        2 => Just(Op::Copy),
        1 => Just(Op::OtherComputer),
        2 => Just(Op::ThatComputerClosed),
        1 => Just(Op::Unsafe),
        1 => Just(Op::Safe),
        3 => Just(Op::Observe),
    ]
}

/// Steps that set up the places' harder states, so random runs reach them: interleaved with
/// single random steps, every interleaving stays possible.
fn fragment() -> impl Strategy<Value = Vec<Op>> {
    let f = 0..FIELDS;
    let t = 0..TEXTS.len();
    prop_oneof![
        // Two runs type one field: the page's item holds the first run's entry.
        (f.clone(), t.clone(), t.clone()).prop_map(|(f, a, b)| vec![
            Op::Type(f, a),
            Op::Crash,
            Op::Type(f, b),
            Op::Crash
        ]),
        // An open answers, and the page drops what is held elsewhere.
        Just(vec![Op::Offer, Op::Settle]),
        // Typing a full storage refuses, with its draft in drafts.json.
        (f.clone(), t.clone()).prop_map(|(f, a)| vec![
            Op::StorageFull,
            Op::Type(f, a),
            Op::DraftTick(f),
            Op::Flush
        ]),
        // A save, all the way.
        (f.clone(), t.clone()).prop_map(|(f, a)| vec![
            Op::Type(f, a),
            Op::Send(f),
            Op::Land(f),
            Op::Ack(f)
        ]),
        // Typing on while a save is on its way, which then lands.
        (f.clone(), t.clone(), t.clone()).prop_map(|(f, a, b)| vec![
            Op::Type(f, a),
            Op::Send(f),
            Op::Type(f, b),
            Op::Land(f)
        ]),
        // A save all the way while the storage refuses: the item keeps an older text.
        (f.clone(), t.clone(), t.clone()).prop_map(|(f, a, b)| vec![
            Op::Type(f, a),
            Op::StorageFull,
            Op::Type(f, b),
            Op::Send(f),
            Op::Land(f),
            Op::Ack(f)
        ]),
        // A draft reaches drafts.json.
        f.clone().prop_map(|f| vec![Op::DraftTick(f), Op::Flush]),
        // The person answers the first question of a field.
        (f.clone(), any::<bool>()).prop_map(|(f, keep)| vec![if keep {
            Op::Keep(f, 0)
        } else {
            Op::Discard(f, 0)
        }]),
        // A second writer, and the copy.
        Just(vec![Op::SecondWriter, Op::Copy]),
    ]
}

fn script() -> impl Strategy<Value = (bool, Vec<Op>)> {
    let steps = prop_oneof![
        3 => op().prop_map(|op| vec![op]),
        2 => fragment(),
    ];
    (
        any::<bool>(),
        proptest::collection::vec(steps, 1..30).prop_map(|steps| steps.concat()),
    )
}

// ---------------------------------------------------------------------------------------
// The record and drafts.json
// ---------------------------------------------------------------------------------------

type V = u32;

#[derive(Debug, Clone)]
struct Version {
    field: usize,
    text: String,
}

#[derive(Debug, Clone)]
struct Store {
    versions: Vec<Version>,
    parents: Vec<Vec<V>>,
    heads: [Vec<V>; FIELDS],
}

impl Store {
    fn new() -> Self {
        let mut store = Store {
            versions: Vec::new(),
            parents: Vec::new(),
            heads: [Vec::new(), Vec::new()],
        };
        for field in 0..FIELDS {
            let v = store.add(field, SEED_TEXT, &[]);
            store.heads[field] = vec![v];
        }
        store
    }

    fn add(&mut self, field: usize, text: &str, parents: &[V]) -> V {
        self.versions.push(Version {
            field,
            text: text.to_owned(),
        });
        self.parents.push(parents.to_vec());
        V::try_from(self.versions.len() - 1).unwrap()
    }

    /// A save over `over`: what it replaced goes, a head it did not know stays beside it.
    fn save(&mut self, field: usize, text: &str, over: V) -> V {
        let v = self.add(field, text, &[over]);
        self.heads[field].retain(|head| *head != over);
        self.heads[field].push(v);
        v
    }

    fn text(&self, v: V) -> &str {
        &self.versions[v as usize].text
    }

    fn shown(&self, field: usize) -> V {
        *self.heads[field].iter().max().unwrap()
    }

    /// Whether a version of `field` made at or after `since` holds `text`, wherever it stands.
    fn made_since(&self, field: usize, since: usize, text: &str) -> bool {
        self.versions
            .iter()
            .skip(since)
            .any(|v| v.field == field && same_text(&v.text, text))
    }

    /// Whether a version between the heads and `base` held `text`.
    fn held_since(&self, field: usize, base: V, text: &str) -> bool {
        self.between(field, base)
            .iter()
            .any(|v| same_text(self.text(*v), text))
    }

    /// The versions after `base`: those `base` is an ancestor of, back from the heads.
    fn between(&self, field: usize, base: V) -> Vec<V> {
        fn reaches(store: &Store, v: V, base: V, memo: &mut BTreeMap<V, bool>) -> bool {
            if v == base {
                return true;
            }
            if let Some(known) = memo.get(&v) {
                return *known;
            }
            let parents = store.parents[v as usize].clone();
            let found = parents.into_iter().any(|p| reaches(store, p, base, memo));
            memo.insert(v, found);
            found
        }
        let mut memo = BTreeMap::new();
        for head in &self.heads[field] {
            reaches(self, *head, base, &mut memo);
        }
        memo.into_iter()
            .filter_map(|(v, after)| after.then_some(v))
            .collect()
    }
}

struct FieldRecord<'a>(&'a Store, usize);

impl Record<V> for FieldRecord<'_> {
    fn heads(&self) -> Vec<String> {
        self.0.heads[self.1]
            .iter()
            .map(|v| self.0.text(*v).to_owned())
            .collect()
    }
    fn sole(&self) -> Option<V> {
        match self.0.heads[self.1].as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    }
    fn text_of(&self, version: &V) -> Option<String> {
        Some(self.0.text(*version).to_owned())
    }
    fn held_since(&self, version: &V, text: &str) -> bool {
        self.0.held_since(self.1, *version, text)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    field: usize,
    text: String,
    base: V,
    at: i64,
    run: Option<u8>,
    offered: bool,
}

/// `drafts.json`: the writer's memory and the file.
#[derive(Debug, Clone, Default)]
struct DraftsFile {
    current: BTreeMap<usize, Entry>,
    offered: BTreeMap<u64, Entry>,
}

impl DraftsFile {
    fn all(&self) -> impl Iterator<Item = &Entry> {
        self.offered.values().chain(self.current.values())
    }
}

// ---------------------------------------------------------------------------------------
// The truth: what the person typed
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answered {
    Open,
    /// The person's own save of it landed.
    Saved,
    Kept,
    Discarded,
}

/// The last text of one run's typing of one field.
#[derive(Debug, Clone)]
struct Item {
    text: String,
    /// The text of the version this stretch of typing began over, and that version.
    base_text: String,
    base: V,
    /// A save of this stretch landed.
    landed: bool,
    /// The clock at its last keystroke: a draft written since holds this text.
    typed_at: i64,
    /// How many versions the store had then: one made since that holds the text stored it,
    /// as does a head that held it then.
    since: usize,
    held_then: bool,
    answered: Answered,
    /// The text reached a place that outlives a crash: the page's storage (until a page at a
    /// new origin starts with it empty), the shell's mirror, `drafts.json`.
    in_storage: bool,
    in_mirror: bool,
    in_file: bool,
    /// The storage refused a write of the stretch: its item keeps an older text, which A4b
    /// prefers to none, and which an offer may bring back beside the saved one.
    refused: bool,
    /// Every text of the stretch: when the last one reached no such place (a browser with no
    /// shell, whose item holds another run's entry or whose storage is full), an earlier one
    /// is the best left, and offering it is no fault (A4b keeps an older text over none).
    history: Vec<String>,
}

impl Item {
    fn durable(&self) -> bool {
        self.in_storage || self.in_mirror || self.in_file
    }

    /// Typed back to where it began, with nothing of it saved.
    fn undone(&self) -> bool {
        self.answered == Answered::Open && !self.landed && same_text(&self.text, &self.base_text)
    }

    /// Unanswered: not saved, kept or discarded, and not undone.
    fn open(&self) -> bool {
        self.answered == Answered::Open && !self.undone()
    }
}

// ---------------------------------------------------------------------------------------
// The world
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Writable,
    ReadOnly(Why),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    SecondWriter,
    OtherComputer,
    Unsafe,
}

#[derive(Clone)]
struct World {
    seed: Option<Seed>,
    shell: bool,
    store: Store,
    drafts: DraftsFile,
    file: DraftsFile,
    next_id: u64,
    storage: MemStore,
    mirror: Vec<String>,
    run: u8,
    clock: i64,
    folder: ProjectRef,
    copies: u32,
    mode: Mode,
    page: [FieldDraft<V>; FIELDS],
    inflight: [Option<(String, V)>; FIELDS],
    lock_elsewhere: bool,
    trusted: bool,
    unsafe_place: bool,
    /// By field, run and stretch: a stretch begins when the page holds no text of the field.
    items: BTreeMap<(usize, u8, u32), Item>,
    stretch: [u32; FIELDS],
}

fn token(v: V) -> ShownToken {
    serde_json::from_value(json!(format!("st1.v{v:0>30}"))).unwrap()
}

fn version(token: &ShownToken) -> V {
    let text: String = serde_json::from_value(serde_json::to_value(token).unwrap()).unwrap();
    text.trim_start_matches("st1.v").parse().unwrap()
}

fn instance(run: u8) -> ServerInstance {
    ServerInstance::from_bytes([run; 16])
}

fn id() -> ProjectId {
    ProjectId::parse(ID).unwrap()
}

type Check = Result<(), TestCaseError>;

impl World {
    fn new(seed: Option<Seed>, shell: bool) -> Self {
        Self {
            seed,
            shell,
            store: Store::new(),
            drafts: DraftsFile::default(),
            file: DraftsFile::default(),
            next_id: 1,
            storage: MemStore::new(),
            mirror: Vec::new(),
            run: 1,
            clock: 1_000,
            folder: ProjectRef::parse("en_ult").unwrap(),
            copies: 0,
            mode: Mode::Writable,
            page: Default::default(),
            inflight: Default::default(),
            lock_elsewhere: false,
            trusted: false,
            unsafe_place: false,
            items: BTreeMap::new(),
            stretch: [0; FIELDS],
        }
    }

    fn seeded(&self, seed: Seed) -> bool {
        self.seed == Some(seed)
    }

    fn key(&self, field: usize) -> BufferKey {
        BufferKey {
            project: self.folder.clone(),
            id: id(),
            field: FieldKey {
                kind: FieldKind::Verse,
                place: FieldPlace::parse(&format!("TIT 1:{field}")).unwrap(),
            },
        }
    }

    fn field_of(&self, key: &BufferKey) -> Option<usize> {
        (0..FIELDS).find(|f| self.key(*f) == *key)
    }

    fn tick(&mut self) -> i64 {
        self.clock += 10;
        self.clock
    }

    fn mirror_line(&mut self, line: Option<String>) {
        if self.shell
            && let Some(line) = line
        {
            self.mirror.push(line);
        }
    }

    fn write_file(&mut self) {
        self.file = self.drafts.clone();
        for ((field, run, _), item) in &mut self.items {
            let held = self.file.current.values().any(|entry| {
                entry.field == *field
                    && entry.run == Some(*run)
                    && entry.at >= item.typed_at
                    && same_text(&entry.text, &item.text)
            });
            item.in_file |= held;
        }
    }

    // --- the page's own field -----------------------------------------------------------

    fn step(&mut self, field: usize, event: Event<'_, V>) -> Vec<Action<V>> {
        let state = std::mem::take(&mut self.page[field]);
        let (state, actions) = machine::next(state, event);
        self.page[field] = state;
        actions
    }

    fn carry_out(&mut self, field: usize, actions: Vec<Action<V>>) {
        for action in actions {
            match action {
                Action::Write {
                    text,
                    at,
                    base,
                    sent,
                } => {
                    let entry = BufferEntry {
                        api: 1,
                        instance: instance(self.run),
                        base: vec![token(base)],
                        text: text.clone(),
                        sent: sent.map(|text| SentSave {
                            request: RequestId::from_bytes([self.run; 16]),
                            text,
                            scope: None,
                        }),
                        at: WallMs(at),
                        draft: None,
                    };
                    let key = self.key(field);
                    let stored = buffer::read(&self.storage, &key)
                        .is_none_or(|old| old.instance == entry.instance);
                    let written = buffer::write_entry(&mut self.storage, &key, &entry);
                    self.mirror_line(written.line);
                    if let Some(item) = self.items.get_mut(&(field, self.run, self.stretch[field]))
                    {
                        item.in_storage |= stored && written.failure.is_none();
                        item.refused |= written.failure.is_some();
                        item.in_mirror |= self.shell;
                    }
                }
                Action::Remove { text } => {
                    let key = self.key(field);
                    let _ =
                        buffer::remove(&mut self.storage, &key, Some(&text), instance(self.run));
                    let line = buffer::removed_line_if(&key, instance(self.run), &text).ok();
                    self.mirror_line(line);
                }
                Action::Draft { text, base } => {
                    if self.mode == Mode::Writable {
                        let at = self.tick();
                        self.drafts.current.insert(
                            field,
                            Entry {
                                field,
                                text,
                                base,
                                at,
                                run: Some(self.run),
                                offered: false,
                            },
                        );
                    }
                }
                Action::Send { text, base } => {
                    if self.mode == Mode::Writable {
                        self.inflight[field] = Some((text, base));
                    } else {
                        self.refused(field);
                    }
                }
                other => panic!("the page planned {other:?}"),
            }
        }
    }

    fn typed(&mut self, field: usize, text: &str) {
        if self.mode != Mode::Writable {
            return;
        }
        let at = self.tick();
        let shown = self.store.shown(field);
        let fresh = matches!(self.page[field].live, Live::Clean);
        if fresh {
            self.stretch[field] += 1;
        }
        let item = self
            .items
            .entry((field, self.run, self.stretch[field]))
            .or_insert_with(|| Item {
                text: String::new(),
                base_text: String::new(),
                base: 0,
                typed_at: 0,
                since: 0,
                held_then: false,
                landed: false,
                answered: Answered::Open,
                in_storage: false,
                in_mirror: false,
                in_file: false,
                refused: false,
                history: Vec::new(),
            });
        if fresh {
            item.base_text = self.store.text(shown).to_owned();
            item.base = shown;
            item.landed = false;
            item.refused = false;
            item.history.clear();
        }
        if fresh || !same_text(&item.text, text) {
            item.typed_at = at;
            item.since = self.store.versions.len();
            item.held_then = self.store.heads[field]
                .iter()
                .any(|v| same_text(self.store.text(*v), text));
            item.text = text.to_owned();
            item.history.push(text.to_owned());
            item.answered = Answered::Open;
            item.in_storage = false;
            item.in_mirror = false;
            item.in_file = false;
        }
        let actions = self.step(
            field,
            Event::Typed {
                text: text.to_owned(),
                at,
                shown,
            },
        );
        self.carry_out(field, actions);
    }

    fn landed(&mut self, field: usize) {
        if self.mode != Mode::Writable {
            return;
        }
        let Some((text, base)) = self.inflight[field].take() else {
            return;
        };
        let over = self.store.save(field, &text, base);
        // tc-store's `cleared_by`: a save removes the field's current draft.
        self.drafts.current.remove(&field);
        if let Some(item) = self.items.get_mut(&(field, self.run, self.stretch[field])) {
            item.landed = true;
            if same_text(&item.text, &text) {
                item.answered = Answered::Saved;
            }
        }
        let actions = self.step(field, Event::SaveLanded { over });
        self.carry_out(field, actions);
    }

    fn acked(&mut self, field: usize) {
        let base_after = match &self.page[field].live {
            Live::Landed { sent, over, .. } => Some((sent.clone(), *over)),
            _ => None,
        };
        let actions = self.step(field, Event::SaveAcked);
        if let Some((sent, over)) = base_after
            && let Some(item) = self.items.get_mut(&(field, self.run, self.stretch[field]))
        {
            item.base_text = sent;
            item.base = over;
            item.landed = false;
        }
        self.carry_out(field, actions);
    }

    fn refused(&mut self, field: usize) {
        if !matches!(self.page[field].live, Live::Sending { .. }) {
            return;
        }
        self.inflight[field] = None;
        let actions = self.step(field, Event::SaveRefused);
        self.carry_out(field, actions);
    }

    // --- opens --------------------------------------------------------------------------

    /// tc-store's open: the file is judged, what is still unsaved is offered and marked so,
    /// written before the open returns.
    fn open(&mut self) {
        self.mode = if self.unsafe_place {
            Mode::ReadOnly(Why::Unsafe)
        } else if self.lock_elsewhere && !self.trusted {
            Mode::ReadOnly(Why::OtherComputer)
        } else {
            self.lock_elsewhere = false;
            self.trusted = false;
            Mode::Writable
        };
        if self.mode != Mode::Writable {
            self.drafts = DraftsFile::default();
            return;
        }
        let entries: Vec<Entry> = self.file.all().cloned().collect();
        let mut judged = DraftsFile::default();
        for mut entry in entries {
            if !entry.offered && entry.run == Some(self.run) {
                judged.current.insert(entry.field, entry);
                continue;
            }
            let saved = (!entry.offered && same_text(self.store.text(entry.base), &entry.text))
                || self.store.heads[entry.field]
                    .iter()
                    .any(|v| same_text(self.store.text(*v), &entry.text));
            if saved {
                continue;
            }
            entry.offered = true;
            judged.offered.insert(self.next_id, entry);
            self.next_id += 1;
        }
        self.drafts = judged;
        self.write_file();
    }

    fn relaunch(&mut self) {
        for field in 0..FIELDS {
            let actions = self.step(field, Event::Relaunch);
            assert!(actions.is_empty());
        }
        self.inflight = Default::default();
        self.run += 1;
        self.drafts = DraftsFile::default();
        self.open();
    }

    // --- the offer ----------------------------------------------------------------------

    fn owned(&self, key: &BufferKey, run: ServerInstance) -> bool {
        run == instance(self.run)
            && self
                .field_of(key)
                .is_some_and(|f| !matches!(self.page[f].live, Live::Clean))
    }

    /// The page sends its lines, the server judges, takes texts into `drafts.json`, answers.
    fn offer(&mut self) -> Check {
        if self.mode != Mode::Writable {
            return Ok(());
        }
        let owned = |key: &BufferKey, run: ServerInstance| self.owned(key, run);
        let none = |_: &BufferKey, _: ServerInstance| false;
        let lines = offer::buffered_lines(
            &self.storage,
            &self.mirror,
            &self.folder,
            &id(),
            if self.seeded(Seed::OwnFieldsOffered) {
                &none
            } else {
                &owned
            },
        );
        let mut candidates: [Vec<Candidate<V>>; FIELDS] = Default::default();
        for line in lines {
            let Ok(BufferLine::Changed { key, entry }) = serde_json::from_str(&line) else {
                continue;
            };
            let Some(field) = self.field_of(&key) else {
                continue;
            };
            let landed_back = self.seeded(Seed::UndoneBeforeLanded)
                && same_text(self.store.text(version(&entry.base[0])), &entry.text);
            candidates[field].push(Candidate {
                text: entry.text.clone(),
                at: entry.at.0,
                base: Base::Verified(version(&entry.base[0])),
                sent: entry.sent.map(|sent| sent.text).filter(|_| !landed_back),
                run: Some(entry.instance),
                home: Home::Buffer(BufferSlot {
                    key,
                    instance: entry.instance,
                    text: entry.text,
                    at: entry.at,
                }),
            });
        }
        for (id, entry) in &self.drafts.offered {
            candidates[entry.field].push(Candidate {
                text: entry.text.clone(),
                at: entry.at,
                base: Base::Verified(entry.base),
                sent: None,
                run: entry.run.map(instance),
                home: Home::Stored(StoredDraftId(*id)),
            });
        }
        for (field, candidates) in candidates.into_iter().enumerate() {
            let store = self.store.clone();
            let record = FieldRecord(&store, field);
            let mirror = self.shell && !self.seeded(Seed::StaleDraftOffered);
            let actions = self.step(
                field,
                Event::Offer {
                    candidates,
                    record: &record,
                    mirror,
                },
            );
            for action in actions {
                match action {
                    Action::Absorb {
                        text,
                        at,
                        over,
                        then,
                        older,
                        ..
                    } => {
                        let id = self.next_id;
                        self.next_id += 1;
                        self.drafts.offered.insert(
                            id,
                            Entry {
                                field,
                                text: text.clone(),
                                base: over,
                                at,
                                run: None,
                                offered: true,
                            },
                        );
                        if let Some(offer) =
                            self.page[field].offers.iter_mut().find(|o| o.text == text)
                        {
                            machine::absorbed(offer, StoredDraftId(id));
                        }
                        for slot in &then {
                            let run = slot.instance.bytes()[0];
                            for ((f, r, _), item) in &mut self.items {
                                if *f == field && *r == run && same_text(&item.text, &slot.text) {
                                    item.in_file = true;
                                }
                            }
                        }
                        self.page[field].settle.extend(then);
                        for copy in older {
                            self.drafts.offered.remove(&copy.0);
                        }
                        self.write_file();
                    }
                    Action::Unstore(id) => {
                        self.drafts.offered.remove(&id.0);
                        self.write_file();
                    }
                    other => panic!("an open planned {other:?}"),
                }
            }
            // I5: an offer in a run never settles the text of a field the save machine holds
            // (an older item of its run, which a full storage left, may go).
            let current = match &self.page[field].live {
                Live::Clean => None,
                Live::Typed { current, .. }
                | Live::Sending { current, .. }
                | Live::Landed { current, .. } => Some(current.at),
            };
            for slot in &self.page[field].settle {
                prop_assert!(
                    !(self.owned(&slot.key, slot.instance)
                        && current.is_some_and(|at| slot.at.0 >= at)),
                    "the page's own field {field} is settled: {slot:?}"
                );
            }
        }
        Ok(())
    }

    /// The question shows once the page has settled what the offer answered.
    fn asking(&self) -> bool {
        self.mode == Mode::Writable && self.page.iter().all(|f| f.settle.is_empty())
    }

    fn keep(&mut self, field: usize, index: usize) {
        if !self.asking() {
            return;
        }
        let actions = self.step(field, Event::Keep(index));
        for action in actions {
            match action {
                Action::Keep { text, over, .. } => {
                    self.store.save(field, &text, over);
                    // tc-store's `kept`: the offered entries of the field with this text.
                    let all = self.seeded(Seed::KeepTakesTheFieldsOtherTexts);
                    self.drafts
                        .offered
                        .retain(|_, e| e.field != field || !(all || same_text(&e.text, &text)));
                    // An offer stands for earlier runs' texts: the page's own are never offered.
                    let run = self.run;
                    for item in self
                        .items
                        .iter_mut()
                        .filter(|((f, r, _), _)| *f == field && *r != run)
                    {
                        if item.1.open() && same_text(&item.1.text, &text) {
                            item.1.answered = Answered::Kept;
                        }
                    }
                }
                Action::Unstore(id) => {
                    self.drafts.offered.remove(&id.0);
                    self.write_file();
                }
                other => panic!("Keep planned {other:?}"),
            }
        }
    }

    fn discard(&mut self, field: usize, index: usize) {
        if !self.asking() {
            return;
        }
        let text = self.page[field].offers.get(index).map(|o| o.text.clone());
        let actions = self.step(field, Event::Discard(index));
        for action in actions {
            match action {
                Action::Unstore(id) => {
                    self.drafts.offered.remove(&id.0);
                    if self.seeded(Seed::DiscardTakesTheFieldsDraft) {
                        self.drafts.current.remove(&field);
                    }
                    self.write_file();
                }
                other => panic!("Discard planned {other:?}"),
            }
        }
        if let Some(text) = text {
            let run = self.run;
            for item in self
                .items
                .iter_mut()
                .filter(|((f, r, _), _)| *f == field && *r != run)
            {
                if item.1.open() && same_text(&item.1.text, &text) {
                    item.1.answered = Answered::Discarded;
                }
            }
        }
    }

    fn settle(&mut self) {
        for field in 0..FIELDS {
            let actions = self.step(field, Event::Settle);
            let mut own = Vec::new();
            for action in actions {
                let Action::Settle(slot) = action else {
                    own.push(action);
                    continue;
                };
                let mut lines = offer::settle(&mut self.storage, std::slice::from_ref(&slot), &[]);
                if self.seeded(Seed::MirrorRemovalIgnoresText) {
                    lines = vec![buffer::removed_line(&slot.key, slot.instance).unwrap()];
                }
                for line in lines {
                    self.mirror_line(Some(line));
                }
            }
            // The page's own text, written again into what the settling freed (the same
            // write `offer::settle` makes of the machine's templates).
            if !self.seeded(Seed::MirrorRemovalIgnoresText) {
                self.carry_out(field, own);
            }
        }
    }

    // --- read-only ----------------------------------------------------------------------

    fn copy(&mut self) -> Check {
        if self.mode == Mode::Writable {
            return Ok(());
        }
        // tc-app's `copy` looks at the projects folder itself: CreateRefusal::CopyUnsafe, and
        // no copy is made.
        if self.unsafe_place && !self.seeded(Seed::CopyIntoTheUnsafeFolder) {
            return Ok(());
        }
        self.copies += 1;
        let to = ProjectRef::parse(&format!("en_ult - Copy {}", self.copies)).unwrap();
        let mirror = if self.seeded(Seed::CopyLeavesTheMirror) {
            Vec::new()
        } else {
            self.mirror.clone()
        };
        let copied = offer::copy(
            &mut self.storage,
            &mirror,
            (&self.folder, &id()),
            (&to, &id()),
        );
        for line in copied.lines {
            self.mirror_line(Some(line));
        }
        // An item the storage would not take under the copy stays with the original folder.
        for slot in &copied.stayed {
            let run = slot.instance.bytes()[0];
            for ((_, r, _), item) in &mut self.items {
                if *r == run && same_text(&item.text, &slot.text) {
                    item.in_storage = false;
                }
            }
        }
        // The copy is another folder: no lock, the place it lies in.
        self.folder = to;
        for field in 0..FIELDS {
            let actions = self.step(
                field,
                Event::Copy {
                    slots: Vec::new(),
                    to: self.key(field),
                },
            );
            assert!(
                actions
                    .iter()
                    .all(|a| matches!(a, Action::CopyFolder | Action::Rekey { .. }))
            );
            // A save the read-only project refused; the page sends the field's text to the copy.
            if self.inflight[field].is_some() {
                self.refused(field);
            }
        }
        self.lock_elsewhere = false;
        self.trusted = false;
        self.drafts = DraftsFile::default();
        self.open();
        prop_assert_ne!(
            self.mode,
            Mode::ReadOnly(Why::Unsafe),
            "I3: the copy opens read-only for the reason it was made"
        );
        Ok(())
    }

    fn that_computer_is_closed(&mut self) -> Check {
        if self.mode != Mode::ReadOnly(Why::OtherComputer) {
            return Ok(());
        }
        self.trusted = true;
        if !self.seeded(Seed::ReopenJoinsTheLeavingSession) {
            for field in 0..FIELDS {
                let actions = self.step(field, Event::Reload);
                assert!(actions.is_empty());
            }
            self.open();
        }
        prop_assert_ne!(
            self.mode,
            Mode::ReadOnly(Why::OtherComputer),
            "I4: the project stays read-only after That computer is closed"
        );
        Ok(())
    }

    // --- the properties -----------------------------------------------------------------

    /// A crash, a relaunch and the open's offer on a copy of the world; I1 and I2.
    fn observe(&self) -> Check {
        let mut after = self.clone();
        after.relaunch();
        if after.mode != Mode::Writable {
            return Ok(());
        }
        after.offer()?;
        for field in 0..FIELDS {
            let offered: Vec<&str> = after.page[field]
                .offers
                .iter()
                .map(|o| o.text.as_str())
                .collect();
            let heads: Vec<&str> = after.store.heads[field]
                .iter()
                .map(|v| after.store.text(*v))
                .collect();
            for ((f, run, _), item) in &self.items {
                // A version after the one its stretch is typed over held it: stored, whoever
                // wrote it.
                let held = item.held_then
                    || after.store.held_since(field, item.base, &item.text)
                    || after.store.made_since(field, item.since, &item.text);
                if *f != field || !item.open() || held || !item.durable() {
                    continue;
                }
                let held = offered.iter().any(|t| same_text(t, &item.text))
                    || heads.iter().any(|t| same_text(t, &item.text));
                prop_assert!(
                    held,
                    "I1: run {run}'s last text {:?} of field {field} is neither stored nor \
                     offered (offered {offered:?}, heads {heads:?})",
                    item.text
                );
            }
            for text in &offered {
                let open = self.items.iter().any(|((f, _, _), item)| {
                    *f == field
                        && (((item.open() || item.undone()) && same_text(&item.text, text))
                            || ((!item.durable() || item.refused || !self.shell)
                                && item.history.iter().any(|t| same_text(t, text))))
                });
                prop_assert!(
                    open,
                    "I2: {text:?} of field {field} is offered but is no unanswered last text"
                );
            }
        }
        Ok(())
    }

    fn apply(&mut self, op: &Op) -> Check {
        match *op {
            Op::Type(f, t) => self.typed(f, TEXTS[t]),
            Op::DraftTick(f) => {
                if self.mode == Mode::Writable {
                    let actions = self.step(f, Event::DraftTick);
                    self.carry_out(f, actions);
                }
            }
            Op::Flush => {
                if self.mode == Mode::Writable {
                    self.write_file();
                }
            }
            Op::Send(f) => {
                let actions = self.step(f, Event::SaveSent);
                self.carry_out(f, actions);
            }
            Op::Land(f) => self.landed(f),
            Op::Ack(f) => self.acked(f),
            Op::Refuse(f) => self.refused(f),
            Op::Teammate(f, t) => {
                let over = self.store.shown(f);
                self.store.save(f, TEXTS[t], over);
            }
            Op::Reload => {
                for field in 0..FIELDS {
                    let actions = self.step(field, Event::Reload);
                    assert!(actions.is_empty());
                }
            }
            Op::Crash => self.relaunch(),
            Op::CrashNewOrigin => {
                self.storage = MemStore::new();
                for item in self.items.values_mut() {
                    item.in_storage = false;
                }
                self.relaunch();
            }
            Op::StorageFull => self.storage.fail_writes = true,
            Op::StorageOk => self.storage.fail_writes = false,
            Op::Offer => self.offer()?,
            Op::Keep(f, i) => self.keep(f, i),
            Op::Discard(f, i) => self.discard(f, i),
            Op::Settle => self.settle(),
            Op::SecondWriter => {
                if self.mode == Mode::Writable {
                    self.mode = Mode::ReadOnly(Why::SecondWriter);
                    self.drafts = self.file.clone();
                }
            }
            Op::Copy => self.copy()?,
            Op::OtherComputer => self.lock_elsewhere = true,
            Op::ThatComputerClosed => self.that_computer_is_closed()?,
            Op::Unsafe => self.unsafe_place = true,
            Op::Safe => self.unsafe_place = false,
            Op::Observe => self.observe()?,
        }
        Ok(())
    }
}

fn play(seed: Option<Seed>, shell: bool, ops: &[Op]) -> Check {
    let mut world = World::new(seed, shell);
    for op in ops {
        world.apply(op)?;
    }
    world.observe()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// I1 to I5 hold for every interleaving.
    #[test]
    fn every_typed_text_is_stored_offered_or_answered((shell, ops) in script()) {
        play(None, shell, &ops)?;
    }
}

/// Each finding of the review, put back, breaks a property within the cases the runner tries.
#[test]
fn the_property_catches_every_finding_put_back() {
    let mut missed = Vec::new();
    for seed in SEEDS {
        let mut runner = TestRunner::new_with_rng(
            Config {
                cases: 20_000,
                failure_persistence: None,
                ..Config::default()
            },
            TestRng::deterministic_rng(RngAlgorithm::ChaCha),
        );
        match runner.run(&script(), |(shell, ops)| play(Some(seed), shell, &ops)) {
            Err(TestError::Fail(why, (shell, ops))) => {
                eprintln!("{seed:?}: {why}\n  shell {shell}, {ops:?}");
            }
            other => missed.push((seed, format!("{other:?}"))),
        }
    }
    assert!(missed.is_empty(), "not caught: {missed:#?}");
}
