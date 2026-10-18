//! A team of three to five devices that edit one project from their own stale views and
//! exchange random subsets of what they hold (§16.1). Every change is kept as a recipe
//! whose parents, and the change ids inside its value, are labels (indexes into the
//! recipes), so the same history can be written again with other clocks, other device ids
//! or fields from a newer minor, and compared label by label.

// Generators build known-valid values and may fail loudly.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::large_enum_variant
)]

use crate::support::{
    Dev, check, check_key, covered, decision, md5, oid, pins, presence_of, verse, words_of,
};
use proptest::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use tc_journal::change::{Verified, parse_change};
use tc_journal::command::{Choice, PlannedChange};
use tc_journal::fold::{
    Anomaly, BookView, Conflict, ConflictKind, FoldInput, FoldState, ProjectKind, ResolveError,
    Resolved,
};
use tc_journal::json::{Json, JsonMap};
use tc_journal::record::{
    AlignmentValue, BatchReleaseValue, BookPresence, CarriedValue, ChapterFront, ChapterVerse,
    CheckValue, CommentTarget, CommentValue, LayoutItem, MarkerWs, PresentHow, Scope, TargetWord,
    UserStatus, UsfmBytes, VerseValue,
};
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, ChangeId, ChapterNo, CheckKey, CommentId,
    DeviceId, DeviceSeq, ImportSource, MetaField, MetaPath, PinGroup, Record, RecordKey, RepoPath,
    SettingKey, ToolId, UnknownKey, VerseNo, WallTime,
};

// ---------------------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------------------

/// One step of a script. Numbers pick among what the acting device sees, so every script
/// is valid whatever came before.
#[derive(Debug, Clone)]
pub enum Op {
    Edit {
        dev: u8,
        at: u8,
        stale: bool,
        newer: bool,
    },
    Layout {
        dev: u8,
        at: u8,
    },
    Join {
        dev: u8,
        at: u8,
        len: u8,
    },
    Split {
        dev: u8,
        at: u8,
    },
    Renumber {
        dev: u8,
        at: u8,
    },
    Drag {
        dev: u8,
        at: u8,
    },
    Remove {
        dev: u8,
        ruth: bool,
    },
    Restore {
        dev: u8,
        ruth: bool,
        text: bool,
    },
    Align {
        dev: u8,
        at: u8,
        current: bool,
        newer: bool,
    },
    Decide {
        dev: u8,
        check: u8,
        what: u8,
        stale: bool,
    },
    CarryOver {
        dev: u8,
        check: u8,
    },
    Comment {
        dev: u8,
        on: u8,
    },
    Pins {
        dev: u8,
        slot: u8,
        value: u8,
    },
    Setting {
        dev: u8,
        path: u8,
        value: u8,
    },
    Meta {
        dev: u8,
        field: u8,
        value: u8,
    },
    Carried {
        dev: u8,
        path: u8,
        value: u8,
    },
    BookLayout {
        dev: u8,
        seed: u8,
    },
    Header {
        dev: u8,
        front: bool,
    },
    Resolve {
        dev: u8,
        pick: u8,
        how: u8,
    },
    Release {
        dev: u8,
    },
    Exchange {
        from: u8,
        to: u8,
        mode: u8,
        seed: u8,
    },
    Twin {
        dev: u8,
        at: u8,
    },
    Unknown {
        dev: u8,
        value: u8,
    },
    Poison {
        dev: u8,
        at: u8,
    },
    /// Two devices that hold the same changes act on one record before they exchange
    /// again: the way most conflicts and merges start.
    Fork {
        a: u8,
        b: u8,
        what: u8,
        at: u8,
    },
    /// One verse record written alone with a structure its neighbours do not share (a
    /// foreign tool's edit): what the coherence pass is for.
    Stray {
        dev: u8,
        at: u8,
        covered: bool,
    },
    /// The project's versification (`vrs.json`).
    Versification {
        dev: u8,
        value: u8,
    },
    /// The device's own profile (its display name).
    Profile {
        dev: u8,
        value: u8,
    },
}

#[derive(Debug, Clone)]
pub struct Script {
    pub devices: u8,
    pub ops: Vec<Op>,
    /// The script ends with everyone in step and the last two books removed at the same
    /// time (§7.6): a state any later edit of either book would change.
    pub last_books_removed: bool,
}

pub fn arb_op() -> impl Strategy<Value = Op> {
    let d = || any::<u8>();
    prop_oneof![
        10 => (d(), d(), any::<bool>(), prop::bool::weighted(0.15))
            .prop_map(|(dev, at, stale, newer)| Op::Edit { dev, at, stale, newer }),
        2 => (d(), d()).prop_map(|(dev, at)| Op::Layout { dev, at }),
        3 => (d(), d(), d()).prop_map(|(dev, at, len)| Op::Join { dev, at, len }),
        2 => (d(), d()).prop_map(|(dev, at)| Op::Split { dev, at }),
        2 => (d(), d()).prop_map(|(dev, at)| Op::Renumber { dev, at }),
        2 => (d(), d()).prop_map(|(dev, at)| Op::Drag { dev, at }),
        2 => (d(), any::<bool>()).prop_map(|(dev, ruth)| Op::Remove { dev, ruth }),
        2 => (d(), any::<bool>(), any::<bool>())
            .prop_map(|(dev, ruth, text)| Op::Restore { dev, ruth, text }),
        3 => (d(), d(), any::<bool>(), prop::bool::weighted(0.2))
            .prop_map(|(dev, at, current, newer)| Op::Align { dev, at, current, newer }),
        4 => (d(), d(), d(), any::<bool>())
            .prop_map(|(dev, check, what, stale)| Op::Decide { dev, check, what, stale }),
        3 => (d(), d()).prop_map(|(dev, check)| Op::CarryOver { dev, check }),
        2 => (d(), d()).prop_map(|(dev, on)| Op::Comment { dev, on }),
        2 => (d(), d(), d()).prop_map(|(dev, slot, value)| Op::Pins { dev, slot, value }),
        2 => (d(), d(), d()).prop_map(|(dev, path, value)| Op::Setting { dev, path, value }),
        2 => (d(), d(), d()).prop_map(|(dev, field, value)| Op::Meta { dev, field, value }),
        2 => (d(), d(), d()).prop_map(|(dev, path, value)| Op::Carried { dev, path, value }),
        2 => (d(), d()).prop_map(|(dev, seed)| Op::BookLayout { dev, seed }),
        3 => (d(), any::<bool>()).prop_map(|(dev, front)| Op::Header { dev, front }),
        4 => (d(), d(), d()).prop_map(|(dev, pick, how)| Op::Resolve { dev, pick, how }),
        3 => d().prop_map(|dev| Op::Release { dev }),
        12 => (d(), d(), d(), d())
            .prop_map(|(from, to, mode, seed)| Op::Exchange { from, to, mode, seed }),
        1 => (d(), d()).prop_map(|(dev, at)| Op::Twin { dev, at }),
        2 => (d(), d()).prop_map(|(dev, value)| Op::Unknown { dev, value }),
        1 => (d(), d()).prop_map(|(dev, at)| Op::Poison { dev, at }),
        12 => (d(), d(), d(), d()).prop_map(|(a, b, what, at)| Op::Fork { a, b, what, at }),
        1 => (d(), d(), any::<bool>()).prop_map(|(dev, at, covered)| Op::Stray { dev, at, covered }),
        1 => (d(), d()).prop_map(|(dev, value)| Op::Versification { dev, value }),
        1 => (d(), d()).prop_map(|(dev, value)| Op::Profile { dev, value }),
    ]
}

pub fn arb_script() -> impl Strategy<Value = Script> {
    (
        3u8..=5,
        prop::collection::vec(arb_op(), 20..70),
        prop::bool::weighted(0.3),
    )
        .prop_map(|(devices, ops, last_books_removed)| Script {
            devices,
            ops,
            last_books_removed,
        })
}

// ---------------------------------------------------------------------------------------
// Recipes
// ---------------------------------------------------------------------------------------

/// A record, with the changes its value names given as labels.
#[derive(Debug, Clone)]
pub enum Template {
    Plain(Record),
    Removed { book: BookId, seen: Vec<usize> },
    Release { batch: BatchId, changes: Vec<usize> },
}

/// How a change line is made from its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forge {
    /// `Change::new`.
    Plain,
    /// From a newer minor: an inert field on the change and in its value, which this app
    /// ignores but keeps in the stored bytes.
    Newer,
    /// A record kind from a newer minor on the verse of the body's record.
    UnknownKind(u8),
    /// A line whose hash verifies but which breaks rule V5 (a parent named twice).
    Poison,
}

#[derive(Debug, Clone)]
pub struct Recipe {
    pub template: Template,
    pub parents: Vec<usize>,
    /// The writing device (twins share it).
    pub device: usize,
    pub seq: u64,
    pub at: i64,
    pub batch: BatchRef,
    pub cause: Cause,
    pub forge: Forge,
    /// Every parent was held by the writing device when it wrote (I4).
    pub parents_held: bool,
}

impl Recipe {
    /// The record key, as the line's change holds it (an unknown kind's key for a forged
    /// unknown record).
    pub fn key(&self) -> RecordKey {
        match (&self.template, self.forge) {
            (Template::Plain(record), Forge::UnknownKind(_)) => unknown_key(record),
            (Template::Plain(record), _) => record.key(),
            (Template::Removed { book, .. }, _) => RecordKey::BookPresence(*book),
            (Template::Release { batch, .. }, _) => RecordKey::BatchRelease(*batch),
        }
    }
}

pub const UNKNOWN_KIND: &str = "verseAudio";

fn verse_fields(record: &Record) -> (BookId, u16, u16) {
    match record {
        Record::Verse {
            book,
            chapter,
            verse,
            ..
        } => (*book, chapter.get(), verse.get()),
        _ => (BookId::Titus, 1, 1),
    }
}

pub fn unknown_key(carrier: &Record) -> RecordKey {
    let (book, c, v) = verse_fields(carrier);
    let key = Json::Obj(JsonMap::from([
        ("book".to_owned(), Json::from(book.code())),
        ("chapter".to_owned(), Json::from(c)),
        ("verse".to_owned(), Json::from(v)),
    ]));
    RecordKey::Unknown(UnknownKey {
        kind: UNKNOWN_KIND.into(),
        key_json: key.to_canonical_string().into(),
    })
}

/// Writes recipes as change lines: `ids` gives the id each earlier label got.
pub struct Mint<'a> {
    pub at: &'a dyn Fn(usize, i64) -> i64,
    pub device: &'a dyn Fn(usize) -> DeviceId,
    pub newer: &'a dyn Fn(usize, Forge) -> Forge,
}

pub fn mint(recipes: &[Recipe], how: &Mint) -> Vec<Option<Verified>> {
    let mut out: Vec<Option<Verified>> = Vec::with_capacity(recipes.len());
    let mut ids: Vec<Option<ChangeId>> = Vec::with_capacity(recipes.len());
    for (label, recipe) in recipes.iter().enumerate() {
        let line = mint_one(label, recipe, &ids, how);
        ids.push(line.as_ref().map(Verified::id));
        out.push(line);
    }
    out
}

fn mint_one(
    label: usize,
    recipe: &Recipe,
    ids: &[Option<ChangeId>],
    how: &Mint,
) -> Option<Verified> {
    let id_of = |l: &usize| ids.get(*l).copied().flatten();
    let record = match &recipe.template {
        // A device's profile names its writer (V4), whatever id the writer is given.
        Template::Plain(Record::DeviceProfile { value, .. }) => Record::DeviceProfile {
            device: (how.device)(recipe.device),
            value: value.clone(),
        },
        Template::Plain(record) => record.clone(),
        Template::Removed { book, seen } => {
            let mut seen: Vec<_> = seen.iter().filter_map(id_of).map(|i| i.short()).collect();
            seen.sort();
            seen.dedup();
            Record::BookPresence {
                book: *book,
                value: BookPresence::Removed { seen },
            }
        }
        Template::Release { batch, changes } => {
            let mut changes: Vec<ChangeId> = changes.iter().filter_map(id_of).collect();
            changes.sort();
            changes.dedup();
            Record::BatchRelease {
                batch: *batch,
                value: BatchReleaseValue { changes },
            }
        }
    };
    let mut parents: Vec<ChangeId> = recipe.parents.iter().filter_map(id_of).collect();
    parents.sort();
    parents.dedup();
    let body = ChangeBody {
        v: 0,
        record,
        parents,
        device: (how.device)(recipe.device),
        seq: DeviceSeq(recipe.seq),
        at: WallTime((how.at)(label, recipe.at)),
        batch: recipe.batch,
        cause: recipe.cause.clone(),
    };
    let change = Change::new(body).ok()?;
    match (how.newer)(label, recipe.forge) {
        Forge::Plain => Some(Verified::Change(change)),
        Forge::Newer => Some(forge(&change, |map| {
            map.insert("v".into(), Json::from(tc_journal::FORMAT_MINOR + 1));
            map.insert("xNewer".into(), Json::from(1u8));
            if let Some(Json::Obj(record)) = map.get_mut("record")
                && let Some((_, Json::Obj(inner))) = record.iter_mut().next()
                && let Some(Json::Obj(value)) = inner.get_mut("value")
            {
                for nested in ["text", "decision"] {
                    if let Some(Json::Obj(obj)) = value.get_mut(nested) {
                        obj.insert("xNewer".into(), Json::from(1u8));
                    }
                }
            }
        })),
        Forge::UnknownKind(n) => {
            let (book, c, v) = verse_fields(&change.body().record);
            Some(forge(&change, |map| {
                map.insert("v".into(), Json::from(tc_journal::FORMAT_MINOR + 1));
                map.insert(
                    "record".into(),
                    Json::Obj(JsonMap::from([(
                        UNKNOWN_KIND.to_owned(),
                        Json::Obj(JsonMap::from([
                            ("book".to_owned(), Json::from(book.code())),
                            ("chapter".to_owned(), Json::from(c)),
                            ("verse".to_owned(), Json::from(v)),
                            (
                                "value".to_owned(),
                                Json::Obj(JsonMap::from([(
                                    "oid".to_owned(),
                                    Json::from(oid(n).to_hex()),
                                )])),
                            ),
                        ])),
                    )])),
                );
            }))
        }
        Forge::Poison => Some(forge(&change, |map| {
            let twice = match map.get("parents") {
                Some(Json::Arr(list)) if !list.is_empty() => {
                    vec![list[0].clone(), list[0].clone()]
                }
                _ => vec![Json::from("0".repeat(64)), Json::from("0".repeat(64))],
            };
            map.insert("parents".into(), Json::Arr(twice));
        })),
    }
}

fn forge(change: &Change, edit: impl FnOnce(&mut JsonMap)) -> Verified {
    let mut json = tc_journal::json::parse(change.raw()).expect("canonical");
    if let Json::Obj(map) = &mut json {
        edit(map);
    }
    let raw: Arc<[u8]> = json.to_canonical().into();
    parse_change(ChangeId::of(&raw), raw)
}

// ---------------------------------------------------------------------------------------
// The team
// ---------------------------------------------------------------------------------------

pub struct Device {
    pub id: DeviceId,
    seq: u64,
    clock: i64,
    batches: u32,
    /// Labels in the order this device received them.
    pub held: Vec<usize>,
    has: BTreeSet<usize>,
    pub state: FoldState,
    /// Screens drawn earlier and still open: the token, what was shown, whether the
    /// record was in conflict.
    screens: BTreeMap<RecordKey, Screen>,
}

#[derive(Debug, Clone)]
pub struct Screen {
    pub token: Vec<ChangeId>,
    pub shown: Option<Record>,
    pub heads: Vec<ChangeId>,
    pub conflicted: bool,
}

/// One change a person's screen wrote: the token it carried and what the screen showed.
#[derive(Debug, Clone)]
pub struct Write {
    pub label: usize,
    pub screen: Screen,
}

pub struct Sim {
    pub devices: Vec<Device>,
    pub recipes: Vec<Recipe>,
    pub items: Vec<Option<Verified>>,
    pub labels: HashMap<ChangeId, usize>,
    pub writes: Vec<Write>,
    counter: u32,
    /// Compare every device's incremental state with a fold from scratch after each step.
    pub check_incremental: bool,
    /// States reached on the way (some, such as a held batch, are gone at the end).
    pub reached: BTreeSet<&'static str>,
}

/// The checks the team decides: translationNotes and translationWords of Titus, and
/// translationNotes of Ruth.
pub const CHECKS: [(ToolId, BookId, u16, u16); 6] = [
    (ToolId::TranslationNotes, BookId::Titus, 1, 1),
    (ToolId::TranslationNotes, BookId::Titus, 1, 3),
    (ToolId::TranslationNotes, BookId::Titus, 2, 2),
    (ToolId::TranslationNotes, BookId::Titus, 1, 5),
    (ToolId::TranslationWords, BookId::Titus, 1, 2),
    (ToolId::TranslationNotes, BookId::Ruth, 1, 1),
];

impl Sim {
    pub fn new(devices: u8) -> Self {
        let mut sim = Self {
            devices: (0..devices)
                .map(|n| {
                    let dev = Dev::new(n + 1);
                    Device {
                        id: dev.id,
                        seq: 0,
                        clock: dev.at,
                        batches: 0,
                        held: Vec::new(),
                        has: BTreeSet::new(),
                        state: FoldState::new(ProjectKind::Bible),
                        screens: BTreeMap::new(),
                    }
                })
                .collect(),
            recipes: Vec::new(),
            items: Vec::new(),
            labels: HashMap::new(),
            writes: Vec::new(),
            counter: 0,
            check_incremental: false,
            reached: BTreeSet::new(),
        };
        // Maria imports Titus and Ruth; Juan adds Titus from a template at the same time.
        let titus = import_items(BookId::Titus, &[5, 4]);
        sim.write(
            0,
            titus,
            Cause::Import(ImportSource::Usfm {
                file_name: "TIT.usfm".into(),
            }),
        );
        let ruth = import_items(BookId::Ruth, &[3]);
        sim.write(
            0,
            ruth,
            Cause::Import(ImportSource::Usfm {
                file_name: "RUT.usfm".into(),
            }),
        );
        let seed = seed_items(BookId::Titus, &[5, 4, 2], "___");
        sim.write(1, seed, Cause::Seed);
        // Ana's template too, with empty stubs: chapter 3 exists only in the two
        // templates, so every head of its records is a seed stub (§7.3 rule 2).
        let seed = seed_items(BookId::Titus, &[5, 4, 2], "");
        sim.write(2, seed, Cause::Seed);
        for to in 1..sim.devices.len() {
            sim.exchange(0, to, 0, 0);
        }
        sim.exchange(1, 0, 0, 0);
        sim.exchange(2, 0, 0, 0);
        sim
    }

    pub fn run(script: &Script, check_incremental: bool) -> Self {
        let mut sim = Self::new(script.devices);
        sim.check_incremental = check_incremental;
        for op in &script.ops {
            sim.step(op);
        }
        if script.last_books_removed {
            sim.sync_all();
            sim.remove(0, BookId::Titus);
            sim.remove(1, BookId::Ruth);
        }
        sim
    }

    /// Everyone receives everything.
    pub fn sync_all(&mut self) {
        for from in 0..self.devices.len() {
            for to in 0..self.devices.len() {
                if from != to {
                    self.exchange(from, to, 0, 0);
                }
            }
        }
    }

    /// Every line written, in label order.
    pub fn union(&self) -> Vec<FoldInput> {
        self.items
            .iter()
            .flatten()
            .map(|line| FoldInput::new(line.clone(), None))
            .collect()
    }

    pub fn union_state(&self) -> FoldState {
        FoldState::fold(ProjectKind::Bible, self.union())
    }

    fn dev(&self, n: u8) -> usize {
        usize::from(n) % self.devices.len()
    }

    fn fresh(&mut self) -> u32 {
        self.counter += 1;
        self.counter
    }

    fn fresh_words(&mut self, d: usize, n: usize) -> String {
        (0..n)
            .map(|_| format!("d{d}w{}", self.fresh()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    // -----------------------------------------------------------------------------------
    // Writing
    // -----------------------------------------------------------------------------------

    /// One batch from device `d`; returns the labels, or nothing when a change breaks a
    /// rule (the actor would refuse the command).
    fn write(
        &mut self,
        d: usize,
        items: Vec<(Template, Vec<ChangeId>)>,
        cause: Cause,
    ) -> Vec<usize> {
        let forged = items
            .into_iter()
            .map(|(t, p)| (t, p, Forge::Plain))
            .collect();
        self.write_forged(d, forged, cause, None)
    }

    fn write_forged(
        &mut self,
        d: usize,
        items: Vec<(Template, Vec<ChangeId>, Forge)>,
        cause: Cause,
        twin_seq: Option<u64>,
    ) -> Vec<usize> {
        if items.is_empty() {
            return Vec::new();
        }
        let dev = &mut self.devices[d];
        dev.batches += 1;
        let mut id = [0u8; 16];
        id[..15].copy_from_slice(&dev.id.as_bytes()[1..]);
        id[12..16].copy_from_slice(&dev.batches.to_be_bytes());
        if twin_seq.is_some() {
            id[11] ^= 0x80;
        }
        let Some(batch) = BatchRef::new(BatchId::from_bytes(id), items.len() as u32) else {
            return Vec::new();
        };
        let start = self.recipes.len();
        let mut seq = twin_seq.unwrap_or(dev.seq);
        let mut clock = dev.clock;
        let mut recipes = Vec::new();
        for (template, parents, forge) in items {
            if twin_seq.is_none() {
                seq += 1;
            }
            clock += 60_000;
            let parents: Vec<usize> = parents
                .iter()
                .filter_map(|p| self.labels.get(p).copied())
                .collect();
            let parents_held = parents.iter().all(|p| self.devices[d].has.contains(p));
            recipes.push(Recipe {
                parents_held,
                template,
                parents,
                device: d,
                seq,
                at: clock,
                batch,
                cause: cause.clone(),
                forge,
            });
        }
        // Mint against the labels so far; refuse the batch if a plain change breaks a rule.
        let identity = Mint {
            at: &|_, at| at,
            device: &|d| self.devices[d].id,
            newer: &|_, forge| forge,
        };
        let mut lines = Vec::new();
        let ids: Vec<Option<ChangeId>> = self
            .items
            .iter()
            .map(|i| i.as_ref().map(Verified::id))
            .collect();
        let mut ids = ids;
        for (i, recipe) in recipes.iter().enumerate() {
            let line = mint_one(start + i, recipe, &ids, &identity);
            match &line {
                Some(Verified::Change(_)) => {}
                Some(Verified::Quarantined(_)) if recipe.forge != Forge::Plain => {}
                _ => return Vec::new(),
            }
            ids.push(line.as_ref().map(Verified::id));
            lines.push(line);
        }
        let dev = &mut self.devices[d];
        if twin_seq.is_none() {
            dev.seq = seq;
        }
        dev.clock = clock;
        let mut labels = Vec::new();
        for (recipe, line) in recipes.into_iter().zip(lines) {
            let label = self.recipes.len();
            if let Some(line) = &line {
                self.labels.insert(line.id(), label);
            }
            self.recipes.push(recipe);
            self.items.push(line);
            labels.push(label);
        }
        self.receive(d, &labels);
        labels
    }

    /// A person's edit of one record from a screen. Saving what the screen shows writes
    /// nothing (§5.0).
    fn write_screen(
        &mut self,
        d: usize,
        record: Record,
        screen: Screen,
        cause: Cause,
        forge: Forge,
    ) {
        if screen.shown.as_ref() == Some(&record) {
            return;
        }
        let labels = self.write_forged(
            d,
            vec![(Template::Plain(record), screen.token.clone(), forge)],
            cause,
            None,
        );
        for label in labels {
            self.writes.push(Write {
                label,
                screen: screen.clone(),
            });
        }
    }

    /// A section of records from fresh screens, as one batch.
    fn write_section(&mut self, d: usize, records: Vec<Record>, cause: Cause) {
        let mut items = Vec::new();
        let mut screens = Vec::new();
        for record in records {
            let screen = self.screen(d, &record.key(), false);
            if screen.shown.as_ref() == Some(&record) {
                continue;
            }
            items.push((Template::Plain(record), screen.token.clone()));
            screens.push(screen);
        }
        let labels = self.write(d, items, cause);
        for (label, screen) in labels.into_iter().zip(screens) {
            self.writes.push(Write { label, screen });
        }
    }

    /// The screen of a record: an open one when `stale` and one is open, else drawn now.
    fn screen(&mut self, d: usize, key: &RecordKey, stale: bool) -> Screen {
        if stale && let Some(screen) = self.devices[d].screens.get(key) {
            return screen.clone();
        }
        let state = &self.devices[d].state;
        let conflicted = matches!(
            state.key_state(key).map(|k| k.state),
            Some(Resolved::Conflict(_))
        );
        let screen = Screen {
            token: state.shown_heads(key, None),
            shown: state.projected(key),
            heads: state.heads(key),
            conflicted,
        };
        self.devices[d].screens.insert(key.clone(), screen.clone());
        screen
    }

    fn receive(&mut self, to: usize, labels: &[usize]) {
        let mut inputs = Vec::new();
        for &label in labels {
            if !self.devices[to].has.insert(label) {
                continue;
            }
            self.devices[to].held.push(label);
            if let Some(line) = &self.items[label] {
                inputs.push(FoldInput::new(line.clone(), None));
            }
        }
        if inputs.is_empty() {
            return;
        }
        self.devices[to].state.add(inputs);
        let state = &self.devices[to].state;
        for anomaly in state.anomalies() {
            match anomaly {
                Anomaly::IncompleteBatch { .. } => {
                    self.reached.insert("incomplete batch");
                }
                Anomaly::MissingParent { .. } => {
                    self.reached.insert("missing parent");
                }
                _ => {}
            }
        }
        if self.check_incremental {
            let full = FoldState::fold(
                ProjectKind::Bible,
                self.devices[to]
                    .held
                    .iter()
                    .filter_map(|l| self.items[*l].clone())
                    .map(|line| FoldInput::new(line, None)),
            );
            crate::support::same_summary(&full, &self.devices[to].state, "incremental");
        }
    }

    /// `mode` 0: everything; 1 and 3: a random half; 2: all but one change of one batch.
    pub fn exchange(&mut self, from: usize, to: usize, mode: u8, seed: u8) {
        if from == to {
            return;
        }
        let missing: Vec<usize> = self.devices[from]
            .held
            .iter()
            .copied()
            .filter(|l| !self.devices[to].has.contains(l))
            .collect();
        let send: Vec<usize> = match mode % 4 {
            0 => missing,
            1 | 3 => missing
                .into_iter()
                .filter(|l| (l.wrapping_mul(2_654_435_761) >> 7) as u8 & 1 == seed & 1)
                .collect(),
            _ => {
                let mut by_batch: BTreeMap<BatchId, Vec<usize>> = BTreeMap::new();
                for l in missing {
                    by_batch
                        .entry(self.recipes[l].batch.id)
                        .or_default()
                        .push(l);
                }
                let partial = by_batch
                    .into_values()
                    .filter(|ls| ls.len() >= 2)
                    .nth(usize::from(seed) % 3);
                match partial {
                    Some(mut ls) => {
                        ls.remove(usize::from(seed) % ls.len());
                        ls
                    }
                    None => Vec::new(),
                }
            }
        };
        self.receive(to, &send);
    }

    // -----------------------------------------------------------------------------------
    // What a device sees
    // -----------------------------------------------------------------------------------

    /// The verses of Titus the device has records of, in order.
    fn verse_slots(&self, d: usize) -> Vec<(u16, u16)> {
        self.devices[d]
            .state
            .record_keys()
            .into_iter()
            .filter_map(|k| match k {
                RecordKey::Verse(BookId::Titus, c, v) => Some((c.get(), v.get())),
                _ => None,
            })
            .collect()
    }

    fn slot(&self, d: usize, at: u8) -> Option<(u16, u16)> {
        let slots = self.verse_slots(d);
        (!slots.is_empty()).then(|| slots[usize::from(at) % slots.len()])
    }

    fn text_of(&self, d: usize, c: u16, v: u16) -> Option<VerseValue> {
        match self.devices[d].state.projected(&vkey(c, v)) {
            Some(Record::Verse {
                value: value @ VerseValue::Text { .. },
                ..
            }) => Some(value),
            _ => None,
        }
    }

    /// The verse's text when it is a verse of its own (not a span).
    fn single(
        &self,
        d: usize,
        c: u16,
        v: u16,
    ) -> Option<(MarkerWs, String, String, Option<ChapterVerse>)> {
        match self.text_of(d, c, v)? {
            VerseValue::Text {
                marker,
                words,
                layout,
                last,
                from,
            } if last.get() == v => Some((marker, words, layout, from)),
            _ => None,
        }
    }

    fn live_check(&self, d: usize, key: &CheckKey) -> CheckKey {
        let mut at = key.clone();
        for _ in 0..8 {
            match self.devices[d]
                .state
                .projected(&RecordKey::Check(at.clone()))
            {
                Some(Record::Check {
                    value: CheckValue::MovedTo { to },
                    ..
                }) => at = to,
                _ => break,
            }
        }
        at
    }

    fn book_records(&self, d: usize, book: BookId) -> Vec<usize> {
        self.devices[d]
            .held
            .iter()
            .copied()
            .filter(|l| belongs_to(&self.recipes[*l], book))
            .filter(|l| {
                self.items[*l]
                    .as_ref()
                    .is_some_and(|i| i.as_change().is_some())
            })
            .collect()
    }

    // -----------------------------------------------------------------------------------
    // Steps
    // -----------------------------------------------------------------------------------

    pub fn step(&mut self, op: &Op) {
        match *op {
            Op::Edit {
                dev,
                at,
                stale,
                newer,
            } => self.edit(self.dev(dev), at, stale, newer, None),
            Op::Twin { dev, at } => {
                let d = self.dev(dev);
                let seq = self.devices[d].seq;
                if seq > 0 {
                    self.edit(d, at, false, false, Some(seq));
                    self.reached.insert("twin");
                }
            }
            Op::Layout { dev, at } => self.layout(self.dev(dev), at),
            Op::Join { dev, at, len } => self.join(self.dev(dev), at, 1 + u16::from(len % 2)),
            Op::Split { dev, at } => self.split(self.dev(dev), at),
            Op::Renumber { dev, at } => self.renumber(self.dev(dev), at),
            Op::Drag { dev, at } => self.drag(self.dev(dev), at),
            Op::Remove { dev, ruth } => self.remove(self.dev(dev), book(ruth)),
            Op::Restore { dev, ruth, text } => self.restore(self.dev(dev), book(ruth), text),
            Op::Align {
                dev,
                at,
                current,
                newer,
            } => self.align(self.dev(dev), at, current, newer),
            Op::Decide {
                dev,
                check,
                what,
                stale,
            } => self.decide(self.dev(dev), check, what, stale),
            Op::CarryOver { dev, check } => self.carry_over(self.dev(dev), check),
            Op::Comment { dev, on } => self.comment(self.dev(dev), on),
            Op::Pins { dev, slot, value } => self.pins(self.dev(dev), slot, value),
            Op::Setting { dev, path, value } => self.setting(self.dev(dev), path, value),
            Op::Meta { dev, field, value } => self.meta(self.dev(dev), field, value),
            Op::Carried { dev, path, value } => self.carried(self.dev(dev), path, value),
            Op::BookLayout { dev, seed } => self.book_layout(self.dev(dev), seed),
            Op::Header { dev, front } => self.header(self.dev(dev), front),
            Op::Resolve { dev, pick, how } => self.resolve(self.dev(dev), pick, how),
            Op::Release { dev } => self.release(self.dev(dev)),
            Op::Exchange {
                from,
                to,
                mode,
                seed,
            } => {
                let (from, to) = (self.dev(from), self.dev(to));
                self.exchange(from, to, mode, seed);
            }
            Op::Unknown { dev, value } => self.unknown(self.dev(dev), value),
            Op::Poison { dev, at } => self.poison(self.dev(dev), at),
            Op::Fork { a, b, what, at } => self.fork(self.dev(a), self.dev(b), what, at),
            Op::Stray { dev, at, covered } => self.stray(self.dev(dev), at, covered),
            Op::Versification { dev, value } => self.versification(self.dev(dev), value),
            Op::Profile { dev, value } => {
                let d = self.dev(dev);
                let screen = self.screen(d, &RecordKey::DeviceProfile(self.devices[d].id), false);
                self.profile(d, value, screen, None);
            }
        }
    }

    fn fork(&mut self, a: usize, b: usize, what: u8, at: u8) {
        if a == b {
            return;
        }
        // Both start from the same view.
        self.exchange(a, b, 0, 0);
        self.exchange(b, a, 0, 0);
        // The rarer outcomes (merges by unit, moved checks, newer kinds) come twice.
        let what = match what % 23 {
            14 => 2,
            15 => 6,
            16 => 3,
            17 => 10,
            other => other,
        };
        match what {
            0 => {
                self.edit(a, at, false, false, None);
                self.edit(b, at, false, false, None);
            }
            1 => {
                self.decide(a, at, 0, false);
                self.decide(b, at, 1, false);
            }
            2 => {
                self.decide(a, at, 0, false);
                self.decide(b, at, 2, false);
            }
            3 => {
                self.carry_over(a, at);
                self.decide(b, at, 1, false);
            }
            4 => {
                self.meta(a, at, 0);
                self.meta(b, at, 1);
            }
            5 => {
                self.pins(a, at, 1);
                self.pins(b, at, 2);
            }
            6 => {
                self.setting(a, at, 1);
                self.setting(b, at / 3, 2);
            }
            7 => {
                // The same file two ways, or two files whose names differ in case.
                let other = if at.is_multiple_of(2) { 0 } else { 1 };
                self.carried(a, 0, at | 1);
                self.carried(b, other, (at | 1).wrapping_add(2));
            }
            8 => {
                self.header(a, at.is_multiple_of(2));
                self.header(b, at.is_multiple_of(2));
            }
            9 => {
                self.book_layout(a, at);
                self.book_layout(b, at.wrapping_add(1));
            }
            10 => {
                self.unknown(a, at);
                self.unknown(b, at);
            }
            11 => {
                self.align(a, at, true, false);
                self.align(b, at, false, false);
            }
            12 => {
                self.remove(a, book(at.is_multiple_of(3)));
                self.edit(b, at, false, false, None);
            }
            18 => {
                self.versification(a, at);
                self.versification(b, at.wrapping_add(1));
            }
            19 => {
                // One device's profile saved by two machines that share its identity.
                let key = RecordKey::DeviceProfile(self.devices[a].id);
                let screen = self.screen(a, &key, false);
                self.profile(a, at, screen.clone(), None);
                let seq = self.devices[a].seq;
                self.profile(a, at.wrapping_add(1), screen, Some(seq));
                self.reached.insert("twin");
            }
            20 => {
                // The last two books removed at the same time (§7.6).
                self.remove(a, BookId::Titus);
                self.remove(b, BookId::Ruth);
            }
            22 => {
                // A foreign tool rewrites one verse's structure while a person edits it: a
                // structure conflict of one record (§8.1).
                self.stray(a, at, at.is_multiple_of(2));
                self.edit(b, at, false, false, None);
            }
            21 => {
                // Two comments under one id: a broken random source (§8.1), on two verses.
                let id = self.comment_id(a);
                self.comment_as(a, at | 1, id);
                self.comment_as(b, (at | 1).wrapping_add(3), id);
            }
            _ => {
                let book = book(at.is_multiple_of(3));
                if self.devices[a].state.book(book).projects() {
                    self.remove(a, book);
                    self.exchange(a, b, 0, 0);
                }
                self.restore(a, book, true);
                self.restore(b, book, false);
            }
        }
    }

    fn stray(&mut self, d: usize, at: u8, covered: bool) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let key = vkey(c, v);
        let screen = self.screen(d, &key, false);
        let value = if covered && v > 1 {
            VerseValue::Covered { by: vno(v - 1) }
        } else {
            let words = self.fresh_words(d, 2);
            VerseValue::Text {
                marker: MarkerWs::canonical(),
                words,
                layout: "\n".into(),
                last: vno(v + 1),
                from: None,
            }
        };
        let cause = Cause::External {
            source: tc_journal::ExternalSource::LocalFile {
                path: RepoPath::parse("ingredients/TIT.usfm").expect("path"),
                seen_md5: md5("before"),
            },
            unknown_base: false,
        };
        self.write_screen(d, verse(c, v, value), screen, cause, Forge::Plain);
        self.reached.insert("stray");
    }

    fn edit(&mut self, d: usize, at: u8, stale: bool, newer: bool, twin: Option<u64>) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let key = vkey(c, v);
        let screen = self.screen(d, &key, stale);
        let Some(Record::Verse {
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last,
                    from,
                    ..
                },
            ..
        }) = screen.shown.clone()
        else {
            return;
        };
        let words = self.fresh_words(d, 2);
        let record = verse(
            c,
            v,
            VerseValue::Text {
                marker,
                words,
                layout,
                last,
                from,
            },
        );
        let forge = if newer { Forge::Newer } else { Forge::Plain };
        match twin {
            Some(seq) => {
                let labels = self.write_forged(
                    d,
                    vec![(Template::Plain(record), screen.token.clone(), forge)],
                    Cause::Edit,
                    Some(seq),
                );
                for label in labels {
                    self.writes.push(Write {
                        label,
                        screen: screen.clone(),
                    });
                }
            }
            None => self.write_screen(d, record, screen, Cause::Edit, forge),
        }
    }

    fn layout(&mut self, d: usize, at: u8) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let screen = self.screen(d, &vkey(c, v), false);
        let Some(Record::Verse {
            value:
                VerseValue::Text {
                    marker,
                    words,
                    layout,
                    last,
                    from,
                },
            ..
        }) = screen.shown.clone()
        else {
            return;
        };
        let layout = if layout == "\n" { "\n\\p\n" } else { "\n" }.to_owned();
        let record = verse(
            c,
            v,
            VerseValue::Text {
                marker,
                words,
                layout,
                last,
                from,
            },
        );
        self.write_screen(d, record, screen, Cause::Edit, Forge::Plain);
    }

    fn join(&mut self, d: usize, at: u8, len: u16) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let mut parts = Vec::new();
        for k in v..=v + len {
            let Some(part) = self.single(d, c, k) else {
                return;
            };
            parts.push(part);
        }
        let words: Vec<String> = parts.iter().map(|p| p.1.clone()).collect();
        let first = &parts[0];
        let mut records = vec![verse(
            c,
            v,
            VerseValue::Text {
                marker: first.0.clone(),
                words: words.join(" "),
                layout: parts[parts.len() - 1].2.clone(),
                last: vno(v + len),
                from: first.3,
            },
        )];
        for k in v + 1..=v + len {
            records.push(verse(c, k, covered(v)));
        }
        self.write_section(d, records, Cause::Edit);
    }

    fn split(&mut self, d: usize, at: u8) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let Some(VerseValue::Text {
            marker,
            words,
            layout,
            last,
            from,
        }) = self.text_of(d, c, v)
        else {
            return;
        };
        let last = last.get();
        if last <= v {
            return;
        }
        let mut tokens: Vec<String> = words.split_whitespace().map(str::to_owned).collect();
        let parts = usize::from(last - v + 1);
        while tokens.len() < parts {
            tokens.push(self.fresh_words(d, 1));
        }
        let rest: Vec<String> = tokens.split_off(tokens.len() - (parts - 1));
        let mut records = vec![verse(
            c,
            v,
            VerseValue::Text {
                marker: marker.clone(),
                words: tokens.join(" "),
                layout: "\n".into(),
                last: vno(v),
                from,
            },
        )];
        for (i, word) in rest.into_iter().enumerate() {
            let k = v + 1 + i as u16;
            records.push(verse(
                c,
                k,
                VerseValue::Text {
                    marker: MarkerWs::canonical(),
                    words: word,
                    layout: if k == last {
                        layout.clone()
                    } else {
                        "\n".into()
                    },
                    last: vno(k),
                    from: None,
                },
            ));
        }
        self.write_section(d, records, Cause::Edit);
    }

    fn renumber(&mut self, d: usize, at: u8) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let end = self
            .verse_slots(d)
            .into_iter()
            .filter(|(cc, _)| *cc == c)
            .map(|(_, vv)| vv)
            .max()
            .unwrap_or(v);
        if end >= 9 {
            return;
        }
        let mut parts = Vec::new();
        for k in v..=end {
            let Some(part) = self.single(d, c, k) else {
                return;
            };
            parts.push((k, part));
        }
        let mut records = vec![verse(
            c,
            v,
            VerseValue::Absent {
                moved_to: Some(ChapterVerse {
                    chapter: chno(c),
                    verse: vno(v + 1),
                }),
            },
        )];
        for (k, (marker, words, layout, from)) in parts {
            records.push(verse(
                c,
                k + 1,
                VerseValue::Text {
                    marker,
                    words,
                    layout,
                    last: vno(k + 1),
                    from: Some(from.unwrap_or(ChapterVerse {
                        chapter: chno(c),
                        verse: vno(k),
                    })),
                },
            ));
        }
        self.write_section(d, records, Cause::Edit);
    }

    fn drag(&mut self, d: usize, at: u8) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let (Some(a), Some(b)) = (self.single(d, c, v), self.single(d, c, v + 1)) else {
            return;
        };
        let mut next: Vec<&str> = b.1.split_whitespace().collect();
        if next.len() < 2 {
            return;
        }
        let moved = next.remove(0);
        let records = vec![
            verse(
                c,
                v,
                VerseValue::Text {
                    marker: a.0,
                    words: format!("{} {moved}", a.1),
                    layout: a.2,
                    last: vno(v),
                    from: a.3,
                },
            ),
            verse(
                c,
                v + 1,
                VerseValue::Text {
                    marker: b.0,
                    words: next.join(" "),
                    layout: b.2,
                    last: vno(v + 1),
                    from: b.3,
                },
            ),
        ];
        self.write_section(d, records, Cause::Edit);
    }

    pub fn remove(&mut self, d: usize, book: BookId) {
        if !self.devices[d].state.book(book).projects() {
            return;
        }
        let key = RecordKey::BookPresence(book);
        let screen = self.screen(d, &key, false);
        let seen = self.book_records(d, book);
        let labels = self.write(
            d,
            vec![(Template::Removed { book, seen }, screen.token.clone())],
            Cause::Edit,
        );
        for label in labels {
            self.writes.push(Write {
                label,
                screen: screen.clone(),
            });
        }
    }

    fn restore(&mut self, d: usize, book: BookId, text: bool) {
        if !matches!(
            self.devices[d].state.book(book),
            BookView::Removed | BookView::RemovedWhileEdited { .. }
        ) {
            return;
        }
        let key = RecordKey::BookPresence(book);
        let screen = self.screen(d, &key, false);
        if text {
            self.write_screen(
                d,
                presence_of(book, PresentHow::RestoredText),
                screen,
                Cause::Edit,
                Forge::Plain,
            );
            return;
        }
        let mut items = vec![(
            Template::Plain(presence_of(book, PresentHow::StartedEmpty)),
            screen.token.clone(),
        )];
        let verses: Vec<RecordKey> = self.devices[d]
            .state
            .record_keys()
            .into_iter()
            .filter(|k| matches!(k, RecordKey::Verse(b, ..) if *b == book))
            .collect();
        for key in verses {
            let Some(Record::Verse {
                value:
                    VerseValue::Text {
                        marker,
                        layout,
                        last,
                        from,
                        ..
                    },
                book,
                chapter,
                verse,
            }) = self.devices[d].state.projected(&key)
            else {
                continue;
            };
            let token = self.devices[d].state.shown_heads(&key, None);
            items.push((
                Template::Plain(Record::Verse {
                    book,
                    chapter,
                    verse,
                    value: VerseValue::Text {
                        marker,
                        words: "___".into(),
                        layout,
                        last,
                        from,
                    },
                }),
                token,
            ));
        }
        self.write(d, items, Cause::StartOver);
    }

    fn align(&mut self, d: usize, at: u8, current: bool, newer: bool) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let Some(VerseValue::Text {
            words,
            layout,
            last,
            ..
        }) = self.text_of(d, c, v)
        else {
            return;
        };
        let target = if current {
            md5(&tc_usfm::plain_text_v1(&format!("{words}{layout}")))
        } else {
            md5("an older text")
        };
        let word = words.split_whitespace().next().unwrap_or("x").to_owned();
        let key = RecordKey::Alignment(BookId::Titus, chno(c), vno(v));
        let screen = self.screen(d, &key, false);
        let record = Record::Alignment {
            book: BookId::Titus,
            chapter: chno(c),
            verse: vno(v),
            value: AlignmentValue {
                span_end: last,
                alignments: vec![],
                word_bank: vec![TargetWord {
                    word,
                    occurrence: 1,
                    occurrences: 1,
                    extra: JsonMap::new(),
                }],
                target_verse_md5: target,
                source_version: "dcs::unfoldingWord/el-x-koine_ugnt@v0.34".into(),
                source: None,
                done: current,
                marked_invalid: false,
                dismissed: false,
                extra: JsonMap::new(),
            },
        };
        let forge = if newer { Forge::Newer } else { Forge::Plain };
        self.write_screen(d, record, screen, Cause::Edit, forge);
    }

    fn decide(&mut self, d: usize, which: u8, what: u8, stale: bool) {
        let target = CHECKS[usize::from(which) % CHECKS.len()];
        let key = self.live_check(d, &tck(target));
        let rkey = RecordKey::Check(key.clone());
        let screen = self.screen(d, &rkey, stale);
        let mut value = match &screen.shown {
            Some(Record::Check {
                value: CheckValue::Decision(value),
                ..
            }) => value.clone(),
            Some(_) => return,
            None => {
                // A first decision records the resource the checker's screen resolved.
                let mut first = decision(Some(UserStatus::Todo), false, None);
                if let Some(pin) = current_pin(&self.devices[d].state, target.0) {
                    first.resource = pin;
                }
                first
            }
        };
        match what % 4 {
            0 => value.judgement.status = Some(UserStatus::Valid),
            1 => value.judgement.status = Some(UserStatus::Invalid),
            2 => value.bookmarked = !value.bookmarked,
            _ => value.comment = Some(self.fresh_words(d, 1)),
        }
        let newer = what >= 200;
        let forge = if newer { Forge::Newer } else { Forge::Plain };
        self.write_screen(d, check(&key, value), screen, Cause::Edit, forge);
    }

    fn carry_over(&mut self, d: usize, which: u8) {
        let target = CHECKS[usize::from(which) % CHECKS.len()];
        let (tool, book, c, v) = target;
        let key = self.live_check(d, &tck(target));
        let Some(Record::Check {
            value: CheckValue::Decision(mut value),
            ..
        }) = self.devices[d]
            .state
            .projected(&RecordKey::Check(key.clone()))
        else {
            return;
        };
        let n = self.fresh();
        let to = check_key_id(tool, book, &format!("m{n}"), c, v);
        // The carry-over of an upgrade moves the decision to the pinned resource when the
        // pins name one, else to a version nobody pinned.
        value.resource = match current_pin(&self.devices[d].state, tool) {
            Some(pin) if n.is_multiple_of(2) => pin,
            _ => tc_journal::record::ResourceRef {
                repo_path: value.resource.repo_path.clone(),
                sha: oid((n % 200) as u8),
            },
        };
        let old = self.screen(d, &RecordKey::Check(key.clone()), false);
        let new = self.screen(d, &RecordKey::Check(to.clone()), false);
        self.write(
            d,
            vec![
                (Template::Plain(crate::support::moved(&key, &to)), old.token),
                (Template::Plain(check(&to, value)), new.token),
            ],
            Cause::CarryOver {
                group: PinGroup::Primary,
            },
        );
    }

    fn comment(&mut self, d: usize, on: u8) {
        let id = self.comment_id(d);
        self.comment_as(d, on, id);
    }

    fn comment_id(&mut self, d: usize) -> CommentId {
        let n = self.fresh();
        let mut id = [0u8; 16];
        id[..4].copy_from_slice(&n.to_be_bytes());
        id[4] = d as u8;
        CommentId::from_bytes(id)
    }

    fn comment_as(&mut self, d: usize, on: u8, id: CommentId) {
        let target = if on.is_multiple_of(3) {
            CommentTarget::Check(tck(CHECKS[usize::from(on / 3) % CHECKS.len()]))
        } else {
            let Some((c, v)) = self.slot(d, on) else {
                return;
            };
            CommentTarget::Verse {
                book: BookId::Titus,
                chapter: chno(c),
                verse: vno(v),
            }
        };
        let text = self.fresh_words(d, 2);
        self.write(
            d,
            vec![(
                Template::Plain(Record::Comment {
                    id,
                    value: CommentValue { target, text },
                }),
                vec![],
            )],
            Cause::Edit,
        );
    }

    fn pins(&mut self, d: usize, slot: u8, value: u8) {
        let key = RecordKey::Pins(PinGroup::Primary);
        let screen = self.screen(d, &key, false);
        let mut slots: Vec<(String, Json)> = match &screen.shown {
            Some(Record::Pins { value, .. }) => value
                .0
                .iter()
                .map(|(k, v)| (k.as_str().to_owned(), v.0.clone()))
                .collect(),
            _ => Vec::new(),
        };
        let lang = ["en", "es", "fr"][usize::from(value) % 3];
        let (name, json) = match slot % 4 {
            0 => ("gatewayLanguage", Json::from(lang)),
            1 => ("translationNotes", resource_slot(lang, "tn", value)),
            2 => ("translationWords", resource_slot(lang, "tw", value)),
            _ => ("translationWordsLinks", resource_slot(lang, "twl", value)),
        };
        slots.retain(|(k, _)| k != name);
        slots.push((name.to_owned(), json));
        let refs: Vec<(&str, Json)> = slots.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        self.write_screen(
            d,
            pins(PinGroup::Primary, &refs),
            screen,
            Cause::Edit,
            Forge::Plain,
        );
    }

    fn setting(&mut self, d: usize, path: u8, value: u8) {
        let key = SettingKey::parse("checkingOptions").expect("key");
        let rkey = RecordKey::Setting(key.clone());
        let screen = self.screen(d, &rkey, false);
        let mut json = match &screen.shown {
            Some(Record::Setting {
                value: Some(Json::Obj(map)),
                ..
            }) => map.clone(),
            _ => JsonMap::new(),
        };
        match path % 3 {
            0 => {
                json.insert("fontSize".into(), Json::from(value % 4));
            }
            1 => {
                let mut inner = match json.get("filters") {
                    Some(Json::Obj(m)) => m.clone(),
                    _ => JsonMap::new(),
                };
                inner.insert("tw".into(), Json::from(value.is_multiple_of(2)));
                json.insert("filters".into(), Json::Obj(inner));
            }
            _ => {
                json.insert(
                    "view".into(),
                    Json::from(["list", "grid"][usize::from(value) % 2]),
                );
            }
        }
        self.write_screen(
            d,
            Record::Setting {
                key,
                value: Some(Json::Obj(json)),
            },
            screen,
            Cause::Edit,
            Forge::Plain,
        );
    }

    fn meta(&mut self, d: usize, field: u8, value: u8) {
        let (field, json) = match field % 4 {
            0 => (
                MetaField::Name,
                [
                    tc_obj(&[("en", Json::from("Titus"))]),
                    tc_obj(&[("en", Json::from("Tito"))]),
                    Json::from("Titus "),
                ][usize::from(value) % 3]
                    .clone(),
            ),
            1 => (
                MetaField::Description,
                [
                    tc_obj(&[("en", Json::from("A translation"))]),
                    tc_obj(&[("es", Json::from("Una traducción"))]),
                    Json::from(7u8),
                ][usize::from(value) % 3]
                    .clone(),
            ),
            2 => (
                MetaField::Copyright,
                [
                    tc_obj(&[(
                        "shortStatements",
                        Json::Arr(vec![tc_obj(&[("statement", Json::from("CC BY-SA 4.0"))])]),
                    )]),
                    Json::from("All rights reserved"),
                ][usize::from(value) % 2]
                    .clone(),
            ),
            _ => (
                MetaField::Extra(MetaPath::parse("localizedNames").expect("path")),
                [
                    tc_obj(&[(
                        "book-tit",
                        tc_obj(&[
                            ("short", tc_obj(&[("en", Json::from("Titus"))])),
                            ("abbr", tc_obj(&[("en", Json::from("Tit"))])),
                            ("long", tc_obj(&[("en", Json::from("Titus"))])),
                        ]),
                    )]),
                    tc_obj(&[("book-tit", Json::from("Titus"))]),
                ][usize::from(value) % 2]
                    .clone(),
            ),
        };
        let key = RecordKey::ProjectMeta(field.clone());
        let screen = self.screen(d, &key, false);
        self.write_screen(
            d,
            Record::ProjectMeta {
                field,
                value: Some(json),
            },
            screen,
            Cause::Edit,
            Forge::Plain,
        );
    }

    fn carried(&mut self, d: usize, path: u8, value: u8) {
        let path = ["audio/A.mp3", "audio/a.mp3", "notes/x.txt"][usize::from(path) % 3];
        let path = RepoPath::parse(path).expect("path");
        let key = RecordKey::CarriedFile(path.clone());
        let screen = self.screen(d, &key, false);
        let value = if value.is_multiple_of(7) {
            CarriedValue::Removed
        } else {
            CarriedValue::Present {
                oid: oid(value),
                size: u64::from(value),
                md5: md5(&value.to_string()),
            }
        };
        self.write_screen(
            d,
            Record::CarriedFile { path, value },
            screen,
            Cause::Edit,
            Forge::Plain,
        );
    }

    fn book_layout(&mut self, d: usize, seed: u8) {
        let mut slots = self.verse_slots(d);
        if slots.len() < 3 {
            return;
        }
        let n = slots.len();
        slots.swap(usize::from(seed) % n, usize::from(seed / 7) % n);
        let mut items = Vec::new();
        let mut chapter = 0;
        for (c, v) in slots {
            if c != chapter {
                items.push(LayoutItem::Chapter(chno(c)));
                chapter = c;
            }
            items.push(LayoutItem::Verse(chno(c), vno(v)));
        }
        let key = RecordKey::BookLayout(BookId::Titus);
        let screen = self.screen(d, &key, false);
        self.write_screen(
            d,
            Record::BookLayout {
                book: BookId::Titus,
                value: Some(items),
            },
            screen,
            Cause::Edit,
            Forge::Plain,
        );
    }

    fn header(&mut self, d: usize, front: bool) {
        let n = self.fresh();
        let record = if front {
            Record::ChapterFront {
                book: BookId::Titus,
                chapter: chno(1),
                value: ChapterFront::Present(UsfmBytes(format!("\\c 1\n\\s1 Part {n}\n\\p\n"))),
            }
        } else {
            Record::BookHeader {
                book: BookId::Titus,
                value: UsfmBytes(format!("\\id TIT edition {n}\n")),
            }
        };
        let screen = self.screen(d, &record.key(), false);
        self.write_screen(d, record, screen, Cause::Edit, Forge::Plain);
    }

    fn versification(&mut self, d: usize, value: u8) {
        let key = RecordKey::Versification;
        let screen = self.screen(d, &key, false);
        let record = Record::Versification {
            value: tc_journal::record::VersificationValue {
                name: Some(format!("custom {}", value % 3).into()),
                bytes: format!("{{\"maxVerses\":{{\"TIT\":[\"{}\"]}}}}", 15 + value % 4),
            },
        };
        self.write_screen(d, record, screen, Cause::Edit, Forge::Plain);
    }

    /// Device `d` saves its profile from `screen`; with `twin`, a second machine that
    /// shares its identity saves it, reusing seq `twin`.
    fn profile(&mut self, d: usize, value: u8, screen: Screen, twin: Option<u64>) {
        let record = Record::DeviceProfile {
            device: self.devices[d].id,
            value: tc_journal::record::DeviceProfileValue {
                display_name: Some(format!("Person {d} {}", value % 4).into()),
                door43_username: None,
                legacy: None,
            },
        };
        if screen.shown.as_ref() == Some(&record) {
            return;
        }
        let labels = self.write_forged(
            d,
            vec![(Template::Plain(record), screen.token.clone(), Forge::Plain)],
            Cause::Edit,
            twin,
        );
        for label in labels {
            self.writes.push(Write {
                label,
                screen: screen.clone(),
            });
        }
    }

    /// The device that may write record `key`: a device profile only by its own device
    /// (V4), anything else by `d`.
    pub fn writer_of(&self, key: &RecordKey, d: usize) -> usize {
        match key {
            RecordKey::DeviceProfile(id) => self
                .devices
                .iter()
                .position(|dev| dev.id == *id)
                .unwrap_or(d),
            _ => d,
        }
    }

    fn resolve(&mut self, d: usize, pick: u8, how: u8) {
        let conflicts: Vec<Conflict> = self.devices[d].state.conflicts().cloned().collect();
        if conflicts.is_empty() {
            return;
        }
        let conflict = conflicts[usize::from(pick) % conflicts.len()].clone();
        let chosen = choose(&self.devices[d].state, &conflict, pick, how);
        let Some(chosen) = chosen else {
            return;
        };
        let writer = self.writer_of(&conflict.key, d);
        if writer != d {
            self.exchange(d, writer, 0, 0);
        }
        self.write_planned(writer, chosen.planned);
    }

    pub fn write_planned(&mut self, d: usize, planned: Vec<PlannedChange>) {
        let cause = planned.first().map_or(Cause::Resolve, |p| p.cause.clone());
        let items = planned
            .into_iter()
            .map(|p| (template_of(&self.labels, p.record), p.parents))
            .collect();
        self.write(d, items, cause);
    }

    fn release(&mut self, first: usize) {
        // The first device, from `first` on, that holds part of a batch.
        let n = self.devices.len();
        let found = (0..n).map(|i| (first + i) % n).find_map(|d| {
            self.devices[d].state.anomalies().find_map(|a| match a {
                Anomaly::IncompleteBatch { batch, .. } => Some((d, *batch)),
                _ => None,
            })
        });
        let Some((d, batch)) = found else {
            return;
        };
        let changes: Vec<usize> = self.devices[d]
            .held
            .iter()
            .copied()
            .filter(|l| self.recipes[*l].batch.id == batch)
            .filter(|l| matches!(self.items[*l], Some(Verified::Change(_))))
            .collect();
        if changes.is_empty() {
            return;
        }
        self.write(
            d,
            vec![(Template::Release { batch, changes }, vec![])],
            Cause::Edit,
        );
        self.reached.insert("release");
    }

    fn unknown(&mut self, d: usize, value: u8) {
        // Two verses only, so two devices meet on one record often.
        let Some((c, v)) = self.slot(d, value % 2) else {
            return;
        };
        let carrier = verse(c, v, crate::support::text("carrier", v));
        let key = unknown_key(&carrier);
        let screen = self.screen(d, &key, false);
        self.write_forged(
            d,
            vec![(
                Template::Plain(carrier),
                screen.token,
                Forge::UnknownKind(value),
            )],
            Cause::Edit,
            None,
        );
    }

    fn poison(&mut self, d: usize, at: u8) {
        let Some((c, v)) = self.slot(d, at) else {
            return;
        };
        let Some((w, x)) = self.slot(d, at.wrapping_add(1)).map(|s| (s, s)) else {
            return;
        };
        let _ = x;
        if (c, v) == w {
            return;
        }
        let first = self.screen(d, &vkey(c, v), false);
        let second = self.screen(d, &vkey(w.0, w.1), false);
        let (Some(a), Some(b)) = (first.shown.clone(), second.shown.clone()) else {
            return;
        };
        let (Some(_), Some(_)) = (words_of(&a), words_of(&b)) else {
            return;
        };
        let rewrite = |r: Record, words: String| match r {
            Record::Verse {
                book,
                chapter,
                verse,
                value:
                    VerseValue::Text {
                        marker,
                        layout,
                        last,
                        from,
                        ..
                    },
            } => Record::Verse {
                book,
                chapter,
                verse,
                value: VerseValue::Text {
                    marker,
                    words,
                    layout,
                    last,
                    from,
                },
            },
            other => other,
        };
        let wa = self.fresh_words(d, 1);
        let wb = self.fresh_words(d, 1);
        let labels = self.write_forged(
            d,
            vec![
                (Template::Plain(rewrite(a, wa)), first.token, Forge::Plain),
                (Template::Plain(rewrite(b, wb)), second.token, Forge::Poison),
            ],
            Cause::Edit,
            None,
        );
        if !labels.is_empty() {
            self.reached.insert("poison");
        }
    }
}

/// A person's choice made into changes, and the versions the person picked.
pub struct Chosen {
    pub planned: Vec<PlannedChange>,
    pub picks: Vec<ChangeId>,
    pub kept: Kept,
}

/// How a picked version is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// Its record is written with exactly its value.
    Exact,
    /// Its record is written with its words (a structure choice numbers a text again).
    Words,
    /// Its record is left as it is (a path-case choice removes the other files).
    AsItIs,
}

type Plan<'a> = dyn Fn(&[ChangeId]) -> Result<Vec<PlannedChange>, ResolveError> + 'a;

/// A choice for a conflict, as a person might make it. Where a choice leaves a record's
/// version to the person ([`ResolveError::MissingValue`]: its side holds several), the
/// person picks one of that record's candidates and chooses again.
pub fn choose(state: &FoldState, conflict: &Conflict, pick: u8, how: u8) -> Option<Chosen> {
    let id = &conflict.id;
    let first = conflict
        .candidates
        .get(usize::from(pick) % conflict.candidates.len().max(1))
        .cloned();
    let structure = |c: &tc_journal::fold::Candidate| {
        matches!(
            &c.value,
            Record::Verse {
                value: VerseValue::Text { .. },
                ..
            }
        )
    };
    // Picks a candidate for each record a choice leaves open, and chooses again.
    let with_picks = |mut picks: Vec<ChangeId>, kept: Kept, plan: &Plan| -> Option<Chosen> {
        let exact = kept == Kept::Exact;
        let side = picks.first().and_then(|p| {
            conflict
                .candidates
                .iter()
                .find(|c| c.head == *p)
                .and_then(|c| c.side)
        });
        for round in 0..64usize {
            match plan(&picks) {
                Ok(planned) => {
                    return Some(Chosen {
                        planned,
                        picks,
                        kept,
                    });
                }
                Err(ResolveError::MissingValue(key)) => {
                    // The kept side's versions first; a record that side did not write
                    // may hold two versions of the others.
                    let mut options: Vec<(bool, ChangeId)> = conflict
                        .candidates
                        .iter()
                        .filter(|c| c.value.key() == key && (exact || structure(c)))
                        .filter(|c| !picks.contains(&c.head))
                        .map(|c| (c.side != side, c.head))
                        .collect();
                    if options.is_empty() && !exact {
                        // The text a verse had before a join may be two concurrent
                        // versions in its History.
                        options = state
                            .history(&key)
                            .into_iter()
                            .filter(|h| !picks.contains(h))
                            .filter(|h| {
                                state.body(h).is_some_and(|b| {
                                    matches!(
                                        b.record,
                                        Record::Verse {
                                            value: VerseValue::Text { .. },
                                            ..
                                        }
                                    )
                                })
                            })
                            .map(|h| (false, h))
                            .collect();
                    }
                    options.sort();
                    let same = options.iter().filter(|(other, _)| !other).count();
                    let options: Vec<ChangeId> = options
                        .into_iter()
                        .take(if same > 0 { same } else { usize::MAX })
                        .map(|(_, head)| head)
                        .collect();
                    if options.is_empty() {
                        return None;
                    }
                    picks.push(options[(usize::from(pick) + round) % options.len()]);
                }
                Err(_) => return None,
            }
        }
        None
    };
    let separate = |picks: &[ChangeId]| state.keep_separate(id, picks);
    let joined = |picks: &[ChangeId]| state.keep_joined(id, picks);
    let passage = |picks: &[ChangeId]| state.keep_passage(id, picks);
    let by_candidate = || {
        let first = first.clone()?;
        let head = first.head;
        match state.resolution(id, &Choice::Candidate { head }) {
            Ok(planned) => Some(Chosen {
                planned,
                // A structure choice keeps a covered or absent candidate's record as each
                // verse's own text.
                picks: if conflict.kind == ConflictKind::StructureIssue && !structure(&first) {
                    Vec::new()
                } else {
                    vec![head]
                },
                kept: match conflict.kind {
                    ConflictKind::StructureIssue => Kept::Words,
                    ConflictKind::PathCase => Kept::AsItIs,
                    // A verse typed after *Start empty* keeps that side as it is (§7.6).
                    ConflictKind::BookPresence if matches!(first.value, Record::Verse { .. }) => {
                        Kept::AsItIs
                    }
                    _ => Kept::Exact,
                },
            }),
            Err(ResolveError::MissingValue(_)) => match conflict.kind {
                ConflictKind::Passage => with_picks(vec![head], Kept::Exact, &passage),
                ConflictKind::StructureIssue => {
                    let spans = matches!(
                        &first.value,
                        Record::Verse { value: VerseValue::Text { last, .. }, verse, .. } if last > verse
                    );
                    let picks = if structure(&first) {
                        vec![head]
                    } else {
                        Vec::new()
                    };
                    if spans {
                        with_picks(picks, Kept::Words, &joined)
                    } else {
                        with_picks(picks, Kept::Words, &separate)
                    }
                }
                _ => None,
            },
            Err(_) => None,
        }
    };
    let values = |planned: Option<Vec<PlannedChange>>| {
        planned.map(|planned| Chosen {
            planned,
            picks: Vec::new(),
            kept: Kept::Words,
        })
    };
    match (conflict.kind, how % 4) {
        (ConflictKind::VerseText, 1) => match state.combine(id) {
            Some(record) => values(state.resolution(id, &Choice::Values(vec![record])).ok()),
            None => by_candidate(),
        },
        (ConflictKind::Passage, 1) => match state.combine_passage(id) {
            Some(records) => values(state.resolution(id, &Choice::Values(records)).ok()),
            None => by_candidate(),
        },
        (
            ConflictKind::Passage | ConflictKind::VerseStructure | ConflictKind::StructureIssue,
            2,
        ) => with_picks(Vec::new(), Kept::Words, &separate),
        (
            ConflictKind::Passage | ConflictKind::VerseStructure | ConflictKind::StructureIssue,
            3,
        ) => with_picks(Vec::new(), Kept::Words, &joined).or_else(by_candidate),
        (ConflictKind::CheckMoved, 1 | 3) => values(state.apply_to_moved_check(id).ok()),
        // *Choose per line*: each disputed unit from a candidate.
        (_, 2) if !conflict.disputed.is_empty() => {
            let n = conflict.candidates.len().max(1);
            let picks: BTreeMap<tc_journal::fold::UnitName, ChangeId> = conflict
                .disputed
                .iter()
                .enumerate()
                .filter_map(|(i, unit)| {
                    let c = conflict.candidates.get((usize::from(pick) + i) % n)?;
                    Some((unit.clone(), c.head))
                })
                .collect();
            let record = state.per_unit(id, &picks).ok()?;
            values(state.resolution(id, &Choice::Values(vec![record])).ok())
        }
        (ConflictKind::BookRemovedWhileEdited { .. }, 1) => match &conflict.key {
            RecordKey::BookPresence(book) => values(Some(vec![state.keep_removed(*book)])),
            _ => None,
        },
        (ConflictKind::BookRemovedWhileEdited { .. } | ConflictKind::LastBookRemoved, _) => {
            values(state.restore_book(*id).ok())
        }
        _ => by_candidate(),
    }
}

fn template_of(labels: &HashMap<ChangeId, usize>, record: Record) -> Template {
    match record {
        Record::BatchRelease { batch, value } => Template::Release {
            batch,
            changes: value
                .changes
                .iter()
                .filter_map(|c| labels.get(c).copied())
                .collect(),
        },
        Record::BookPresence {
            book,
            value: BookPresence::Removed { seen },
        } => {
            let seen: BTreeSet<_> = seen.into_iter().collect();
            Template::Removed {
                book,
                seen: labels
                    .iter()
                    .filter(|(id, _)| seen.contains(&id.short()))
                    .map(|(_, l)| *l)
                    .collect(),
            }
        }
        other => Template::Plain(other),
    }
}

/// Whether a recipe's record is a record of `book` (§7.6).
pub fn belongs_to(recipe: &Recipe, book: BookId) -> bool {
    if matches!(recipe.forge, Forge::UnknownKind(_)) {
        return false;
    }
    match &recipe.template {
        Template::Plain(record) => match record {
            Record::BookHeader { book: b, .. }
            | Record::ChapterFront { book: b, .. }
            | Record::Verse { book: b, .. }
            | Record::BookLayout { book: b, .. }
            | Record::Alignment { book: b, .. } => *b == book,
            Record::Check { key, .. } => key.book() == Some(book),
            Record::Comment { value, .. } => match &value.target {
                CommentTarget::Verse { book: b, .. } => *b == book,
                CommentTarget::Check(key) => key.book() == Some(book),
                CommentTarget::Frame { .. } => false,
            },
            _ => false,
        },
        _ => false,
    }
}

fn import_items(book: BookId, chapters: &[u16]) -> Vec<(Template, Vec<ChangeId>)> {
    let mut items = vec![
        Record::BookPresence {
            book,
            value: BookPresence::Present {
                scope: Scope::Whole,
                how: PresentHow::Added,
            },
        },
        Record::BookHeader {
            book,
            value: UsfmBytes(format!("\\id {}\n", book.code())),
        },
    ];
    for (i, &count) in chapters.iter().enumerate() {
        let c = i as u16 + 1;
        items.push(Record::ChapterFront {
            book,
            chapter: chno(c),
            value: ChapterFront::Present(UsfmBytes(format!("\\c {c}\n\\p\n"))),
        });
        for v in 1..=count {
            items.push(Record::Verse {
                book,
                chapter: chno(c),
                verse: vno(v),
                value: VerseValue::Text {
                    marker: MarkerWs::canonical(),
                    words: format!("{}{c}v{v}a {}{c}v{v}b", book.code(), book.code()),
                    layout: "\n".into(),
                    last: vno(v),
                    from: None,
                },
            });
        }
    }
    items
        .into_iter()
        .map(|r| (Template::Plain(r), vec![]))
        .collect()
}

fn seed_items(book: BookId, chapters: &[u16], stub: &str) -> Vec<(Template, Vec<ChangeId>)> {
    import_items(book, chapters)
        .into_iter()
        .map(|(t, p)| match t {
            Template::Plain(Record::ChapterFront {
                book,
                chapter,
                value: ChapterFront::Present(_),
            }) if stub.is_empty() => (
                Template::Plain(Record::ChapterFront {
                    book,
                    chapter,
                    value: ChapterFront::Present(UsfmBytes(format!("\\c {}\n", chapter.get()))),
                }),
                p,
            ),
            Template::Plain(Record::Verse {
                book,
                chapter,
                verse,
                value:
                    VerseValue::Text {
                        marker,
                        layout,
                        last,
                        from,
                        ..
                    },
            }) => (
                Template::Plain(Record::Verse {
                    book,
                    chapter,
                    verse,
                    value: VerseValue::Text {
                        marker,
                        words: stub.into(),
                        layout,
                        last,
                        from,
                    },
                }),
                p,
            ),
            other => (other, p),
        })
        .collect()
}

fn resource_slot(lang: &str, kind: &str, value: u8) -> Json {
    tc_obj(&[
        (
            "repoPath",
            Json::from(format!("git.door43.org/unfoldingWord/{lang}_{kind}")),
        ),
        ("sha", Json::from(oid(value).to_hex())),
        ("version", Json::from(format!("v{}", value % 5))),
    ])
}

pub fn tc_obj(fields: &[(&str, Json)]) -> Json {
    Json::Obj(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )
}

fn book(ruth: bool) -> BookId {
    if ruth { BookId::Ruth } else { BookId::Titus }
}

pub fn chno(n: u16) -> ChapterNo {
    ChapterNo::new(n).expect("chapter")
}

pub fn vno(n: u16) -> VerseNo {
    VerseNo::new(n).expect("verse")
}

pub fn vkey(c: u16, v: u16) -> RecordKey {
    RecordKey::Verse(BookId::Titus, chno(c), vno(v))
}

fn tck((tool, book, c, v): (ToolId, BookId, u16, u16)) -> CheckKey {
    check_key_id(tool, book, &format!("c{c}{v}"), c, v)
}

fn check_key_id(tool: ToolId, book: BookId, id: &str, c: u16, v: u16) -> CheckKey {
    let mut key = check_key("abcd", c, v);
    key.tool = tool;
    key.check_id = tc_journal::CheckId::parse(id).expect("id");
    key.target = tc_journal::CheckTarget::Verse {
        book,
        chapter: tc_journal::RefPart::parse(&c.to_string()).expect("chapter"),
        verse: tc_journal::RefPart::parse(&v.to_string()).expect("verse"),
    };
    key
}

/// The pin a checker's tool resolves to in the device's projected pins, as `resolution`
/// does (§7.7), when there is one: a decision made now records it.
fn current_pin(state: &FoldState, tool: ToolId) -> Option<tc_journal::record::ResourceRef> {
    let slot = match tool {
        ToolId::TranslationNotes => "translationNotes",
        ToolId::TranslationWords => "translationWordsLinks",
        ToolId::TranslationQuestions => "translationQuestions",
    };
    let Some(Record::Pins { value, .. }) = state.projected(&RecordKey::Pins(PinGroup::Primary))
    else {
        return None;
    };
    let pin = value.0.get(&tc_journal::SlotName::parse(slot).ok()?)?;
    let map = pin.0.as_obj()?;
    Some(tc_journal::record::ResourceRef {
        repo_path: map.get("repoPath")?.as_str()?.into(),
        sha: tc_journal::GitOid::parse(map.get("sha")?.as_str()?)?,
    })
}
