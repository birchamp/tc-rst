# Storage, journal, merge and exchange

Status: normative. This is the design that `tc-journal`, `tc-store`, `tc-sync`, the git transport
in `tc-door43` and `tc-import` implement. A change to anything in this document is a change to the
project format and follows §12.

Sources: the research in the decision log (`docs/rebuild.tsv`), the three candidate designs and
the three judge reports of the storage design panel. Appendix A says which part came from where.
Words used here: a **change** is one immutable version of one record (tC4 called the unit an
"event"); a **pack** is an immutable file of changes (tC4 had one "segment" per action); the
**fold** turns the set of changes into the project state.

---

## 0. The design on one page

1. **Records and changes.** All authored data is a set of immutable changes. A change sets one
   **record** to a whole value. A record is the smallest unit a person edits and a conflict can
   be about: one verse, one chapter front, one alignment, one check decision, one user comment,
   one pin group, one top-level setting, one carried file. A change names its **parents**: the
   versions of the same record that the author's screen showed. Its id is the SHA-256 of its
   bytes, so a change can only name a parent its author actually held.
2. **Heads decide state.** The heads of a record are its changes that no other change of that
   record names as a parent. One head, or several heads with equal values, is settled. Several
   heads with different values go through a three-way merge on fixed **merge units**. If two
   heads changed one unit differently, the record is a **conflict**: every version is kept and
   a person chooses. No clock, device or author decides which version survives.
3. **Verses have stable keys.** A verse record is keyed by (book, chapter, verse number). A
   verse span is a value: verse 4 holds the text of `4-5` with `last: 5`, and verse 5 holds
   `Covered { by: 4 }`. Spans in different places never touch the same record. A span against an
   edit of a verse inside it is a visible conflict on that verse. A save that rewrites two or
   more verses at once (a section save, a renumber, a join) is one **passage**: a concurrent
   change to any of its verses is one conflict over the whole passage, and until a person
   decides, the text shows the whole passage from one side, so no word is shown twice or lost.
4. **Invalidation is computed.** An alignment or check decision stores what it was made against
   (verse text hash, resource sha). The fold computes "invalid, kept" from the current text and
   pins. A resource upgrade writes nothing on alignments, and a text edit nothing on
   decisions. A text edit carries the verse's alignment over to the new text in the same batch
   (`tc_align::reflow_edit`: each link follows its own word into the new text, the others go
   back to the bank), so a verse and its alignment are never seen apart; the earlier alignment stays in
   History (§5.1).
5. **Storage.** A save is one atomic file in `.tc/pending/`, outside the burrito, named by its
   device and its content hash, so no other save can ever replace it. A **seal**
   moves pending changes into an immutable NDJSON pack under `ingredients/tc/changes/`, which is
   a burrito ingredient. Completed months are consolidated into one pack per device.
6. **Derived and carried files.** USFM, the tC4-shaped checking sidecars, `resources.json`,
   `settings.json`, `vrs.json`, comments, conflicts and `metadata.json` are projections of the
   fold. They are rewritten with a placement ledger and a compare-and-swap, so the app
   never overwrites a file it did not write last. A derived file that is not what the app
   wrote is kept first; a book file then waits for the person's answer (§11.1), any other
   derived file is written again with a notice. Files the app does not derive (audio, a
   `LICENSE.md` added on Door43) are **carried files**, tracked by records like everything else.
7. **Git is transport and backup.** git2 fetches and pushes. Every commit tree is built in
   memory from the fold and validated as a burrito before it is written. The app never runs a
   git merge or a checkout. A sync is: seal, fetch, take in new packs, commit with parents
   `[local, remote]`, push, confirm. A file changed on Door43 by another tool is reported once,
   never merged (§11.2).
8. **USB.** An exchange file is a valid Scripture Burrito zip. Intake validates each pack on its
   own, salvages what it can from a damaged one, and takes the rest.
9. **The fold is total.** A damaged file, an unknown record kind, a missing parent or a
   duplicated device identity becomes an anomaly a person sees. Nothing in the data can stop a
   project from opening or a team from syncing.

| Requirement | Mechanism | Section |
|---|---|---|
| Offline work exchanged later, in any order, while others work | Changes form a grow-only set; the fold is a function of the set; devices write disjoint files | §2, §4, §7 |
| Never lose, hide or rewrite work | Parents come from what the screen showed; every concurrent difference is a visible conflict for every record kind; a changed file is kept, and nothing in it enters the project unless a person imports it; History keeps every version | §5, §7, §8, §11 |
| Valid Scripture Burrito on disk and in every export | Pending saves live outside the burrito; packs, derived and carried files are ingredients; `metadata.json` is generated by one writer and validated before it is written | §3, §5.10 |
| Crash safety | One atomic file per save; immutable packs; a placement ledger written before every rename; derived files are reproducible | §5, §6 |
| tC4 4.0 and tC3 projects import | Deterministic event-by-event conversion, checked against tC4's own derived files | §13 |

---

## 1. Invariants

Each invariant is proven by the tests named in §16.

| Id | Invariant |
|---|---|
| I1 | **Convergence.** The fold output, the derived bytes and the conflict list are a pure function of the set of valid changes (and the derivation versions, §12). Any order, grouping or duplication of unions gives byte-identical results. |
| I2 | **Conservation.** Every valid change is in exactly one of these states: (a) a head of a settled or merged record, projected and reachable from a screen (an alignment, decision or comment whose verse or check no longer exists is listed under *Detached*, §7.7); (b) a head shown as a candidate in a listed conflict; (c) an ancestor of a head, reachable in History, because a later change named it as a parent; (d) a head of a record of a removed book, held by the removal's author (§7.6), restored with the book; (e) part of a listed anomaly (an incomplete batch, a quarantined change, or a record of a book that no `BookPresence` change makes present); (f) a seed stub absorbed by the stub rules (§7.3), starting text no person wrote, listed in History; (g) a head that lost to the newest version of a record that is not verse or frame text (owner ruling H5, §8.1), listed in History, where *Restore this version* brings it back, or, for a carried file, kept as a file of its own beside it (§11.4). There is no eighth state. |
| I3 | **No survival by time or identity, except the newest-wins rule.** Heads, merges and text conflicts never read `at`, the device or the author. Owner ruling H5: between concurrent heads of a record that is not verse or frame text and of a book's presence, the head with the largest (rank, at, id) wins (`ChangeMeta::order`, the order of the provisional head); a change that has another as an ancestor always replaces it, whatever the clocks say. The loser stays a head (I2 state g). Only provisional choices read `at`: which head derived files show while a conflict is open, which side of a passage conflict they show (§7.3), the scope of a contested book, the fallback text of an incoherent verse, and the book kept by the last-book rule (§7). Display order reads it too. A wrong clock can change what is shown provisionally and which version of a newest-wins record wins, never what is kept. |
| I4 | **Observed parents.** The parents of a change are the heads the person's screen displayed (a `ShownToken`, §2.7), or the exact base an automated writer computed. They are never "the current heads at write time". |
| I5 | **Total fold.** The fold never fails or panics. Defects become anomalies with a message for the person. |
| I6 | **Atomic files.** Every file the app writes in a project goes through `atomic_write` (§5.0). No project file is written in place. |
| I7 | **Valid burrito.** Every commit tree, exchange file and export is built in memory and validated before it is written. The working folder is a valid burrito except inside a **write group** and, after a crash inside one, until the next open repairs it; when another program holds a file the group renamed, the open cannot read which bytes are there, and the repair waits until the first open or checkpoint that can (§5.10 M2). Until then every list keeps the entry `metadata.json` already has for that file (none, when it has none or cannot be read), so no write makes a claim the folder did not make, and the commits, exchange files and exports, built from the projection, stay valid. A write group is a materialization (§5.10), a seal and consolidation (§5.11, §5.12, closed by the checkpoint's materialization), or the pack writes of an intake (§9.3 T1, §10.2 R7): it starts at its first rename under `ingredients/` and ends with the `metadata.json` rewrite that lists the result. A materialization lasts milliseconds; a large intake can last seconds. |
| I8 | **Nothing overwritten that someone else wrote.** Before the app replaces or removes a derived or carried file, it checks whether the bytes on disk are its own: the bytes it last wrote, the bytes of a write it had begun, or the bytes the project would write now (§6.2). Other bytes are kept first. A book file or story is kept and left as it is until the person answers (§11.1). Any other derived file is copied, with a notice, and then written again. A carried file's new version is recorded as a version (§11.4). |
| I9 | **No editing lock.** Sync and exchange read snapshots and add immutable files. Saves continue during them. Materialization and the long part of a checkpoint run on the writer from a snapshot; a save waits for none of them (PERFORMANCE.md: 25 ms p95 while background work runs). |
| I10 | **Newer data is refused in words or carried, never destroyed.** |
| I11 | **Deletions are deliberate.** A missing file is never read as a deletion. A verse or record that an imported file lacks becomes a conflict, never a removal (§13.6). A book removal never hides work its author had not seen. |
| I12 | **No machine input.** Derived bytes never depend on installed resources, the clock or the machine. |
| I13 | **The record is the project.** No derived file's content enters the project without a person's choice (*Import*, *Bring it back*). Changes that arrive as packs from translationCore (sync, exchange file) are the only exception. A carried file is not derived: a new version of it found in the folder is recorded as a version (§11.4), since that is the only way such a file changes. |

The owner's ruling of 2026-10-22: losing up to the last 2 minutes of work at a crash, a power cut
or another program's change is acceptable; a lost, corrupted or unopenable project never is.

---

## 2. Data model

All types in this section live in `tc-journal` unless marked. The crate does no I/O. Types parse
external bytes at the boundary and are trusted inside.

### 2.1 Identifiers

```rust
/// SHA-256 of a change's canonical JSON bytes (§2.5). Text form: 64 lowercase hex.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChangeId([u8; 32]);

/// The first 16 bytes of a ChangeId. Used only in `BookPresence::Removed::seen` (§7.6).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShortId([u8; 16]);

/// A pack's id: SHA-256 of its header line, then the ids of its change lines (§4.3). The
/// file name is the first 32 hex digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackId([u8; 32]);

/// 16 bytes. Byte 0 is the kind. Bytes 1..16 are random (Local) or a hash (every other kind).
/// Text form: "d" + 26 lowercase RFC 4648 base32 characters, no padding. Always a valid
/// ingredient path segment.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId([u8; 16]);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DeviceKind {
    /// One installation writing in one project folder on one machine (§6.3).
    Local = 1,
    /// A tC4 actor: sha256("tc4-actor\0" ‖ actorId)[..15].
    Tc4Actor = 2,
    /// A tC3 user: sha256("tc3-user\0" ‖ projectId ‖ userName)[..15].
    Tc3User = 3,
    /// One file edited in one Door43 commit: sha256("door43-edit\0" ‖ commit oid ‖ path)[..15].
    /// Not written since 2026-10-22 (§11.2); removable before the freeze (§12.2).
    Door43Edit = 4,
    /// One file edited inside one exchange file: sha256("exchange-edit\0" ‖ md5 of the file's
    /// metadata.json ‖ path)[..15]. Not written since 2026-10-22 (§11.3); removable before the freeze.
    ExchangeEdit = 5,
    /// Deterministic import changes such as the tC4 reconcile: sha256("import\0" ‖ kind ‖ projectId)[..15].
    Importer = 6,
}

/// Random UUID v4 at project creation; derived for a tC4 import (§13.1). Stored in
/// `ingredients/tc/project.json`. Two projects with different ids are never merged.
/// Text form: lowercase hyphenated.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectId(uuid::Uuid);

/// Local devices: 1, 2, 3, … strictly increasing, one per change. Other kinds: the index of the
/// change inside its batch. Used to detect two machines writing as one Local device (§6.3) and
/// to name files. Never used for ordering or survival.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceSeq(u64);

/// One user action (a section save, a resolution, an upgrade, one import of a file).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BatchId([u8; 16]);
/// Every change names its batch and the batch's size. A batch applies only when all `n`
/// changes are present (§7.2 step F2), so no device ever sees half of an action. `n ≥ 1`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatchRef { pub id: BatchId, pub n: u32 }

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommentId([u8; 16]);   // random

/// Milliseconds since the Unix epoch, UTC, from the author's clock. Display and provisional
/// choice only (I3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WallTime(i64);

pub use tc_core::BookId;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChapterNo(std::num::NonZeroU16);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerseNo(std::num::NonZeroU16);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StoryNo(u8);            // 1..=50, checked at parse
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameNo(u16);           // 0 = the story title

pub use tc_core::{GitOid, Md5};    // one type each for every crate (§17)
// Md5([u8; 16]): lowercase hex on the wire.
// GitOid([u8; 20]): git object id; also the pin identity (a commit sha). Text form 40
// lowercase hex, tC4's SHA_RE (`journal/grammar.mjs`); `GitOid::parse_any_case` reads a
// commit a person pasted in upper case, at the boundaries that take hand-written input.

/// A path relative to the repository root, `/`-separated, obeying the ingredient path grammar
/// (§3.3) for every segment. Parsed at the boundary; never constructed from unchecked strings.
/// Two paths **collide** when their case-folded forms are equal (§3.3). Concretely: in NFC,
/// accepted by `tc_burrito::IngredientPath`, and holding none of `~ & + # % { } $ ! '`
/// (the characters of §3.3 that `IngredientPath` allows), so every `RepoPath` is also a valid
/// burrito ingredient path.
pub struct RepoPath(Box<str>);
```

In the hash formulas of `DeviceKind` and elsewhere, `‖` is byte concatenation: strings are
their UTF-8 bytes, and ids and digests (`GitOid`, `Md5`, `ProjectId`) their raw bytes.

### 2.2 Record keys

A key names one record. The fold indexes by key; two changes conflict only if they have the
same key.

```rust
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKey {
    // ---- Bible text ----
    BookPresence(BookId),                       // in scope with a passage-set scope, or removed
    BookHeader(BookId),                         // every byte before the first `\c` marker
    ChapterFront(BookId, ChapterNo),            // `\c N` through the byte before the chapter's first `\v`
    Verse(BookId, ChapterNo, VerseNo),          // one verse number of the versification frame
    BookLayout(BookId),                         // only for imported books whose order is not numeric
    // ---- checking ----
    Alignment(BookId, ChapterNo, VerseNo),      // keyed by the anchor (§7.7) of the first verse of the span it aligns
    Check(CheckKey),                            // one tN / tW / tQ decision
    // ---- comments (grow-only) ----
    Comment(CommentId),
    // ---- project ----
    Pins(PinGroup),
    ResourcesExtra(ResourcesKey),               // a resources.json member no pin group holds
    Setting(SettingKey),                        // one top-level key of settings.json
    ProjectMeta(MetaField),
    Versification,
    DeviceProfile(DeviceId),                    // written only by that device (rule V4, §4.5)
    // ---- Open Bible Stories ----
    StoryBase(StoryNo),                         // template bytes of content/NN.md
    Frame(StoryNo, FrameNo),                    // frame paragraph; frame 0 = title
    StoryRef(StoryNo),                          // the closing `_…_` reference line
    // ---- files the app does not derive ----
    CarriedFile(RepoPath),
    // ---- a person's decision about an incomplete batch (§7.4); grow-only ----
    BatchRelease(BatchId),
    // ---- a kind from a newer minor format (§12) ----
    Unknown(UnknownKey),
}

/// tC4's normative decision identity, so decisions round-trip with tC3 and tC4 tools.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckKey {
    pub tool: ToolId,
    pub check_id: CheckId,                      // NFC, non-empty, no `|` or `:`; refused otherwise
    pub target: CheckTarget,
    pub occurrence: u32,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolId { TranslationWords, TranslationNotes, TranslationQuestions }
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckTarget {
    /// Chapter and verse are tC4's strings: "1", "9-10", "front", "1,3,8,12". Compared as
    /// strings. A `RefPart` is tC4's identity part (`identityPartError`, `journal/grammar.mjs`
    /// at d310bf3): not empty, no `|` and no `:`; and NFC (V2).
    Verse { book: BookId, chapter: RefPart, verse: RefPart },
    Frame { story: StoryNo, frame: FrameNo },
}

/// `resources.json` is projected from five groups. A gateway change rewrites `Primary` and
/// `ExtraScripture` in one batch.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PinGroup { Primary, Fallback, OriginalLanguage, Lexicon, ExtraScripture }

pub struct SettingKey(Box<str>);   // a top-level key: NFC, not empty; `__proto__`, `constructor`, `prototype` refused
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetaField { Name, Abbreviation, Description, Languages, Copyright, Extra(MetaPath) }
/// The dotted metadata.json path of a field the app does not model (`localizedNames`,
/// `identification.primary.ebible`): NFC, non-empty `.`-separated segments, no control
/// characters, no segment `__proto__`, `constructor` or `prototype`, and neither a path a
/// modelled field owns (`identification.name`, `identification.abbreviation`,
/// `identification.description`, `languages`, `copyright`) nor one above or below such a path
/// (`identification`, `identification.name.en`), so no field has two records.
pub struct MetaPath(Box<str>);
/// The kind name and the canonical JSON of the key fields: every member of the record object
/// except `value` (§2.8).
pub struct UnknownKey { pub kind: Box<str>, pub key_json: Box<str> }
/// A pin slot name such as `gatewayLanguage` or `obs-tn`: an ASCII letter, then ASCII
/// letters, digits, `_` and `-`, at most 64 bytes, not `constructor` or `prototype`. tC4's
/// slot grammar (`PIN_SLOT_RE`, `journal/grammar.mjs` at d310bf3) names `obs-tn`,
/// `obs-twl`, `obs-tq` and `obs-images` and allows `-` in an `extraScripture` pane id.
pub struct SlotName(Box<str>);
/// A `resources.json` member that no pin group holds, kept so that an import or an outside
/// edit never loses it (tC4 writes none; another tool or a newer app may). `name` is NFC, not
/// empty, not `__proto__`, `constructor` or `prototype`, and not a member the projection
/// writes at its level.
pub struct ResourcesKey { pub level: ResourcesLevel, pub name: Box<str> }
/// `Top`: beside `schemaVersion`, `languageSets`, `resources` and `extraScripture`.
/// `LanguageSets`: beside `primary` and `fallback`. `Resources`: beside `originalLanguage` and
/// `lexicon`.
pub enum ResourcesLevel { Top, LanguageSets, Resources }
```

**References as tC4 writes them.** A check's chapter and verse are kept as the resource writes
them and tC4 reads them (`ref.split(':').map(refPart)`, `src/data/derive.ts` at d310bf3):
digits become a number, and anything else stays a string, so en_tn's `sbh4` at `PSA 5:1,3,8,12`
is a check on verse `"1,3,8,12"` of chapter 5, and a superscription row `N:front` is a check
on verse `"front"` of chapter N. Such a target names no verse text the fold can read (§7.7, the
no-text rule), so its decision is never invalidated by a text edit, as in tC4; it is
invalidated when the resource changes, like every decision.

Pane layouts and other per-person UI preferences are not records. They live in app data per
device, so two translators' layouts never conflict. The `ui` key of an imported `settings.json`
is kept as a `Setting` record and projected unchanged.

### 2.3 Records and values

Each change carries one `Record`. The enum pairs every key with its value type, so a verse key
with an alignment value cannot be built.

```rust
pub enum Record {
    BookPresence { book: BookId, value: BookPresence },
    BookHeader   { book: BookId, value: UsfmBytes },
    ChapterFront { book: BookId, chapter: ChapterNo, value: ChapterFront },
    Verse        { book: BookId, chapter: ChapterNo, verse: VerseNo, value: VerseValue },
    BookLayout   { book: BookId, value: Option<Vec<LayoutItem>> },
    Alignment    { book: BookId, chapter: ChapterNo, verse: VerseNo, value: AlignmentValue },
    Check        { key: CheckKey, value: CheckValue },
    Comment      { id: CommentId, value: CommentValue },
    Pins         { group: PinGroup, value: PinGroupValue },
    ResourcesExtra { key: ResourcesKey, value: Option<Json> }, // None = member removed
    Setting      { key: SettingKey, value: Option<Json> },   // None = key removed
    ProjectMeta  { field: MetaField, value: Option<Json> },
    Versification { value: VersificationValue },
    DeviceProfile { device: DeviceId, value: DeviceProfileValue },
    StoryBase    { story: StoryNo, value: String },
    Frame        { story: StoryNo, frame: FrameNo, value: String },
    StoryRef     { story: StoryNo, value: Option<String> },
    CarriedFile  { path: RepoPath, value: CarriedValue },
    BatchRelease { batch: BatchId, value: BatchReleaseValue },
    Unknown(UnknownRecord),
}

/// A JSON value with integers only (−2⁶³ to 2⁶⁴−1), so every value has one canonical text;
/// `JsonMap` is its object. Floats cannot be stored; an importer that meets one refuses it.
pub enum Json { Null, Bool(bool), Int(JsonInt), Str(String), Arr(Vec<Json>), Obj(JsonMap) }

/// `Whole`, or a passage set: one or more SB scope ranges ("1:1-9"), in the order given.
pub enum Scope { Whole, Ranges(ScopeRanges /* ≥ 1 */) }

pub enum BookPresence {
    /// `how` is part of the value, so "Restore the text" against a concurrent "Start empty"
    /// is a visible `BookPresence` conflict, never an equal-value merge (§7.6).
    Present { scope: Scope, how: PresentHow }, // scope: Scope::Whole, or SB range strings (D26)
    /// `seen` = short ids of every head of every record of this book that the remover held,
    /// sorted. A change of the book outside their ancestry raises BookRemovedWhileEdited (§7.6).
    Removed { seen: Vec<ShortId> },
}
/// `Added`: a new book (§5.7 Add, an import, a book file added outside the app, §11.1).
pub enum PresentHow { Added, RestoredText, StartedEmpty }

/// USFM bytes that are not verse content, kept byte for byte (line endings included).
pub struct UsfmBytes(String);

pub enum ChapterFront { Present(UsfmBytes), Absent }

pub enum VerseValue {
    /// A verse, or the first verse of a span when `last > verse`.
    Text {
        marker: MarkerWs,
        /// Everything after the marker up to `layout`: the words, inline markup, internal
        /// poetry and heading lines. Never contains a `\v` or `\c` marker (rule V3).
        words: String,
        /// The trailing run of lines that hold only paragraph-level markers and whitespace
        /// (§2.4). `words + layout` is the tC4 §8.4 verse content, byte for byte.
        layout: String,
        last: VerseNo,
        /// Set by a renumber: the verse number this text had before it was moved here, kept
        /// when the text is edited or moved again, omitted when it equals this verse. The
        /// verse's **anchor** is `from`, else its own number; alignments and comments are
        /// keyed by anchor, so they follow the text (§7.7).
        from: Option<ChapterVerse>,
    },
    /// This verse is inside the span that starts at `by` (`by < verse`, same chapter).
    Covered { by: VerseNo },
    /// The verse is not in this translation. `moved_to` is set by a renumber.
    Absent { moved_to: Option<ChapterVerse> },
}
pub struct ChapterVerse { pub chapter: ChapterNo, pub verse: VerseNo }

/// The whitespace of the verse marker `\v<lead><label><term>`. The label is computed from
/// `verse` and `last`. Canonical: lead " ", term ' '.
pub struct MarkerWs { pub lead: Box<str> /* [ \t]+ */, pub term: MarkerTerm }
pub enum MarkerTerm { Space, Tab, Lf, Cr }

pub enum LayoutItem { Chapter(ChapterNo), Verse(ChapterNo, VerseNo) }

/// The tC4 §5.1 alignment record plus what it was made against. `invalid` is never stored.
pub struct AlignmentValue {
    pub span_end: VerseNo,                      // last verse of the span the aligner showed
    pub alignments: Vec<AlignmentGroup>,        // topWords / bottomWords, occurrences as u32 (I-2)
    pub word_bank: Vec<TargetWord>,
    pub target_verse_md5: Md5,                  // md5(plain_text_v1(span content)) when aligned
    pub source_version: Box<str>,               // tC4 label, e.g. "dcs::unfoldingWord/el-x-koine_ugnt@v0.34"
    pub source: Option<ResourceRef>,            // OL pin (repoPath + sha); None only for imports
    pub done: bool,                             // D73
    pub marked_invalid: bool,                   // tC3/tC4 said invalid; cleared by "Mark valid"
    pub dismissed: bool,                        // dismissed from the Detached list (§7.7); kept
    pub extra: JsonMap,                         // unknown fields, preserved
}
/// tC4 §5.1 `topWords` / `bottomWords` entries. Each `extra` keeps fields tC3 or tC4 wrote
/// that the model does not name.
pub struct AlignmentGroup { pub top_words: Vec<SourceWord>, pub bottom_words: Vec<TargetWord>, pub extra: JsonMap }
pub struct SourceWord {
    pub word: String, pub strong: Option<String>, pub lemma: Option<String>, pub morph: Option<String>,
    pub occurrence: u32, pub occurrences: u32, pub extra: JsonMap,
}
pub struct TargetWord { pub word: String, pub occurrence: u32, pub occurrences: u32, pub extra: JsonMap }

pub enum CheckValue {
    Decision(DecisionRecord),
    /// A carry-over re-attached this decision under `to` (D36). It is a whole value of the
    /// same record, so a concurrent edit of the old decision is a conflict (§8.3).
    MovedTo { to: CheckKey },
}

/// The tC4 §5.2 record. `invalidated`, `verseEdits`, `userInvalid` and a derived `status` are
/// not stored; the fold computes them (§7.7).
pub struct DecisionRecord {
    pub context: JsonMap,                       // full tC3 contextId, verbatim
    pub category: Option<Box<str>>,
    pub judgement: Judgement,
    pub comment: Option<String>,                // check comment; None projects `false`
    pub bookmarked: bool,                       // projects `reminders`
    pub resource: ResourceRef,                  // the tN / TWL pin the item list came from
    pub imported_invalid: bool,                 // tC3/tC4 said invalidated; cleared by a new judgement
    pub dismissed: bool,                        // dismissed from the Detached list (§7.7); kept
    pub extra: JsonMap,
}
pub struct Judgement {
    pub selections: Option<Vec<Selection>>,     // None projects `false`
    pub nothing_to_select: bool,
    pub status: Option<UserStatus>,             // what the person chose
    pub verse_text_md5: Md5,                    // md5(plain_text_v1(verse)) when judged
}
pub enum UserStatus { Valid, Invalid, Todo }
pub struct Selection { pub text: String, pub occurrence: u32, pub occurrences: u32, pub extra: JsonMap }

pub struct CommentValue { pub target: CommentTarget, pub text: String }
pub enum CommentTarget {
    Verse { book: BookId, chapter: ChapterNo, verse: VerseNo },   // a verse anchor (§7.7), not a label
    Check(CheckKey),
    Frame { story: StoryNo, frame: FrameNo },
}

/// Slots of one group, keyed by tC4 slot name (`gatewayLanguage`, `translationNotes`, …).
/// `ExtraScripture` is one slot holding the ordered list.
pub struct PinGroupValue(BTreeMap<SlotName, SlotValue>);
pub struct SlotValue(Json);                     // the slot as tC4 writes it in resources.json
pub struct ResourceRef { pub repo_path: Box<str>, pub sha: GitOid }   // repoPath compared case-insensitively

pub struct VersificationValue { pub name: Option<Box<str>>, pub bytes: String }   // vrs.json verbatim
pub struct DeviceProfileValue {
    pub display_name: Option<Box<str>>,
    /// Written once per device, at its first sync, after the fetch succeeded with the token,
    /// and only after the sign-in name notice (D98; tC4 `signIn.nameNotice`, test id
    /// `signin-name-notice`). A sync refused before its fetch succeeds writes none. Never
    /// changed later, even when another Door43 account signs in on that device: the first
    /// stays and the sync goes on without an error (tC4's first-wins rule). For a tC4 actor:
    /// the first `actor.name.set` username in `ts` order (§13.1 T5).
    pub door43_username: Option<Door43Username>,
    pub legacy: Option<LegacyIdentity>,            // the tC4 actor or tC3 user a derived device stands for
}
/// tC4's grammar (`door43UsernameError`, `journal/grammar.mjs` at d310bf3): an ASCII letter or
/// digit, then letters, digits, `-`, `.` and `_`, never two of `-._` in a row or one last, at
/// most 40 characters. So it is never an email address and never a token.
pub struct Door43Username(Box<str>);
pub enum LegacyIdentity { Tc4Actor { actor: Box<str> }, Tc3User { user: Box<str> } }

pub enum CarriedValue {
    /// The file's bytes are the git blob `oid` in the local object database (§11.4).
    Present { oid: GitOid, size: u64, md5: Md5 },
    /// Removed by an explicit "Remove file" action. Never written because a file went missing.
    Removed,
}

/// "Apply what was recovered" (§7.4): the changes of an incomplete batch this person saw and
/// chose to apply. Grow-only: no parents, never in conflict; the fold applies the union.
pub struct BatchReleaseValue { pub changes: Vec<ChangeId> }   // sorted, unique

/// `value_json` is `None` when the record object has no `value` member.
pub struct UnknownRecord { pub kind: Box<str>, pub key_json: Box<str>, pub value_json: Option<Box<str>> }
```

**Fields from a newer minor.** A change of a known kind whose `v` is above this app's minor may
hold fields this app does not know (§4.5 V1). It parses as that kind, under that kind's key, with
the known fields in the typed value. The unknown fields stay in the stored bytes: values are
compared through their stored canonical JSON (§7.3), so a difference in an unknown field is
never invisible, and projection ignores them (§12.2 rule 7 makes that safe). An enum variant
this app does not know (a new kind of verse value, a new book code) cannot be held in the typed
value: such a change is quarantined as made by a newer translationCore (`NewerVariant`, §4.5
V1). Rule 7 makes a new variant raise `minReaderMinor`, so only a read-only older app meets one.

### 2.4 The verse content split

The tC4 verse content of verse `v` is every byte after its marker up to the next `\v` or `\c`
marker anywhere, or the end of the file (tC4 R-8.4.1). It is split into `words` and `layout`:

- Split the content into lines, each keeping its terminator (`\n` or `\r\n`; a lone `\r` is
  not a terminator).
- A line is **layout-only** if, after removing its terminator, it is empty or consists only of
  spaces, tabs and paragraph-level markers from the closed set `PARA_MARKERS`: tC4's
  `BODY_ENDING_TAGS` (`src/data/usfm/indexer.ts`) with optional digit suffixes, for example `\p`,
  `\m`, `\q1`, `\pi2`, `\li1`, `\b`, `\nb`, `\ts\*`. A line that holds any word, heading text
  (`\s1 Heading`) or character marker is not layout-only. Precisely, as tC4 matches them: a
  marker is `\` and a name (an ASCII letter, then ASCII letters, digits and `-`), optionally
  closed at once by `\*`; it is paragraph-level when its name, lower-cased and with trailing
  digits removed unless it holds a `-`, is in the set; after it comes a space, a tab, another
  marker or the end of the line.
- If every line is layout-only, `words` is empty and `layout` is the whole content.
- Otherwise let L be the last line that is not layout-only. `words` is the content up to L's
  terminator; `layout` is L's terminator and everything after it (empty when L has no
  terminator).

So a poetry verse `\v 1 line one\n\q2 line two\n\q1\n` has `words = "line one\n\q2 line two"` and
`layout = "\n\q1\n"`. Words of a verse are never split into parts. `split_verse` and
`join_verse` live in `tc-usfm` and are property-tested for `join(split(x)) == x`.

A stored value's `layout` passes the **layout rule** (V3) when it is empty, or when every line of
it is layout-only and, if `words` is not empty, it starts with a line terminator. Every split
passes it; a command that edits `words` or `layout` alone keeps the pair passing it.

### 2.5 Changes and canonical bytes

```rust
/// One immutable version of one record. Never edited, never deleted.
pub struct Change {
    pub id: ChangeId,            // sha256(raw)
    pub raw: Arc<[u8]>,          // the canonical JSON exactly as stored; never re-serialized
    pub body: ChangeBody,
}

pub struct ChangeBody {
    pub v: u16,                  // the format minor that introduced the kinds and fields used (§12)
    pub record: Record,
    /// Heads of the SAME record the author held and replaced. Sorted ascending, no duplicates.
    /// Empty = "no version I know of". A resolution names every head it resolves.
    pub parents: Vec<ChangeId>,
    pub device: DeviceId,
    pub seq: DeviceSeq,
    pub at: WallTime,
    pub batch: BatchRef,
    pub cause: Cause,
}

pub enum Cause {
    Edit,                                   // a person typed, clicked, aligned or checked
    Resolve,                                // chose among conflicting heads
    Restore { from: ChangeId },             // brought an older version back from History
    CarryOver { group: PinGroup },          // a resource upgrade or gateway change re-attached it
    /// A carried file's new version found in the folder (§11.4). For derived files no longer
    /// written since 2026-10-22: a changed derived file is imported (§13.6), never recorded on
    /// its own. `unknown_base` marks a change recorded without parents; such a change is never
    /// chosen as provisional (§7.2 F7). Removable before the freeze if no frozen sample holds it.
    External { source: ExternalSource, unknown_base: bool },
    /// tC4, tC3, USFM, foreign burrito (§13); `Usfm` and `Markdown` also for "Update a book"
    /// (§13.6). `unknown_base` on an import marks a change made with no parents, ranked last by
    /// `CauseClass::rank`, so a no-base import is never the provisional text (§13.6 U3).
    Import(ImportSource),
    Seed,                                   // project or book creation
    /// "Start empty" when restoring a removed book (§5.7): a person's choice to blank the
    /// text. Never a seed stub, so a concurrent edit makes an ordinary conflict (§7.3).
    StartOver,
    System(SystemReason),                   // pin coverage backfill (tC4 D41)
    /// A cause from a newer minor (§12): shown as "made by a newer translationCore". Only
    /// read, never written.
    Newer { name: Box<str> },
}
/// `Door43` and `ExchangeFile` are not written since 2026-10-22 (§11.2, §11.3).
pub enum ExternalSource {
    LocalFile { path: RepoPath, seen_md5: Md5 },
    Door43 { commit: GitOid, path: RepoPath, author: Box<str> },
    ExchangeFile { path: RepoPath, exporter: Option<DeviceId> },
}
pub enum ImportSource {
    Tc4 { actor: Box<str>, ts: Box<str>, op: Box<str> },
    Tc4Reconcile,
    Tc3 { user: Option<Box<str>> },
    Usfm { file_name: Box<str> },
    Burrito { file_name: Box<str> },
}
pub enum SystemReason { PinCoverageBackfill }
```

**Canonical JSON.** The author serializes a change once:

- object keys sorted by their UTF-8 bytes, none twice; no insignificant whitespace;
- integers only, no floats anywhere in the format, from −2⁶³ to 2⁶⁴−1, without leading
  zeros, `+` or `-0`; nesting at most 64 levels (V7);
- strings exactly as given (see "Unicode" below); escapes are `\"`, `\\`, `\b`, `\f`, `\n`,
  `\r`, `\t`, and `\u00xx` (lowercase hex) for the other characters below U+0020; every other
  character is raw UTF-8;
- ids and hashes as lowercase hex; `DeviceId` in its `d…` form; field names in camelCase;
  a variant of an enum whose variants carry no data (`MarkerTerm`, `UserStatus`, `ToolId`,
  `PinGroup`) as its camelCase name in a string; every other enum variant as
  `{"variantName": {…}}`, with `{}` when that variant has no fields and, for a variant that
  wraps one value (`Cause::Import`, `ChapterFront::Present`, `MetaField::Extra`), that value's
  encoding in place of `{…}`; `None` fields omitted; `extra` maps omitted when empty; empty
  arrays written. §2.8 gives the wire form of every type.

A reader checks the form, it does not assume it: bytes that are not exactly what the
serializer would write (a space, a key out of order, `1.0`, `\u0041`) break rule V1. So every
value has one byte form, and comparing stored bytes compares values (§7.3).

`ChangeId = sha256(those bytes)`. A reader verifies the hash, then parses. It never
re-serializes another device's change, so a newer app's extra fields and a different serializer
cannot change anyone's ids. Canonical form matters for one thing only: deterministic changes
(ingested Door43 edits, imports) must have equal bytes on every device, so all writers use the
one serializer in `tc-journal::canon`.

**Unicode.** Text a person types in the app is normalized to NFC before it goes into a change
(tC4 I-4). Imported text is stored byte for byte, so an import never changes a byte of the
source (P9); `metadata.json` declares `normalization: "NFC"` only when every text ingredient is
in NFC. Identity strings must already be NFC (rule V2).

Example (one line on disk, wrapped here):

```json
{"at":1760520000000,"batch":{"id":"6f0c…","n":2},"cause":{"edit":{}},"device":"dk3q…",
 "parents":["9f2c…"],"record":{"verse":{"book":"TIT","chapter":1,"value":{"text":{"last":5,
 "layout":"\n\\p\n","marker":{"lead":" ","term":"space"},"words":"Ellos deben enseñar…"}},
 "verse":4}},"seq":4182,"v":0}
```

### 2.6 Fold output

`FoldState` keeps its tables in shared, copy-on-write containers (append-only chunked vectors
and sharded hash maps; persistent ordered maps for the lists), so a clone is a snapshot that
costs a few hundred pointer copies, whatever the project's size. Its fields are private; screens,
commands and projections read it through methods:

```rust
impl FoldState {
    pub fn fold(kind: ProjectKind, changes: impl IntoIterator<Item = FoldInput>) -> Self;  // §7.2
    pub fn fold_from(kind: ProjectKind, source: Option<Arc<dyn ValueSource>>,
                     changes: impl IntoIterator<Item = FoldInput>) -> Self;              // values on demand, §14
    pub fn add(&mut self, changes: impl IntoIterator<Item = FoldInput>) -> Delta;            // §7.8
    pub fn key_state(&self, key: &RecordKey) -> Option<KeyState>;
    pub fn projected(&self, key: &RecordKey) -> Option<Record>;   // what projection shows
    pub fn conflicts(&self) -> impl Iterator<Item = &Conflict>;    // sorted: canon book order, then key, then kind
    pub fn conflict_counts(&self) -> &BTreeMap<(Option<BookId>, u8), u32>;  // kept by every step
    pub fn anomalies(&self) -> impl Iterator<Item = &Anomaly>;     // sorted by kind, then key
    pub fn detached(&self) -> impl Iterator<Item = &RecordKey>;    // heads with no place on any screen (§7.7)
    pub fn decisions(&self, tool: ToolId, collection: Collection) -> Vec<ProjectedDecision>; // a Check session's input (§7.7)
    pub fn frontier(&self) -> Frontier;                            // sha256 of the sorted set of all head ids
    pub fn shown_heads(&self, key: &RecordKey, displayed: Option<ChangeId>) -> Vec<ChangeId>;  // the token rule, §2.7
    pub fn unreadable_records(&self) -> Vec<RecordKey>;   // versions the source cannot read, below
    pub fn retry_unreadable(&mut self) -> Delta;          // judges those records again
    // also: change, body, raw, history, book, books, placement, derived_files, render,
    // book_usfm, chapter_usfm, carried_files, metadata_source, resolution and the choices of §8.3
}
/// A verified line and the pack holding it. `prepare()` reads what the fold keeps and drops
/// the decoded value, so a loader does that on its reading threads. `prepare_at(at)` also
/// drops the bytes and keeps only where they lie; the fold reads them through its
/// `ValueSource` when it merges, lists a candidate or projects (§14).
pub struct FoldInput { line: Verified, pack: Option<PackId> }
/// Where a loader found a change's canonical JSON: its own number for the file, and the span
/// of the JSON in it (`PackRead::spans`, `PendingRead::spans`).
pub struct ValueAt { pub offset: u64, pub file: u32, pub len: u32 }
/// `tc-store` reads the file again, or the pack's blob when the file is gone or damaged, and
/// reports a value it cannot read as a notice. `prepare_at` takes a 64-bit fingerprint of the
/// bytes it verified against the change id, and the fold checks the bytes it reads against it,
/// so a file changed after it was read does not pass for the change. The fingerprint runs four
/// lanes over the 8-byte words, each step `lane = mix(lane ^ word)` with `mix` splitmix64's
/// finalizer (a bijection in which every input bit flips about half the output bits), then
/// folds the lanes and the length through the same step: a change within one 8-byte word never
/// passes (every step is one to one), any other accidental change with a chance of about
/// 2⁻⁶⁴. It is no defence against forgery (§18 item 3). (Computing SHA-256 again was a third
/// of a book's projection without SHA instructions, §14; the id was proved when the bytes
/// were read. FxHash, used first, let a two-bit burst across a word boundary pass.)
///
/// A value the source cannot read is unknown, never absent and never equal to another: a
/// record with several heads and such a head or base is not merged or settled but listed as
/// a conflict whose `unreadable` names the versions, a token never names an unreadable
/// version (the screen did not show it), and no choice is made into changes while a version
/// of the conflict is unreadable (`ResolveError::Unreadable`). `retry_unreadable` judges
/// those records again, for example at the next checkpoint. Because such a state depends on
/// this machine, `tc-store` writes no derived file while `unreadable_records` is not empty.
pub trait ValueSource: Send + Sync { fn read(&self, at: ValueAt) -> Option<Vec<u8>>; }
/// What one `add` changed: the records, books and chapters to re-project (§5.0 B5), the
/// conflicts that did not exist before (B6), and whether pins or paths changed.
pub struct Delta { pub keys: Vec<RecordKey>, pub books: Vec<BookId>, pub chapters: Vec<(BookId, u16)>,
                   pub new_conflicts: Vec<ConflictId>, pub pins: bool, pub paths: bool, pub everything: bool }

pub struct KeyState { pub heads: Vec<ChangeId>, pub state: Resolved }

pub enum Resolved {
    Settled { head: ChangeId },
    /// Several heads; the projection uses `head`, no conflict is listed (§7.3 stub rules).
    SettledByRule { head: ChangeId, rule: AutoRule },
    /// Several heads merged without disagreement. Computed, never stored.
    Merged { merged: Box<Record>, how: MergeHow },  // EqualValues | Units(Vec<UnitName>) | Union (a grow-only BatchRelease)
    /// The record is in this listed conflict: its own, or a grouped one (Passage).
    Conflict(ConflictId),
}
pub enum AutoRule { SeedStubs }

pub struct Conflict {
    pub id: ConflictId,                      // sha256 of the sorted head ids: the same on every device
    pub key: RecordKey,
    pub also_keys: Vec<RecordKey>,           // the other records of a grouped conflict (Passage, StructureIssue, PathCase)
    pub kind: ConflictKind,
    pub bases: Vec<ChangeId>,                // maximal common ancestors of the heads (§7.3)
    pub candidates: Vec<Candidate>,          // one per distinct head value (newest first); for a Passage, every head of its records
    pub agreed: Vec<(UnitName, UnitValue)>,  // units that merged cleanly; `UnitValue(None)` = the fields are absent
    pub disputed: Vec<UnitName>,             // empty for whole-value kinds
    pub provisional: ChangeId,               // head with the largest (rank, at, id), or the passage side's newest change (§7.2 F7)
    pub batches: Vec<BatchId>,               // batches of the heads it lists, a restore race's verse heads included, for grouping (§8.1)
    pub incomplete_history: bool,            // a head it involves has a MissingParent link (§7.4, §8.1); a warning, never a reason to hide a choice
    pub unreadable: Vec<ChangeId>,           // versions this computer cannot read yet (§2.6); not candidates
}
pub struct Candidate {
    pub head: ChangeId,
    pub also: Vec<ChangeId>,                 // other heads with an identical value
    pub device: DeviceId,
    pub author: Option<Box<str>>,            // from DeviceProfile: the Door43 username, else the display name
    pub at: WallTime,
    pub cause: Cause,
    pub value: Record,
    pub side: Option<Side>,                  // for a Passage: Batch(id) of a passage batch, or Others (§7.3)
}

pub enum ConflictKind {
    VerseText, VerseStructure, Passage, StructureIssue, BookHeader, ChapterFront, BookPresence,
    BookRemovedWhileEdited { unseen: u32 }, LastBookRemoved,
    Alignment, Check, CheckMoved, Comment, Pins, Setting, ProjectMeta, Versification,
    StoryBase, Frame, StoryRef, CarriedFile, PathCase, Unknown,
    BookLayout,      // two orders of an imported book
    DeviceProfile,   // two versions of one device's profile (only a duplicated device makes this)
}

/// Change-level defects. A function of the change set, so every device with the same changes
/// lists the same anomalies.
pub enum Anomaly {
    QuarantinedChange { change: ChangeId, rule: RuleViolation },     // §4.5
    IncompleteBatch { batch: BatchId, device: DeviceId, have: u32, of: u32 },
    MissingParent { change: ChangeId, parent: ChangeId },
    ParentOnOtherRecord { change: ChangeId, parent: ChangeId },
    DuplicateDevice { device: DeviceId, seq: DeviceSeq, changes: Vec<ChangeId> },
    UnknownKinds { kinds: Vec<Box<str>>, changes: u32 },             // newer minor format (§12)
    /// Records of a book that no `BookPresence` change makes present (§7.6): kept, listed.
    RecordsWithoutBook { book: BookId, records: u32 },
    /// A value that would make `metadata.json` invalid; an older valid value is used (§3.4).
    InvalidForBurrito { key: RecordKey, change: ChangeId, reason: Box<str> },
}

/// Notices depend on the files and the machine (which copies are damaged here, the clock, the
/// object database), so `tc-store` computes them outside the fold, and they are not part of the
/// convergence comparison. The person sees anomalies and notices in one list.
pub enum Notice {
    QuarantinedPack { path: RepoPath, reason: PackError, salvaged: u32 },
    QuarantinedPending { file: Box<str>, reason: PackError, salvaged: u32 },
    NewerPack { path: RepoPath, tcpack: u64 },                       // carried, not read
    ClockAhead { device: DeviceId, by_ms: i64 },                     // `at` > this machine's now + 24 h
    CarriedBytesMissing { path: RepoPath, oid: GitOid },
    DeviceRotated { old: DeviceId, new: DeviceId, reason: RotationReason },
    RestoredBackup { missing: u64, last_at: WallTime },
    /// The report of an import of a kept copy or of "Update a book" (§13.6 U6).
    BookImported { path: RepoPath, imported: u32, conflicts: u32, unreadable: u32, batch: BatchId },
    /// The file lost alignments, decisions or comments that a file cannot remove (§13.6 U3).
    OutsideRemovalKept { path: RepoPath, records: u32 },
    /// `metadata.json` fields the app writes itself were edited; they are written again.
    DerivedFieldsEdited { path: RepoPath, fields: u32 },
    /// `project.json`, `format.json` or `conflicts.json` was edited (§3.5): written again, `copy` holds the bytes found.
    FixedFileRestored { path: RepoPath, copy: Box<str> },
    /// A generated file other than a book (`metadata.json`, a settings file, a sidecar) held
    /// bytes the app did not write; they are kept in `copy` and the file was written again (§5.10).
    GeneratedFileRewritten { path: RepoPath, copy: Box<str> },
    /// The app's own bytes show work the journal lost; a copy is kept, with *Bring it back* (§6.2 N4).
    LostWorkKept { path: RepoPath, copy: Box<str> },
    /// A file changed on Door43, or inside an exchange file, by another tool: reported once,
    /// never merged (§11.2, §11.3). `commit` is none for an exchange file.
    OutsideToolEdit { path: RepoPath, commit: Option<GitOid>, who: Box<str> },
    OutsideEditUnreadable { path: RepoPath, copy: Box<str>, reason: Box<str> },
    FileMoved { from: Box<str>, to: Box<str> },                      // not a valid ingredient path
    MissingFileRebuilt { path: RepoPath },
    /// A pending file of this session's saves was removed or cut short by another program
    /// before the seal took it; the seal wrote the `changes` it lost from the fold's bytes (§5.11 K1).
    PendingWrittenAgain { file: Box<str>, changes: u32 },
    /// A pack missing from the working tree or damaged there, written back from a valid blob
    /// in the object database (HEAD, the confirmed commit or R, §4.4).
    PackRestored { path: RepoPath, was: PackLoss },                  // Missing | Damaged
    /// A file that could not be read (sharing violation, cloud placeholder, permission).
    /// Never treated as damage; retried at the next checkpoint.
    FileUnreadable { path: Box<str>, reason: Box<str> },
    /// A book file or story changed outside the app, kept with a copy and left as it is until
    /// the person answers *Import* or *Not now* (§11.1).
    OutsideEditHeld { path: RepoPath, copy: Box<str> },
    /// Derived files (§5.10) or a commit (§5.13) could not be written and the work is saved.
    /// Listed once for each reason and path set, and gone when the blockage clears (APP R16).
    MaterializeBlocked { paths: Vec<RepoPath>, reason: Blocked },
    /// An expected pack (§4.4) is missing and no copy could be restored: nothing derived is
    /// removed and no tree is built until it is back.
    PackMissing { path: RepoPath },
    /// Line-ending or trailing-whitespace differences in a changed file, not imported (§11.1 Q2).
    WhitespaceOnlyEdit { path: RepoPath, keys: u32 },
    /// Another installation writes into this folder (a cloud-synced folder, §5.0).
    SecondWriter { device: DeviceId },
    /// A save's file was renamed into place but the disk reported an error after it (§5.0).
    SavedWithDiskError { reason: Box<str> },
    ImportUnfinished { source: Box<str> },
}
pub enum PackLoss { Missing, Damaged }
pub enum Blocked {
    FileBusy,                           // another program holds the file; its rename kept failing for 2 s
    FileUnreadable,                     // the file could not be read to compare it before it was written over
    ValuesUnreadable { records: u32 },  // values of the fold cannot be read on this computer yet (§2.6)
    PackMissing,                        // an expected pack is missing: nothing derived is removed (§4.4)
    TreeInvalid { problem: Box<str> },  // the commit's tree is not a valid burrito: no commit (§9.4)
    Failed { reason: Box<str> },        // a write failed for another reason
}
pub enum RotationReason { FolderCopied, BackupRestored, DuplicateDevice }   // §6.3
```

`ConflictId` and `Frontier` are SHA-256 over the 32-byte head ids in ascending order,
concatenated, without repeats. A conflict that is not one record's or one passage group's heads
(`StructureIssue`, `BookRemovedWhileEdited`, `LastBookRemoved`, `PathCase`) takes its id over its
kind tag and a NUL byte, then the head ids (for `StructureIssue`, the tag also holds its key's
canonical JSON; for `PathCase`, the collision form), so a book's presence conflict and its
removed-while-edited conflict over the same heads never share an id.

### 2.7 Shown tokens and commands

The core hands the UI an opaque `ShownToken` with every record it displays. A command that
changes a record must carry the token of what was displayed. This is how I4 holds across about
60 screens.

```rust
/// Serialized to the UI as an opaque base64url string. `mac` is HMAC-SHA256, truncated to 16
/// bytes, under a key derived from the installation secret (§3.7) and the project id, so a token
/// held in the browser-side draft buffer survives a restart. The key never leaves the server.
/// The core rejects a token with a bad MAC, so a screen cannot construct "the current heads".
pub struct ShownToken { kind: TokenKind, key: RecordKey, heads: SmallVec<[ChangeId; 2]>, mac: [u8; 16] }
pub enum TokenKind { Edit, Review }
```

Byte form: base64url without padding of `payload ‖ mac`. `payload` is the canonical JSON
`{"heads":[…],"key":{…},"kind":"edit"|"review"}`, heads sorted without repeats and the key as
in §2.8; `mac` is the first 16 bytes of HMAC-SHA256 of `payload` under the token key, and the
token key is HMAC-SHA256 under the installation secret of `"tc4-shown-token\0" ‖ the project
id's 16 bytes`. A payload that is not canonical, or whose heads are not sorted without repeats,
is refused even under a good MAC.

The kind is under the MAC (APP-DESIGN R19). An **edit** token lists the heads by the table
below and is what a save takes; a **review** token lists every head of the record and is what
only a resolution (`Resolve`, `ResolveBulk`, §5.9) takes. The actor refuses a review token in a
save and an edit token in a resolution (`Refusal::WrongTokenKind`), so a resolution screen's
token can never be replayed as an edit that silently resolves, nor an editor's as a resolution
of versions it never showed. A token whose MAC this installation can no longer check (a new
installation secret) is read only to offer a draft's text (§5.14, `parse_unverified`), never
as a base.

What `heads` holds depends on the record's state when it was displayed:

| State displayed | `heads` in the token |
|---|---|
| Settled, SettledByRule, Merged | every head (the person saw a value that stands for all of them) |
| Conflict | only the displayed candidate's head and its `also` heads |
| An earlier version shown for a conflict (a passage side rewound for the provisional text, §7.3; a *Start empty* rewound after *Restore the text*, §7.6) | that version's change, which is not a head, so an edit continues what was shown and every head stays |
| No record yet, or nothing shown for it while its passage side did not have it | empty |

Commands (types in `tc-journal`, executed by the project actor in `tc-store`, called by
`tc-app`). Each one is one batch, all or nothing:

```rust
pub enum Command {
    SaveVerse    { shown: ShownToken, words: String },                 // keeps marker, layout, last, from
    SaveLayout   { shown: ShownToken, layout: String },                // paragraph break after a verse
    /// One token per verse the editor displayed (every verse of the range, chapter fronts
    /// included); `None` = displayed and unchanged (§5.2).
    SaveSection  { book: BookId, shown: Vec<(ShownToken, Option<SectionValue>)>, layout: Option<ShownToken> },
    SaveAlignment{ shown: ShownToken, verse_shown: ShownToken, value: AlignmentEdit },
    SaveDecision { shown: ShownToken, verse_shown: ShownToken, value: DecisionEdit },
    AddComment   { target: CommentTarget, text: String },
    SetSetting   { shown: ShownToken, value: Option<Json> },
    SetMeta      { shown: ShownToken, value: Option<Json> },
    SetPins      { shown: ShownToken, value: PinGroupValue },          // outside an upgrade (§5.6)
    AddBook      { book: BookId, scope: Scope, seed: SeedSource },
    RemoveBook   { book: BookId, presence_shown: ShownToken },
    RestoreBook  { book: BookId, presence_shown: ShownToken, mode: RestoreMode },  // KeepText | StartEmpty
    SaveFrame    { shown: ShownToken, text: String },                  // OBS; StoryRef the same
    ApplyUpgrade { plan: UpgradePlan },                                // §5.8
    Resolve      { conflict: ConflictId, shown: Vec<ShownToken>, choice: Choice }, // review tokens
    /// One candidate per conflict, made on a computer of `devices` (§5.9); review tokens.
    ResolveBulk  { devices: BTreeSet<DeviceId>, conflicts: Vec<(ConflictId, Vec<ShownToken>)> },
    /// Re-key a decision a carry-over could not place (§8.3).
    MoveCheck    { shown: ShownToken, to: CheckKey, to_shown: ShownToken },
    Restore      { shown: ShownToken, from: ChangeId },
    RemoveCarriedFile { shown: ShownToken },
    SetBookScope { presence_shown: ShownToken, scope: Scope },          // a passage set (D26)
    ReleaseBatch { batch: BatchId },                                   // "Apply what was recovered" (§7.4)
    Dismiss      { shown: ShownToken },                                // a Detached alignment or decision (§7.7)
    ReviseComment{ shown: ShownToken, text: String },                  // §5.5
    Draft        (Draft),                                              // §5.14; writes no change
    /// *Keep* a draft offered at open (§5.14); `section` holds a section draft's values.
    KeepDraft    { draft: Draft, section: Vec<(ShownToken, Option<SectionValue>)> },
}
/// Unsaved editor text: the tokens of every record the editor displayed (a section draft
/// carries one per displayed verse and chapter front, APP-DESIGN R6; a new comment none).
pub struct Draft { pub field: DraftField, pub shown: Vec<ShownToken>, pub text: String }
```

`SectionValue` is a `VerseValue` or a `ChapterFront` value for the record its token names. The
other argument types:

```rust
/// What the aligner made; the actor adds span end, text hash and source from the displayed
/// verse and the pins (§5.3).
pub struct AlignmentEdit { pub alignments: Vec<AlignmentGroup>, pub word_bank: Vec<TargetWord>, pub done: bool }
/// What the checker changed (§5.4): `basis` (context, category) for a first decision, and
/// `resource`, the pin of the check list the person judged in, which a judgement records.
/// `DecisionEdit::apply` is the one implementation of §5.4; the Check screens build the edit.
pub struct DecisionEdit { pub basis: Option<DecisionBasis>, pub resource: ResourceRef,
                          pub changes: Vec<DecisionChange> }
pub enum DecisionChange { Status(Option<UserStatus>), Selections(Option<Vec<Selection>>),
                          NothingToSelect(bool), Comment(Option<String>), Bookmarked(bool) }
/// The literal source book a new book's stubs copy their structure from (§5.7).
pub struct SeedSource { pub usfm: String }
pub enum RestoreMode { KeepText, StartEmpty }
/// §5.8: each planned change carries its record's heads in the snapshot as parents; the actor
/// confirms every record still has them before it writes the plan as one batch.
pub struct UpgradePlan { pub changes: Vec<PlannedChange> }
pub struct PlannedChange { pub record: Record, pub parents: Vec<ChangeId>, pub cause: Cause }
/// §8.3: keep one candidate (and its side, for a grouped conflict), or values the person
/// edited, combined or chose unit by unit, one per record.
pub enum Choice { Candidate { head: ChangeId }, Values(Vec<Record>),
                  KeepPassage { heads: Vec<ChangeId> }, KeepSeparate { heads: Vec<ChangeId> },
                  KeepJoined { heads: Vec<ChangeId> }, ApplyToMovedCheck,
                  RenameCarried { from: RepoPath, to: RepoPath } }
/// `UserComment` is the comment box (a new comment on `target`, or a revision of the comment
/// its one token names, APP-DESIGN R15); `Alignment` the aligner's unsaved links as
/// `AlignmentEdit::to_json` writes them (R23).
pub enum DraftField { Words, Layout, Section { book: BookId }, Frame, StoryRef, CheckComment,
                      UserComment { target: CommentTarget }, Alignment }
```

The reply to every command carries the new tokens and the new conflicts it caused, so the screen
can say "Juan changed this verse while you were editing; both versions are kept" at once.

### 2.8 Wire form

`tc_journal::canon` writes every type as below (§2.5 rules; keys appear here in canonical
order). `?` marks a member left out when it is `None` (or, for `extra`, empty). The fixture
`fixtures/journal/v0-every-kind.ndjson` pins these bytes: a test decodes each line and encodes
it again.

| Type | JSON |
|---|---|
| `ChangeBody` | `{"at", "batch", "cause", "device", "parents", "record", "seq", "v"}` |
| `BatchRef` | `{"id", "n"}` |
| `Record` | `{"<kind>": {<key members>, "value"?}}` with kinds and key members `bookPresence {book}`, `bookHeader {book}`, `chapterFront {book, chapter}`, `verse {book, chapter, verse}`, `bookLayout {book}`, `alignment {book, chapter, verse}`, `check {key}`, `comment {id}`, `pins {group}`, `resourcesExtra {level, name}`, `setting {key}`, `projectMeta {field}`, `versification {}`, `deviceProfile {device}`, `storyBase {story}`, `frame {frame, story}`, `storyRef {story}`, `carriedFile {path}`, `batchRelease {batch}`; `value` is left out only for a `None` `BookLayout`, `ResourcesExtra`, `Setting`, `ProjectMeta` or `StoryRef` value; `ResourcesLevel`: `"top"`, `"languageSets"`, `"resources"` |
| `RecordKey` | the record object without `value` (in shown tokens and `UnknownKey`) |
| `BookPresence` | `{"present": {"how", "scope"}}`, `{"removed": {"seen": [..]}}` (sorted, no repeats) |
| `PresentHow` | `"added"`, `"restoredText"`, `"startedEmpty"` |
| `Scope` | `{"whole": {}}`, `{"ranges": ["1:1-9", ..]}` (at least one, each as SB writes it) |
| `ChapterFront` | `{"present": "<usfm>"}`, `{"absent": {}}` |
| `VerseValue` | `{"text": {"from"?, "last", "layout", "marker", "words"}}`, `{"covered": {"by"}}`, `{"absent": {"movedTo"?}}` |
| `ChapterVerse` | `{"chapter", "verse"}` |
| `MarkerWs` | `{"lead", "term"}`; `MarkerTerm`: `"space"`, `"tab"`, `"lf"`, `"cr"` |
| `LayoutItem` | `{"chapter": {"chapter"}}`, `{"verse": {"chapter", "verse"}}` |
| `AlignmentValue` | `{"alignments", "dismissed", "done", "extra"?, "markedInvalid", "source"?, "sourceVersion", "spanEnd", "targetVerseMd5", "wordBank"}` |
| `AlignmentGroup` | `{"bottomWords", "extra"?, "topWords"}` |
| `SourceWord` | `{"extra"?, "lemma"?, "morph"?, "occurrence", "occurrences", "strong"?, "word"}` |
| `TargetWord` | `{"extra"?, "occurrence", "occurrences", "word"}` |
| `ResourceRef` | `{"repoPath", "sha"}` |
| `CheckValue` | `{"decision": <DecisionRecord>}`, `{"movedTo": {"to"}}` |
| `DecisionRecord` | `{"bookmarked", "category"?, "comment"?, "context", "dismissed", "extra"?, "importedInvalid", "judgement", "resource"}` |
| `Judgement` | `{"nothingToSelect", "selections"?, "status"?, "verseTextMd5"}`; `UserStatus`: `"valid"`, `"invalid"`, `"todo"` |
| `Selection` | `{"extra"?, "occurrence", "occurrences", "text"}` |
| `CheckKey` | `{"checkId", "occurrence", "target", "tool"}`; `ToolId`: `"translationWords"`, `"translationNotes"`, `"translationQuestions"` |
| `CheckTarget` | `{"verse": {"book", "chapter", "verse"}}` (strings), `{"frame": {"frame", "story"}}` |
| `CommentValue` | `{"target", "text"}`; target `{"verse": {"book", "chapter", "verse"}}`, `{"check": <CheckKey>}`, `{"frame": {"frame", "story"}}` |
| `PinGroup` | `"primary"`, `"fallback"`, `"originalLanguage"`, `"lexicon"`, `"extraScripture"` |
| `PinGroupValue` | `{"<slot name>": <slot value>}` |
| `MetaField` | `{"name": {}}`, `{"abbreviation": {}}`, `{"description": {}}`, `{"languages": {}}`, `{"copyright": {}}`, `{"extra": "<path>"}` |
| `VersificationValue` | `{"bytes", "name"?}` |
| `DeviceProfileValue` | `{"displayName"?, "door43Username"?, "legacy"?}`; legacy `{"tc4Actor": {"actor"}}`, `{"tc3User": {"user"}}` |
| `CarriedValue` | `{"present": {"md5", "oid", "size"}}`, `{"removed": {}}` |
| `BatchReleaseValue` | `{"changes": [..]}` (sorted, no repeats) |
| `Cause` | `{"edit": {}}`, `{"resolve": {}}`, `{"restore": {"from"}}`, `{"carryOver": {"group"}}`, `{"external": {"source", "unknownBase"}}`, `{"import": <ImportSource>}`, `{"seed": {}}`, `{"startOver": {}}`, `{"system": "pinCoverageBackfill"}` |
| `ExternalSource` | `{"localFile": {"path", "seenMd5"}}`, `{"door43": {"author", "commit", "path"}}`, `{"exchangeFile": {"exporter"?, "path"}}` |
| `ImportSource` | `{"tc4": {"actor", "op", "ts"}}`, `{"tc4Reconcile": {}}`, `{"tc3": {"user"?}}`, `{"usfm": {"fileName"}}`, `{"burrito": {"fileName"}}` |
| identifiers | `ChangeId` 64 hex; `ShortId`, `BatchId`, `CommentId`, `Md5`, `PackName` 32 hex; `GitOid` 40 hex; `DeviceId` `d` + 26 base32, whose unused bits are zero; `BookId` its upper-case USFM code; `ProjectId` lowercase hyphenated; numbers as integers (`ChapterNo`, `VerseNo` 1–65,535, `StoryNo` 1–50, `FrameNo` 0–65,535, `WallTime` an `i64`, `DeviceSeq` a `u64`) |

---

## 3. On-disk layout

### 3.1 Bible project

```
<projects>/<folder>/                         git work tree; never checked out by git
  .gitignore                                 "/.tc/\n.~*\n**/*.bak\n"                       app-owned, not an ingredient
  .gitattributes                             "* -text\n"                                       app-owned, not an ingredient
  metadata.json                              DERIVED  SB 1.0; lists every ingredient (§3.4)
  ingredients/
    <BOOK>.usfm                              DERIVED  one per present book
    vrs.json                                 DERIVED  Versification bytes, verbatim
    checking/alignments/<BOOK>.json          DERIVED  tC4 §5.1 shape, one record per line, `invalid` computed
    checking/translationWords/<BOOK>.json    DERIVED  tC4 §5.2 shape, flags computed
    checking/translationNotes/<BOOK>.json    DERIVED
    checking/translationQuestions/<BOOK>.json  DERIVED only when tQ decisions exist
    checking/resources.json                  DERIVED  tC4 §5.3 schemaVersion 2, from the Pins records
    checking/settings.json                   DERIVED  tC4 §5.4, from the Setting records
    tc/project.json                          WRITTEN ONCE at creation (§3.5)
    tc/format.json                           DERIVED  format version, maximum over every intake (§12)
    tc/comments/<BOOK>.json                  DERIVED  user comments, for tools that are not translationCore
    tc/conflicts.json                        DERIVED  open conflicts; present only when there are any
    tc/changes/<sealer>/<yyyy-mm>/<pack32>.ndjson   CANONICAL immutable packs (§4)
    audio/…, any other file                  CARRIED  CarriedFile records (§11.4)
    <stem>.conflict-<oid8>.<ext>             CARRIED  the other version of a carried file in conflict
  .tc-moved/<path>, dot paths, other names   FOREIGN  entries from Door43 the app does not own, kept verbatim in commits (§9.4)
  .tc/                                       LOCAL ONLY, git-ignored, never exchanged, not an ingredient (§3.6);
                                             a folder whose .tc/ a cloud client replicates is gated (§5.0)
  .git/                                      core.autocrlf=false, core.fsyncObjectFiles=true, tc.branch=<b>
```

`.tc/` is outside `ingredients/`, so the burrito is valid between saves: a save changes only
`.tc/pending/`. Ingredients change only at a materialization, a seal, a consolidation or an
intake.

### 3.2 OBS project

The same top level and `tc/` subtree. Instead of books:

- `ingredients/content/01.md` … `50.md`: DERIVED from `StoryBase`, `Frame` and `StoryRef` by
  byte-strict splices (tC4 R-10.3.4).
- `ingredients/LICENSE.md`, `content/front/title.md`, `content/front/intro.md`,
  `content/back/intro.md`: CARRIED files, seeded at creation as `CarriedFile` records with
  `cause: Seed`.
- `checking/translationNotes/OBS.json`, `checking/translationWords/OBS.json`: DERIVED, `"book":"OBS"`.
- `tc/comments/OBS.json`: DERIVED.
- No `vrs.json` and no alignment sidecars. `currentScope` is the template table verbatim.

OBS projects sync and exchange exactly like Bible projects. Nothing in the core is Bible-only.

### 3.3 Ingredients and paths

**Path grammar** (tC4 §2, Pankosmia `utils/paths.rs`, tC4 `ipathError`): every segment is
non-empty; does not start with `.`; does not end with `.`; contains none of
`.. ~ \ & * + | space ? # % { } < > $ ! '`, no C0/C1 control and no bidi control character; is not
a Windows device name (`con prn aux nul com1-9 lpt1-9`, any case, with or without extension).
`RepoPath` and `IngredientPath` parse with this grammar.

**Case collisions.** Windows and macOS file systems ignore letter case, so two paths **collide**
when their NFC forms are equal after Unicode default case folding. The app never writes a path to
the working tree that collides with another path it writes or keeps there:

- A carried path that collides with a derived path, or with another carried path, is a
  `PathCase` conflict (§8.1). The derived path, or else the carried path with the byte-wise
  smallest name, keeps its place; each other colliding file projects at
  `<stem>.case-<first 8 hex of its oid>.<ext>` until a person renames or removes it (§7.7).
- A file of R that this app keeps verbatim (§9.2 N5, §12.2 rule 4) and that collides with a
  path the app writes stays in the object database and the commit tree only; the person is told.
- Detection never reads a collision as an outside edit: a projected path is compared with the
  bytes the app wrote at that exact path, and the materializer writes colliding paths only at
  their projected, non-colliding names.

**The exact-set rule.** The ingredients listed in `metadata.json` are exactly the files under
`ingredients/` whose path has no segment starting with `.`. Temporary files are named
`.~<name>.<16 hex>.tmp`, so they are never listed and Pankosmia's rescan skips them.

**Files that cannot be ingredients.** A path with a segment starting with `.` is never listed and
never carried as a record (open removes only the app's own `.~*.tmp` files). In the working tree,
a file under `ingredients/` whose path has no such segment but breaks the grammar (for example
`TIT (conflicted copy).usfm` from a cloud-sync client), and anything under tC4's exchange folders
`ingredients/tc4-sync/` and `ingredients/tc4-open/`, is moved to
`.tc/external/<utc>-<escaped path>` (a rename, both folders flushed), and the person is told where
(`FileMoved`). OS junk (`Thumbs.db`, `desktop.ini`, `*.bak`, `*~`, `*.tmp`) is moved the same way
without a notice. Nothing is deleted. The files are found by a listing of the root and of
`ingredients/` (not `tc/changes/`, and no folder or file whose name starts with `.`), which opens
no file: the open does it after O9 classified, the window focus does it, and the watcher's word of
a new path does it for that path (§11.1). A file another program holds is left, and the next look
tries again.
Such entries arriving from Door43 or in an exchange file are **foreign entries**: they are kept
in every commit tree, not dropped (§9.4). A foreign entry whose name Windows cannot write as named (a device
name, a trailing dot or space, a forbidden character, an 8.3 short name), or that Windows or
macOS would store as one file with another, is carried byte for byte in commits and exchange
files but written to disk by no join and no new project from a file, on every OS alike; the
report log lists it (owner ruling "carry, skip on disk", 2026-10-28). A name that could leave
the folder refuses the whole join or file, and so does such a name on a file the app owns.

| Path | Class | mimeType | role | scope |
|---|---|---|---|---|
| `ingredients/<BOOK>.usfm` | derived | `text/plain` | — | from the book's `Scope`: `{BOOK: []}` or ranges |
| `ingredients/vrs.json` | derived | `application/json` | — | — |
| `checking/alignments/<BOOK>.json` | derived | `application/json` | `x-alignment` | `{BOOK: []}` |
| `checking/<tool>/<BOOK>.json` | derived | `application/json` | `x-check-decisions` | `{BOOK: []}` (none for `OBS.json`) |
| `checking/resources.json` | derived | `application/json` | `x-resource-links` | — |
| `checking/settings.json` | derived | `application/json` | `x-check-settings` | — |
| `tc/project.json`, `tc/format.json` | written once / derived | `application/json` | `x-tc-project` | — |
| `tc/comments/<BOOK>.json` | derived | `application/json` | `x-tc-comments` | `{BOOK: []}` (none for `OBS.json`) |
| `tc/conflicts.json` | derived | `application/json` | `x-tc-conflicts` | — |
| `tc/changes/**.ndjson` | canonical | `application/x-ndjson` | `x-tc-changes` | — |
| `content/NN.md` (OBS) | derived | `text/markdown` | — | — |
| carried files | carried | extension table | — | — |

Extension table for every other file: `.usfm .sfm .txt` → `text/plain`; `.md` → `text/markdown`;
`.json` → `application/json`; `.tsv` → `text/tab-separated-values`; `.wav` → `audio/wav`;
`.mp3` → `audio/mpeg`; `.ogg` → `audio/ogg`; `.png` → `image/png`; `.jpg .jpeg` → `image/jpeg`;
`.pdf` → `application/pdf`; `.zip` → `application/zip`; anything else →
`application/octet-stream`. Roles are written because the app owns `metadata.json`, but no
reader depends on a role: paths are authoritative (tC4 rule S-2).

### 3.4 `metadata.json`

One function, `tc_burrito::build_metadata(&MetadataSource, &[IngredientEntry])`, produces it.
`tc-burrito` cannot see the fold (`tc-journal` depends on it), so the fold hands it a plain
`MetadataSource` (`FoldState::metadata_source`): every field value by its dotted path, the
present books' scopes, the pins mirrored as relationships and whether the text is NFC; the
caller adds the ingredient table (path, md5, size, role, scope of the bytes about to be on disk).
Its bytes are a pure function of the fold and the bytes of the ingredients, so two devices with
the same changes write identical files (I1, I12).

| Field | Source |
|---|---|
| `format` | `"scripture burrito"` |
| `meta` | `version "1.0.0"`, `category "source"`, `normalization "NFC"` only when every USFM and story ingredient is in NFC (omitted otherwise), `defaultLocale` and `dateCreated` (RFC 3339 with `Z`) from `ProjectMeta` set at creation, `generator {softwareName: "translationCore", softwareVersion: "tc-project-<major>.<minor>"}`: the format version, never the app version |
| `idAuthorities` | `local` always; `dcs` when `relationships` is non-empty; every other authority from `ProjectMeta(Extra("idAuthorities.<name>"))` |
| `identification` | name, abbreviation, description from `ProjectMeta`; `primary.local.<abbreviation> = {revision: "1", timestamp: dateCreated}`; other `primary` authorities and `upstream` from `ProjectMeta(Extra("identification.primary.<authority>"))` and `ProjectMeta(Extra("identification.upstream"))` |
| `languages` | `ProjectMeta(Languages)`; always written, although the 1.0 schema does not require it |
| `confidential`, `copyright` | `ProjectMeta` |
| `localizedNames` | `ProjectMeta(Extra("localizedNames"))`, `{}` when there is none |
| `type.flavorType` | Bible: `scripture/textTranslation` with all five flavor fields, `currentScope` from the present books' scopes. OBS: `gloss/textStories`, `currentScope` = the template table verbatim |
| `relationships` | the tC4 `relationshipsFromPins` mirror of the projected pins |
| other top-level fields from an import (`agencies`, `targetAreas`, `progress`) | `ProjectMeta(Extra(name))`, written back verbatim |
| `ingredients` | every listed file (§3.3), sorted by path: `checksum.md5` (lowercase), `size`, `mimeType`, `role`, `scope` |

Byte form: `tc_burrito`'s canonical writer: keys sorted at every level, two-space indentation,
one trailing LF (the same rule as `format.json`, so equal documents are equal bytes). Before
every write the document passes `tc_burrito::check` in memory: the vendored SB 1.0.0 schema
(`jsonschema` 0.58.6, offline, draft-07, formats asserted), the checks the schema misses
(`languages` present, RFC 3339 timestamps, declared authorities, the path grammar, md5 present
on every entry, `currentScope` coverage, flavor kind, the mimeType table), and the exact-set,
md5 and size check against the bytes about to be on disk.

**`build_metadata` is total.** Values in records come from people and other tools (the Door43
web editor, a text editor, a newer app), so a record value can be valid JSON but invalid in a
burrito: a name with a trailing space, `copyright` as a string. Each value that feeds the
document (every `ProjectMeta` field, the pins mirrored into `relationships`, every book `Scope`)
is checked on its own (`tc_burrito::check_field`, `check_relationship`, `check_scope`): the
value is set in an otherwise valid document, which must pass every check.
An invalid value is never projected: the field takes the newest value among the record's
projected head and its ancestors that passes (the creation value is among them), else the
field's default (omitted when optional), and the fold lists `InvalidForBurrito` in words ("The project
name jdoe set on Door43 is not valid in a Scripture Burrito: it ends with a space. The previous
name is used until it is fixed. [Edit]"). The invalid value stays in History. A command that
would write an invalid value (`SetMeta`, `SetBookScope`) is refused at B1 with the same words.
So no data can make the document fail; a failing document after these steps is a bug report,
is never written, and the previous file stays.

A Bible project with zero present books cannot be a valid burrito (`currentScope` needs one key),
so the app refuses to remove the last present book, and the fold keeps one book present if
concurrent removals would remove all of them (§7.6).

### 3.5 Files the app writes once

`ingredients/tc/project.json`, written at creation and never changed:

```json
{"schemaVersion":1,"projectId":"6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50","kind":"bible","created":"2026-10-15T09:00:00.000Z"}
```

If it is changed outside the app, open restores it from the HEAD tree and says so. If it is
missing in both, the folder is not a project of this format and goes to the importers (§13).
If it is deleted or damaged on Door43, the next merge commit restores it (§9.2 N2).

`ingredients/tc/format.json` is described in §12.

### 3.6 Local state in `.tc/`

None of these files is project data that another device needs. Each is written with
`atomic_write`.

| File | Content | Purpose |
|---|---|---|
| `lock` | `{host, pid, device, at}` | exclusive advisory lock (`std::fs::File::try_lock`) while the project is open. It guards the folder against a process a crash left running and against other programs; translationCore itself runs once per computer (§5.0). A lock another process holds refuses the open (O1), and the content names the holder in the error. It is written through the locked handle (truncate, write, flush), never with `atomic_write`: a new file renamed over it would be a file whose lock nobody holds, and another process could take it |
| `device.json` | `{device, fingerprint, created}` | this folder's Local device (§6.3) |
| `pending/<device>-<hash32>.batch` | header + change lines (§4.2) | saves not yet sealed: the commit point of every save |
| `materialized.json` | `{minor, derivations, paths: {<path>: {state, placed or old/new or held, scope, oid, nfc, blob}}, packs: {<path>: {md5, size, fingerprint, blob}}}`: the format minor and derivation versions (§12.1) the files were projected with; per derived or carried path the placement ledger's state (§6.2): `placed` (the bytes the app placed: md5, size, mtime once taken, and whether they were read back after their timestamp tick was over, `settled`), `writing` (the bytes a write named before its rename, `new`, and the placement it replaces, `old`) or `held` (a book file or story changed outside the app, kept and left as it is until the person answers, §11.1: its md5, the path of its copy in `.tc/external/`, size and mtime); the digest of the heads the projection stands for (`scope`); for a USFM or story file whether its bytes are NFC (`nfc`), so `metadata.json` states its `normalization` without projecting every book again; per derived path the git blob that holds exactly those bytes once a checkpoint stored it (`blob`, §9.4); per pack the md5 that `metadata.json` lists and its blob, which also makes the manifest's pack list part of the expected pack set (§4.4: a listed pack that is gone and that no valid pack's `replaces` names is missing); `kept`: one entry for each kept copy that still has a notice, `{path, copy, md5, size, base, why}`. `base` is `placed:<md5>` (the placement the change was found against), `head` (the heads of HEAD's packs, when `.tc/` was lost) or `none`; a `placed:` base whose heads are not in the heads file counts as `none`. `why` is `changed` (a file changed outside the app) or `lost` (the app's own bytes that show work the journal lost, §6.2 N4). Each copy has its own entry: a second save of the same file adds an entry and never takes over the first one's (§11.1). The entry is written in the same manifest write as the ledger state that follows the copy (§5.10) | the ledger: who wrote what at each path (§5.10, §6.2), written before every rename that matters. Its own mtime, which the filesystem sets when it is written, is a time stamp that settles placements (§6.2) |
| `heads/<escaped path>.bin` | postcard `HeadsFile` behind the magic `TCH\x03`: for each placement the ledger names at the path, and each placement a `kept` entry names as its base, by the md5 of its bytes, every record the bytes were built from with the heads they stand for by the token rule of §2.7 (every head of a settled or merged record; only the provisional candidate and its equal heads for a conflict; the version the projection shows when its value cannot be read now), and, for a book's USFM, a 16-byte digest of how the bytes show the record (MD5 of its piece of the file after line ends and blanks at line ends are normalized, `tc_journal::outside::shown_digest`). `metadata.json` has one too (its project details, no digests). Written before the ledger names the placement (§5.10 M3) | the base of an import of a kept copy (§13.6), and what the ledger asks the fold about before it lets the bytes be written over (§6.2 `holds`); an outside editor saw only the provisional text, so its import never resolves a conflict. The digests let the import skip every record whose decoded digest equals its base digest, so it costs the records that differ (§14); a heads file without them is as good, and costs the book. A file of an earlier format names no placement: the ledger keeps no heads for those bytes |
| `quarantine/…` | damaged pending files, byte copies; change lines too long for any pack (§5.11 K1) | kept until the person dismisses the notice (only while its bytes are still the ones the notice was raised for: see `external/` below) |
| `quarantine.json` | blob ids of damaged or unreadable incoming files already reported | each damaged file is reported once, not at every sync |
| `intake.json` | `{source, started}` | present only while an exchange-file import writes (§10.2 R7); at open it means the import did not finish |
| `external/<utc>-<escaped path>` | byte copies of files changed outside the app, and files moved out of `ingredients/`; a second copy of the same path in the same second is `…-2`, `…-3`: no copy replaces another | kept until the person dismisses the notice or its *Import* finishes (§11.1). Every copy is made by one no-replace helper; a copy whose bytes are already there under an earlier name is found and flushed, not written twice. A notice keeps the SHA-256 and size of its copy as it was when the notice was raised; dismissing it removes the copy only while every byte still matches, checked again before each removal attempt. A copy that cannot be read, or holds other bytes (another program saved over the name), stays with its notice and the dismissal fails; a copy already gone ends the notice |
| `sync.json` | `{remoteUrl, branch, confirmed: {commit, at}}` | last confirmed Door43 state; rebuilds `.git` config after git damage (§6.4 G4) |
| `drafts.json` | `[{field, shown: [token text], text, at}]` | unsaved editor text, written at most every second while text is unsaved, by a writer beside the actor (§5.14) |
| `index.bin` | the fold cache (`tc_journal::index`): for each valid pack and pending file an open read in full, a section keyed by the file's path, size and 64-bit fingerprint, holding the file's header and the prepared changes the fold takes from it (each change's id, where its JSON lies in the file, the fingerprint of those bytes and its summary), so the facts O7's rules read of the file (its header, status and change ids) come from the section as from a read; stamped with the cache version (`INDEX_VERSION`), the format minor, the build's probe stamp (below) and the project | a cold open takes each unchanged file's changes from it instead of verifying and parsing the file again (§6.1 O8, §14). It never decides what exists: a missing or damaged cache, one of another version, minor, stamp or project, or a file it holds no section for, only means that file is read |

**What the fold cache trusts.** A section stands for a file when the file's path, its size and
the 64-bit fingerprint (§2.6) of its whole content, taken from the bytes the open reads at that
open, equal the section's key: no time, name or inode. Any build of the app reads a cache of its
own `INDEX_VERSION`, format minor and probe stamp, so an update that changes none of them keeps
the cache. Against accidental damage that is enough: the size catches every truncation,
extension, insertion and deletion; the fingerprint catches every change within one aligned
8-byte word (each of its steps is one to one) and any other accidental change except with a
chance of about 2⁻⁶⁴ for each changed file (bit flips, a torn or zeroed page, a file another
program rewrote or replaced, a restored older copy). Each section carries a fingerprint of its
own bytes, so a damaged cache loses sections and never gives other changes; and only a file
whose every line verified and parsed has a section, so a damaged, newer or foreign file, or one
with a quarantined line, is read and reported at every open. The cache does not protect against
a file made on purpose to match a section's key (the fingerprint is not a cryptographic hash) or
a cache written on purpose. **The probe stamp** keeps builds that summarize changes differently
apart without anyone having to remember a version raise: the cache is stamped with the
fingerprint of the sections the writing build makes of a fixed probe
(`tc-journal/src/index_probe.batch`: every record kind seeded and not, every verse value class
seeded and not, every value of every field the summary keeps; `the_probe_covers_every_class`),
computed once a process (5.5 ms on the VM of §14), and a build whose stamp differs does not read
it. So a build that changes `FoldInput::prepare_at`, the section encoding or a per-change rule
in a way any probe change shows never takes an older build's summaries; `the_encoding_is_pinned`
also pins the stamp, so the change is seen in review and `INDEX_VERSION` raised. A change of
behaviour on bytes the probe does not hold (a rule made stricter for a rare value) is not seen
by the stamp: it must raise `INDEX_VERSION` or add a probe change that shows it. The remaining
risk is acceptable: a file whose damage collides still gives the changes it held when they were
verified, so nothing is lost or altered in the fold; what is missed is the notice that the file
is damaged, and a value read from the damaged part fails its own fingerprint and is listed as
unreadable (§2.6). Trusting size and modification time instead would spare reading the files but
would miss damage that keeps them.

### 3.7 Application data (per installation, never in a project)

| File | Content |
|---|---|
| `installation.json` | `{secret: 32 random bytes hex, created}`, written once |
| `ledger.json` | per Local device: `{project, folder, lastSeq, lastAt}`, updated after every seal and at close (§6.3) |
| `prefs/<projectId>.json` | pane layout and other per-person UI preferences |
| `resources/dcs/<owner>/<repo>/<sha>/` | an installed Door43 resource at its full commit sha, with its `receipt.json` and `checked.json` and its files under `content/` (`tc-resources`, `ResourceStore`); owner and repository folders in lower case, as Door43 resolves names without regard to case (the receipt keeps the pin's casing); nested, not joined as `<owner>--<repo>`, because both names may hold `--` |
| `resources/aquifer/<owner>/<repo>/<commit>/<lang>-<16 hex of the asset's SHA-256>/` | an installed Aquifer release asset, laid out the same way |
| `resources/.partial/` | installs and downloads in progress; removed when the store opens (on Windows, a rename or removal that another program such as a virus scanner blocks is tried again for about two seconds, and a leftover it still holds open is kept until a later open) |
| OS keychain | the Door43 token, only when the person chose "Stay signed in" |

**Names inside an installed resource.** A resource is installed only if every name in it can be
written, and be told apart from every other, on Linux, macOS and Windows alike. The same check
(`tc_door43::check_names`) runs on a Door43 archive or export, on an Aquifer asset and on every
file the store stages, and it refuses and never repairs, because repairing could merge two files:

- a path that leaves the content folder: absolute, a drive prefix such as `C:`, a backslash or
  NUL, an empty segment, `.` or `..` (or dots and spaces that Windows reads as `..`);
- a segment Windows cannot write as it is named: a C0 control character or one of
  `< > : " | ? *` (`:` would write an NTFS alternate stream), a name of only dots and spaces, a
  name that ends in a dot or space (Windows strips it, so the file lands under another name,
  for a file as for a folder), a device name (`CON PRN AUX NUL COM0-9 LPT0-9 CONIN$ CONOUT$`, any
  case, with or without an extension) and the shape of an 8.3 short name (`PROGRA~1`,
  `IMG~1.JPG`: at most one dot, one to six characters then `~` and digits before it, at most
  three after it), through which Windows can open another file. The 8.3 rule applies on every
  platform, so that one resource is the same bytes everywhere; `notes~1.json` is not refused;
- two names that Windows or macOS would store as one: they differ only in letter case, in
  Unicode normalization, or in a letter that macOS folds (`σ` and `ς`, `ſ` and `s`, `µ` and
  `μ`), a file and a folder of one folded name (`A` and `a/x`), and folders that a path implies
  (`Notes/a` and `notes/b`). `ı` and `i` are not folded: no supported file system merges them.

One such name refuses the whole resource: the Door43 download, the Aquifer asset or the
export installs nothing, and the error names the path. A file is staged with `create_new`, so a
name the volume merges despite these checks (an 8.3 alias that exists only on that volume) stops
the install instead of overwriting the earlier file.

---

## 4. Framing, checksums and packs

### 4.1 The change line

Every stored change is one line:

```
<canonical change JSON> TAB <64 lowercase hex: sha256 of the JSON bytes> LF
```

Canonical JSON never contains a raw TAB or LF, so the split is unambiguous. The hash on the line
is the change id, so each line verifies on its own: a damaged byte in the JSON or in the hash
breaks the equality. The same format is used in pending files and in packs.

### 4.2 Pending batch files

`.tc/pending/<device>-<hash32>.batch`: `device` is the Local device that wrote the file and
`hash32` the first 32 hex digits of the file's hash, computed as a pack's id is (§4.3): SHA-256
of the header line, then the ids of the change lines. A name is never reused by another file: two writers have different devices, and one writer's two files have different
bytes. So even when two installations share a folder (a cloud-synced folder that slipped past
the gate of §5.0), a rename can only replace a file with identical bytes. Nothing reads write
order from the names; the fold is order-independent. Content:

```
{"device":"dk3q…","pending":1,"project":"6f1c…","purpose":"save"}<TAB><sha256>
<change line>
…
```

`purpose` is `save`, `intake` (changes an intake writes, such as the salvage of a received pack) or
`salvage` (changes recovered from a damaged pack, §4.4). The batch of an *Import* or *Bring it
back* of a kept copy adds `"reconciles":{"path":…,"seenMd5":…}`, `seenMd5` the copy's md5, so
that a crash after the batch never imports the copy twice (§6.4 X3); `path` is the file the
copy was taken of, an ingredient path (`RepoPath::parse_source`; `metadata.json` is no longer a
valid `reconciles` path, since nothing in it is imported). A header that read `metadata.json` as a path no ingredient may have
would make the app's own file fail to verify when it is read again, and the seal could not take it.
One file holds whole batches only: a section save that creates a span is one file. `device` is a
Local device. `pending` is the framing version, 1: a file whose verified header declares a larger
one is from a newer format and is kept, not read, never quarantined or removed. Readers ignore
header members they do not know, so a newer minor can add one.

**Every file in `.tc/pending/` is read**, whatever its name: a cloud client's
`…-DESKTOP.batch` or `… (conflicted copy).batch` is read like any other. A file whose header and
lines verify is folded and sealed; a file that does not verify is quarantined and salvaged
(§4.4); a file that cannot be read is retried (§4.4, read errors). No file there is ignored.

### 4.3 Packs

Path: `ingredients/tc/changes/<sealer>/<yyyy-mm>/<pack32>.ndjson`. `sealer` is the Local device
that wrote the pack; only that device ever seals a new pack in that directory (others only write
byte-identical copies of its packs, by intake or restore), so no two devices can produce one
path with different bytes. `yyyy-mm` is the UTC month of `sealed`. `pack32` is the first 32 hex
digits of the pack's id. Content:

```
{"count":N,"project":"6f1c…","replaces":[],"sealed":1760520000000,"sealer":"dk3q…","tcpack":1}<TAB><sha256>
<change line>                                             N lines, sorted by (device, seq, id)
```

**The pack's id** is the SHA-256 of the header line as stored, its line feed included,
followed by the 32 raw bytes of each change line's id, in file order. It covers every byte of
the file, as the SHA-256 of the whole file would: the header line is hashed whole, and each
change line's JSON is fixed by its id, which its hash field states in lowercase hex and P4
verifies. Reading a pack therefore hashes each byte once, in its line's own hash, plus 32 bytes
a line; hashing the whole file for the name as well cost a second pass over every byte (§14).
A file whose header or lines do not verify has no such id; `read_pack` gives it the SHA-256 of
all its bytes, so two different damaged files never share an id.

`sealer` is a Local device. `replaces` lists the names (32 hex digits) of the packs a
consolidation absorbed, sorted, no repeats. A clock before the year 0 or after 9999 seals into
`0000-01` or `9999-12`, so a wrong clock never makes a pack path invalid. Readers ignore header
members they do not know.

A pack may hold changes of any device (ingested Door43 edits, salvaged changes, imports). The
fold does not care which pack holds a change; the same change in two packs is one change.

### 4.4 Validation, quarantine and salvage

Pack validation, in this order (P2 comes last, because the name is computed from the ids that
P4 verifies):

0. **Newer format, other project.** A file whose first line verifies (a CR before its line feed
   allowed, as in salvage) and declares `tcpack` above 1 is from a newer format, whatever else
   is wrong with it: `Notice::NewerPack`, carried, not read, never reported as damaged (§12).
   Every future pack framing therefore keeps a first line that is a verifiable canonical JSON
   object with a `tcpack` member. Otherwise, a file whose first line verifies and names another
   project in `project` is foreign, whatever else is wrong with it: not read, not salvaged, and
   reported, because salvaging its lines would merge two projects.
1. **P1 path.** The path matches `tc/changes/<DeviceId>/<\d{4}-\d{2}>/<32 hex>.ndjson`, the
   device is a Local device and the month is `01` to `12`. If not, the file is damaged: go to
   salvage.
2. **P3 header.** The first line verifies and parses, with `tcpack` 1 and `project` this
   project (step 0 has set aside every other value). `sealer` must equal the directory;
   otherwise the file is damaged: go to salvage.
3. **P4 lines.** Exactly `count` lines follow, each verifies, and the file ends with a line
   feed; otherwise the file is damaged. A line that verifies but fails the per-change rules of
   §4.5 (its JSON may not even parse) is a quarantined change; the rest of the pack is used.
4. **P2 name.** The pack's id (§4.3), from the header line and the ids P4 verified, starts with
   the name. If not, the file is damaged: its lines all verified, so salvage takes every one.
   Because a damaged byte in a line breaks that line in P4 before P2 is reached, "passes P2"
   alone says nothing about the lines: every rule below that needs an undamaged pack asks for
   P2 to P4.

**Salvage.** In a damaged pack, every line that verifies and parses is a genuine change, and
every line that verifies but breaks a per-change rule (§4.5) is a quarantined one: its id is the
hash of its bytes, so whoever salvages it gets exactly what its author wrote. For salvage a line
also verifies with a CR before its line feed, as a tool that rewrites line endings leaves it:
canonical JSON never holds a raw CR, so the JSON is untouched (P3 and P4 stay strict, so such a
file is still damaged). The first line is left out when it is a header: a JSON object with a
`tcpack` or `pending` member, which no change has. The fold uses the salvaged changes and reports
`Notice::QuarantinedPack { salvaged, … }`. Where the damaged file stays in the tree (a Door43
tip, §9.2 N3), salvage is recomputed at every open and gives the same result on every device.
Where it does not (a USB intake, §10.2), the salvaged lines are written to a `salvage` pending
batch so they persist; a line too long for any pack is set aside by the next seal (§5.11 K1). A
salvaged part of a batch waits as an `IncompleteBatch` until the rest arrives.

**The expected pack set.** Which packs exist is project state, not whatever the working tree
happens to hold today: an antivirus program, a cloud client, a copy that skipped long paths or a
person freeing space can remove a pack without anyone deciding anything. The **expected set** is
the non-redundant packs (§4.7) of HEAD's tree, of the tree of the commit confirmed in
`.tc/sync.json`, and of every pack the fold has read since open. At open (§6.1 O7) and before
every tree build (§5.13 C3), each expected pack that is missing from the working tree or fails
P2 to P4 there (at open, a pack whose bytes equal a section of the fold cache passes: they
passed P1 to P4 at that path when the section was made, §6.1 O8) is written back from a blob at
the same path that passes P2 to P4 (HEAD, the confirmed
commit, R, or the blob stored when the pack was first read or written, §9.4), its changes are
folded, and the person
sees `PackRestored` ("A file of changes from March was missing and was restored"). This applies
to every sealer directory, not only this device's: after a folder move this device's own older
packs are in another device's directory, and a teammate's pack on this disk is as precious as
one's own. Packs are immutable, so restoring one is always safe. If an expected pack cannot be
restored or read, no materialization removes anything derived from it and no tree is built
(§5.13 C3): a commit without it would remove it from Door43 by fast-forward.

A pack this session read or wrote is not read again at a checkpoint when its size and
modification time are those it was last seen with: reading 674 MiB at every checkpoint would
break the checkpoint's budget, and a changed pack cannot hide, because its size or time moves.
A pack that is gone is found by the stat; one whose size or time moved is read and checked
(P2 to P4, and its fingerprint); the packs of HEAD's tree that are not in the working tree at all
are found at the open (O7), which lists HEAD's `ingredients/tc/changes` folder once. A damaged
file that is replaced is first copied into `.tc/quarantine/`.

**All copies damaged.** When no valid blob exists for a damaged path (§9.2 N3), every copy is
salvaged, and the union of their verified lines is written to a `salvage` pending batch
(`commit_received`, §5.0) and sealed, so a line that only one copy still holds intact survives.

**Read errors are not damage.** A file that cannot be opened or read (a Windows sharing
violation, a cloud placeholder whose bytes are not downloaded, a permission error) is
`FileUnreadable`, retried at the next checkpoint and at the next open. It is never quarantined,
never salvaged, never removed, and a pack that cannot be read blocks the tree build as above.

**Pending files.** A pending file that does not verify is moved to `.tc/quarantine/`, its valid
lines are salvaged into a new pending file, and the person sees "One save from 14:02 could not be
read completely. N of M changes were recovered." "Saved" is shown only after the file's data and
its rename are durable (§5.0), so this arises only from hardware that lies about flushes. A
complete temporary pending file is never deleted: open renames it into place (§6.1 O2). A pending
file whose header verifies and names another project (a `.tc/` folder copied between projects)
is moved to `.tc/quarantine/` unread and reported, even when the rest of it is damaged: nothing
of it is salvaged. A newer `pending` version is checked first, as in step 0.

### 4.5 Per-change rules

These rules look at one change alone, so the result does not depend on arrival order. A change
that breaks one is a **quarantined change**: kept in its file, not folded, listed.

- **V1 shape.** The bytes are canonical JSON (§2.5) and parse as a `ChangeBody`. In a change
  whose `v` is above this app's minor: a record kind this app does not know parses as
  `Record::Unknown` and is folded as an opaque value under its own key (§12); a known kind with
  fields this app does not know parses as that kind, under that kind's own `RecordKey`, with the
  unknown fields kept in the stored bytes (§2.3, "Fields from a newer minor"), so it joins that
  key's heads like any change; a cause this app does not know, or a known cause with a part this
  app does not know, parses as `Cause::Newer`; any other enum variant this app does not know (a
  new kind of verse value, a new book code) cannot be held, and the change is quarantined as
  `NewerVariant`, listed as made by a newer translationCore. An unknown kind, field, variant or
  cause with `v` at or below this app's minor is malformed.
- **V2 identity strings.** Check ids, reference parts, setting keys, slot names, project detail
  paths and paths are NFC and match their grammar (§2.1, §2.2). Normalizing an identity could
  merge two records, so a non-NFC identity is refused, never normalized (tC4 R-8.5.13).
- **V3 USFM values.** `words` and `layout` contain no `\v` or `\c` marker
  (`\\[vc](?:[ \t\r\n]|$)`); `layout` passes the layout rule of §2.4; `last ≥ verse`;
  `Covered.by < verse`. A `BookHeader` holds no `\c` marker. A `ChapterFront::Present` starts
  with its own chapter's marker (`\c`, spaces or tabs, the chapter number, then whitespace or
  the end) and holds no other `\v` or `\c` marker. So no record can add or move a verse or
  chapter of the projected book.
- **V4 device profiles.** A `DeviceProfile(d)` change is written by device `d`, or by an Importer
  device for a Tc4Actor or Tc3User profile.
- **V5 parents.** Sorted, unique, at most 4,096.
- **V6 grow-only records.** A `BatchRelease` change has no parents. A `Comment` change has no
  parents unless its cause is `Edit` (a revision of the text the comment box showed, §5.5),
  `Restore` (a version from History) or `Resolve`: two changes under one comment id are a
  `Comment` conflict, and its resolution names both (§8.3).
- **V7 limits.** JSON nesting depth ≤ 64; a change line ≤ 4 MiB, counting its tab, hash and
  line feed.
- **V8 carried paths.** A `CarriedFile` path is not a path the app derives or owns (anything
  under `ingredients/tc/`; `metadata.json` is no record path at all, V2), not a folder that holds
  one of those (`ingredients`, `ingredients/tc`, `ingredients/checking`, `ingredients/content`),
  and not a path inside one of those files (`ingredients/TIT.usfm/a.wav`, `metadata.json/x.wav`),
  comparing collision forms (§3.3): one path cannot be both a file and a folder, so such a file
  could never be written, and unlike a case collision that does not depend on other records. The
  derived paths are those of the change's own minor, or of this app's minor for a newer change
  (§12.1, `derivedPaths`), never those of the project's `format.json`, so the verdict on a
  change does not depend on which format documents a device has seen.

Collisions between paths of different records (letter case, §3.3) depend on the set, not on one
change, so they are handled by the projection (§7.7), never by quarantine.

### 4.6 Size limits

A change line is at most 4 MiB. A pack is at most 64 MiB; a seal writes as many packs as it
needs, and a batch may span packs (batch completeness, §7.2, keeps it atomic for readers). A
pending file holds one or more whole batches and has no size limit; a gateway change over a
checked Bible is one pending file of about 150 MB.

### 4.7 Consolidation and redundant packs

Packs are never edited. To bound the number of files, a device merges its own packs:

- **When.** At a seal, for every month directory of its own sealer directory that is before the
  current UTC month and holds two or more of its packs (all of them are merged); and in the
  current month when it holds more than 32 packs smaller than 1 MiB (those are merged, so a
  month's data is rewritten only two or three times in all).
- **What.** Only packs that pass P2 to P4 on this device. The new pack's lines are the union of
  their lines, sorted, and its header `replaces` lists the names of the packs merged and every
  name those packs themselves listed. If the union is over 64 MiB, whole old packs are grouped
  in the order of their first line (device, seq, id) into new packs of at most 64 MiB, and each
  new pack lists the packs it absorbed and the names they listed. A group of one pack is not
  rewritten, and a pack too large to share a new pack stays as it is.
- **Order.** Write the new pack (§5.12), re-read and verify it, then remove the packs it
  replaces.

**Redundancy rule**, applied to any set of pack files (a working tree, a merged commit tree, an
exchange file): a pack F is redundant if another pack P in the same sealer directory passes P2
to P4 and lists F's name in `replaces`. Redundant packs are left out of every built tree and
removed from the working tree once P is durable. When F is present and valid, the reader also
checks that every change id of F is in P; if one is missing (a bug), F is kept and the anomaly is
reported. Because the rule is a function of the file set, every device converges on the same
tree, and a damaged copy of a replaced pack disappears with it.

---

## 5. Write paths

### 5.0 Primitives

**One writer per project.** Each open project has two writing threads in `tc-store`, each the
only code that writes its own files in the project folder. The `ProjectActor` owns the
in-memory `FoldState`, the journal (pending files, packs), the device and its sequence counter:
it commits batches, publishes snapshots and seals. The **writer** owns everything about
derived and carried files: the manifest, the holds and busy paths, the byte copies in
`.tc/external/`, the heads files, the placement ledger, `metadata.json` and, later, the
git objects. Commands, sync results and import results are messages to the actor. The actor
hands the writer jobs (a dirty set, the newest snapshot, the packs it knows) and never waits
for one except at close; the writer never waits for the actor (it sends it a message when the
open's first job fails). So **a save never waits for a derived file or for the long part of a
checkpoint**, however long a book takes or however slowly a scanner lets go of a file (owner
ruling 2026-10-18). A job runs with the snapshot it was sent, or a newer one when the writer
has already run on a newer one: it never writes older bytes over newer. Network, parsing,
validation and fold snapshots run on other threads and hand finished data to the actor. An
exclusive advisory
lock on `.tc/lock` (`std::fs::File::try_lock`, stable since Rust 1.89, so no `fd-lock`) refuses
the open of a project another process holds. translationCore runs as a single instance per
computer, as tC4 4.0 does (its D39: a second launch brings the running window to the front and
exits), so nothing supports a second app, process or window working on a project on one
computer; the lock guards against a process a crash left running and against other programs,
and O2's cleanup of temporary files relies on it. Several people work on one project at the same
time only from separate computers, through sync. The lock is local to one machine. A folder that a
cloud client (OneDrive, Dropbox, iCloud Drive, Google Drive, Syncthing) replicates to another
machine can get a second writer the lock cannot see, so such folders are gated below, and a
second writer that slips past the gate is detected and stopped before it can do harm.

**All file I/O goes through the `Vfs` trait** in `tc-store` (`std` in production, `FaultFs` in
tests, §16.2). Only these helpers write, rename or remove project files:

```rust
/// Replace or create `dir/name` atomically and durably.
pub fn atomic_write(vfs: &dyn Vfs, dir: &Path, name: &str, bytes: &[u8], sync_dir: SyncDir<'_>) -> Result<(), WriteError>;
/// Creates a new target without ever replacing an occupied name.
pub fn atomic_create(vfs: &dyn Vfs, dir: &Path, name: &str, bytes: &[u8], sync_dir: SyncDir<'_>) -> Result<(), WriteError>;
/// `NotWritten`: the target is unchanged. `WrittenNotDurable`: the new bytes are at the target,
/// but a flush after the rename failed, so a power cut may still lose them.
pub enum WriteError { NotWritten(StoreError), WrittenNotDurable(StoreError) }
pub fn atomic_create_dirs(vfs: &dyn Vfs, path: &Path) -> Result<(), StoreError>;
pub fn atomic_remove(vfs: &dyn Vfs, path: &Path, sync_dir: SyncDir<'_>) -> Result<(), StoreError>;
pub enum SyncDir<'a> { Now, Deferred(&'a mut DirSet) }   // Deferred collects directories to fsync once
/// The rename or removal of a project file, retried while another program holds the file.
pub fn rename_held(vfs: &dyn Vfs, from: &Path, to: &Path, retry: &HeldRetry) -> io::Result<()>;
pub fn remove_held(vfs: &dyn Vfs, path: &Path, retry: &HeldRetry) -> io::Result<()>;
pub fn remove_dir_all_held(vfs: &dyn Vfs, path: &Path, retry: &HeldRetry) -> io::Result<()>;
pub fn rename_dir_noreplace_held(vfs: &dyn Vfs, from: &Path, to: &Path, retry: &HeldRetry) -> io::Result<()>;
/// Milliseconds to wait before each further try. `SAVE`: 10 more tries, 2,004 ms in all (a
/// save, a seal, the open, staging); `BACKGROUND`: 3 more tries, 35 ms (a derived file).
pub struct HeldRetry(&'static [u64]);
```

`atomic_write`, in order:

1. Create `dir/.~<name>.<16 random hex>.tmp` with `create_new` (O_CREAT|O_EXCL, plus O_NOFOLLOW
   on Unix). Same directory, so the rename never crosses filesystems.
2. Write all bytes. `File::sync_all()` (Linux `fsync`, macOS `F_FULLFSYNC`, Windows
   `FlushFileBuffers`).
3. `std::fs::rename(tmp, dir/name)` (POSIX `rename`; Windows `MoveFileExW` with
   `MOVEFILE_REPLACE_EXISTING`). **A file another program holds.** On Windows, Defender, the
   Search indexer and OneDrive open a file just written without `FILE_SHARE_DELETE`, and the
   rename fails with error 5, 32 or 33 for a moment. These three errors (`Vfs::is_held`; the
   test filesystem raises the same error) are retried with a bounded back-off: after 2, 4, 8,
   16, 32, 64, 128, 250, 500 and 1,000 ms (`HeldRetry::SAVE`, 2,004 ms in all, so the
   scanner almost always lets go within the retries, and a save that cannot be written still
   fails within about two seconds). A derived file's write retries 3 times within 35 ms
   (`HeldRetry::BACKGROUND`) and, still refused, becomes a busy path (§5.10): one held file
   never stalls the other files of a book. Every rename and removal of a project file goes
   through the `*_held` helpers.
4. Make the rename durable, now or once per batch of renames (`Deferred`). Unix: open `dir`
   read-only and `sync_all()` it. Windows: NTFS commits a rename to its log lazily, so a power
   cut can keep the complete temp file and drop the rename. Open `dir` with write access and
   `std::os::windows::fs::OpenOptionsExt::custom_flags(FILE_FLAG_BACKUP_SEMANTICS)` and
   `sync_all()` it (`FlushFileBuffers` on the directory), then open the renamed file and
   `sync_all()` it. Both calls are safe Rust; the power-cut probe (§16.5 item 5) proves that
   they make the rename durable, and the release is blocked until it does.
5. On an error before the rename: removal of the temp file (retried like the rename), return
   `NotWritten`; the target is unchanged. If the removal fails too (the scanner still holds
   the file), **the temp file stays**: it is a complete save the person was told failed (B3),
   and the next open decides what it is (§6.1 O2). On an error at or after step 4: return
   `WrittenNotDurable`; the new bytes are in place and the caller must treat them as written.

`atomic_create_dirs` creates each missing directory and fsyncs its parent. `atomic_remove`
unlinks (retried like a rename) and fsyncs the parent. A removal that still fails is
tolerated where the file only waits to go: a pending file or replaced pack that a seal or a
consolidation cannot remove stays and is removed by the next open (O7), and the open never
fails for a temp file or a leftover it cannot remove; a staging folder that cannot be renamed
into place refuses the create with its reason.

**A directory found existing is not trusted.** Its creator may have stopped between the `mkdir`
and the flush of its parent: a killed process leaves the folder in the page cache, and a seal
that then wrote a pack into it, removed the pending files the pack holds and was followed by a
power cut would lose the folder, the pack and the saves with it (the crash matrix found this:
a seal into a month folder that did not exist yet; after a consolidation the folder held the
whole journal). On a project's filesystem (`Vfs::secure_dir_chain`) the first time a session
creates or finds a directory below the project folder, `atomic_create_dirs` flushes the entry
of every directory on the way to it that the session has not flushed yet, above the ones it has
just made too (a killed seal can leave a device's folder unflushed, L121); the session
remembers them, so each directory costs its flushes once, and the open does it for `.tc` and
`.tc/pending`, so the first save pays nothing.

**Filesystems.** A project on FAT32 or exFAT opens read-only with "This drive cannot keep the
project safe if the power fails. Copy the project to this computer?": those filesystems do not
make rename atomic. The type is read from `sysinfo::Disks`. Exchange files may live on any
filesystem (§10).

**Cloud-synced folders** open read-only the same way, with "This folder is copied to other
computers by OneDrive. Two computers saving into one project folder can lose work. Copy the
project to a folder that is not synced?" (the copy keeps every pending save: it copies the
pending files before the packs, then lists both again and copies what is new until a round
finds nothing new, so a seal or consolidation the other writer runs meanwhile cannot take a
change out of both places the copy reads, since a change leaves a file only after it is in a
newer one). A folder is
cloud-synced when it lies under a known sync root (the `OneDrive`, `OneDriveConsumer` and
`OneDriveCommercial` environment variables; the Dropbox roots in its `info.json`;
`~/Library/Mobile Documents` and `~/Library/CloudStorage/*` on macOS; a Google Drive root), when
an ancestor holds a sync marker (`.dropbox`, `.stfolder`), or when a file in it carries a cloud
placeholder attribute (Windows `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` or `…_OFFLINE`, read
through `std::os::windows::fs::MetadataExt::file_attributes`). Door43 is the supported way to
work on two computers.

**A second writer** that the gate misses is detected from the data. While the project is open,
a pending file or pack that appears (seen by the watcher on `.tc/pending/` and
`ingredients/tc/changes/`, or by a seal's directory listing) and was written by a Local device
other than this folder's means another installation is saving into this folder now. At open,
`.tc/lock` content that names another host (written by that host's open and cleared at its
close) means the same unless the person confirms that the other computer is closed. Either way
the project switches to read-only with `SecondWriter` and the copy offer above. Every change
already written by either installation is kept: pending names never collide (§4.2), and the fold
takes both. The watcher (§11.1) also watches `.tc/pending/` and `ingredients/tc/changes/` and
tells the actor to list them; a checkpoint lists the journal folders before it seals as well,
and a file this session does not know is folded and turns the project read-only. A file this
session made redundant and could not remove because another program held it (a pending file K4
sealed, a pack Q4 replaced, a file O7 found redundant) is one it knows: it stays for the next
open's O7, and is no second writer's.

**`commit_batch`**, used by every write path below:

```
B1  Check every change against the fold and the command's rules (§5.1–§5.9) and §4.5.
    Any refusal → nothing is written; the UI keeps the text and shows the reason.
B2  Assign seq (the in-memory counter, §6.3), at = now, the batch ref. A batch never holds two
    changes of one record. Serialize canonically, hash.
B3  atomic_write(.tc/pending, "<device>-<hash32>.batch", header + lines, SyncDir::Now)
    ← the durability point of every save. "Saved" is never shown before this returns.
B4  Apply the changes to the in-memory fold (§7.8).
B5  Mark the affected books and files dirty (a verse of Psalms marks Psalms, and the books of
    a book's change are named by the fold's delta; nothing else is listed or looked at, so the
    mark costs the change, not the project), and re-arm the materialize timer (3 s after the
    last batch). The writer expands the marks to files from its snapshot, off the actor (§5.10).
B6  Reply { tokens, new_conflicts }.
```

If B3 returns `NotWritten` (disk full, permission, a file another program holds for longer
than the retries), the counter does not advance, the editor keeps its buffer and shows "Not
saved: <reason>. Retry". A retry builds the change again from the same token, so a retry after
a teammate's sync becomes a conflict, never a revert (I4). The failed attempt's temp file may
stay (§5.0 step 5), and the retry reuses the attempt's seq: the next open therefore never
rolls a temp file forward into a seq a later save already used (§6.1 O2). Advancing the counter
instead would not help: the abandoned temp would then roll forward with its own seq and still
conflict with the person's retried text. If B3
returns `WrittenNotDurable`, the file is in place, so the save counts as written: B4 to B6 run,
the counter advances, and the person sees `SavedWithDiskError` ("Saved, but the disk reported an
error: <reason>. Your work may not survive a power cut. Check the disk."). The app never reports
"Not saved" for a file that exists, so a retry can never reuse its seq.

**Closing.** A close runs a checkpoint (§5.13; the actor serves the writer's messages until the
checkpoint ends, so neither waits for the other), writes the fold cache,
stops the draft writer and waits for it, stops a repack that runs (§9.8), and only then clears `.tc/lock`'s content and releases
the lock: nothing is written into the folder after the lock is gone, so the next open (after a
restart, or by a program that takes the lock as this session ends) never races a late
`drafts.json`. The open's background work (O8's cache write
and O14) ends before the actor closes, while the actor can still answer it.

**`commit_received`** writes changes whose bytes were fixed elsewhere: deterministic ingests of
Door43 and exchange-file edits (§11.2, §11.3) and salvaged lines of another device's damaged pack
(§4.4, §10.2 R4). It runs B1 with the rules of §4.5 only (no command rules), skips B2 (no seq,
`at` or batch is assigned; the raw lines are written exactly as received, never re-serialized,
§2.5), then B3 with purpose `intake` or `salvage`, B4 and B5. A salvaged part of a batch stays an
`IncompleteBatch` (§7.4) under its original batch ref.

**The handle** (`tc-store`'s `ProjectHandle`, APP-DESIGN R4, R5, R14, R16). The application
layer holds an open project as a handle:

- `snapshot()` is the fold the actor published after its last batch, with its sequence
  number: an `Arc` clone, never waiting behind the actor (R5), since the actor publishes a
  clone of its fold (a few hundred pointer copies, §2.6) after each batch. Tokens are minted
  from the snapshot the screen read (`Snapshot::token`, `review_token`) and parsed back by kind
  (`parse_edit`, `parse_review`); the actor refuses a review token in a save and an edit token
  in a resolution (§2.7, R19).
- `submit(command)` returns at once with a value the caller waits on or awaits; it resolves
  after B6 with `StoreReply { tokens, new_conflicts, stored, seq, changes, skipped }`, where
  `stored` is `Durable`, `WrittenNotDurable(reason)` or `Unchanged`, and `seq` is the first
  published snapshot that holds the batch (R16). A refused command resolves with its reason;
  every command of a read-only project resolves with `ReadOnly(reason)`. A `Draft` goes to the
  draft writer, not the queue (§5.14).
- `subscribe()` gives typed events: `Committed { seq, delta, changes }` for every batch,
  `Notice` (typed: `SecondWriter`, `SavedWithDiskError`, `MaterializeBlocked`, `OutsideEditHeld`,
  `GeneratedFileRewritten`, `LostWorkKept`, …), `Mode` when the project turns read-only, `Materialized { paths }`, and
  `Progress { work, progress }` while a long materialization runs (§5.10).
- `read_book` reads a book's text from one snapshot off the caller's thread: a pending read the
  screen waits for, never an empty book (`docs/TC4-DELTA-2026-10-16.md` §3.10).
  `Snapshot::training_input` gives the aligner's training input, each verse's text and
  alignment from that one snapshot (§3.10 item 3).
- `leave`, `checkpoint`, `import_kept`, `not_now`, `focus`, `dismiss_notice`,
  `offered_drafts`, `discard_draft`, `wait_idle`, `close`, and `counters()`. `focus()` is the
  window getting focus (§11.1): it resolves with a `FocusReport` (files read, books kept and
  asked about, files moved out) once the writer has looked. `import_kept(copy)` is *Import* of a
  kept copy, and *Bring it back* (§11.1, §13.6): it resolves with a `BookImported` report when
  the batch is durable. `not_now(path)` is *Not now*. `Command::ResolveOutsideEdit { batch,
  keep, conflicts }` is the import row's *Accept all from the file* and *Keep translationCore's
  text* (§8.1). There is no *Undo* after an import: History keeps every version
  (`rulings/2026-10-22-H22.md` part 4). `StoreConfig` carries
  `watch_debounce` (one second in the app) and the `Vfs` a `watch` that the real filesystem
  answers with `notify` and the test filesystem with the writes of `put`, `delete` and
  `set_mtime`. `leave` resolves when the writer has finished the job it
  sent it; `checkpoint` resolves with a `CheckpointReport` (the commit, the packs sealed, why
  there is no commit) when the actor has moved the branch or the writer has said why not (the
  seal's error comes first, then the files'); the actor serves commands meanwhile. `not_now` and `dismiss_notice` go to the writer. `wait_idle` waits
  for the actor's queue, then the writer's (not for a retry it has scheduled), then the
  background jobs and the drafts. The counters are: files opened, created, read and written, fsyncs,
  renames, listings, derived files read (by the open's classification and by compare-and-swaps,
  `derived_read`, and apart, by the storage check O14, `check_read`), projected and written,
  heads files read, journal files read and how many of them came from the fold cache, and the
  time commands waited in the queue (R14). Tests hold the store at named points (before a book read, a draft write,
  the durability point of a save, a checkpoint, a materialization, a staging removal and
  rename; R8).

### 5.1 Save one verse (Translate, verse by verse)

1. The editor shows a verse with its `ShownToken`. On blur, or 2 s after the last keystroke, it
   sends `SaveVerse { shown, words }`. Its unsaved text is also in the browser-side draft buffer
   (`ARCHITECTURE.md`) and in `.tc/drafts.json` (§5.14).
2. The actor checks the token's MAC and takes the value the token stands for: the displayed
   candidate for a conflict, the merged value for a merged record, the head's value otherwise.
   A merged record whose heads moved since the screen showed it (a sync landed) stands for
   the merge of the token's heads (`FoldState::merged_at`), so the save is stored beside the
   newer text, never refused. The value must be `Text`; structure changes go through
   `SaveSection`.
3. New value = that value with `words` replaced by the NFC form of the sent text. Rule V3 is
   checked.
4. If the new value equals the displayed value, nothing is written. A no-op save after a sync
   must not replace a teammate's newer version.
5. One `Verse` change, `parents = shown.heads`, `cause: Edit`. In the same batch, the verse's
   alignment carried over to the new text (APP-DESIGN R25): when the verse's heads are still
   the token's (a save from a screen a sync overtook knows only the old text, so it leaves the
   alignment as it is and its hash marks it for review), the alignment keyed by the
   run's anchor (§7.7) is not in conflict, covers this run's span, is not marked invalid, has a
   card, and its text hash no longer matches, `tc_align::reflow_edit`, from the old text to the
   new, keeps on each card the target words that follow into the new text, at their new
   occurrences, sends new words to the bank and drops
   words that are gone; the change has `parents` = the alignment's heads in the actor's fold and
   `cause: Edit`. tC4 reflows when the aligner next opens the verse; writing the reflow with the
   verse keeps the two in one batch, so no crash, sync or reader ever sees a verse beside an
   alignment of words it no longer has. A teammate's alignment edit that arrives later is a
   second head of the alignment, a visible conflict, never a lost edit. An alignment in
   conflict, made for another span or marked invalid is not written and shows as needing
   review (§7.7). `commit_batch`.
6. If a head that the token does not list was added after the verse was displayed (a teammate's
   change arrived by sync), the new change and that head are both heads: a conflict. The reply
   says "Juan changed this verse while you were editing; both versions are kept" and the verse
   gets its conflict marker at once.
7. The editor's new token lists only the new change. Its next save continues its own version.

Editing a verse that is already in conflict works the same way: the token lists only the
displayed candidate, so the edit continues that version and the conflict stays open (§8.2).

`SaveLayout { shown, layout }` changes only `layout` (a paragraph or poetry break after the
verse). `layout` must pass the layout-only rule of §2.4.

### 5.2 Save a section; verse spans, renumbering, chapters

The translator types a section straight through and places verse numbers. Stacking 4 and 5
makes the span `4-5`; dragging a number past text breaks one (tC4 J2).

1. The editor sends `SaveSection { book, shown, layout }` with one token for **every** verse and
   chapter front it displayed in the section, and a new value for each one whose value changed
   (`None` for the others):
   - verse 4 → `Text { words: <text of 4 and 5>, layout, last: 5, marker }`;
   - verse 5 → `Covered { by: 4 }`;
   - any other verse whose words or layout changed → `Text`;
   - verse 6, displayed and unchanged → `None`.
2. The actor builds the section as the author saw it (every token's displayed value) with the
   new values applied and checks that this view is coherent: every `Text` at `v` with `last = L`
   has `Covered { by: v }` on `v+1..=L`; every `Covered { by: a }` has a `Text` at `a` whose `last`
   reaches it; no span crosses a chapter. Only an incoherent view (an editor bug) is refused.
   The check never reads current values the author did not see: if a teammate's join of 5–6
   arrived after the screen was drawn, the save goes through and the overlap is a visible
   `Passage` conflict (§7.3), never a refusal the author cannot clear.
3. One batch, one change per changed record, each with `parents` = its own token's heads. A
   batch that writes two or more verses is a **passage batch** (§7.3).
4. Each verse whose words or layout changed and whose span did not gets its alignment
   reflowed in the same batch, as in §5.1 step 5. Nothing is written on the alignment of a verse
   whose span changed, nor on any decision: the alignment of 4 was made for a span ending at 4,
   its text hash no longer matches, so it shows "needs review" and is kept (tC4 J2 end state).
5. While the section editor holds unsaved text, its whole buffer is kept as one draft
   (`DraftField::Section`, §5.14), so a refused save or a reload never loses typed text.

Breaking a span writes `Text { last: 4 }` on 4 and `Text` on 5. A **renumber** writes, in one
batch, `Text` on each destination verse with `from` = the verse number its text had before (or
the `from` it already carried, so the anchor travels with the text), and `Absent { moved_to }` on
each source verse that no text moves into. Alignments and comments are keyed by anchor, so they
follow the text with no change written on them (§7.7). Adding a chapter writes
`ChapterFront::Present` and the chapter's verses; removing one writes `ChapterFront::Absent` and
`Absent` on its verses. In a book that has a `BookLayout` record, a save that adds a verse or
chapter the layout does not list also writes the new `BookLayout` value (the old order with the
new entries after their numeric predecessors), with `parents` = the `layout` token's heads.
There is no structural action and no chain: a section save touches only the records it changed.

### 5.3 Save an alignment

1. The aligner holds two tokens: the alignment record's (`shown`) and the verse record's it
   displayed (`verse_shown`). On leaving the verse, or 2 s after the last link change, it sends
   `SaveAlignment { shown, verse_shown, value }`.
2. The actor computes from the **displayed** verse value, not the current one:
   `span_end` = the anchor's verse plus the run's length (`last` minus the run's first verse),
   the run's end in the anchor's numbering, so a renumbered verse's alignment covers the verses
   it covered where it was made (§7.7); `target_verse_md5 = md5(plain_text_v1(words + layout))`. If the verse
   changed meanwhile, the saved alignment is stale at once and is shown so; it is never
   re-stamped silently.
3. `source` = the current original-language pin of the book's testament (repoPath + sha),
   `source_version` = its tC4 label, `marked_invalid = false`, `done` as the aligner computed it
   (D73), `extra` copied from the displayed value. The word bank and every group's target words
   are put in the order they stand in the displayed verse (`AlignmentValue::put_in_verse_order`
   over the verse's words and occurrences; a word the verse does not hold goes last), so every
   value the rebuild saves is in verse order and two saves never differ by order alone
   (owner rulings Q15 and Q19, `docs/TC4-DELTA-2026-10-16.md` §5). The aligner shows the bank
   and each card in that order from the first view of a verse, after every edit and after
   Clear (tC4 3d43f7c, 91131d9, 26aded2); opening a verse that only reorders writes nothing,
   and the next edit or *Mark valid* saves the order. An imported value keeps the order it came
   in until someone edits it: an alignment is compared as one whole value (§7.3), so
   reordering it on import would turn equal tC4 values into a conflict.
4. One `Alignment` change on the record keyed by the anchor of the run's first verse (§7.7),
   `parents = shown.heads`. `commit_batch`.

An alignment is one whole value. Two people's alignments of one verse are never merged link by
link: a mixed alignment is one nobody made. A suggestion is written only when the person accepts
it (tC4 J5).

### 5.4 Record a check decision, check comment or bookmark

1. Clicking Valid, Invalid or To do, selecting words, writing a check comment or toggling the
   bookmark changes the item's record in the checking screen. On leaving the item or 1 s after
   the last change it sends `SaveDecision { shown, verse_shown, value }`.
2. Starting from the displayed value (or, for a first decision, from the derived check item:
   `context`, `category`, and `resource` = the tN or TWL pin the item list came from), the actor
   applies the edit with `DecisionEdit::apply` (`tc-journal`), the one implementation of this
   step. A **judgement** (selections, nothing to select, or the status Valid or Invalid) is
   made against what is in front of the person, so it sets
   `judgement.verse_text_md5 = md5(plain_text_v1(text))`, where `text` is the target's text by
   the no-text rule of §7.7 (the empty text when it has none), clears `imported_invalid`, and
   sets `resource` to the edit's `resource`: the pin of the check list the person judged in, the
   (tool, book)'s current resolution as the checking screen resolved it (§7.7; the screen and
   the fold agree once the coverage backfill of §5.6 has recorded the pins' books). So a
   decision re-judged after a resource upgrade is valid again in the file as on the screen.
   **To do**, and clearing the status, change only the status, as tC4's To do button does
   (`markTodo`, `src/views/Check.jsx` at d310bf3): it is not a judgement, so it stamps nothing,
   and an invalidated check put back to do stays invalidated, its stale words never taken as a
   judgement of the new text. A bookmark or a check comment changes only itself.
3. One `Check` change with `Decision`, `parents = shown.heads`. `commit_batch`.

A bookmark and a status set by two people merge without a conflict (different merge units,
§7.3). Two different statuses are a conflict.

### 5.5 Add a user comment

`AddComment { target, text }` mints a random `CommentId` and writes one `Comment` change with no
parents. Comments are append-only: two comments on one verse are two comments, and none can
conflict. A comment on a verse targets the verse's anchor, and a comment on a check targets the
check's key; the projection follows later structure changes, so a comment on 1:5 still shows
when 5 becomes part of `4-5`, when 1:5 is renumbered, or when a carry-over moves the check
(§7.7). Because comments cannot conflict, this following is what keeps a comment made
concurrently with a structure change on a screen. (Retracting a comment, if it is added later,
is a new record kind that hides a comment from the default view and keeps it in History.)

`ReviseComment { shown, text }` (APP-DESIGN R24) writes a new version of the comment its edit
token names, with the token's heads as parents and cause `Edit`: same id, same target,
the new text NFC. The earlier text stays in the comment's History, as every version of every
record does. A revision on one computer and another revision of the same comment on
another are two heads of one comment: a `Comment` conflict, resolved as any other (§8.3), so
neither text is lost. An empty text is refused (a retraction is the record kind above).

### 5.6 Settings, project details and pins

- `SetSetting { shown, value }`: one `Setting` change for one top-level key; nested fields merge
  per leaf (§7.3).
- `SetMeta { shown, value }`: one `ProjectMeta` change.
- Choosing or re-pinning resources outside an upgrade: one `Pins` change per group, with the new
  slot values, `parents = shown.heads`. A new pin always carries its `books` coverage, computed
  from the installed resource when it is installed.
- `resources.json` members no pin group holds (`ResourcesExtra`, §2.2) are written only by an
  import, never by a screen; they project at their
  level, after the modelled members, in name order.
- **Coverage backfill** (tC4 D41, `backfillCoverage` in `src/data/coverageBackfill.ts:56` at
  d310bf3): once, at open, when the group is settled, for each pin of a language set's Bible
  slots and each `extraScripture` pane whose exact commit is installed, a `Pins` change with
  `cause: System(PinCoverageBackfill)` records what the installed copy holds, `parents` = the
  group's heads: `books` for a pin that records none (`["BIBLE"]` for a resource not divided
  into books), and the missing codes appended to a recorded list that the sha-exact install
  shows to be incomplete (a record captured from a partial install). A record is never shrunk
  or replaced, a whole-collection record is never touched, and a code that is not a book
  (`TRANSLATE`) is never written. Widening only adds facts about the same commit. Every
  device with the same resource computes the same value, so two backfills merge.
- A slot value that the resource store could not write back byte for byte (not in tC4's §5.3
  form: a pin with a tag and no commit, a `books` code in lower case or an empty list, a
  repository path with a scheme) is kept as it was read (`LanguageSet::unreadable`,
  `Testaments::unreadable`, `ProjectPins::unreadable_panes` in `tc-resources`) and written back
  unchanged with any edit of its group, never refused with the whole group; the ladder reads it
  by the journal's rules (§7.7). `gatewayLanguage` is one more slot of a set (tC4
  `PIN_SLOT_RE`, `journal/grammar.mjs`): a set without one, or with one that is not
  `{languageId, owner}` with an owner Door43 accepts, is kept the same way (`SetGateway`), and
  its pins resolve as before. A member of `resources.json` that the groups hold
  (`languageSets`, `primary`, `fallback`, `resources`, `originalLanguage`, `lexicon`,
  `extraScripture`) with another JSON type than its own refuses the file, so the import report
  lists it rather than read it without the member; `null` there is absent, as tC4 reads it. The cost of a change of gateway language (§5.8) is counted by
  `tc-check`'s `change` module over the journal's ladder; `tc-resources` only makes the pin
  edits.

### 5.7 Add, remove and restore a book

- **Add** (a book never present): seed from the project's literal source as tC4
  `seedBookFromSource` does. One batch, `cause: Seed`: `BookPresence::Present { scope, how: Added }`,
  `BookHeader`, one `ChapterFront` per chapter, one `Verse` per verse (`Text { words: "___" }`
  with the source's paragraph markers in `layout`, or `Covered` where the source has a span).
  About 2,600 changes for Psalms; one pending file.
- **Remove**: `RemoveBook { book, presence_shown }`. Refused if it is the last present book of a
  Bible project. One `BookPresence::Removed { seen }` change, `parents = presence_shown.heads`,
  where `seen` lists the short ids of every head of every record of the book that this device
  holds now: `BookHeader`, `ChapterFront`, `Verse`, `BookLayout`, `Alignment`, `Check` with a
  target in the book, and `Comment` with a target in the book. Every record stays. The book's
  derived files are removed at the next materialization (with the compare-and-swap of §5.10).
- **Restore** a removed book: the dialog asks "Restore the text (N verses drafted, M checks) or
  start with empty verses?". *Restore* writes `Present { scope, how: RestoredText }` (the scope
  of the last `Present` value) with `parents = presence_shown.heads`; every record of the book
  projects again. *Start empty* writes `Present { scope, how: StartedEmpty }` plus, per verse, a
  `Text { words: "___" }` change with `cause: StartOver` and `parents` = that verse's heads in
  the restoring device's view, so the old text stays in History. These changes are a person's
  choice, not seed stubs: the stub rules never absorb them, so a concurrent edit of a verse is a
  visible conflict, and a concurrent *Restore the text* is a `BookPresence` conflict because
  the two `Present` values differ (§7.6).

### 5.8 Resource upgrade (J12) and gateway change (J13)

1. **Outside the project.** Download every resource of the release into the machine store,
   verify each export's commit sha (tC4 `verifyExportRevision`, D23b, D71), write each to a
   temp directory under `resources/.partial/`, fsync, rename into
   `resources/dcs/<owner>/<repo>/<sha>/` (§3.7), fsync the parent. Any
   failure leaves the project untouched (J12: a failed download changes nothing).
2. **Plan, off the actor**, from an O(1) snapshot `S` of the fold:
   - the `Pins` changes: new group values, `parents` = the groups' heads in `S`;
   - for every decision in the affected (tool, book) units of books that project as present
     (a removed book is not touched, so an upgrade never brings it back), tC4's re-attach
     (`mergeAndReattach`: exact identity key, then cross-language quote match, D17):
     - **same key**: a `Check` change that updates only `resource` and the context fields the
       new resource changed (the `basis` merge unit), `cause: CarryOver`, `parents` = heads in `S`;
     - **new key**: the old key gets `MovedTo { to: new }` with `parents` = its heads in `S`;
       the new key gets the whole `DecisionRecord` (judgement, comment, bookmark,
       `imported_invalid`) with `parents = []`. If the new key already has a decision, the two
       are a visible conflict, never a silent replacement;
     - **not placeable**: no change. Its `resource` no longer matches the resolution, so the
       fold shows it invalidated and kept (§7.7). Pinning the old resource again makes it valid.
   - alignments: no change. An original-language upgrade changes the pin; every alignment whose
     `source.sha` differs computes as invalid and is kept.
   - The dialog says "N checks carried over, M to check again, K alignments to review".
3. **Confirm.** The actor checks that every record the plan touches still has the heads it had
   in `S`. If any moved (a sync landed, a teammate's check arrived), it rebuilds the plan and
   shows the dialog again. Otherwise the whole plan is one batch, `commit_batch`.

Fsync order: resource directories, then the pending file. The project never points at a
resource that is not fully installed.

### 5.9 Resolve a conflict; restore a version

`Resolve { conflict, shown, choice }` carries one token per record involved, each listing every
head as the resolution screen displayed it.

1. If any record's current heads differ from its token, the actor refuses: "A newer change to
   this verse just arrived. Look again." It never guesses.
2. It writes one batch, `cause: Resolve`, one change per record, `parents` = all heads of that
   record. The value depends on the choice (§8.3).
3. A bulk choice ("Keep all versions from this computer in Titus") is one batch with one change
   per record and is confirmed with its count first. Its side is a set of devices, never a
   username: the person picks computers (this one, or each computer listed by name). A conflict
   in which two or more candidates come from devices in the chosen set, or in which the set holds
   none of the candidates, is left out of the bulk choice and stays listed for a choice of its
   own. So one shared Door43 account on two computers never lets one person discard the other's
   versions unseen. There is no "Keep newest" bulk action: it would hand the decision back to the
   clock.

`Restore { shown, from }` writes the value of `from` with `parents = shown.heads` and
`cause: Restore { from }`. For a carried file, *Restore this version* is offered only when the
version's blob is in the local object database; otherwise History says "The file of this
version is not on this computer" (§11.4), and the command is refused with
`Refusal::FileNotOnThisComputer`.

`ReleaseBatch { batch }` ("Apply what was recovered", §7.4) writes one `BatchRelease` change
listing the ids of the batch's changes this device holds. `Dismiss { shown }` writes the
displayed alignment or decision with `dismissed: true`, `parents = shown.heads`.

### 5.10 Materialize derived and carried files

Runs on the writer thread (§5.0) 3 s after the last batch, on leaving a book or mode, after a
checkpoint's seal, after an intake, at close, and as the open's first job (O10). It writes only
dirty paths, from the snapshot the actor sent with them.

**Write groups.** The actor marks books and files (B5); the writer turns the marks into groups,
one for each book (M0 to M7 for that book's files and no other book's) and one for the files of
no book (settings, resources, versification, conflicts, stories, carried files), each naming its
writes in the placement ledger before its renames (§6.2) and each ending in `metadata.json`, so the folder is a valid burrito after every
group (I7), and a process stopped between two groups leaves the burrito valid and the rest to
the next open. A save of a verse marks its book, and one book's materialization lists, projects,
reads and writes that book's files only: it never lists the project or scans the manifest. The
whole listing is asked for in one place: a job marked `everything` (a rebuilt fold). A book
added, removed, restored or rescoped, a pins change, a resource upgrade and a sync's intake
mark many books, show determinate progress from 300 ms on (`Progress { work: Materialize }`,
groups done of groups in all; PERFORMANCE.md rule 4) and go on while saves continue. One
book's materialization shows nothing. Each rendered buffer is hashed once: its md5 stands for it
in the comparison, the ledger, the heads file and the list of `metadata.json`; a file read for the
compare-and-swap is hashed once. A path whose new md5 equals its entry's, that is clean by
stat, is not read and not written (only its scope digest may change). The writer's flushes and
renames wait, for at most 50 ms each, while a save is being written (the actor raises a gate
around B3), so a save is durable within 25 ms p95 whatever the writer is doing.

```
M0  D = dirty derived and carried paths, with their new bytes computed in memory (carried bytes
    come from the git object database, §11.4; a carried head whose blob is missing keeps the
    file on disk as it is and reports CarriedBytesMissing, and one whose blob is there but
    cannot be read keeps it too and waits, R19); X = derived or carried paths to
    remove (a removed book, a carried file a settled `Removed` record names whose manifest entry
    names its blob, §5.16). Nothing in X is removed while an expected pack is missing or
    unreadable (§4.4). A held path (§6.2) is in neither: it is left as it is until the person
    chooses. A held path whose file is gone ends its hold first, and joins D (a deletion is
    never an edit, §6.2).
M1  Compare-and-swap check. For each p in D ∪ X: read it (d = md5(disk), or missing). The bytes
    are always read: size and mtime never stand for them here, because an open may have taken p
    as converged by its size and mtime alone (§6.2), and a program can change a file and put its
    old mtime back; the one exception is a path whose placed bytes are the new ones and that is
    clean by stat (nothing to write). The ledger decides p by the table of §6.2 with the
    projection's md5 and whether every value it read could be read: Write keeps p in D, Remove
    keeps it in X, Refresh drops it (converged), Wait drops it and leaves it dirty, and the
    keeping actions (bytes that are not the app's own, §6.2 N4, N9, N10) copy the bytes first:
    *Keep and hold* drops a book file or story until the person answers; *Keep and write* keeps
    any other derived file in D. The actions are in the table below.
M2  Build metadata.json for the planned result (total, §3.4) and run tc_burrito::check on it in
    memory. A failure is a bug report; nothing is written. Every file is listed as its bytes
    are on disk (I7): a derived file by the manifest or the planned bytes, and the fixed files
    (`project.json`, `format.json`), the carried files and the held files by the manifest while
    they are clean by stat and otherwise by a read; bytes there that the app did not write are
    kept with a copy (§6.2) and listed as they are, so the list never states the
    md5 and size of bytes that are not in the folder. A path in `Writing{old, new}` whose
    file cannot be read may hold either, so it keeps the entry `metadata.json` has for it now
    (no guess from its size: two versions can have the same size) until a read decides it;
    with no such entry, or no `metadata.json` it can read, the path is not listed. A carried file is listed until the job of
    the carried files removes it from the folder, also once the fold no longer places it (a
    removed file, a version moved apart), and one whose new version is being recorded (§11.4) is listed
    with the md5 and size it was stored with: the writer keeps no carried bytes in memory.
M3  The ledger names the writes: for each p in D (and `metadata.json`), the heads of the new
    bytes go into p's heads file, keyed by their md5, with the heads of the placement the state
    names (directory flushed once); then each state becomes `Writing{old, new}` and
    atomic_write(.tc, "materialized.json", manifest). A heads file that cannot be written (a
    program holds it, the disk is full) drops p from D: no rename without its heads, p stays
    dirty and is tried again; for `metadata.json` the group stops.
M4  For each p in D: write a temp, fsync, then **look at p once more** and rename (directory
    fsync deferred). For each p in X: unlink (directory fsync deferred), state `Unknown`, its
    heads file removed. The last look (I8) reads p and compares its bytes with the bytes M1 read
    (or with "missing"): bytes other than those or the new ones were written in the window
    between M1 and the rename: the temp is removed, p is
    not renamed over (`WriteFailed`), and the ledger decides those bytes as M1 does. A file that
    is gone by then is written (a deletion is never an edit, §6.2). The look repeats before every
    rename or removal attempt, including after a sharing wait (§5.0 step 3): another program
    can save during that wait, and size and mtime alone do not establish unchanged bytes.
    Once the rename of p has gone through, the
    session's state of p is `Placed{new}` (`Renamed`), whatever the group does next; a rename
    refused by a program that holds the file is `WriteFailed`, and the state is `Placed{old}`.
M5  fsync every touched directory once.
M6  Rebuild metadata.json from the files as they now are (a path skipped in M4 keeps its old
    bytes), check it again; bytes other than the ones M3 named are named first (their heads,
    `WriteBegun`, the manifest). Look at `metadata.json` once more: when it changed since M1
    read it and holds bytes other than the ones M1 read or the new ones, a program wrote it
    meanwhile: they are copied to `.tc/external/` with a notice (*Keep and write*), and the
    file is written in this same group. Then atomic_write(<root>, "metadata.json", …,
    SyncDir::Now), and `Renamed`.
M7  atomic_write(.tc, "materialized.json", manifest). Each path M4 wrote is `Placed` with the
    size and mtime of the file renamed into place, the scope digest it was projected from, and
    unsettled; every other placement keeps its own settled mark, so an unsettled placement is
    never written again as settled (§6.2). Stat the manifest: its mtime is a time stamp (§6.2).
```

**The keeping actions** of M1 and M6, decided by the table of §6.2:

| Action | What the writer does |
|---|---|
| *Write*, *Remove*, *Refresh*, *Wait* | as above |
| *Keep and hold* (a book file or story that is not the app's own, N9) | 1. Copy the bytes with the no-replace helper and flush the copy and its folder. 2. Write the heads of the base into the path's heads file, if they are not there already. 3. Write the manifest once: state `Held{d}` and a `kept` entry. 4. Drop the path from D and X. 5. Add the book to the question (§11.1). The file is left as it is. |
| *Keep and write* (any other derived file that is not the app's own, `metadata.json` included, N10) | Step 1 as above; then step 3 writes only the `kept` entry (the state is not `Held`). Raise a one-line notice (`GeneratedFileRewritten`: "settings.json was changed outside translationCore and was written again. A copy is in .tc/external/…"). The path stays in D and is written in this same group. |
| *Keep and write, lost* (the app's own bytes that show work the journal lost, N4) | Steps 1 to 3, with `why: lost`. Raise a notice with *Bring it back* (`LostWorkKept`). Then write the path. |
| *Restore* (`project.json`, `format.json`) | as §3.5: copy, then restore |

**A copy that cannot be made.** When step 1, 2 or 3 fails (a full disk, a folder another
program holds, the project turned read-only), the file is left exactly as it is and the path
waits, as R19 does: no group writes over it, `metadata.json` lists it as it is, and the next
look tries again (L130's rule: only a durable copy counts). Nothing is ever written over bytes
whose copy is not durable. The notice says why ("Titus was changed outside translationCore.
The disk is full, so translationCore cannot keep a copy yet and leaves the file as it is.").

**The materializer leaves a changed book file alone until *Import* or *Not now*.** The ruling
says *Not now* writes the book file from the project and *Import* writes it after the import, so
until an answer the file is not written. The person may still have the file open in Paratext
or a text editor; writing over it there would change a file under the editor, and its next save
would write the old buffer back. The waiting state already exists and is crash-safe: a held
path is skipped by every group (M0), `metadata.json` lists its bytes as they are (M2), a commit
tree takes the fold's projection (§9.4), and a deletion releases it (R16, R18) while its `kept`
entry and copy stay, with *Import*, as after *Not now*. The copy is durable before the state
names it, so a crash at any point leaves either the changed file, found again at the next open,
or the copy and its `kept` entry (§6.4 X1, X2). Other derived files are not left waiting: no
question is asked about them, and `metadata.json` must list the folder as it is (I7).

There is no intent file and nothing to settle after a group: a group that stops after some of
its renames (a full disk, a kill, a power cut) leaves each path it named in `Writing{old, new}`,
on disk and in the session. The session knows which renames went through (`Placed{new}`). Bytes
equal to a placement the state names (`old` or `new`) are the app's own (§6.2). A program that
puts the app's earlier bytes back has put back a version the project holds, and writing over it
loses nothing while the fold holds what those bytes show (`holds`, row N4). A path the open
cannot read keeps its state until a read decides it, so the retry, the next save and a focus
never take the app's own bytes for a change made outside, and no false copy or question
follows; and since a write's state names only
its own bytes and the ones it replaced, the ledger never names an older write's bytes as the
app's own (the review of 2026-10-19 found a merged intent that did).

The heads of every placement the ledger names are on disk before the ledger names it (M3), so
no heads file lags its bytes and none is kept in memory: a later change of the bytes in the
folder, in this session or after a restart, is imported against the heads they stand for (§13.6). A file M1
or O9 finds equal to the projection while the ledger did not name those bytes with that scope (a
program wrote the projection, the heads moved with the same bytes) gets the fold's heads written
for them.

The burrito is valid before M4 and after M6; in between, `metadata.json` lags by milliseconds. A
crash there is repaired at open (§6.4, rows M1 to M6).

A project with no book yet (a create before its first book is added, §6.4 N1) has no valid
`metadata.json` (`currentScope` needs a book): M2 fails, nothing is written, and the paths stay
dirty until a book is there. Bytes in
`metadata.json` that are not the app's own are copied to `.tc/external/` with a notice, then
written over in the same group. Nothing in them is recorded. If a rename in M4 fails because another program holds the file open
(Windows: Paratext, an indexer), a program is probably writing that file right now, and the
writer **does not wait for it** beyond the short retries of §5.0 step 3 (`HeldRetry::BACKGROUND`,
35 ms, so one held file never stalls the other files of its book): the write of that path is
skipped, the path stays dirty, `metadata.json` lists the file as it is, and a timer of the
writer tries again after 200 ms, then after as long as the path has waited so far, up to 3 s.
The actor serves commands meanwhile, so a save never waits for a derived file. **Every try starts at M0 and so repeats M1 for that
path** (the md5 of its bytes): bytes the ledger does not name were saved by the other program
while the app waited, and they are kept (§6.2 N9, N10) instead of written over. After 2 s the path stays dirty and the person sees "translationCore
can't update TIT.usfm because another program has it open. Your work is saved."
(`MaterializeBlocked` with `Blocked::FileBusy`; the other reasons are in §2.6, and a notice is
listed once for each reason and path set and goes when the path is written; a `Failed` notice goes
when a later job finishes without an error). A file the open cannot read, or a compare-and-swap that cannot read the file,
is treated the same way: nothing is written over it, and it is tried again. Writes of pending files are never blocked by a derived file. A checkpoint
materializes after its seal even when the seal failed, because a pack the seal renamed before
it failed must be listed (I7); a path that stays dirty after any materialization, or a
materialization that fails, is tried again by the writer (200 ms to 3 s, above).

The residual race: a program that writes `p` between the last compare-and-swap check and the
rename that follows it is overwritten. That window is milliseconds on every attempt, and the
next look (a focus, an open, or the next M1 of that path) keeps that program's next save.

### 5.11 Seal

Seals run at every checkpoint (§5.13). The seal is the one long step of a checkpoint that runs
on the actor, so it runs in steps (two pending files read, the packs built, each pack written and
read back, four pending files removed, the ledger, the consolidation) and the actor serves the
commands that wait between two steps: a save waits for one step, not for the seal. A save made
meanwhile is a pending file that K1 did not read, so K4 leaves it. Each one:

```
K1  L = every verified change line of every pending file, deduplicated by id, sorted by
    (device, seq, id); split at line boundaries into chunks of at most 64 MiB. A line too long
    for a pack even alone with its header is set aside: only a change V7 quarantines can be
    that long (a salvaged line, §4.4), and it must never stop the seal of every other line.
    What K1 takes of each pending file is compared with the change ids the fold took from it,
    whatever happened to the file. A change the file no longer holds (another program removed
    the file, cut it short at a line boundary, which still verifies, cut it inside a line or
    changed a byte of a line, so that the line no longer verifies, or put a file of a newer
    format or of another project at its name) is taken from the fold's own bytes, checked
    against its id: in memory for a file this session wrote, or, for a file the open read or a
    salvage wrote, the bytes it verified, which it keeps (a pending file is small and sealed
    within minutes), since a handle reads a change made in place too. Its line goes into the
    pack, and the person is told (`PendingWrittenAgain`, the file and how many changes). The lines of a file that still verify are taken from the file,
    as salvage takes them (§4.4). Such a file is removed by K4 like any other (a removed one has
    nothing left to remove); one that no longer verifies is moved to `.tc/quarantine/` with its
    bytes (`QuarantinedPending`), as the open moves a damaged pending file; one of a newer
    format or of another project stays where it is, never removed, and the fold takes nothing
    more from it (the next open reads it by §4.2). The fold's values of any of them are not
    moved to the pack's offsets: they stay where the fold reads them. A pending file K1 cannot
    read (a program holds it open) is never removed; the changes the fold took from it that no
    pack holds yet are sealed from the fold's own bytes when it can give every one of them (a
    save of this session), so a damage to the file the seal cannot see takes nothing this
    session holds, and the next read decides the file. A pending file the fold holds changes
    of that K1 cannot take (it cannot be read and the fold cannot give its changes, or a
    change it lost is in neither the file nor the fold's bytes) is left where it is, never
    removed, and listed in the seal's report as unsealed; the person is told once
    (`FileUnreadable`, which goes when a later seal takes the file). The changes it lost for
    good are in no journal file any more, and a derived file the app wrote from them (the book
    file of a save) may be their only copy: the ledger names them among the heads of those
    bytes, the fold can no longer give their values, so `holds` is false and the bytes are
    kept with *Bring it back* instead of written over (§6.2 N4), in this session and after a restart. A file of
    a newer format was never read by the fold, so nothing of it
    is lacking and it is left without a word.
K2  For each chunk: atomic_create_dirs(ingredients/tc/changes/<me>/<yyyy-mm>);
    atomic_write(<that dir>, "<pack32>.ndjson", header + lines, SyncDir::Now). Each line set
    aside: atomic_write(.tc/quarantine, "<id>.line", line, SyncDir::Now); it is listed with the
    quarantined changes.
K3  Re-read every new pack from disk; P2 to P4 must pass and its id set must equal the chunk's.
    Any failure: remove the bad pack, keep the pending files, report, stop. Store each verified
    pack as a git blob (repo.blob), so it can be restored even before its commit (§4.4). A blob
    that cannot be stored now is stored by the checkpoint's tree build.
K4  atomic_remove each pending file all of whose change ids are in the verified packs or were
    set aside in K2;
    fsync .tc/pending. A pending file that could not be read is never removed (§4.4). One
    another program removed after K1 read it is as good as removed: the verified pack holds
    every change it held, so the seal goes on and the next one writes no second pack of it.
K5  Update ledger.json (lastSeq, lastAt) in app data.
K6  Consolidate if §4.7 says so (§5.12).
```

### 5.12 Consolidate

```
Q1  Pick the packs (§4.7). Read and verify each (P2–P4); skip any that fails.
Q2  Build the new pack(s) with `replaces`; atomic_create_dirs; atomic_write, SyncDir::Now.
Q3  Re-read; P2–P4 must pass and its ids must include every id of the packs it replaces.
Q4  atomic_remove each replaced pack (directory fsync once).
```

### 5.13 Checkpoint: seal, materialize, commit

A checkpoint runs on leaving Translate or the project (tC4 J8), before a sync, an export or a USB
export, after a USB import, at app exit, every
30 minutes while there are pending batches, and once a save has waited 2 minutes unsealed by the
store's clock (the actor checks after every command and on its timer): until a seal its only
copy is a pending file, and the owner's ruling allows losing at most the last 2 minutes.
Only the seal and the move of the branch stand in the actor's queue; the files, the tree and the
commit object are the writer's work from a snapshot, so saves go on meanwhile (R21, I9).

```
C1  On the actor: list the journal folders for a second writer (§5.0), check the expected pack
    set (§4.4: a pack that is missing from the working tree, or changed and failing P2 to P4, is
    written back from a blob at the same path and its changes are folded, `PackRestored`), seal
    (§5.11; K3 stores each new pack as a blob), and hand the writer the snapshot taken when the
    seal starts, with the dirty marks of that moment. The actor serves saves between the seal's
    steps; each is a pending file the seal never reads, so it is in neither the snapshot nor the
    marks, and it belongs to the next checkpoint and to a materialization of its own (a timer is
    set). The commit shows the project as its packs hold it: a seal that leaves a pending file
    unsealed (K1) therefore makes no commit (C3).
C2  On the writer: materialize (§5.10) from that snapshot. The new packs are now listed in
    metadata.json. This closes the write group the seal opened (I7). A path the writer's own
    retry or a look at the folder wrote meanwhile from a newer snapshot (it holds a save served
    between the seal's steps) is left as it is in M1: the job never writes an older projection
    over the person's newer save, and the job that follows the served saves finds the file equal
    to its projection. The same holds for a carried file placed or recorded from a newer
    snapshot (§11.4): its newer version stays in the folder, and is listed as it is. `metadata.json` is listed and written from the job's snapshot, so the
    project details of such a newer write show their older values in the folder, never in the
    tree or the journal, until that job (about 3 s).
C3  On the writer. If the seal failed, or finished with a pending file unsealed (K1: the snapshot
    shows its changes, which no pack holds), stop: the tree's files would show changes its packs
    lack (`SealFailed`; the next checkpoint reads the file again).
    If an expected pack could be neither restored nor read, stop: no commit, and the notice
    (`MaterializeBlocked`, reason `PackMissing`) names the file ("translationCore can't read a
    file of changes; sync waits until it can"). Otherwise build the commit tree in memory (§9.4)
    from the manifest's blobs for the derived files the last materialization made equal to the
    snapshot's projection, from the snapshot for the rest (a held path, a path another program
    refuses, a path left dirty, a held book file, and a path the
    writer wrote from a newer snapshot meanwhile, which a retry or a look at the folder may do),
    the expected pack set, the carried bytes, and the files kept verbatim (§9.4), and run tc_burrito::validate_project over the tree's
    entries. Failure: bug report (`MaterializeBlocked`, reason `TreeInvalid`), no commit.
C4  If the tree equals HEAD's tree, stop: no empty commits (tC4 J8).
C5  The writer writes the commit object itself. The message is "Checkpoint, <reason>: <books>
    (tC)" plus the trailers `Tc-Device: d…`, `Tc-Format: <major>.<minor>`, `Tc-Frontier: <64
    hex>`, the author is the Door43 username after the person consented to show it (D98),
    otherwise "translationCore user", the email is empty (D85), and the time is the store's
    clock. The object is written raw through the object database, because libgit2's signature
    refuses an empty email. The actor then moves the branch to it if the branch is still where
    the writer read it: libgit2 takes the reference's lock file, writes, flushes the file and
    renames it (core.fsyncObjectFiles=true makes libgit2 1.9 flush references as well as
    objects). A branch that moved meanwhile leaves the commit unreferenced; the next checkpoint
    commits again (`HeadMoved`).
C6  The actor writes the index from the tree (index.read_tree, index.write), so `git status` in a
    terminal shows a clean tree.
```

The signature never reaches libgit2's reflog: `core.logAllRefUpdates=false`, because libgit2's
reflog identity falls back to the machine's `user.name` and `user.email` (D85). A lost ref
update after a power cut means the next checkpoint commits again, and a damaged `.git` is rebuilt
(§6.4 G4). The reason words are "leaving Translate", "leaving the project", "before sync",
"before export", "after import", "every 30 minutes", and for a project just made "created".
The checkpoint's reply (`CheckpointReport`) names the commit, the packs sealed, and why there is
none when there is none (`NoChanges`, `ReadOnly`, `SecondWriter`, `NoHistory`, `SealFailed`,
`PackMissing`, `TreeInvalid`, `HeadMoved`).

### 5.14 Drafts

While an editor holds unsaved text, the UI sends `Draft { field, shown, text }`: the field, the
token of every record the editor displayed, and its text. A draft never goes through the
actor's queue: a writer thread of its own keeps the newest draft of each field and writes
`.tc/drafts.json` with `atomic_write` at most once a second, so a draft write never delays a
save (APP-DESIGN R22). A draft is best effort; after a crash the file holds the old list or the
new one (crash row D1). A successful save removes a current entry only when it was put before
the command was submitted, or its text equals a value now stored for the field (APP-DESIGN
R26, C5). A newer differing draft survives before the save's reply, independently of the
editor. Ordering uses local monotonic instants kept only in memory, never the page's clock;
the file format is unchanged. A newer section buffer is kept conservatively because the
section command carries parsed values rather than that raw buffer. The fields:

| Field | Text | Tokens |
|---|---|---|
| `Words`, `Layout` | the verse's words or layout | the verse's |
| `Section { book }` | the section editor's whole buffer | every verse and chapter front it displayed (APP-DESIGN R6) |
| `Frame`, `StoryRef` | the frame's or the reference line's text | the record's |
| `CheckComment` | the decision's comment | the decision's |
| `UserComment { target }` | the comment box (APP-DESIGN R15) | none for a new comment; the comment's for a revision (§5.5) |
| `Alignment` | the aligner's unsaved links, as `AlignmentEdit::to_json` writes them (APP-DESIGN R23) | the alignment's, then the verse's it displayed, so *Keep* saves it as `SaveAlignment` would, a verse's first alignment included |

An entry of `drafts.json` is `{field, shown: [token text], text, at}`.

Only a missing `drafts.json` means no drafts. A `drafts.json` is never written over unread:
- One the open cannot read (held by another program past the retries, no permission, a disk
  error) may hold sound bytes, so it stays where it is: the notice `FileUnreadable` names
  `.tc/drafts.json`, this session writes no drafts, and the next open reads it again.
- One read but not understood (damaged, cut short, a newer app's layout) is moved to
  `.tc/drafts.unreadable-N.json` under the first free N, the notice names that path, and drafts
  are written as usual. When it cannot be moved, it is handled as one that cannot be read.
- Every open, a read-only one included, names each `.tc/drafts.unreadable-N.json` there in a
  notice `FileUnreadable`, whether or not the session that moved it wrote drafts since, until
  the file is gone. The notice keeps no copy: dismissing it hides it for the session and leaves
  the file where it is, since it may hold the only trace of unsaved text. The app has no action
  that reads such a file or removes it; the person moves or deletes it outside the app.

An entry whose field this version does not know stays in the file as it is.

Cancelling an open section card drains its pending draft calls, then calls
`discard_current_draft` with that exact field, displayed tokens, text and original keystroke
time (APP-DESIGN C2). The draft writer removes only a matching current entry and atomically
writes the remaining list before answering. A newer current entry and independent offered
entries survive. A failed write is reported to the card; its browser buffer and mirror are
kept until Retry confirms the durable removal. This uses the same file format and atomic
helper as the draft writer.

At open (§6.1 O12), a draft is offered when its text differs both from the value its token
stands for and from what the record holds now: "Unsaved text from your last session was found
for Titus 1:4. Keep it / Discard." A draft whose text equals either was saved or is
superseded, and goes. *Keep* sends `KeepDraft`, which saves the draft as the save of its field
would, with the draft's tokens, so if the record changed since, the result is a conflict, not a
revert; a section draft's values come in `KeepDraft.section`, read from its buffer by the
section editor. A token whose MAC this installation can no longer check (the installation
secret was lost and made again) still names its record and heads: the draft is offered as
text, marked unverified, and *Keep* writes its value with no parents, so it stands as a
conflict with what the record holds unless the two are equal (APP-DESIGN R20); a section draft
is checked for coherence against the records' current values, since the draft's tokens are no
base, so a span the draft keeps is coherent against the verses it covers, and a verse the draft
leaves unchanged is not written again. A read-only open
offers nothing and writes no draft; a copy of the project (§5.0) carries its `drafts.json`, and
the copy's first open offers them.

*(APP-DESIGN C1b, built in tc-store by the app's unit C1b, 2026-10-19: the file holds two kinds
of entry. A* current *entry is the newest draft of a field this session typed, and a successful
save removes it under the ordering/equality rule above; `KeepDraft` does not. An* offered *entry is a text an open offered
(`offered: true` in the file), from the last session's entries or taken in from the page's
buffer (`ProjectHandle::offer_draft`, written before it returns its number): only `KeepDraft` of
its text or `discard_draft` of its number removes it, and `discard_draft` is written before it
returns, so a save of the field, the field's typing since or a reload never takes another text
of it, and a discarded text never comes back after a crash. An entry records the app's run that
typed it (`run`, from `OpenOptions::run`). At open, an entry whose text the record holds now (any
head), or, for one no open offered yet, the value its token stands for, goes; an entry of the
opening run itself stays that run's current entry; every other becomes offered, and the file is
written before the open returns. Whether a later save of its session superseded a draft is the
app's to judge, with the page's buffer and the shell's mirror (APP-DESIGN §6.6, the draft
machine).)*

### 5.15 OBS frames

`SaveFrame { shown, text }` writes one `Frame` change; `SaveStoryRef` one `StoryRef` change. The
text rules are tC4's `frameTextError`: no CR, no blank or whitespace-only line, no leading or
trailing newline, no line in image, reference or title form; the title and the reference are one
trimmed line. `StoryBase` is written once at creation from the template and never edited by the
app's screens; an edit of the template lines made outside the app (an image link changed in
a text editor) enters only through "Update a story from Markdown" (§13.6).

### 5.16 Carried files

`RemoveCarriedFile { shown }` writes `CarriedValue::Removed` with `parents = shown.heads`. The
next materialization removes the file (compare-and-swap first): a carried path whose manifest
entry names its blob, and whose record is a settled `Removed` (a conflict between a removal and
an edit removes nothing), is in X (§5.10 M0), so the folder is a valid burrito again at M6. A
file the ledger does not name as the app's is recorded as a new version (§11.4), not removed. A file that merely went missing is never recorded as removed (I11; §11.4).

---

## 6. Open and recovery

Every multi-file operation is a sequence of idempotent steps whose partial result is either
valid on its own or named by the placement ledger. A materialization names its writes in the
ledger before their renames (§5.10 M3, §6.2); a seal and a
consolidation leave pending files or packs that the redundancy rules recognize; an intake writes
immutable packs, each valid alone. Recovery never rolls back: it completes the operation from
bytes already on local disk. There is no scratch copy of the project and no marker that can lock
it (tC4 F10).

### 6.1 The open sequence

```
O1  Take .tc/lock (exclusive, non-blocking). Held → refuse the open with `OpenError::LockHeld`,
    which names the holder from the lock file's content (host and pid; unnamed when the holder
    wrote none or the platform will not read a locked file). The open writes nothing: the
    lock is only tried, and an existing file is not changed. translationCore runs once per
    computer (§5.0), so a holder is a process a crash left running or another program.
    Taken: if its content names another host, the folder may be open on that computer (§5.0,
    second writer) and the project opens read-only (`OpenOnAnotherComputer`); otherwise write
    {host, pid, device, at} into it, and clear it at close. A project that opens read-only
    for another reason (O4, O5) takes no lock.
O2  Finish or remove temporary files. A `.tc/pending/` or `ingredients/tc/changes/` temp file
    whose bytes verify (every line; for a pack, P2 to P4 with the name in the temp name) is
    complete: never delete it (on Windows a power cut can keep the temp file and lose the
    rename after "Saved" was shown). A complete pack temp is renamed into place durably (§5.0
    step 4). A complete pending temp waits until O8 has read the journal, and is then decided
    by the `(device, seq)` of its changes (O2b below): the person may have been told that the
    save failed (§5.0 B3) and saved again at the same seq, and rolling the failed attempt
    forward beside the later save would be a false *another computer was saving as this one*
    and a conflict with their own text. Remove every other `.~*.tmp` outside .git, every
    `*.lock` file under `.git` (index, refs, HEAD, config, packed-refs, shallow), and
    `.git/objects/pack/tmp_*` (safe: we hold the project lock and the app is single-instance).
    A temp that cannot be read is never removed (a pending temp so left counts as an
    unreadable pending file, §6.3), and a rename or removal that another program still holds
    after the retries (§5.0 step 3) leaves the file for the next open: the open never fails
    for a temp file.
O2b After O8 (the fold exists), for each complete pending temp, newest first by `(at, batch)`:
    (1) every change it holds is in the fold: it duplicates a file in place; remove it.
    (2) The pending file of its name is in place and not folded (unreadable): leave the temp
    and count it as an unreadable pending file. (3) One of its changes has a `(device, seq)`
    that the fold maps to another change, or that a temp placed earlier in this walk holds:
    it is a save the person was told failed and then saved past. Move the file, bytes
    unchanged, to `.tc/external/<utc>-not-saved-<name>` and list `UnsavedAttemptKept` ("A save
    that could not be written on 3 Oct at 10:42 was kept as a copy. Your later saves are in
    the project."); dismissing the notice never removes the copy. (4) Otherwise rename it into
    place durably, fold its changes and record it as a pending file. A temp that can be
    neither moved nor placed stays, and its changes' largest seq counts as held in O6's ledger
    rule, and as an unreadable pending file.
O3  git: open the repository in the folder `Vfs::outside_dir` names (the project folder; a
    mirror folder under the test filesystem), making one on branch `main` when there is none.
    If libgit2 cannot open it or HEAD is unreadable, rebuild it (§6.4 row G4) before
    continuing. If libgit2 refuses the folder because another account owns it
    (`ErrorCode::Owner`; the check cannot be turned off safely), that is not damage: nothing is
    rebuilt or moved, the person sees `HistoryUnavailable`, and the session runs without
    history (no commit, no carried bytes). Set core.autocrlf=false, core.fsyncObjectFiles=true
    and core.logAllRefUpdates=false in the repository's own config if they are not set, and
    open the repository again when one was written (libgit2 caches its configuration).
O4  Format gate: read ingredients/tc/format.json. Larger major → refuse (§12). minReaderMinor
    above this app → read-only (§12). Damaged or missing → restore from the HEAD tree; if HEAD
    has none either and tc/project.json is absent, this is not a project of this format: offer
    the importers (§13).
O5  Filesystem gate (§5.0): FAT32, exFAT or a cloud-synced folder → read-only with the copy
    offer.
O6  Device identity (§6.3). May mint a new Local device before any write.
O7  Finish interrupted work:
    - every pending file all of whose change ids are in a pack that passes P2–P4 → remove it
      (a seal that stopped after K3);
    - every pack made redundant by a valid pack's `replaces` (§4.7) → remove it;
    - every expected pack (§4.4) missing from the working tree or failing P2 to P4 there →
      restore it
      from a valid blob, with `PackRestored`;
    Before a pending file or a replaced pack goes, the pack that holds its changes is flushed,
    the file and its folder: a seal or consolidation that stopped between its rename (K2, Q2)
    and the folder flush leaves a pack only the page cache holds, and a power cut after the
    removal would take both copies of those changes.
    These rules read each file's facts (`tc_journal::index::Facts`: a pack's id, path, header,
    status and change ids; a pending file's header, status and change ids), which O8's read of
    the file gives, from its bytes or from its cache section alike, so no file is read twice.
    The open does not hold every pack's change ids until the rules run: for a whole Bible that
    is 20 MB more at the open's peak (§14). Only a
    pack another pack's `replaces` lists, and the pack that lists it, need their ids (none in a
    converged tree); a pack that only a later header lists gives its ids again from its section,
    or by a second read, which only an interrupted consolidation leaves to do.
O8  Load: read the fold cache `.tc/index.bin` (§3.6; missing, damaged, of another project, or of
    another cache version, format minor or probe stamp: no cache; any build of the app reads a
    cache of its own version, minor and stamp). Read every pack and pending file in parallel
    (rayon) and take its fingerprint. A file whose path, size and fingerprint equal a cache
    section's takes that section's prepared changes: those bytes passed P1 to P4 at that path
    when the section was made and give the same changes again, so they are not hashed, verified
    or parsed again. Every other file is verified (§4.4; each line is hashed once) and salvaged,
    and its lines are parsed and prepared one at a time (`tc_journal::index::load_pack`), so a
    reading thread holds one decoded change, not a pack's; a section is made of each valid file
    whose every line is a change that parsed (a damaged, newer or foreign file, or one with a
    quarantined line, is read and reported at every open). Store as a blob every pack HEAD's
    tree does not hold. The fold indexes each file's changes as they arrive (§7), keeping where
    each change's JSON lies, not its bytes: it reads values on demand through the store's
    `ValueSource` (§2.6, §14). The fold never fails.
    When the sections differ from the cache's (a new, changed or gone file), write the cache
    again in the background with `atomic_write`; a crash before that costs the next open the
    parse of those files only.
O9  Classify every derived and carried path (§6.2), reading and projecting only what may have
    changed. A path is converged without being read or projected when it is clean by stat (its
    manifest entry settled, its size and mtime equal to the entry's), the digest of the heads
    the fold holds for its scope, computed from the fold, equals the entry's `scope` (by the
    token rule of §2.7), and the format minor and derivation versions in the manifest are this
    app's. That test reads no file, neither the path nor its heads file. Every other path is
    read; it is projected only when its scope or versions differ, or its bytes are not its
    manifest md5. Changed files are kept (§11.1) before anything is rewritten, and changed book files
    are gathered for one question. The root and `ingredients/` are listed once, after the
    classification and with no file opened: a file that cannot be an ingredient is moved out
    (§3.3), and a book file the project does not show is kept and asked about (§11.1). An open after no change projects no file, and reads none
    once the files the last materialization wrote are settled (§6.2).
O10 Materialize every path whose disk bytes differ from the projection, except held paths, and
    metadata.json if it is missing, damaged or stale. If O9
    settled or refreshed entries, write the manifest again, so the next open takes them by
    stat. This is the writer's first job: the open does not wait for it, the project is
    usable while it runs, and saves go on. If it fails (a full disk, a folder that went
    read-only), the project turns read-only with `WriteFailed`, the lock is released, and the
    next open finishes the work (every write of the open is best effort).
O11 Schedule pin coverage backfill (§5.6) if a group needs it.
O12 Offer drafts (§5.14).
O13 Report: conflicts, anomalies, the question about changed books, the notices of kept
    copies, identity notices. The project is usable.
O14 The storage check, in the background (PERFORMANCE.md rule 4): read every path O9 took as
    converged without reading it, and every path whose entry is unsettled, and compare its md5
    with its manifest entry (§6.2): equal settles the entry once its tick is over; different is
    classified by the table of §6.2: a changed book file is kept and asked about; any other
    file is copied and written again. Saves and every screen go on
    meanwhile; a path a command must rewrite first is read by M1 (§5.10) anyway. **Its cost
    is every derived file, once per open**: for a whole Bible 334 files and 290 MB (277 MiB,
    measured: `docs/evidence/storage-open-save-2026-10-17.md`), read after the project is
    usable on one thread of its own, 0.5 to 1.0 s after it. The counters report these reads as
    `check_read`, apart from `derived_read` (the open's own classification and the
    compare-and-swaps), so "an open after no change reads no derived file" is true of the
    open itself and the check's cost is not hidden in it. On Windows with Defender on, every
    file opened is scanned (§14), so those 334 opens are paid in the background after each
    open, in competition with the saves: that is the price of reading what O9 spared, and
    the reason the check is the only place a stat that lies is found.
O15 A writable open that found pending files (saves a session that ended without a seal left)
    seals them (§5.11) after O9, so O10 lists their packs (O9 classifies the files against the
    packs they were written from) and K3 stores each as a blob;
    the next checkpoint commits them. Their only copy was a pending file another program could
    damage, and their age is unknown, so none may wait for the next seal (owner ruling
    2026-10-22-acceptable-loss: no save older than 2 minutes may be lost). One exception: when
    O9 could not read a derived file (another program holds it), a group a kill left half done
    cannot be finished, so the open does not seal and the 2-minute seal (§5.13) takes them. A
    seal that fails at the open keeps its pending files and says why in Notices; the open goes
    on writable.
```

Open refuses only for: another process holds the lock (`LockHeld`, which names the holder),
the folder cannot be read, the format major is newer, or the folder is not a project. It never
refuses because of content.
A journal file that cannot be read is listed (`FileUnreadable`) and never removed; so is one
whose stat fails (a cloud placeholder, a delete-pending file on Windows): it is listed with
size 0, read like any other, and reported when the read fails too. The open fails as
"cannot be read" only when `project.json` can no longer be read either, so a cloud placeholder
never stops an open and a folder that went away does. A derived file that cannot be read
(another program holds it) is neither damaged nor gone: it is reported (`FileUnreadable`),
left as it is and kept dirty, so the next materialization reads it again (M1) and writes it
when it can. **Every write of the open is best effort** (the lock's content, O2, O4's repair,
O6's device file, O7, O9, O10 and the manifest): the first write that fails (a full disk, a
folder that became read-only) ends the open's writes, and the project opens read-only with
`WriteFailed(reason)` and the lock released, showing everything it holds, instead of being
refused; the next open finishes the work (the tests `an_open_on_a_full_disk_is_read_only_and_never_refused`
and `a_disk_that_fills_during_the_opens_materialization_leaves_a_read_only_project`).

**Notes on the sequence as `tc-store` runs it.** O4 restores `project.json` and `format.json`
from HEAD's tree when they are missing or damaged (§3.5), and writes this app's `format.json` when
HEAD has none. O7's expected pack set is the manifest's pack
list (§3.6) and HEAD's packs (§4.4): a listed pack that is gone, and that no valid pack's
`replaces` names, is restored from a blob at the same path when one passes P2 to P4
(`PackRestored`), and is otherwise missing (`PackMissing`), and nothing derived is removed
while it is. A damaged pending file is salvaged at O7: its verified lines, byte for byte, go into
a new pending file of this device (purpose `salvage`) and the file moves to `.tc/quarantine/`
(`QuarantinedPending`); a pending file of another project moves there unread. The second half
of O6 runs after O8, because it needs the fold: the ledger's `lastSeq` above this device's
largest seq in the folder (a backup put back, `RestoredBackup`), or a `DuplicateDevice` anomaly
naming this device, or a pending file of the folder that could not be read (`PendingUnreadable`,
§6.3), mints a new device before anything is written, and the new device's profile copies the
old one's names. A changed file O9 finds is kept before the open ends (§11.1): a book file or story is
held for the question, any other derived file is copied and written again (§6.2). Each journal file is
opened once per open: the reading thread hands its handle to the
fold's `ValueSource`, which reads values on demand through it (§2.6), so a value the fold reads
during or after the open never opens the file again. O8's cache write and O14 run on one
background thread after the project is usable; `wait_idle` waits for them.

**Progress.** The open shows determinate progress within 300 ms of the person's action
(PERFORMANCE.md rule 4), as tC4's open does (its J15, `e2e/j15-slow-open.spec.ts`). An open
still running 300 ms after the command shows from then on its phase and how much of it is done,
never a spinner alone; an open that ends sooner shows nothing. The phases are: finishing
interrupted work (O2 to O7); reading changes (O8: files and bytes read, of the totals the
listing gives); folding (the fold's work after the last file, in steps of a known total);
checking files (O9: paths, of the total); writing files (O10: paths, of the total). The actor's threads count into shared
counters, and a reporter thread, never the screen thread, sends the first report 300 ms after
the command, whatever a read is still waiting for, then at most every 100 ms while a count
moves. Counts only grow within a phase. The last report comes before the project is usable;
an open that fails ends with its error after its last report. Neither leaves an indicator
standing.

The test `open_shows_determinate_progress_by_300_ms` (unit S1) opens a corpus project through a
`FaultFs` that delays every read so that the open lasts several seconds (the Psalms corpus in
the debug test run, where generating the whole Bible would take minutes; the whole Bible's open
is measured in release builds, `docs/evidence/storage-open-save-2026-10-17.md`), and asserts:
the first report, with a phase and a total, is sent no earlier than 300 ms and no later than
400 ms after the open command; no report is sent from the caller's thread; counts never fall
within a phase, and each phase's last count is its total; the last report precedes "usable".
It opens a small fixture without delays and asserts that, if the open ended before the 300 ms
mark, no report was sent. It opens a folder that stops being readable 400 ms into the open
(the `FaultFs` fails every read from then on), and asserts that the open ends with its error,
after its last report.

**A read-only open only reads.** When O1, O4 or O5 makes the open read-only, the app skips O2,
the rebuild in O3, every restore in O4 and O7, the blob and cache writes of O8, O9 to O12, and
every write of O14 (it still reports what it finds):
it loads (taking what it can from the fold cache),
folds in memory and shows the project. It writes nothing in the folder, so it can never delete
another writer's temporary files or lock files, or rename a `.git` that another process is using.

### 6.2 Telling the app's interrupted writes from someone else's edits

An open must not read and project every derived file: for a whole Bible that is 210 to 277 MiB
to read (the memo's estimate, and 334 files of 290 MB on the corpus unit S1 measured) and about 10 s of projection (`docs/JOURNAL-DECISION-2026-10-16.md` §2). So each path
is first tested without reading any file, and read only when that test fails.

**Settled entries.** A program that changes a file within the same timestamp tick as the app's
own write leaves its mtime as it was, and if it keeps the size too, no stat can show the change
(git's racy-clean problem). The tick is at most 2 s on a filesystem the app may read: FAT32
keeps 2 s, HFS+ and ext3 1 s, exFAT 10 ms, NTFS 100 ns but set at the system clock's tick
(about 16 ms), ext4 and APFS 1 ns at the kernel's tick. So a manifest entry's size and mtime
stand for the bytes only once the app has read the bytes after that tick was over. An entry is
**settled** when a read of `p` found `d == m` with the entry's size and mtime, and the read
began after a **time stamp** at least 2 s later than the entry's mtime. A time stamp is the
mtime, as the filesystem set it, of a file the app wrote in `.tc/` and stat'ed before the read
began: `.tc/lock` at O1, and `materialized.json` at each M7. The filesystem stamps both the
file and the time stamp, so no clock of this computer is trusted; on a network drive the
server's clock does.

The entry of a path M4 wrote is unsettled, and so is an entry refreshed from a read that did not
settle it. Whenever the manifest is written again, every other entry keeps its own state: an
unsettled entry is never written again as settled (git smudges racily clean entries when it
writes its index, for the same reason). Comparing a file's mtime with the manifest's own mtime
instead would fail when the manifest is written again later: on an HFS+ disk (1 s timestamps),
TIT.usfm and its entry are written at t = 0.1 s, both stamped 0; at t = 0.6 s Paratext saves a
same-size fix, still stamped 0, and the watcher misses it; at t = 10 s saving a verse of
Genesis writes the manifest again, after which TIT.usfm's mtime is 10 s older than the
manifest's and the fix would be taken for the app's own file. Here the TIT entry stays
unsettled until a read settles it, so the window-focus check (§11.1), O14 and the next open
read it, and the fix is kept and asked about.

**Clean by stat.** A path is clean by stat when the ledger's state is `Placed` with settled bytes,
the file exists, and its size and mtime equal the placement's. A network drive (SMB, NFS, AFP, WebDAV, from
`sysinfo::Disks`) can answer sizes and times from a client cache, so there no path is clean by
stat and no entry settles: each path is read. A program that puts an older mtime back after an
edit of the same size (some sync clients, `rsync -t`, `cp -p`, archive extractors) can leave an
edited file clean by stat; the storage check (O14) reads it, and it is kept like any changed file (N9, N10).

**The scope digest.** Each manifest entry keeps `scope`: the SHA-256 of what the projection of
`p` reads (`FoldState::scope_digest`), for the bytes the entry places or holds; when the entry
comes to stand for other bytes (a read finds a write's new bytes, a hold begins), the scope is
dropped until a job sets it, never kept from the placement before (L123). O9 computes the same
digest from the fold in memory. The
scope is the fold's own list of the records the projection of `p` reads (for a book's files,
the book's records of each kind from the fold's per-book key lists, and the project records the
file lists), never the heads file's record list: a record that entered the scope since the file
was written (the first comment on a book, a teammate's new check decision) or left it changes
the digest, as a new head does. The scope is cut into parts that several files share (a book's
text, its alignments, the decisions of one tool in one book, its comments, the pins, the
settings, the versification, a story, the conflicts, the project details), and the digest of
`p` is the SHA-256 of its parts' digests in a fixed order. A part's digest takes, for each of
its records in key order, the canonical key, a zero byte, the record's heads, then **every**
folded version of the record, not only the heads the token rule lists: a projection reads more
than the displayed candidate (the newest time of a decision file over every head, which head is
provisional, the bases of a merge, a passage's sides, the newest text of an incoherent verse
among its ancestors), and a parent that arrives after its child changes those while the heads
stay. A book's text part also takes the fold's view of the book (presence, scope, the last-book
rule), and the comments part the authors' device profiles and the chain of moved checks a
comment follows. A digest that changes when nothing written would change costs only one
projection of that file; the property
`a_file_whose_scope_digest_is_unchanged_renders_the_same_bytes` (`tc-journal/tests/scope.rs`)
checks the other direction over random team histories. The heads file keeps the token rule's
heads (`FoldState::file_heads`), since it is the base of an import of a kept copy (§13.6) and an
outside editor saw only the provisional text. The digests cost one pass over the fold's records
in memory and no file read: a heads file is read only when an import needs its base.
`metadata.json`'s entry keeps the digest of every project detail, book presence and pin group
(`FoldState::metadata_scope_digest`), and the store adds the ingredient table it lists.

**Converged without reading** (O9): clean by stat (so `Placed`, never `Writing` or `Held`), its
`scope` equal to the fold's, and the manifest's `minor` and `derivations` this app's. The
projection of `p` is a function of exactly those heads and versions (I1), so it is the manifest
md5, and the file, by its stat, still holds it.
`metadata.json` is converged in the same way when, in addition, the ingredients and md5s it
lists are those the manifest and the pack set now give (the manifest keeps each pack's md5 with
the size and fingerprint it was computed for: packs are immutable, and O8 takes every pack's
fingerprint anyway).

**Read, but not projected:** a path whose scope and versions are unchanged but which is not
clean by stat (unsettled, a moved mtime, a network drive) is read: `d == m` means converged;
the entry's size and mtime are taken again, and it settles if the read began after a time stamp
at least 2 s later than its mtime. Only a path whose scope or versions changed, or whose bytes
are not `m`, is projected and classified by the table below.

**What a skipped read cannot cause.** A path O9 took as converged without reading it is never
written over or removed unread: M1 (§5.10) reads every path's bytes before it replaces or
removes it, and the storage check (O14) reads it in the background. Either one decides it by
the table below, which never writes over bytes that are not the app's own until a copy of them is
durable, and never over a book file until the person answers, so I8 holds.

**The app's own bytes.** The table asks one question about the bytes `d` a read found: are
they the app's own write? The app's own bytes at a path are the bytes of the placement
`Placed{p}` (the version the app last wrote); the bytes `old` or `new` of `Writing{old, new}`
(the version it last wrote, and the version it named in the ledger before renaming); and the
bytes the project would write there now, when every value could be read (N2). Everything else
is **changed externally**, and is kept before anything is written over it: a book file or story
is held until the person answers (*Import* or *Not now*, §11.1); any other derived file is
copied, with a one-line notice, and written again in the same group; a carried file's new
version is recorded as a version (§11.4). A change that kept the file's size and time (a program
that put an older mtime back, a failing disk) is no separate case: the storage check (O14)
finds it like any changed file, and the same rule applies. Nothing in a changed file enters the
project unless a person imports it (I13).

**Held paths.** A path is held only while a changed book file or story waits for the answer.
The ledger's state is `Held{d}` with the copy's path; the manifest is written at once, after the
copy is durable (§5.10 *Keep and hold*). Materializations leave the file as it is (§5.10 M0;
`metadata.json` lists the bytes on disk, M6, so the folder stays a valid burrito, and commit
trees take the fold's projection, §9.4). The person sees the question of §11.1. *Import* runs
§13.6 on the kept copy and ends the hold (`Imported`). *Not now* gives up the held bytes
(`NotNow`: `Placed{h}`) and rewrites the file from the fold; the copy stays in the notices with
*Import*. While the path is held, another save of the file keeps another copy with an entry of
its own, and the hold moves to the newest bytes (N9); the earlier copy stays in the notices. A
deletion ends the hold, and the file is rebuilt (O9 reads a held path that is gone as gone:
`MissingFileRebuilt`; M0 releases it for a path being written, so `metadata.json` never lists a
file that is not there); its `kept` entry and copy stay, with *Import*. So does a projection
that comes to equal the held bytes (`d == e`, N2). A held path is never converged without
reading: each open reads it again.

**The hold outlives the session; so does its question.** The ledger's `Held` state and the
`kept` entries live in the manifest, so every open asks again about every held book and lists
again the notice of every kept copy, until the copy is imported or dismissed: the file does
not silently stop following the project because the app was restarted, and *Import* and *Not
now* stay reachable (O13). Dismissing the notice of a kept copy removes the copy only while
every byte still matches (§3.6).

**The placement ledger.** Who wrote the bytes at a derived path, and at `metadata.json`, is
never inferred from clues; it is looked up (owner ruling 2026-10-19). Each path's entry in
`materialized.json` holds one state of the ledger (`tc_store::ledger::State`), and the path's
heads file holds, for each placement the state names, keyed by the md5 of its bytes, the heads
of every record those bytes show (§3.6):

| State | Meaning |
|---|---|
| `Unknown` | no entry: the app never placed bytes here, or `.tc/` was lost or copied away |
| `Placed { placed }` | the app placed these bytes (md5, size, mtime once taken, settled) and knows its rename went through, or found them its own |
| `Writing { old, new }` | the app named bytes `new` before renaming them over `old` (or over nothing): the rename may or may not have reached the disk |
| `Held { held }` | a book file or story changed outside the app, kept as it is until the person answers (§11.1): md5, copy in `.tc/external/`, size and mtime |

Before any rename the app writes the heads of the new bytes into the path's heads file, then
`Writing` into the manifest, durably (§5.10 M3); once the rename went through it records
`Placed` (M4, M7). There is no other record of a write in flight: a crash anywhere leaves at most
one write per path whose outcome is unknown, and its state names both md5s it may have left. A
book file's second write begins only after a read has settled the first (a read moves `Writing`
to `Placed`). A look that finds `old` settles the path too (`Placed{old}`, N5 to N8), so a second write of
`metadata.json` begins from `Placed` like any other. The ledger never names more than two
placements of a path, and the first write's bytes, put back, are bytes it never named (N9, N10).

**The facts of a read.** The writer gathers them by lookups and stats, never by a guess:

- `disk`: the md5 of the bytes, *missing*, or *unreadable* (another program holds the file);
- `projection`: the md5 of the projection of the path, *absent* (the projection has no file
  there), or *not asked* (the watcher, the focus and a listing do not project);
- `heads readable`: every value the projection read could be read (§2.6). The fold counts the
  reads its source could not give (`FoldState::values_unreadable`); a rendering that moved the
  count showed a version as absent and is not written;
- `holds`: whether the fold holds what the bytes show: every change the ledger's heads of these
  bytes name is held, applied (its batch complete or released, §7.4), and readable unless it is
  still a head the projection reads (whose readability is the fact above). *None* when the
  ledger keeps no heads of these bytes;
- `newer fold`: the job's fold is older than the one that placed the bytes (a checkpoint's job,
  §5.13 C2).

**The transition table** (`ledger::next`, normative; `every_pair_of_the_table_keeps_every_byte`
walks every state, event and fact). The events are `Read`, `WriteBegun`, `Renamed`,
`WriteFailed`, `Stat`, `Adopted`, `Kept`, `Imported`, `NotNow` and `Removed`; the actions are
*Write*, *Rebuild*, *Remove*, *Refresh* (take the size and mtime again), *Wait* (leave the file,
try again), *Forget*, and the keeping actions of §5.10: *Keep and hold*, *Keep and write* and
*Keep and write, lost*. "Own" below is: the bytes are one of the placements the state names,
`Placed{p}` with `d = p`, or `Writing{old, new}` with `d = old` or `d = new`.

A read that found bytes `d`, the first row that applies:

| # | State | Condition | Next state | Action |
|---|---|---|---|---|
| N1 | any | newer fold | same | Wait: the next job, on that fold, looks |
| N2 | any | heads readable and `d` = projection | `Placed{d}` | Refresh: converged, whoever wrote the bytes. A hold ends; its `kept` entry and copy stay, with *Import* |
| N3 | `Held{h}`, `d = h` | | same | Refresh (the hold's time) |
| N3a | own, `d` = the md5 of a kept copy whose entry lives (the person answered *Not now*) | holds is none | `Placed` | as N6 to N8 (Wait, Remove or Write), with no new copy, no notice and no question: the bytes are kept already |
| N4 | own | holds is false or none | `Placed` | *Keep and write, lost*: base = the ledger's heads of `d`, each record's heads cut to the changes the fold holds applied and readable (a record that lost one is compared in full at *Bring it back*): the bytes may be the only record of that work (a journal file cut or removed, a batch that lost a member, a value no longer readable) |
| N5 | own | holds, projection not asked | `Placed` | Refresh |
| N6 | own | holds, heads not readable | `Placed` | Wait: a value that cannot be read is never written as absent; the file keeps its bytes until it can be read (`MaterializeBlocked`, `ValuesUnreadable`) |
| N7 | own | holds, projection absent | `Placed` | Remove |
| N8 | own | holds, projection differs | `Placed` | Write |
| N9 | not own, a book file or story | | `Held{d}` | *Keep and hold*. Base: `Placed{p}` gives `placed:p`; `Held{h}` keeps the base of the waiting entry; `Unknown` gives `head` when HEAD was made by this app (for each record the heads among the changes the packs of HEAD's tree hold: a change another pack or only a pending file holds is newer than the file the person started from, so the import conflicts with it and replaces nothing), else `none`; `Writing` with `d` neither gives `none` (which version the person started from is unknown) |
| N10 | not own, any other derived file | | `Placed` after the write | *Keep and write* (for `project.json` and `format.json`, *Restore*, §3.5) |
| N11 | not own, a carried file | | as §11.4 | a new version (§11.4) |

N3a exists because `NotNow` turns `Held{h}` into `Placed{h}` and no heads file names `h`, so
`holds` is none for those bytes. Without it, a crash before the rewrite, or a rewrite refused
because Paratext still holds the file open (common on Windows: `WriteFailed` gives
`Placed{old}` = `Placed{h}`), would send the person's own given-up bytes to N4: a second copy
and a false notice that a file of saved changes was damaged.

A read that found no file (N1 first), or a file it cannot read:

| # | State | Condition | Next state | Action |
|---|---|---|---|---|
| R15 | any | no file; a projection whose values could not all be read | `Unknown` | Wait: the file the project holds comes back, never with a value shown as absent |
| R16 | any | no file; a projection | `Unknown` | Rebuild ("TIT.usfm was missing and was rebuilt" when the ledger knew the path: a deletion is never an edit, I11; a hold ends) |
| R17 | any | no file; the projection has no file there | `Unknown` | Forget (a removal by this app that stopped before the manifest was written) |
| R18 | any | no file; a look that does not project | `Unknown` | Rebuild: the path is marked for the next job |
| R19 | any | the file is there and cannot be read (another program holds it) | same | Wait (`FileUnreadable`) |

`ledger::hits` counts the rows and events taken in a process; the nightly model-based test
(§16.1) asserts that it took every one the model can reach (`ROWS_OUT_OF_REACH` lists the rest,
which named tests take).

The other events:

| State | Event | Next state | Action |
|---|---|---|---|
| `Unknown`, `Placed{p}` | WriteBegun{n} (M3) | `Writing{old: none or p, new: n}` | — |
| `Writing{old}` | WriteBegun{n} | `Writing{old, new: n}`: the bytes of the first write are no longer named, so found again they are kept by N9 or N10 | — |
| `Held` | WriteBegun | same: held bytes are written over only after *Not now* or *Import* | — |
| `Writing{new}` | Renamed (M4) | `Placed{new}` | — |
| `Writing{old}` | WriteFailed (busy, refused by the last look, the target unchanged by a failed write, a rename the stopped group never tried) | `Placed{old}`, or `Unknown` with no old | — |
| `Placed`, `Held` | Stat (M7, a read) | same, with the size, mtime and settled mark | — |
| any | Adopted{d}: a carried file's version the project places found there, a fixed file the app wrote again | `Placed{d}` | — |
| any | Kept{d, copy}, after *Keep and hold*'s copy | `Held{d}` | — |
| any | Imported{d}: the import batch of a kept copy with bytes `d` is durable and the path still holds `d`; the heads of `d` are written first (§13.6 U6) | `Placed{d}` | — |
| `Held{h}` | NotNow (*Not now*) | `Placed{h}` | Write |
| any | Removed | `Unknown` | Forget |

Every other pair (`Renamed`, `WriteFailed`, `Stat`, `Imported` and `NotNow` in another state)
changes nothing and does nothing. No row writes over or removes bytes unless they are the app's
own and the fold holds what they show, or they are what the project would write; every other
bytes are kept first (a durable copy, and for a book file the person's answer) or left as they
are. A write's `old` bytes are the app's own, so a crash between a write's ledger entry and its
rename shows no question, no copy and no extra conflict (L146). The one visible effect: after
such a crash, a backup tool's restore of the version just before the app's last write is
written over without a question, and that version stays in History. A group that stops with an
error, the app still running, knows which renames it never tried or that left their target
unchanged, and records WriteFailed for them.

A pending file another program holds at the open leaves the fold without the saves it holds:
a book file that shows them is kept by N4 (a copy, with *Bring it back*), and the saves come
back with the file at the next open.

`metadata.json` follows the same table as every file other than a book: it must list the files
as they are (I7), so it is never held and never pending. Bytes that are not the app's own are
copied (`GeneratedFileRewritten`) and written over in the same group (N10). Bytes about to be
written over are
decided even when clean by stat: the table is asked about the md5 the ledger placed, and the
file is read only when the table keeps them. A `metadata.json` that cannot be read while a
write of it is unsettled waits, as any path whose first write no read has settled (L125). When a file the job cannot read or write leaves no valid listing (the only book file
held by another program, with no ledger entry or with no file to list), or a book file the folder
lacks waits for a value that cannot be read (R15), nothing is written and the paths wait for the
file (`MaterializeBlocked`): never a failed write that turns the project read-only or fails a
checkpoint. A fixed file (`project.json`,
`format.json`) shows no record (holds is true), and N10 restores it after a copy (§3.5). A
carried file shows no record either; a new version of it is recorded by §11.4 (N11).

Because the ledger records what this app wrote before it writes, the app never takes its own
stale file for a person's change, and never overwrites a person's change as if it were stale. Nor does
it write over its own bytes when they are the last copy of work the journal lost: the fold's
answer to `holds` covers a pending file cut or removed (before a crash or while open), a batch
that lost a member, and a value that can no longer be read, and every look at a path (O9, M1,
M4's last look, M6 for `metadata.json`, the watcher, the focus, a listing, the storage check)
asks the same table. A changed book file is kept and asked about whatever its stat; a path is held only for
that question.

### 6.3 Device identity

- `fingerprint = sha256(installation secret ‖ machine id ‖ canonical folder path)`. The machine
  id comes from the `machine-uid` crate (`/etc/machine-id`, the Windows `MachineGuid`, the macOS
  `IOPlatformUUID`).
- `.tc/device.json` missing, or its fingerprint differs (a copied or moved folder, app data
  copied to another machine): mint a new Local device. Pending files of the old device stay and
  are sealed as they are; their changes keep their own device.
- `ledger.json` says this device wrote a higher seq than any own change in this folder (an older
  backup was restored): mint a new device and show "This copy is missing N changes this computer
  made (last on 12 Oct). They may be on Door43, in another copy, or in a newer backup." Minting
  prevents reusing seq numbers that exist elsewhere.
- The fold reports `DuplicateDevice` when two different changes share a Local device and seq (two
  machines wrote as one device: a cloned virtual machine with the same app data, folder path and
  machine id). If the device is this folder's, mint a new device and show "Another computer was
  saving as this one in this project. Both computers' work is kept; anything both changed is
  listed as a conflict." Nothing else changes: the fold never treats two changes of one device
  specially, so their concurrent edits are ordinary conflicts.
- A pending file of the folder that cannot be read at open (another program holds it, a
  placeholder) may hold seq numbers this device already used, and the fold does not count it.
  The open mints a new device before the first write (`DeviceRotated`, reason
  `PendingUnreadable`; a device minted by this same open has none to fear), so a save cannot
  reuse a seq, and when the file can be read again nothing says another computer saved as
  this one. The file is never removed or sealed while it cannot be read (§4.4). A complete
  temp of a pending file that O2b could neither place nor keep counts the same way.
- A save that failed (`NotWritten`, §5.0 B3) leaves the counter where it was, and a temp file
  that may stay (§5.0 step 5). The retry reuses the seq. O2b therefore never folds that temp
  beside a later save that used the seq: the later one is the person's, and the attempt is
  kept as a copy. With no later save, the attempt is the person's newest text and O2b folds
  it with no notice and no new device.
- The next seq is one more than the largest seq of this device among its changes in the folder.
- A new device copies the display name and Door43 username into its own `DeviceProfile`, so
  screens group one person's devices by username.

### 6.4 Every crash point

"Before" and "after" mean the user-visible state of the action. Every step is durable before the
next starts, so power loss is covered by the same rows (the row says where a lost rename lands).

| # | Action | Crash after | State on disk | Next open | Result |
|---|---|---|---|---|---|
| W1 | save | temp written and flushed, before rename (or, on Windows, a power cut that dropped a rename after "Saved") | complete `.~*.tmp` in `.tc/pending` | O2 verifies it and renames it into place | after; nothing that was shown as "Saved" is ever deleted |
| W1a | save | temp partly written | incomplete `.~*.tmp` | O2 removes it (it does not verify) | before; "Saved" was never shown; the draft is offered |
| W1b | save | rename refused by a held file past the retries, and the temp's removal refused too | complete `.~*.tmp` in `.tc/pending`; "Not saved" was shown | O2b: kept in `.tc/external/` when a later save used its seq; otherwise folded | the person's later save, or else the attempt; never both as a conflict and never a new device |
| W2 | save | rename, before the rename's flush | the file may be lost on power loss; a complete temp may remain (W1) | read if present | before or after, never torn; "Saved" was not yet shown |
| W3 | save | directory fsync, before the fold update | complete pending file | folded | after |
| W4 | save | the fold update, before the reply | complete pending file | folded | after; the UI shows the draft offer only if the text differs |
| D1 | draft | any step | old or new `drafts.json` | offered or not | the draft is best effort; saves are not affected |
| K1 | seal | pack temp written | temp + all pending files | O2 renames a verifying temp into place (then as K2) or removes it | before or after (the pending files hold the changes) |
| K2 | seal | pack renamed, before the re-read | pack + pending files | O7: pending ids all in a valid pack → removed | after |
| K3 | seal | some pending files removed | pack + some pending files | as K2 | after |
| K4 | seal | pending removed, before materialize | the pack is not listed in `metadata.json` | O10 materializes | after |
| K0 | seal | the pack folder of a new month made, its entry in the parent not flushed | the folder only in the page cache; no pack | the next seal flushes the entry of the folder it finds (§5.0) before it writes the pack into it and removes the pending files | before or after; a power cut after the restart's seal keeps the pack |
| Q1 | consolidate | new pack temp | temp + old packs | O2 (rename if it verifies, then as Q2; else remove) | before or after |
| Q2 | consolidate | new pack renamed, old packs not removed | both | O7 removes redundant packs | after |
| M1 | materialize | ledger written (`Writing`) | old files | O9: `d = old` is the app's own: written again from the project when the fold holds what it shows (N8), else kept with *Bring it back* (N4); no question and no conflict | after |
| M2 | materialize | some files renamed | mixed old and new | O9: `d = new` → own → rewrite; `d = old` → as M1 | after |
| M3 | materialize | power loss loses a rename | that file has its old bytes, `d = old` | as M1 | after |
| M4 | materialize | files written, `metadata.json` not | `metadata.json` stale: the folder is not a valid burrito | O10 rewrites it | after |
| M5 | materialize | `metadata.json` written, manifest not | manifest stale | O9: `d == e` → refresh | after |
| M6 | materialize | a program put the old bytes back after a rename and before M7, with their old mtime or not | `Writing{old, new}`, `d = old` | O9: the app's own, as M1: no question, and no copy unless the fold no longer holds what the bytes show (N4) | after |
| M7 | materialize | killed after a rename, before its directory flush; reopened without a power cut | the rename in the page cache only | O9: `d = new`, the file and its directory are flushed before the ledger names the bytes placed, so a later power cut cannot put the old bytes back under it | after |
| G1 | commit | objects written, ref not moved | HEAD old, loose orphans | the next checkpoint commits again; repack drops orphans | after (git is not the truth) |
| G2 | commit | ref lock file left | `refs/heads/<b>.lock` | O2 removes it | after |
| G3 | commit | power loss loses the ref rename | HEAD at the previous commit | the next checkpoint commits again | after |
| G4 | any git step | power loss damaged `.git` | git2 cannot open it or HEAD does not read (not: libgit2 refuses the folder because another account owns it, which rebuilds nothing, §6.1 O3) | rename `.git` to `.tc/broken-git-<utc>`; `git init`; restore `origin` and `tc.branch` from `.tc/sync.json`; read the broken repository's `objects` directory through an object database of its own (`Odb::new` and `Odb::add_disk_alternate`; attached to the new repository, an alternate would make libgit2 count its objects as stored and write nothing) and copy into the new object database every blob a `CarriedFile` change names and every pack blob it can read; store as blobs the carried files in the working tree whose md5 and oid match a recorded version; commit the tree built from the fold (a carried head whose blob is still missing is left out with `CarriedBytesMissing`, §9.4), "Recovered history (tC)". The next sync merges with Door43 as an unrelated history (§9.4) | after; the packs never depended on git |
| Y1 | sync | during fetch | libgit2 temp pack | O2; the next fetch redoes it | before |
| Y2 | sync | some received packs written (§9.3 T1) | some new packs, each valid alone | folded; a partly received batch waits (§7.4); the next sync fetches R again and takes the rest | after for what arrived; nothing half-applied |
| Y3 | sync | packs written, the notices of §11.2 not shown | packs | the next sync's intake reports each commit by another tool once, by commit id (§11.2) | after |
| Y4 | sync | intake done, merge commit not made | packs in place, HEAD old | the next checkpoint commits; the next sync makes the merge commit | after |
| Y5 | sync | merge commit made, push not sent | local branch ahead of Door43 | the next sync pushes | after; "not yet on Door43" |
| Y6 | sync | push sent, not confirmed | unknown | the next sync's fetch shows whether Door43 has it | after; "sent, not confirmed" until a fetch confirms |
| U1 | USB export | writing the zip temp on the stick | `.~*.tmp` on the stick, project untouched | removed at the next export to that folder | before; "Saved" never shown |
| U2 | USB export | renamed, before the verify (E5 reads from the device, not the cache) | a complete zip, not yet verified | the person never saw "Saved"; the next export overwrites it | before |
| I1 | USB import | reading or validating the zip | nothing written | nothing | before |
| I2 | USB import | some packs written, salvage batch or checkpoint not | some new packs; `.tc/intake.json` present | folded; notice "The import of Juan's file (3 Oct) did not finish. Import it again."; importing again takes the rest and adds nothing twice | after for what arrived; nothing half-applied |
| X1 | keep a changed book file | copy written and flushed, manifest not | the file still changed; a copy with no entry | read again; the helper finds the same bytes under the earlier name, so there is no second copy; the entry is written; the question is asked | after |
| X2 | keep a changed book file | manifest written (`Held`, `kept`), question not shown | held | the question is asked at the open (O13) | after |
| X3 | *Import* | import batch durable, `kept` entry not ended | batch with `reconciles {path, md5}` | a `why: changed` entry whose md5 a durable, applied batch reconciles is ended, never imported twice; `Imported{md5}` if the path still holds those bytes; the file is written from the project. A `why: lost` entry is never ended this way (L115: an older batch that reconciled the same bytes says nothing about a save the journal lost later); *Bring it back* leaves out only the records an earlier reconciling batch changed and the fold still holds applied and readable | after |
| X3a | *Import* | import batch not durable (a crash, a full disk, a refused pending file) | `Held{h}` and its `kept` entry; perhaps an incomplete batch | an incomplete batch marks nothing imported (L127); the question is asked again at the open; after a refusal the report says why and the question stays | before |
| X4 | *Import* or *Not now* | entry ended or `NotNow` written, file not rewritten | `Placed{d}` (heads written by `Imported`) or `Placed{h}` (no heads) | `Placed{d}`: materialized at the open (N8). `Placed{h}`: N3a, materialized with no new copy or notice | after |
| X5 | keep and write another derived file, or *Keep and write, lost* | copy flushed, file not rewritten | the file still changed | read again; the same copy found; written | after |
| X6 | *Keep both* (carried) | batch durable, new path not written or conflict copy not removed | the version still at its `.conflict-` path | the new path is written first, from the blob or, when the blob is missing, from the conflict copy stored again as a blob; the conflict copy is removed only after that (§11.4) | after |
| A1 | upgrade | inside the resource store temp directory | temp directory | deleted at app start | before; pins unchanged |
| A2 | upgrade | resources installed, batch not written | new resources unused | nothing | before; the person runs the upgrade again |
| N1 | create, import or join | inside `<projects>/.staging-<kind>-<16 hex>/` (`kind`: `create`, `copy`, `join`, `import`) | staging folder | deleted at app start, by a cleanup that ends before the first create, import or join starts (tC4 #676), so it never deletes the staging folder of an operation that is running; the store says which folders it deleted and of what kind, so a join that did not finish is told with `refusal.open-door43.removed` (`docs/TC4-DELTA-2026-10-16.md` §3.3) | before |
| N2 | create, import or join | after the folder rename | complete project | normal open | after |
| R1 | repack | pack and index written, loose objects not removed | duplicates | the next repack removes them | after |

The rename of N2 never replaces anything at the name: `rename_dir_noreplace` (Linux
`renameat2(RENAME_NOREPLACE)`, macOS `renamex_np(RENAME_EXCL)` through `rustix`; elsewhere
`std::fs::rename`, which refuses a non-empty target, after a check that the name is free). On a
collision only the staging folder is deleted and the person sees the name-exists message; the
destination is never touched (`docs/TC4-DELTA-2026-10-16.md` §3.10).

The crash tests kill the writer at every write, fsync, rename and unlink of these rows, with and
without power-loss semantics (§16.2).

---

## 7. The fold

### 7.1 Input

The input is a set of changes: every verified line of every pack that passes P2 to P4, every
salvaged line of a damaged pack, and every verified line of every pending file, de-duplicated by
`ChangeId`. Two lines with one id have the same bytes (the id is their hash), so de-duplication
never drops information. Changes that break a rule of §4.5 are listed as quarantined and not
folded.

### 7.2 Algorithm

```
fold(changes) -> FoldState
F1  Index: id → change; key → changes; change → packs holding it.                     O(N)
F2  Batches: group by batch ref (id and `n` together, so changes that disagree on `n` are
    judged as separate batches). A batch with fewer than `n` changes present, or with a
    quarantined change, is held: `IncompleteBatch`, none of its changes is folded, except the
    changes that some `BatchRelease` of that batch lists (the union over every release),
    which are folded (§7.4).                                                              O(N)
F3  Links: for each change c and each parent p of c:
      p unknown or held             → `MissingParent(c, p)`, link ignored
      key(p) ≠ key(c)               → `ParentOnOtherRecord(c, p)`, link ignored
      otherwise                     → mark p as having a child on its key                O(N + P)
F4  Heads: heads(k) = changes on k with no child.                                        O(N)
F5  Devices: two different ids with the same Local device and seq → `DuplicateDevice`.
    (`ClockAhead` compares `at` with this machine's clock, so `tc-store` computes it as a
    notice outside the fold.)
F6  Resolve each key with more than one head (§7.3): equal values → Merged(EqualValues);
    the stub rule → SettledByRule; a record diverging from a passage batch → part of a
    `Passage` conflict, never merged by units; otherwise merge units → Merged(Units) or
    Conflict.
F7  Provisional head of each conflict: the head with the largest (rank, at, id), where rank is
    0 for an External change recorded with an unknown base and 1 for every other change. An
    accidental mass deletion or an old copy restored by a cloud client therefore never becomes
    the text other tools see before a person accepts it. A `Passage` conflict chooses one side
    for all of its records instead (§7.3, "Passages").
F8  Books: presence, removal visibility, the last-book rule (§7.6).
F9  Verse coherence per chapter (§7.5): StructureIssue conflicts.
F10 Validity of alignments and decisions (§7.7).
F11 Projection of derived files, lazily per book and per document (§7.5, §7.7).
F12 Report: conflicts sorted by key (canon book order, then numeric), anomalies by kind then key;
    frontier = sha256 of the sorted list of all head ids.
```

No step can fail. A cycle cannot exist, because a change's id is a hash over bytes that include
its parents' ids. A link to an unknown or foreign parent is ignored and reported, never fatal.
Steps F3 to F6 read no clock, device or author (I3). F7 reads `at` only to choose what derived
files show while a person has not decided.

### 7.3 Merge units

A record with several heads is merged per **unit**: a group of fields that must be taken
together, because mixing them would make a state nobody chose.

| Record | Units |
|---|---|
| `Verse` | If every head and every base is `Text` with equal `last`, `marker` and `from`: `words` and `layout`. Otherwise the whole value is one unit. So a paragraph break added by one person and a rewording by another merge; two rewordings of any part of the words conflict; a span against a text conflicts. A record that diverges from a passage batch is never merged by units (see "Passages"). |
| `Check` with `Decision` values | `judgement` (selections, nothing to select, status, verse text md5, `imported_invalid`); `comment`; `bookmarked`; `dismissed`; `basis` (context, category, resource); each `extra` key. If any head or base is `MovedTo`, the whole value is one unit. |
| `Pins` | each slot of the group (`gatewayLanguage`, `translationNotes`, …); `ExtraScripture` is one slot |
| `Setting`, `ResourcesExtra` | each leaf path of the JSON value (objects recurse; arrays and scalars are leaves); a removed key against a present one is the whole value. An object the merged leaves leave empty, which no head has empty, is left out: one head removed it and the others kept it unchanged. A `ResourcesExtra` conflict is listed as a `Pins` conflict |
| `DeviceProfile` | `display_name`, `door43_username`, `legacy` |
| `ProjectMeta` | as `Setting`: each leaf path of the JSON value (so two translators' `localizedNames` for different books merge) |
| `Alignment` | `dismissed`; every other field together (two people's links are never mixed) |
| everything else | the whole value |

Values are compared through their stored canonical JSON. A field from a newer minor that this app
does not know (§2.3) belongs to the unit of its nearest known enclosing field; at the top level
of a value it is a unit of its own. So two heads that differ only in such a field are never
"equal values".

**Bases.** Within the subgraph of the key, `Common` = the changes that are ancestors of every
head, and the **bases** are the maximal elements of `Common` (those with no descendant in
`Common`). Usually there is one. Two people who each resolved the same earlier conflict produce
two bases (a criss-cross).

**Rule per unit u:**

```
values = { u(h) : h in heads }
if all values are equal                                   → agreed: that value
else if bases is non-empty and every base has the same u(b) = B:
     changed = the distinct values u(h) with u(h) ≠ B
     if changed has exactly one value                     → agreed: that value
     else                                                 → disputed
else                                                      → disputed
```

The rule is conservative: if the bases disagree on a unit, the unit is agreed only when every
head has the same value. A unit is never taken from one head when another head changed it to
something else. No disputed unit → `Merged(Units)` with the merged value; otherwise `Conflict`
with the agreed and disputed units listed. A conflicted record always projects the provisional
head's whole value, never a mix.

**Stub rules.** Starting text that no person wrote never makes a conflict. A **seed stub** is a
change with `cause: Seed` on a `Verse` (whose `words` is `___` or empty), `BookHeader`,
`ChapterFront` or `BookLayout` record. A `StartOver` change (§5.7) is a person's choice and is
never a seed stub. These rules apply before the merge units, to the `Verse`, `BookHeader`,
`ChapterFront` and `BookLayout` records only:

1. **Absorption.** A head that is a seed stub is absorbed when another head of the record is not
   a seed stub. (Maria and Juan each added Titus offline; Maria then drafted verse 1. Juan's
   untouched stub of verse 1 is absorbed and Maria's text is settled. Ana imported Ruth from a
   USFM file while Juan added Ruth: Juan's stubs are absorbed by Ana's text.)
2. **All stubs.** If every head is a seed stub, the record settles on the one with the smallest
   id.

If exactly one head remains, the record is `SettledByRule` (`AutoRule::SeedStubs`), no conflict
is listed, the activity log says "Two copies of the starting text of Titus were merged", and the
absorbed stubs stay in History. If two or more heads remain (none of them a seed stub), they
resolve as usual among themselves. A person's edit of a `SettledByRule` record lists all heads in
its token (§2.7) and so replaces all of them.

**Passages.** A save that rewrites several verses at once can move words between them: a
section save that drags a verse number past a word, a join or a split, a renumber that shifts
every following verse by one, *Start empty*, a structure resolution, an import of a
chapter (§13.6). Judged verse by verse, such a save against a concurrent edit by verse number would show
one verse's words twice or not at all while the conflict is open, and a per-verse choice could
lose them for good. So:

- A **passage batch** is a batch, other than a seed batch, that holds changes with non-empty
  `parents` to two or more `Verse` records of one book. Its **passage** is the set of `Verse`
  records of that book it wrote.
- For a record `k` with several heads, **Div(k)** is the set of changes on `k` that are, or are
  ancestors of, some head of `k` but are not ancestors of every head. If Div(k) holds a change of
  a passage batch P, `k` **diverges** from P.
- A **`Passage` conflict** is a connected group of records: start from a record that diverges
  from a passage batch; add every record of that batch's passage, whether or not it has several
  heads; add every record that diverges from a passage batch of a record already in the group;
  repeat until nothing is added. Its records are never merged by units. It is listed once, with
  `key` = its first record and `also_keys` = the others; its candidates are the heads of its
  records, grouped by side.
- **Sides.** Every passage batch with a change in the Div set of a record of the group is a side.
  A change in a Div set belongs to side P when it is, or descends from, P's change on that
  record; a change that belongs to no passage batch's side belongs to the side **the others**. A
  side's **time** is the largest `(rank, at, id)` over its changes (rank as in F7). The
  provisional side W is the side with the largest time.
- **Rewinding** a set of passage batches L on a record `k`: replace every candidate that is, or
  descends from, the change of a batch in L on `k` by the parents of that change, repeat until no
  candidate descends from such a change, then drop every candidate that is an ancestor of another
  candidate. A record with no candidate left projects nothing (it did not exist before L wrote it).
- **The provisional value** of each record `k` of the group: start from the heads of `k` and
  rewind every passage-batch side except W. If W is a passage batch that wrote `k`, take the
  remaining candidate that is or descends from W's change with the largest `(rank, at, id)`;
  otherwise the remaining candidate with the largest `(rank, at, id)`.

Each side is a whole state of the passage that one person made against what they saw, so the
provisional text shows every word of the passage once: either Maria's renumber, with Juan's edit
of the verse that used to be 2:2 waiting in the conflict, or the old numbering with Juan's edit.
No unit of one side is mixed into the other, so a paragraph break added by Juan never lands on a
different verse's words. The coherence pass (§7.5) still runs on the result. Sides, Div sets and
rewinding read only parents, batch ids and values; only the choice of W reads `at` (I3).

### 7.4 Batch completeness

Every change carries its batch's size (`BatchRef.n`). The fold applies a batch only when all its
changes are present (F2). A section save, a span, a resolution and an upgrade are therefore seen
whole or not at all on every device, even when a damaged pack carried only part of them. A held
batch is listed: "Waiting for the rest of Juan's resource upgrade: 1,203 of 2,140 changes
received." The changes stay on disk and apply when the rest arrives.

The rest may never arrive: Juan's computer is gone and the only copy of one line is damaged. The
listing therefore offers **Apply what was recovered**, with the recovered changes shown first
("11 of 12 verses of Juan's section save of 3 Oct. Verse 1:7 was lost."). It writes a
`BatchRelease` naming exactly the changes this person saw (§5.9); the release is an ordinary
change, so every device applies the same changes, and a later arrival of the full batch applies
the rest. Releases of one batch are grow-only: the fold applies the union of the changes they
list.

**Incomplete history.** A head with a `MissingParent` link (its parent is held or was lost)
continues a version this device cannot see. A conflict that **involves** such a head is marked
`incomplete_history`: the review says "Juan's version continues a change that has not arrived
(or was damaged). It is not a rival of the older text." A conflict involves a head when it
lists it as a candidate, as one of a candidate's equal heads (`also`) or as unreadable; never
a base a merge needed. A book's restore race also involves the verse heads its *Start empty*
candidate stands for (§7.6): the `StartOver` changes that are still heads of their verses.

The mark is a **warning** on the conflict and never a reason to hide a choice: every choice of
the conflict's kind stays offered, so the person can always keep every version, whatever the
state of the history. When the missing parent is in a held batch, *Apply what was recovered*
is offered first, because the conflict usually disappears once it is applied; when the parent
is in no batch this device holds, the continuation is listed first, because that parent may
never come. Nothing in the fold refuses a choice because `incomplete_history` is set
(`an_incomplete_restore_race_offers_every_choice`).

### 7.5 Verse coherence and the USFM projection

Records are independent, so their projected values can disagree about structure: verse 4's
provisional value may be a text ending at 4 while verse 5 is `Covered { by: 4 }` from another
author's span. The projection resolves each chapter in one pass and never drops a verse's text
silently.

For each book that projects as present:

```
order = ascending, or the projected BookLayout order with every chapter and verse the layout
        does not list placed right after its nearest listed numeric predecessor (first if none);
        entries of the layout with no record are skipped
out = BookHeader bytes
for each chapter c in order, over chapters with a present ChapterFront or any Verse record:
    ChapterFront(c) present → out += its bytes
    otherwise, if c has verses → out += "\c c\n"; StructureIssue(c)
    for each verse v of c in order, by its projected value:
      Text { last: L }:
          end = v; while end < L and projected(end + 1) == Covered { by: v }: end += 1
          end < L → StructureIssue(v ..= L)
          out += "\v" + lead + label(v, end) + term + words + layout
          verses v+1 ..= end are consumed
      Covered { by: a }, consumed by a's run → nothing
      Covered, not consumed (incoherent) → emit the newest Text value (largest (at, id)) among
          verse v's heads and their ancestors, labelled v; StructureIssue(v).
          If v never had a Text value → nothing; StructureIssue(v).
      Absent → nothing
label(v, e) = "v" if e == v, else "v-e"
```

The projected value of a record is its settled head, its merged value, or its provisional head
(for a `Passage` conflict, its provisional side's value, §7.3). Every `StructureIssue` is listed
as a conflict of kind `StructureIssue` over the verse range, even when no single record has two
heads, and its resolution writes every verse in the range (§8.3). Guarantee: every record with a
projected `Text` value has its words in the USFM exactly once, whatever the layout lists; a verse
whose span is incoherent shows its own newest text; a passage is shown wholly from one side;
every inconsistent combination is listed.

An unedited verse projects the bytes it was given, so an edit to one verse changes no byte
outside it (tC4 D8, D18), and `recompose(decompose(x)) == x` for every book the codec accepts.

### 7.6 Books

*Owner ruling H5 (2026-10-24, `docs/rulings/2026-10-24-H5.md`), over the rest of this section:* a removal that is the newest of the book's presence heads and of the edits its remover had not seen removes the book, and those edits stay in History; a newer edit keeps the book with its edits; neither is a conflict. A removal that would leave a Bible project with no book loses to the others, with no conflict. Only *Restore the text* against *Start empty* stays a conflict, because one of them empties verse text.

**Presence.** If any head of `BookPresence(b)` is `Present`, the book projects as present, with
the scope of the `Present` head that has the largest `(at, id)`. A `Present` against a `Removed`
is also a `BookPresence` conflict, and so are two `Present` heads with different values: a
different scope, or *Restore the text* against *Start empty* ("Maria restored Jonah with its
text; Juan restored it with empty verses"). While that race is open, the provisional `Present`
head (largest `(at, id)`) decides what the book shows: if it is *Restore the text*, every
verse change that is, or descends from, a `StartOver` change of the concurrent *Start empty*
batch is rewound (§7.3) for projection, so the text shows and no verse typed after *Start
empty* shows beside restored ones; if it is *Start empty*, the empty verses show. A verse
someone edited concurrently with *Start empty* is in the `Passage` conflict of that batch
(§7.3). A verse changed after *Start empty* (descending from its `StartOver` change) is listed
as a candidate of the presence conflict, with its record in `also_keys`, so it is on a screen
while it is rewound (I2); the `StartOver` changes themselves are the *Start empty* candidate's.
A `StartOver` change that is still the head of its verse and continues a change that has not
arrived marks the presence conflict as incomplete (§7.4), as it marks the verse's own
`Passage` conflict. A teammate's verse typed after *Start empty* is one of the heads whose
batches the conflict lists, so the review groups its row with that teammate's other work
(§8.1).
Choosing *Restore the text* writes, in one batch, the presence resolution and, on every verse
whose head is or descends from a `StartOver` change of the losing batch, a `Restore` of the
value rewinding gives. Choosing *Start empty*, or a verse listed as its candidate, keeps that
side whole.

**A book file appearing outside the app adds the book.** A USFM file or a sidecar of a book
that has no `Present` head, imported from a file (§13.6 U1),
brings a `BookPresence::Present { scope from the file (Whole for a sidecar), how: Added }` change
with no parents in the same batch. If the book never had a presence record, it simply appears.
If it was removed, the new `Present` against the `Removed` head is a visible `BookPresence`
conflict ("jdoe added Ruth on Door43 after Maria removed it"), and the book projects meanwhile.

**Records without a book.** Records of a book that no folded `BookPresence` change mentions (for
example their `AddBook` batch is held in a damaged pack while later edits arrived) project
nothing; the fold lists `RecordsWithoutBook { book, records }` with *Add <book>*, which writes a
`Present` (how: Added) so the records project.

**Removal never hides unseen work.** If every head is `Removed`, compute the unseen changes:

```
for each removal head r, closure_r(k) = the changes on key k whose short id is in r.seen,
                                        plus all their ancestors on k
unseen = { c on any record of book b : for every removal head r, c ∉ closure_r(key(c)) }
```

The records of a book are its `BookHeader`, `ChapterFront`, `Verse`, `BookLayout` and
`Alignment` records, every `Check` whose target is in the book, and every `Comment` whose target
is in the book. Two changes under one comment id (§8.1) may target different books: the comment
is then a record of each, by the changes that target it, so which change arrived first never
matters (I1). If `unseen` is non-empty, the fold lists `BookRemovedWhileEdited { unseen }`
("Maria removed Jonah on 3 Oct. Juan made 14 changes to Jonah after that.") and the book projects
as present, with the scope of the nearest `Present` ancestor, until a person decides. If
`unseen` is empty, the book projects nothing and its records stay (I2 state d); restoring the
book brings all of them back.

**Last book.** In a Bible project, if no book would project as present (two people removed the
last two books at the same time), the book whose removal head has the largest `(at, id)` projects
as present and the fold lists `LastBookRemoved` for it, so the burrito stays valid.

### 7.7 Computed validity and the other projections

The derivation functions live in `tc-usfm` and are versioned (§12):

- `plain_text_v1(content)`: byte-compatible with tC4's `verseTextMd5` input (usfm-js 3.4.3:
  parse `"\v 1 " + content` with `{chunk: true}`, concatenate the top-level `text` of every
  verse object, trim with JavaScript's `trim`). Imported tC4 and tC3 alignments keep their
  hashes and their validity. The md5 is taken over the UTF-8 bytes of the result.
- `tokenize_v1(text)`: string-punctuation-tokenizer 2.0.0's default `tokenize`: the words and
  numbers, in order.
- `selections_v1(content, selections)`: word-aligner-lib 1.0.1 `validateVerseSelections`
  semantics. Its input is the raw content (`words + layout` of the run, markers and all), not
  `plain_text_v1`'s output: it goes through usfm-js's `removeMarker`, and the selections still
  fit when each one's `text` occurs in the result exactly `occurrences` times
  (non-overlapping; an empty text occurs 0 times). The golden vectors are made this way, and
  `plain_text_v1` would drop the words of `\w` and `\zaln` markup that `removeMarker` keeps.
  A selection made now records as `occurrences` the count of its text in that same result
  (`SelectionText::occurrences`), so it fits until the text changes.

**Shown text and hashed text.** tC4 reads a verse two ways, and so does the rebuild. The
screens show `tc_usfm::shown_text_v1(content)`: tC4's `verseText` of
`src/data/bookModel.js:15` (through `verseTextIndex`, `src/state.jsx:352`, at d310bf3), which the
Check screen shows and taps and tC4's aligner banks (`src/state.jsx:1183`). The rebuild's aligner
banks `tc_usfm::target_words(content)` instead, the words the export weaves, so a value that
places every banked word weaves (tC4 #796); the shown text decides no word of an alignment. It parses `"\v 1 " +
content` with usfm-js 3.4.3 as `plain_text_v1` does and joins the `text` of the verse objects at
any depth (tC4 9ad4d5c, #782), trimmed with JavaScript's `trim`. So a Psalm's superscription and
its `\qs Selah\qs*` are words to check and to align, while a footnote is not shown. Where tC4
hides words the export counts (the items of a `\li` list, the words after a span that holds a
footnote), the rebuild shows them, a declared difference (`docs/DIFFERENCES.md`; the rules are in
`crates/tc-usfm/src/derivations/shown.rs`). (`tc_usfm::display_text`, the readable text of
`src/views/verseText.js`, which tC4's source panes show, is no verse's shown text.) Every hash is
of `plain_text_v1`, never of the shown text (`TargetText::md5`, `TargetText::shown` in
`tc_journal::decision`). The two texts differ on many verses: `plain_text_v1` keeps a poetry
line break as a line break, drops the text of a `\qs` written over lines (the ULT's Selah)
and runs a verse's slot on into a heading. So an edit of a word the hash leaves out keeps the
hash: the aligner opens a stored value as it is only when its hash matches and it holds
exactly the verse's words, and otherwise carries it over (`tc_align::Aligner::open`).
tC4's aligner stamps `targetVerseMd5` with the md5 of its shown text (`stampTargetVerse`,
`src/data/align/edit.ts:230`), while its fold (`journal/fold.mjs:934`) and export
(`src/data/export/weave.mjs:23`) compare it with `verseTextMd5`, so on such a verse tC4's
files mark invalid an alignment its aligner has just saved, and its rail shows it valid. The
rebuild stamps the md5 of `plain_text_v1` wherever it stamps (§5.3, §5.4), so the rail and the
files agree. A record tC4's aligner stamped on such a verse is imported invalid, as tC4's files
say; opening it, the aligner carries every link whose word is still in the verse over to the
current text (`tc_align::reflow`) and the next save makes it valid.

usfm-js throws a TypeError on a few malformed nestings (for example a `\zaln` milestone with no
words inside another milestone, met by `removeMarker`). The library has no value there, so
version 1 defines one: `plain_text_v1` is the content trimmed, which changes with every edit,
and `selections_v1` says the selections fit, as tC4's `decisionIsStale` does when the validator
fails. A marker named after a member of JavaScript's `Object.prototype` (`\constructor`,
`\toString`) is read as an unknown marker; usfm-js looks it up as that member and may throw.
`removeMarker` also recurses once per level of nesting and runs out of stack (a RangeError) a
few thousand levels down, at a depth that depends on the JavaScript engine and its state, so
the library has no stable value there either. Version 1 has no depth limit: its value is the
walk's when it finishes, which is usfm-js's value at every depth usfm-js finishes. (A verse
object inside itself would make the walk endless; no parse makes one, and it would count as a
throw.) Every derivation takes time linear in the length of the verse and the selections, so
no verse, however built, stalls a fold.

Each is tested against golden outputs generated once from the JavaScript libraries and committed
under `fixtures/derivations/` (every verse of tC4's USFM fixtures, edge cases and seeded random
verses; `crates/tc-usfm/tools/gen-derivation-vectors.cjs`), including tC4's sample hash.

**Anchors: attached records follow the text.** Alignments and verse comments are attached to a
verse's text, not to a number. The **anchor** of a verse with a projected `Text` value is its
`from` (§2.3) when set, else its own number. An attached record keyed by anchor `a` is **placed**:

1. at the verse whose projected `Text` has anchor `a` (after a renumber, the alignment and the
   comments made for the old 2:2 are placed at 2:3, where that text now is);
2. else, if verse `a` is `Covered` inside a run starting at `s`, at that run: the aligner for the
   run `s` shows it as "an earlier alignment of verse 5, made before 4–5 were joined", with
   *Use as a starting point*; a comment shows with the run's comments;
3. else, if verse `a` is `Absent { moved_to: Some(d) }`, by following `d` (with these rules
   again, at most 64 steps);
4. otherwise it has no place: it is listed under **Detached** in the review (`FoldState::detached`,
   "Kept, not on any screen": "Maria's alignment of Titus 1:16, a verse no longer in the text"),
   with *Show*, *Open in the aligner at…* (alignments) and *Dismiss* (§5.9). A dismissed record
   leaves the list and stays in History.

A comment on a check follows `MovedTo` chains (at most 64 steps) to the live key, so a comment
made concurrently with a carry-over shows on the moved check. A comment whose check is not in the
current item list shows with the comments of the check's verse, marked "on a check that is no
longer in translationNotes". A decision that a carry-over could not place (§5.8) shows in the
checker's verse view under "Checks from the previous translationNotes" and is listed under
Detached until dismissed. So every head of every record kind can be reached from a screen.

**Alignments** (`checking/alignments/<BOOK>.json`, tC4 §5.1 shape). For the alignment record
keyed by anchor `a`, placed at the run that starts at `v` by rule 1:

```
run   = the projected run that starts at v
end   = v + (value.span_end − a)        the run the alignment was made for, moved with its text
valid = value.span_end >= a and run.end == end
        and md5(plain_text_v1(words + layout of v's projected Text)) == value.target_verse_md5
        and (value.source is None or value.source.sha == the current original-language pin's sha)
        and not value.marked_invalid
```

The record projects with `"invalid": !valid` under the label `label(v, end)`, with every stored
field unchanged. `AlignmentValue::staleness` (`tc_journal::alignment`) computes `valid` and
says which part fails; the fold and the Align screens both call it. (Comparing `run.end` with `span_end` itself would mark every alignment of a
renumbered verse invalid although its text and span are unchanged.) A record placed by rules 2 to 4, or a second record placed at a run
whose own anchor record exists, projects under its key's own label as invalid. A JSON object
holds one record per label, so a record whose own label another record already holds is left out
of the sidecar; it stays in the journal, in History and in the Detached list.

**Decisions** (`checking/<tool>/<BOOK>.json` and, for the stories, `checking/<tool>/OBS.json`,
tC4 §5.2 shape):

```
resolution(tool, collection) = tC4's D30 ladder (primary, then fallback) over the projected
                         pins only (`tc_journal::pins`); no installed-resource input.
                         The tool's slot: translationNotes, translationWordsLinks (for
                         translationWords decisions), translationQuestions; for the stories
                         obs-tn, obs-twl, obs-tq
resource_ok   = resolution is a usable pin and record.resource is the same resource
                (repoPath equal ignoring ASCII case, sha equal)
text          = the content the target names, by the no-text rule below; or none
verse_changed = text is some and md5(plain_text_v1(text)) != judgement.verse_text_md5
selections_ok = !verse_changed or selections_v1(text, selections)
invalidated   = !resource_ok or !selections_ok or imported_invalid
verseEdits    = verse_changed
status        = invalidated ? (user status == Todo ? "todo" : "invalid") : user status (omitted if none)
userInvalid   = true only when invalidated and the user status is Invalid
comments      = the comment, or false;  reminders = bookmarked
modifiedTimestamp = the newest head's `at` in RFC 3339
```

`tc_journal::decision::assess` computes these flags; the fold and the Check screens both call it,
so a screen and a decision file never disagree on the same inputs. A Check screen opens a session
from `FoldState::decisions(tool, collection)`: each projected decision of the (tool, collection)
with the flags its decision file carries and whether it is listed under Detached
(`ProjectedDecision`). A decision that was not dismissed is listed there when it was made against
another resource than the resolution of its (tool, collection), a book's or the stories' alike,
or when its verse is no longer in the text. The session places them on the derived check list with the carry-over's
two passes (tC4 `completedCheckSession`); a decision placed on the check it is stored under shows
the fold's flags, one shown on another check is assessed against that check's text with the same
function, and one no check carries is listed off the list (`tc_check::session`). The texts a
screen assesses with are the fold's (`FoldState::check_texts`, by the no-text rule below), so
the screen and the file read one text.

**The ladder** (`tc_journal::pins`), every rule tC4's (`src/data/resolve.ts` at d310bf3):

- A rung's slot answers a book when its pin covers the book; the first rung that answers is the
  resolution. The next rung is tried only when the slot is empty or does not cover the book
  (`resolveSetSlot`). A slot is empty when it is absent or holds a value JavaScript reads as
  false (`null`, `false`, `0`, `""`): tC4 tests it with `if (pin)`. Any other value is a pin,
  usable or not.
- **Coverage** is the pin's `books` record when it is a non-empty list of strings: each code is
  read in upper case, and `BIBLE` anywhere in the list covers every book (the whole-collection
  form of a resource not divided into books, such as translationWords). A code that names no
  book covers nothing. An empty list, a list holding anything but strings, or no `books` at all
  records nothing (`coverageFor`): in the fold such a usable pin, not yet backfilled (§5.6),
  covers every book, so the fold never reads what one computer has installed (I12). A screen
  that opens a tool on this computer reads such a pin as tC4 does: the books its installed copy
  holds, else none (`Unrecorded::Installed`); that rule never reaches a file.
- **An unusable pin** (a slot value with no `repoPath`, or no 40-digit lowercase `sha`, such as a
  pin with a tag and no commit) still answers the books it records, so the ladder stops there:
  the next rung is never used in its place, which would check the book against another
  language's list without saying so. No decision matches it, so every decision of that (tool,
  book) is invalidated until the pins are repaired. Having no commit it has no installed copy,
  so if it records no books it covers none and the next rung is tried.
- **The stories** of an OBS project are not divided into books: the first rung whose slot is
  not empty answers, usable or not (`resolveObsSetSlot`).

**The no-text rule** (tC4's Check screen: `verseTextIndex` and `withSpanMembers`, `src/state.jsx`,
and `decisionIsStale`, `src/data/revalidate.ts`, at d310bf3). A decision on a verse reads the
content of a projected run only when its chapter is a number and its verse is a number (the run
that holds that verse, a span included) or a span label `a-b` that a projected run carries
exactly; and only when the run is drafted by the one drafted rule of Home, Check, the store and
the fold (`tc_usfm::codec::words_drafted`, owner ruling H12): its words, on any line, trimmed as
JavaScript trims, are neither empty nor the stub `___`; a line of layout markers alone holds no
words. So a seeded poetry stub (`\v 2 ___` then a `\q2` line) is undrafted, and a verse whose
text starts on a `\q1` line of its own (`\v 1` then `\q1 Blessed is the man`) is drafted. This
differs from tC4 on purpose: tC4's Check screen, Home and aligner read only the body its indexer
finds (up to the first line that starts with a paragraph-level marker, `\v` or `\c`;
`tc_usfm::codec::tc4_indexer_drafted`), so they show such a poem verse as not drafted. Every other target (`front`, `intro`, a list such as
`1,3,8,12`, a span no run carries, an undrafted verse) has no text: the text gives no evidence
either way, so it neither changes the verse nor loses the selections, as in tC4, where such a
decision is never stale. One difference is deliberate: tC4's indexer reads a `\v` only at the
start of a line, so it indexes no verse whose marker stands inside a line (`\q1 \v 14 May
Yahweh…`, the ULT's own layout, 5,138 lines of en_ult v89 by tC4's count in
`src/data/seed.ts:9`), and such a verse has no text on its Check screen at all. The rebuild cuts
verses at every marker (§13.5) and reads such a verse's body from its marker by the rule above,
so its checks are shown and assessed like any other. tC4 writes every verse it seeds or edits as
`\v N …` at the start of a line, so the two agree on every book tC4 wrote.
`fixtures/usfm/check-text-vectors.json`, made by `crates/tc-usfm/tools/gen-check-text-vectors.mjs`
with tC4's own `verseTextIndex`, checks the rule and the shown text on every verse of tC4's
plain drafts and on drafts written for poetry, lists, notes, superscriptions, CRLF line ends
and markers inside a line. A decision on a
frame reads the frame's text: its `Frame` record, else the template's part of the story; a
story with no template has no text.

The document's `resource` record is tC4's `resolutionRecord`: the resolved pin's `repoPath`,
`version` and `sha` (no `sha` for an unusable pin) and its `languageSet`. Decisions are sorted
by the canonical JSON of `contextId` (tC4). `MovedTo` records project nothing at the old key.
Restoring a verse's earlier text makes its decisions valid again; nothing was rewritten. A
decision sidecar exists for a (tool, book) that has decisions, and `OBS.json` for a tool with
decisions on frames; an alignment sidecar for a book that has alignments; a comments file for a
book that has comments, and `tc/comments/OBS.json` when frames or frame checks have comments.
A comment there is placed at its frame (`placed: {frame, story}`, plus `check` for a comment
on a check) while the story holds a text for the frame (its `Frame` record, else the template's
part), else `null` and listed under Detached; sorted by (story, frame, at, id).

**Fields kept for decoding.** Every decoder is complete (§13.6), so each sidecar record also
holds what the journal stores beyond tC4's shape, after tC4's fields: an alignment's `done`,
its `source` pin, `markedInvalid` and `dismissed` when true, `anchor` (its key's chapter and
verse) when it is placed at another verse, and `spanEnd` when the label does not say it; a
decision's `userStatus` (the person's status, which `status` hides once invalidated),
`verseTextMd5`, `pinnedResource` when its `resource` differs from the document's,
`importedInvalid` and `dismissed` when true, and `checkKey` when its key is not the one its
`contextId` names (check id, reference and occurrence). Unknown fields of a record follow. A
decoder ignores the computed flags (`invalid`, `invalidated`, `verseEdits`, `userInvalid`,
`modifiedTimestamp`) and reads `status` as the person's only when no `userStatus` is written
and the record is not invalidated (§13.6 U3). The document's `resource` is computed too: a
decoder reads a decision's resource from its `pinnedResource`, else from the document's
`resource` when that names a commit; when the resolution is an unusable pin it names none,
and every decision carries its `pinnedResource`.

**Other derived files.**

- `checking/resources.json`: `{"schemaVersion":2,"languageSets":{"primary","fallback"},
  "resources":{"originalLanguage","lexicon"},"extraScripture":[…]}` from the `Pins` groups, slots
  in tC4's order (`gatewayLanguage`, `translationNotes`, `translationWordsLinks`,
  `translationWords`, `translationAcademy`, `translationQuestions`, `simplifiedText`, `nt`, `ot`,
  then the rest by name) and each slot's members in tC4's order (`languageId`, `owner`,
  `repoPath`, `version`, `sha`, `flavor`, `books`, `id`, then the rest by name); each
  `ResourcesExtra` member at its level after the modelled members, in name order; two-space
  indentation.
- `checking/settings.json`: `{"schemaVersion":1}` then the `Setting` records in key order,
  two-space indentation.
- `vrs.json`: the `Versification` bytes.
- `tc/comments/<BOOK>.json`: `{"schemaVersion":1,"book":"TIT","comments":[…]}`, each comment
  `{id, target, placed, text, author, at}`, where `placed` is the verse or check the comment is
  placed at by the anchor rules above (`{chapter, verse}`, plus `check` for a comment on a check:
  the live key at the end of its `MovedTo` chain; `null` when detached), sorted by (chapter,
  verse, at, id); comments on checks go with the check's book.
- `tc/conflicts.json`: `{"schemaVersion":1,"conflicts":[…]}`, each `{id, key, alsoKeys?, kind,
  provisional, candidates: [{change, device, author, at}]}`, one conflict per line in the
  sidecar byte form. It travels in commits and exports, so open questions travel with the text.
- `content/NN.md` (OBS): `StoryBase` with `Frame` and `StoryRef` spliced byte-strictly: the
  template's first `# ` line is frame 0, the paragraph after its n-th image line is frame n, and
  its last `_…_` line is the reference; a part with no record keeps the template's text.
- Carried files: the provisional head's blob at the path; each other conflicting head's blob at
  `<stem>.conflict-<first 8 hex of its oid>.<ext>`. A carried path that collides with a derived
  path or another carried path (§3.3) is placed at `<stem>.case-<first 8 hex of its oid>.<ext>`
  and listed as a `PathCase` conflict, with *Rename* and *Remove* choices. A carried record
  takes part in collisions when any of its heads is `Present`, not only its provisional head,
  so whether a `PathCase` conflict is listed never depends on a clock (I3).

**Sidecar byte form.** The tC4 document shapes are kept, but the bytes are compact JSON with one
record per line: the document opens on the first line, each record is one line ending in `,`
except the last, and the closing brackets are on the last line. Keys inside a record follow the
order of the tC4 samples. Readers of the tC4 format parse JSON regardless of whitespace; git
deltas stay small and Door43 shows one changed line per changed record.

### 7.8 Incremental fold and snapshots

The actor keeps the fold in memory in copy-on-write containers (§2.6), so a snapshot for a
sync, an upgrade plan or an export is O(1).

- **Local change** c on key k: `heads(k) := heads(k) − parents(c) + {c}`; recompute the state of
  k, its passage group (§7.3), the coherence of its chapter, the placement of the attached
  records of its verse, and the removal visibility of its book; mark its derived files dirty. O(history of k), normally O(1).
- **Incoming set** of m changes: add them, complete any held batches, recompute only the touched
  keys and chapters. O(m + Σ history of the touched keys).
- After every intake and once after each open, a background thread folds a snapshot from
  scratch and compares the frontier and the conflict list with the incremental state. A
  difference is a bug: the full result is installed and an anomaly is logged. The property tests
  assert equality (§16.1).

### 7.9 Why the invariants hold

- **I1.** F1 to F6 are functions of the set. F7 and F9 use only values, `at` and ids stored in
  the changes. F11 iterates keys in a fixed order and serializes deterministically.
- **I2.** A folded change is either a head or has a child on its key (state c). A head is
  projected (a), a candidate (b), a record of a removed book its remover held (d), a record of a
  book no presence change mentions (e, `RecordsWithoutBook`), or an absorbed seed stub (f). A
  projected attached record is placed on a screen or listed under Detached (§7.7). A head
  rewound for a passage's provisional side is a candidate of that `Passage` conflict (b). A
  change not folded is in a listed anomaly (e). A comment is always a head.
- **I3.** Parents name content hashes, so a parent can only be named by a device that held it.
  Nothing in F3 to F6 compares devices or clocks.
- **I5.** Every step is total; every rejection is local to one file or one change.

---

## 8. Conflicts and resolution

### 8.1 Kinds and grouping

*Owner ruling H5 (2026-10-24):* only verse text and story frame text conflict, with the kinds that hold them (verse text and structure, passage, structure issue, frame, book header, chapter front, story base and reference), the book restore race (§7.6) and `Unknown`. Every other kind of this section settles on its newest head (I3) with no conflict listed (`AutoRule::Newest`); the older heads stay in History. A carried file keeps both versions (§11.4); in a path-case clash one keeps its name by byte order and the other is placed beside it, so a newer version never swaps the names on a disk that ignores case.

| Kind | Raised when |
|---|---|
| `VerseText` | verse heads are all `Text` with the same `last`, and a unit is disputed |
| `VerseStructure` | verse heads disagree on `last`, `Covered` or `Absent` |
| `Passage` | a verse diverges from a passage batch (§7.3): one conflict over every verse of the passage and of any passage it overlaps; it replaces the `VerseText` and `VerseStructure` conflicts of those verses |
| `StructureIssue` | the projection of a chapter is incoherent (§7.5), even with one head per record |
| `BookHeader`, `ChapterFront`, `BookPresence` | two different heads |
| `BookRemovedWhileEdited` | §7.6 |
| `LastBookRemoved` | §7.6 |
| `Alignment` | two different alignments of one verse |
| `Check` | a disputed decision unit |
| `CheckMoved` | a carry-over moved a decision while someone changed it (`MovedTo` against `Decision`) |
| `Comment` | two changes under one comment id (only by forgery or a broken random source) |
| `Pins`, `Setting`, `ProjectMeta`, `Versification` | a disputed unit |
| `StoryBase`, `Frame`, `StoryRef` | two different heads (OBS) |
| `CarriedFile` | two versions that replaced the same version of a file the app does not derive, or a version that replaced none beside another |
| `PathCase` | two paths that differ only in letter case (§3.3) |
| `Unknown` | two values of a record kind from a newer format; listed, resolved by the newer app |

**Verse conflicts.** `VerseText`, `VerseStructure`, `Passage` and `StructureIssue` are the
verse conflicts (`ConflictKind::is_verse`); every other kind is another conflict. A sync or a
file import counts the two classes apart (§8.5, tC4's toast).

**Grouping.** The review lists conflicts grouped by the set of batches that produced their
heads. One upgrade against another becomes one row: "Maria upgraded translationNotes to v92 on
12 Oct; Juan upgraded to v93 on 13 Oct (1 pin group, 214 checks)". All the conflicts one
import batch made (§13.6) form one row, "Imported from TIT.usfm (changed outside
translationCore, 21 Oct): 12 verses in conflict", with *Accept all from the file*, *Keep
translationCore's text* and *Review one by one*. Resolving a
row writes one batch. A conflict marked `incomplete_history` says so on its row (§7.4) and offers every choice of its kind.

### 8.2 Editing a record that is in conflict

The token of a conflicted record lists only the displayed candidate (§2.7). Typing into a verse
marked "2 versions" therefore continues the displayed version; the other version stays a head
and the conflict stays open. The editor offers *Edit and resolve*, which opens the resolution
screen with the editor beside every version; its save is a resolution. A merged record shows
one value that stands for all heads, so an edit of it replaces all of them.

### 8.3 Choices

*Owner ruling H5 (2026-10-24):* the choices below for kinds that now settle by the newest (§8.1) are no longer offered: *Restore this version* in History brings an older version back.

Every choice is one `Resolve` batch whose changes name every current head as parents (§5.9).

- **Verse text:** *Keep this version* on each candidate; *Edit* (the verse editor prefilled with
  one version, the others beside it); *Combine*, offered only when the candidates' word diffs
  against a single base touch disjoint ranges: the core builds the combined text, the person
  sees it and confirms it; *Later* writes nothing.
- **Verse structure and structure issues** are described in words, not markers: "Maria joined
  verses 4 and 5 into 4–5: '…' · Juan changed verse 5: '…'". Each choice writes every verse in
  the range in one batch:
  - *Keep the joined verses 4–5*: verse 4 = Maria's span text, verse 5 = `Covered { by: 4 }`;
    Juan's text of 5 is shown for copying and stays in History.
  - *Keep 4 and 5 separate*: verse 5 = Juan's text; verse 4 = `Text { last: 4 }` with the words
    verse 4 had before the join (the base), in an editor so the person can adjust it. No
    duplicated words are left for the person to find.
  - *Edit the passage*: the section editor with every version beside it.

  A choice keeps the version the person picked exactly. Where a verse of the range holds
  two concurrent versions of its own text (two people changed it), or the range's first verse
  two spans, the choice asks which one to keep, from the candidates or from History; no clock
  picks what is kept (I3).
- **Passage:** described as what each side did to the passage, in words: "Maria renumbered
  Titus 1:16–2:15 (each verse moved one number later). Juan changed the verse that was 2:2
  before the renumber (its text is now at 2:3)." or "Maria moved 'C' from verse 5 to verse 4.
  Juan changed verse 4." Each choice writes every record of the group in one batch:
  - *Keep Maria's passage*: her side's values (with her later edits);
  - *Keep Juan's*: Maria's side rewound (§7.3) plus Juan's changes;
  - *Combine*, offered when the word diffs of the sides against the base, taken over the
    passage's text with its verse markers, touch disjoint ranges: the core builds the combined
    passage ("Juan's change applied to 2:3, where that text now is"; "'A B′ C' in 4, 'D E' in
    5"), the person sees it and confirms it;
  - *Edit the passage*: the section editor with every side beside it.

  *Keep* on a candidate keeps that side and writes exactly that candidate on its record. A
  side can hold two different versions of one record (Juan and Ana each changed verse 5 while
  Maria joined 4–5: both are "the others"), and a record the kept side did not write can keep
  two versions of the others. For each such record the choice asks which version to keep
  (`keep_passage` takes one picked version per record); no clock picks what is kept (I3).
- **Book removed while edited:** *Restore Jonah* (`Present`, every record projects again);
  *Keep it removed* (a new `Removed` whose `seen` covers every current head, so the unseen work
  is now seen and stays in History); *Show Juan's changes* (read-only).
- **Book presence:** *Restore the text* / *Start empty* for a restore race (§7.6); *Keep Ruth*
  / *Keep it removed* for a book added on Door43 after a removal.
- **Last book removed:** *Keep Titus*; *Add another book first, then remove Titus*.
- **Alignment:** *Use this alignment*; *Open in the aligner starting from this one* (its save is
  the resolution); bulk *Dismiss alignments made for text that was not kept*. Each card says
  whether it was made for the current text (§7.7).
- **Check:** *Keep mine*, *Keep Ana's*, *Choose per line* (one value per disputed unit; every
  unit that merged cleanly keeps its merged value, whichever head it came from); for a check
  comment, *Keep both* joins the texts with the authors' names.
- **Check moved:** "Ana's upgrade moved this check to the new note; Pedro changed the old one
  (status Invalid, comment '…')". *Apply Pedro's change to the moved check* writes, in one
  batch, the new key with Pedro's changed units (his values for the units he changed relative to
  the base, applied to the new key's value) and the old key's `MovedTo` as resolution. The units
  are those of §7.3 (judgement, comment, bookmark, dismissal, basis, each extra field). When
  several people changed the old check while it moved, every one's changed units are applied;
  when two of them changed one unit differently, when two moves name different checks, or
  while the new key is itself in conflict, the choice is refused until a version is kept, so
  no clock picks a verdict (I3). *Keep
  Pedro's check where it was* keeps the old key's decision (it computes as invalid against the
  new resource) and leaves the moved copy.
- **Pins:** *Use v92* / *Use v93*, with the consequence counts from the carry-over computation
  ("With v92: 1,204 checks valid, 37 to check again"). The choice runs the plan of §5.8 for the
  chosen value; the pin resolution, the carry-over and the grouped check resolutions are one
  batch. If the chosen version is not installed, *Download* comes first.
- **Settings, project details, book header, chapter front, OBS frames:** *Keep this one*; *Edit*.
- **Comment** (two changes under one comment id): *Keep this one*; *Keep both* joins the texts
  with the authors' names. The resolution is the one comment change with parents (V6).
- **Device profile** (two versions of one device's profile, only from a duplicated device):
  *Keep this one*, written by that device (V4).
- **Carried file** (carried-files ruling rule 3): *Keep this one*, on each version: the other
  version's file is removed from the folder, and the change names every head. *Keep both*: the
  other version becomes a new `CarriedFile` record at its own path, named
  `<stem>-<Door43 user>.<ext>`, for example `jon_01-Juan.wav` (`rulings/2026-10-22-H22.md`
  part 2: the ruling's example `jon_01 (Juan).wav` breaks the path grammar of §3.3). The user
  part is the Door43 username of the other version's device, cleaned as an export file name is
  (§10.1 E4), or its display name when it has none; `-2`, `-3` are added when the name is taken;
  the target must be a free path (`FreePath`).
- **Path case:** *Rename* one of the files (a new `CarriedFile` record at the new path and
  `Removed` at the old one, one batch); *Remove* one.
- **Unknown kinds:** listed with "Made by a newer translationCore. Update to review."
- **Detached** (not a conflict, a list in the same review, §7.7): *Show*; *Open in the aligner
  at…*; *Dismiss*.

History is the other half of every choice: every record has *Versions*, each with author, date
and cause (edited, kept in a conflict, restored, carried over, imported from a file, imported
from tC4 or tC3), and *Restore this version* (§5.9). A carried file's version shows its number
(its depth in the chain of versions it replaced), its Door43 user (the profile of the device
that made it, shown only after D98 consent, otherwise "translationCore user") and its time;
*Restore this version* stays limited to versions whose bytes are on this computer (§5.9).

### 8.4 How resolutions travel

A resolution is an ordinary change in an ordinary pack. It travels by Door43 and by USB like
everything else.

- On any device that holds the resolution and the heads it names, those heads gain a child and
  stop being heads. A device that has the resolution but not yet one of the heads sees the
  resolution as the head; the missing head arrives later already superseded. The result is the
  same.
- Two people who resolve one conflict differently produce two heads again: a conflict, labelled
  "resolved differently by Maria and Juan", whose base is the old conflict's heads.
- Two people who choose the same value produce equal heads, which merge.
- A teammate who edited one of the old heads at the same time keeps a head the resolver never
  saw, so the conflict stays with exactly that version.
- A resolution needs no ancestry condition, so every conflict can always be resolved (tC4 F12).

### 8.5 What the person sees (requirements on the UI)

*Owner ruling H5 (2026-10-24):* the Received sheet says what a newer version replaced, for example "Juan's alignment of Titus 1:1 replaced yours. Your version is in History."

- Nothing is modal and nothing blocks work (tC4 D53e). After a sync or a file import, a sheet
  says "Received 212 changes from Juan and Ana. 3 verses, 1 alignment and 2 checks were changed
  by more than one person. [Review now] [Later]" and lists notices in plain words.
- The Home card shows "N conflicts · Review"; book tiles show counts. A conflicted verse,
  alignment or check item carries a "2 versions" marker in Translate, Align and Check.
- The review screen has tabs by kind (Text, Alignments, Checks, Settings and sources, Detached,
  Notices), "Conflict n of m", Previous, Next and Back to work. Versions are coloured by person:
  "Mine" light blue, teammates cream (tC4 #679). Each version shows the person, the computer
  when one person has two or when two versions share one username (both are then labelled by
  computer, never both "Mine"), the date, and "this computer's clock may be wrong" when
  `ClockAhead` applies. One function names the person on the review and on the sync result,
  so one teammate has one name or one number in both (`FoldState::person_labels`): "Mine"
  (`conflicts.mine`) for this device's person, that is this device and every device signed in
  with its Door43 username; else the Door43 username, devices grouped by it; else the display
  name; else "Teammate n" (`conflicts.teammate`), numbering the devices with neither in the
  order of their ids, leaving this device out. Only Local, tC4-actor and tC3-user devices are
  persons; a Door43 edit, an exchange-file edit or an import is named by its cause.
- Every anomaly is one sentence and, where possible, one button: "1 file of changes from Juan's
  computer is damaged; 212 of 230 changes were recovered. A good copy from Door43 or any
  teammate repairs it; the recovered changes are kept either way." The words never promise a
  repair by a person or device that may never sync again.
- Export and print warn while conflicts are open: "3 verses have two versions. The export uses
  the version shown. Review first?"

---

## 9. Door43 sync with git2

Crate: `git2 = { version = "=0.21.0", default-features = false, features =
["vendored-libgit2"] }` in `tc-store` (libgit2 1.9.7, probed in the research). TLS comes with
`tc-door43`, the only crate that turns on git2's `https` (OpenSSL built from source on Linux),
not with `tc-store`. Network calls (`tc_door43::git::{fetch, push, ls_remote}`) live in
`tc-door43` behind the internet switch; they receive the `git2::Repository` that `tc-store`
opened and the `tc_door43::Http` of the session's Door43 client. Each call is refused before
any request while the switch is off, checks it again in libgit2's callbacks (credentials,
transfer progress, push negotiation, side band), and goes through the proxy that
`Http::proxy_for` names, the same PAC/WPAD finder every HTTP request uses, never libgit2's own
lookup. A pack already being pushed cannot be stopped; the last check is push negotiation. `tc-sync` orchestrates. The app never
calls `checkout`, `merge`, `rebase` or `stash` on a project, and git's content merge never runs.

### 9.1 Steps

`sync()` runs on a worker thread. Steps marked **[actor]** are short messages to the project
actor; saves keep flowing between them (I9).

```
S0  Gate. The internet switch is on, else refuse `sync.offline` before any request (tC4 D95;
    "Not now" sends nothing). A token is in memory or the keychain, else ask to sign in.
    `origin` is on the configured Door43 host and has no credentials in the URL. The project is
    not read-only. b = git config `tc.branch` (set at share or join from the remote's HEAD
    branch, so a repository whose default branch is `master` syncs).
S1  [actor] Checkpoint "before sync" (§5.13).
S2  attempt = 1.
S3  Fetch:
      let mut cb = RemoteCallbacks::new();
      cb.credentials(|_url, _user, _allowed| Cred::userpass_plaintext(&username, token.expose()));
      // tc_door43::git offers the token once: libgit2 asks again after a refusal, forever.
      cb.transfer_progress(|p| { progress(p.received_objects(), p.total_objects()); true });
      let mut fo = FetchOptions::new();
      fo.remote_callbacks(cb).download_tags(AutotagOption::None).proxy_options(/* Http::proxy_for */);
      origin.fetch(&[format!("+refs/heads/{b}:refs/remotes/origin/{b}")], Some(&mut fo), None)?;
    Errors: ErrorCode::Auth or HTTP 401 → `sync.auth-failed`; 404 → `sync.remote-missing`;
    a network error → `sync.unreachable`. Nothing local has changed. Fetch sends credentials,
    so private repositories work.
    R = refs/remotes/origin/{b}, or None for an empty repository. Door43's advertised refs
    (`ls_remote`) say whether b is there: a repository with history but no b had it deleted
    by its owner, and S8 pushes this copy as b again, as tC4 does, with the report's
    `recreated` for the notice "The project was missing from Door43, so tC4 put your copy
    back. Ask your teammates to sync." (owner ruling, `rulings/2026-10-24-H5.md`).
S4  If R is None, R == HEAD, or repo.graph_descendant_of(HEAD, R) is Ok(true): go to S8.
    An error from graph_descendant_of (a shallow history) counts as "not a descendant".
S5  Intake of R (§9.2), on the worker. It reads the object database only.
S6  [actor] Integrate (§9.3).
S7  [actor] Merge commit: seal and materialize (§5.13 C1, C2; this seals the intake batch and
    any saves made since S1), build the tree from the fold (§9.4), validate it, and commit with
    two parents instead of C5's one:
      repo.commit(Some("HEAD"), &sig, &sig, "Sync with Door43 (tC)\n\n<trailers>", &tree, &[&head, &r])
    The trailers are those of §5.13. The merge commit holds everything this device has, so
    every commit tree is a valid burrito whose derived files equal the fold of its packs.
    For a chain (§9.5) the second parent is the last link instead of r; it descends from r.
    Large pushes are split into a chain of such commits (§9.5).
S8  Push (§9.5).
S9  Confirm (§9.5).
S10 Repack if needed (§9.8). Report (`ExchangeReport`, §9.7).
```

### 9.2 Intake of R

```
N1  Format. Read R:ingredients/tc/format.json. A larger major, or a minReaderMinor above this
    app → stop: "A teammate uses a newer translationCore (project format 2). Update to sync.
    Nothing was changed." R has ingredients/checking/journal/ and no tc/format.json → it is a
    tC4 team repository: stop and offer the team move (§13.3). Missing or unreadable in R,
    with no journal → use the local format.json; the merge commit restores it, and the report
    says "format.json was deleted or damaged on Door43 and was restored".
N2  Project. If R:ingredients/tc/project.json is readable and names another projectId, refuse
    `sync.project-mismatch`: two projects are never merged (tC4 D53). Missing or unreadable in
    R is not a mismatch: continue with the local file, which the merge commit restores, and the
    report says "project.json was deleted on Door43 (by jdoe, commit 3fa2c1d) and was restored".
N3  Packs. For each file under ingredients/tc/changes/ in R's tree:
      - path not present locally → new. Read the blob; validate (§4.4). Every new file is taken,
        valid or not: a damaged one is salvaged (its verified lines are used) and recorded in
        .tc/quarantine.json by blob id so it is reported once; a newer-format pack is carried
        unread. Nothing in R's pack set is ever dropped from the tip by this device.
      - same path, different blob → at least one copy is damaged (two valid packs with one name
        have the same bytes). The copy that passes P2–P4 is kept, whatever the sealer
        directory, so the owner's next sync repairs a damaged copy on Door43 and Door43 (or
        HEAD, or the confirmed commit) repairs a damaged local copy. A pack that fails P2–P4 never
        enters a tree while a valid blob for its path exists anywhere in the object database.
        If every copy fails: the lower blob id stays in the tree, and the union of the lines
        salvaged from all copies is sealed through a `salvage` batch (§4.4).
      - redundant by the rule of §4.7 → not taken.
N4  Files changed on Door43 by another tool (§11.2): for each commit after the last app commit
    (one with the `Tc-Frontier` trailer) that changed a derived or carried file, one notice,
    remembered by commit id. Nothing is merged from it: the merge tree takes the fold's
    projection (§9.4).
N5  Newer derived paths: files R's format.json lists in derivedPaths that this app does not
    derive are taken verbatim from R (§12), except that a file whose path collides with a path
    this app writes (letter case, §3.3) is not written to the working tree: it stays in the
    object database and the commit tree, and the person is told.
N6  Foreign entries of R (§9.4) are noted for the merge tree; nothing is written for them.
```

### 9.3 Integrate

```
T1  For each file taken in N3 and N5: atomic_create_dirs; atomic_write into its path in the
    working tree, directory fsync deferred; then fsync each touched directory. A path that
    already exists on disk goes through the compare-and-swap of §5.10 M1 first, so an intake
    never overwrites a file someone else wrote. Each pack is new and immutable (or a repair of a
    damaged copy), so this is idempotent. A crash leaves some of them, each complete; the rest
    come with the next sync (batch completeness keeps every action whole). Then rewrite
    metadata.json (§5.10 M6), which closes this write group (I7).
T2  Raise the N4 notices (§11.2): one for each commit by another tool, remembered by commit
    id. No change is made for them (the salvage changes of N3 go through commit_received as a
    pending file with purpose `intake`, §5.0).
T3  Fold the new changes incrementally. The person now sees the received work and the new
    conflicts, before the push.
```

There is no scratch clone, no apply marker and no project lock. A sync's local effect is "add
immutable files, add one pending batch, commit".

### 9.4 Building a commit tree

`tc_store::git::build_tree(repo, fold, workdir) -> Result<Oid, StoreError>`:

| Entry | Bytes |
|---|---|
| `.gitignore`, `.gitattributes` | the fixed constants of §3.1 (an edit of them on Door43 is reset, with a notice) |
| derived files | the fold's projection (not the disk, so a file held open by another program does not leak stale bytes into a commit) |
| carried files and their conflict copies | blob ids from the `CarriedFile` projection; a head whose blob is not in the object database (or is there and its header does not read: libgit2 takes no such object into a tree, so it is checked for each entry the tree changes) is stored again from the working tree when the file there has the head's md5 and size; else it keeps the first parent's blob at that path (unchanged, so no reader sees a change), or is left out when the first parent has none, with `CarriedBytesMissing`; the commit is never refused for it |
| `ingredients/tc/project.json` | the local file |
| packs | the expected pack set (§4.4), minus redundant packs: every pack of the working tree and every non-redundant pack of HEAD's tree, valid, damaged and newer packs alike, except that a pack failing P2–P4 is replaced by a valid blob of the same path when one exists. A local loss can therefore never reach Door43 by fast-forward |
| newer-format derived paths | the working-tree file, verbatim (or the blob, for a path kept out of the working tree, §3.3) |
| foreign entries | verbatim, by raw git path, mode and blob (below) |
| `metadata.json` | built from the fold and the entries above, checked |

**Foreign entries** are the entries of the trees the app does not own: any path with a segment
starting with `.` other than `.gitignore` and `.gitattributes` (`.gitea/…`, `.editorconfig`,
and `.tc-moved/…` below); root files whose names break the grammar (`Style Guide.pdf`); and
anything else the app neither derives nor carries as a record. They come from the first parent and from R: an
entry of R is kept with R's blob (the app never edits them); an entry the first parent has and R
removed relative to their merge base is removed (a Door43 user deleted a file the app does not
own); otherwise it is kept. A file under `ingredients/` with no dot segment whose name breaks
the grammar (`ingredients/audio/Chapter 1.wav`) cannot be an ingredient, so the merge tree keeps
it at `.tc-moved/<its path>`, a dot path burrito readers skip, and every device that receives
such an entry lists it in Notices: "Chapter 1.wav was moved to .tc-moved/ because its name has a
space. Rename it to use it." Exchange files carry foreign entries under `.tc-root/` (§10.1).

**No entry of R is dropped silently.** After building a merge tree, every path of R that is not
in it must be accounted for: a redundant pack, a derived or carried path the projection does
not produce because of a change in the fold (a removed book, a removed carried file, no open
conflicts for `tc/conflicts.json`), or a foreign entry moved under `.tc-moved/`. Any other path of R is kept in the tree with R's blob and listed as a notice
(a bug report). A derived-pattern file of R for a book that is not present cannot reach this
check: intake decodes it into records that add the book (§7.6, §11.2 D2).

Blobs are written with `repo.blob(&bytes)` (loose objects, fsynced through
`core.fsyncObjectFiles`). The manifest keeps the blob of each derived file and each pack it has
stored (`blob`, §3.6), so a derived file the last materialization made equal to the projection
is never read or hashed again for a tree: only a file whose entry has no blob yet is read (its
md5 must still be the entry's, else the snapshot's projection is used), and a held, busy or
dirty path is projected from the snapshot. Every pack is stored as a blob when it is sealed
(§5.11 K3) or first needed by a tree, so a pack can be restored before it is committed. The
tree is HEAD's tree with the entries that differ set (`TreeUpdateBuilder`): only the directories
on the paths that changed are written again, and a file that is as in HEAD is neither read nor
written. Each blob id is checked to exist before it is inserted (libgit2 refuses missing
objects), and a missing one is handled by the rules of the table, never by failing the commit.
`metadata.json` of the tree lists the md5 and size of the bytes in the tree. The validator
(`tc_burrito::validate_project`) reads the entries through a `ProjectFiles` over the tree's
entries and runs before the commit is written (I7). A foreign entry under `ingredients/` whose
name can be no ingredient moves to `.tc-moved/`.

Because every tree comes from the fold, a git conflict cannot happen and conflict markers never
reach a file. Histories that share no ancestor (a project received by USB, then connected to
Door43; a `.git` rebuilt after damage) need no special case: the tree never depends on a merge
base.

### 9.5 Push and confirmation

```
let rejected = RefCell::new(None::<String>);
let mut cb = RemoteCallbacks::new();
cb.credentials(/* as in S3 */);
cb.push_update_reference(|_refname, status| {
    if let Some(msg) = status { *rejected.borrow_mut() = Some(msg.to_owned()); }
    Ok(())
});
let remote_text = RefCell::new(String::new());
cb.sideband_progress(|text| { remote_text.borrow_mut().push_str(&String::from_utf8_lossy(text)); true });
cb.push_transfer_progress(|cur, total, bytes| progress_push(cur, total, bytes));
let mut po = PushOptions::new();
po.remote_callbacks(cb).proxy_options(/* Http::proxy_for */);
let res = origin.push(&[format!("refs/heads/{b}:refs/heads/{b}")], Some(&mut po));
```

- `Ok` and no rejection → confirm. A rejected ref is never a success, though `push` returned
  `Ok`.
- A rejected ref goes through `tc_door43::refused_push(repo, status, &remote_text)`. It keeps
  Door43's own lines of the side band and drops git's progress, such as the `Resolving deltas:
  …` that `git receive-pack` writes there for a pack above 100 objects, because libgit2 never
  asks it to be quiet.
- `ErrorCode::NotFastForward` or `Door43Error::RefMoved`: someone pushed after S3. libgit2
  compares the push with the advertisement it has just read and refuses a non-fast-forward
  itself, before it sends anything (`NotFastForward`). A race lost on the server comes back as
  a ref status (`RefMoved`): `failed to update ref` from git before 2.51, with `cannot lock ref
  … is at X but expected Y` on the side band; `incorrect old value provided`, `reference already
  exists` (also for a branch another push holds locked) or `reference does not exist` from git
  2.51 on, with nothing on the side band. Door43 advertises git 2.54. Git 2.51's `failed to
  update refs` means the server could not write the ref at all, so it stays `RefRejected`
  (`tc-door43` test `a_push_that_loses_a_race_is_ref_moved_and_keeps_the_teammates_commit`).
  `attempt += 1`; after 6 attempts refuse `sync.busy` ("Door43 is busy with other syncs. Your
  work is saved on this computer. Try again in a minute."). Otherwise wait `min(2^attempt, 16) s
  × U(0.7, 1.3)` and go back to S3. A retry costs one incremental fetch,
  the intake of the few new packs and one tree build, about a second, not a new clone.
- `Door43Error::NoWriteAccess` → `sync.no-write-access`, offering "Save an exchange file
  instead" (§10). Door43 refuses a push from an account that may read the repository but not
  write to it inside an HTTP 200: the ref's status is `pre-receive hook declined` and the side
  band says `User permission denied for writing.` (measured 2026-10-18,
  `docs/evidence/door43-live-2026-10-17.md` step 6).
- `Door43Error::RefRejected` (a protected branch, another hook) → `sync.ref-rejected` with
  Door43's own text.
- An `Err` of class `ErrorClass::Http` with the message `unexpected http status code: 403` →
  `sync.no-write-access` (`NoWriteAccess` with that message), offering the exchange file. Gitea
  answers 403 to the receive-pack request itself, before any hook runs, for a token without
  `write:repository` (as `tc-door43-fake` does) and for a repository that takes no push, such
  as a mirror. libgit2 passes neither the status code nor the body, only that message, and
  never reports a 403 as `ErrorCode::Auth` (`tc-door43` test
  `a_403_on_push_is_an_http_error_and_makes_no_branch`). The app pushes only with the token it
  minted with `TokenScope::FOR_SHARING`, which holds `write:repository`, so the 403 is the
  repository's: a new sign-in would mint the same scopes and meet the same 403.
- `ErrorCode::Auth` or HTTP 401 → `sync.auth-failed`.
- Never a force push.

**Confirm** (tC4 F11): `origin.connect_auth(Direction::Fetch, Some(cb), Some(proxy))`, then
`origin.list()`, and find `refs/heads/{b}`. Equal to the pushed commit → set
`refs/remotes/origin/{b}` to it and `atomic_write` `.tc/sync.json` with `confirmed: {commit,
at}`. Only now does the project card say "Your work up to 14:32 is on Door43". Otherwise:
"Sent, not confirmed".

**Push cost.** libgit2 1.9.7 never builds thin packs on push (`preferred_base` is never set in
`pack-objects.c`; `push.c` only hides remote refs), so each push carries a full zlib copy of every
changed derived file and of `metadata.json`. Compact sidecars (§7.7) and a bounded file count
(§4.7) keep a daily push at about 0.3–1 MB (§14).

**Large pushes are a chain.** A push cannot resume after a dropped connection, so a resource
upgrade over a checked Bible (one batch of about 150 MB, several packs and every decision sidecar)
could fail forever on a poor link. When the new blobs of a merge or checkpoint commit exceed
16 MiB, S7 (or C5) builds a chain of commits instead of one. Each link before the last is the
tree of the commit it grows from (HEAD for a checkpoint, Door43's commit R for a merge, so every
link is a fast-forward of R) with more of the new packs, in path order, and the `metadata.json`
that lists them; a link is cut once it adds 16 MiB, or one pack when a pack is larger. A link
takes a pack only while its packs fold to the same changes as its base's (no batch with a change
only the new packs hold is whole within them, and no release names one;
`FoldState::completes_batch`), so it keeps its base's derived files, and they are the fold of its
own packs. A pack that would make a batch whole waits for the last commit, which adds the rest:
derived files, sidecars and every pack left. A base without the `Tc-Frontier` trailer (another
tool's commit) gets no chain, since its derived files need not be the fold of its packs. In
practice the chain splits an action that spans packs, such as a resource upgrade; new packs whose
batches are each whole (an exchange file of many small saves) stay in one commit.
Every commit of the chain is a valid burrito whose derived files equal the fold of its own packs,
so readers and the intake check of §11.2 treat each one like any app commit; a link's trailers
are its base's. S8 pushes the history between R and HEAD as links: on the way from R to HEAD
(through the parent that descends from R), every commit whose new blobs bring the run since the
last link to 16 MiB is pushed from the local ref `refs/tc/push-link`, then read back with
`ls_remote` and set as `refs/remotes/origin/{b}`, before the next; a link read back elsewhere is a
lost race. This also splits the first push of a long local history. Each confirmed link is
progress that a dropped connection cannot undo, the report's `confirmed` names the last one, and
the next sync continues from there. After three consecutive `sync.unreachable` failures of one
chain (counted in `.tc/sync.json` `chainFailures`; saves between the tries do not reset it, a
confirmed push does), the report's `offer_exchange` offers "Save an exchange file instead" (§10). A push
with no links counts only after a chain failed before it (the last commit, once every link went
through). When the branch had been deleted on Door43 and a failed chain made it again with its
first links, `.tc/sync.json` `putBackUnsaid` keeps the notice of owner ruling H5 for the next sync
that gets through, which says it once.

### 9.6 Share (J11), join (J17) and connecting a project

**Share** (first push):

1. Sign in (the token in memory; in the OS keychain only with "Stay signed in"). Show the author
   notice (D98) before a Door43 username is written into this device's `DeviceProfile`.
2. Choose the account or an organization with create permission, and the repository name
   (default: the folder name).
3. `POST /api/v1/user/repos` or `/api/v1/orgs/{org}/repos` with `{name, private,
   default_branch: "main", auto_init: false}`.
4. Add `origin` without credentials in the URL; set `tc.branch = main`; write `remoteUrl` and
   `branch` to `.tc/sync.json`.
5. Checkpoint, push (§9.5), confirm.

Refusals: the name exists; offline; auth failed; create rejected (with Door43's text).

**Join** ("Get a project from Door43"):

1. In `<projects>/.staging-join-<16 hex>/` (§6.4 N1): `Repository::init`, set `core.autocrlf=false`,
   `core.fsyncObjectFiles=true`, add `origin`, fetch the remote's HEAD branch with credentials,
   set `tc.branch`. Full history by default; a depth-1 fetch only after the probe of §16.5 shows
   fetch, commit and push from a shallow clone working against Gitea.
2. Format gate and project id (N1, N2).
3. Write the working tree from R's tree with `atomic_write` (no checkout, so no line-ending
   conversion can touch a byte), except foreign entries and paths that collide (§3.3), which
   stay in the object database and the tree only; set the branch to R, `index.read_tree`.
4. Mint the Local device, fold, materialize, validate (the §11.2 walk is dropped: H18 B lists
   another tool's commits instead, §9.2 N4).
5. Rename the staging folder into `<projects>/` (§6.4 N1, N2).

A project can also be joined from an exchange file (§10.2), which is smaller than a full clone.

**Connect** a project received by USB to an existing Door43 repository: add `origin`, then sync.
The project ids must match (N2); the first merge commit joins two unrelated histories.

### 9.7 Exchange report

As built (unit X): `tc_sync::SyncReport { ended, received, confirmed, raced, received_by,
new_conflicts, other_tools, recreated }` and `tc_sync::exchange::ReceiveReport { intake,
received_by, new_conflicts, damaged, hand_edited }`; the refusals are tc-api's `SyncFailure`
(`Blocked(SyncBlock)`, `BeforeIntake(SyncRefusal)`, `AfterIntake(SyncRefusal)`). The sketch
below is the design's first shape.

```rust
pub struct ExchangeReport {
    pub source: ExchangeSource,                         // Door43 { url, before, after } | File { path, exporter }
    pub received: BTreeMap<PersonLabel, u32>,           // new changes per person (devices grouped by username)
    pub sent: u32,
    pub new_conflicts: Vec<ConflictId>,
    pub resolved_elsewhere: Vec<ConflictId>,
    pub merged_without_conflict: u32,
    pub outside_edits: Vec<(RepoPath, ExternalSource)>,
    pub anomalies: Vec<Anomaly>,                        // incomplete batches, duplicate devices, …
    pub notices: Vec<Notice>,                           // damaged files, identity changes, clock warnings
    pub confirmed: Option<(GitOid, WallTime)>,
    pub outcome: Result<(), Refusal>,
}
pub enum Refusal {
    Offline, NotSignedIn, AuthFailed, NoWriteAccess { server_message: String },
    RefRejected { server_message: String }, Busy { attempts: u8 }, RemoteMissing, Unreachable,
    ProjectMismatch { theirs: ProjectId }, NeedsNewerApp { major: u16, min_reader_minor: u16 },
    Tc4TeamRepository, ReadOnly, Io(String),
}
```

### 9.8 Repack

libgit2 has no `gc`. When `.git/objects` holds more than 2,000 loose objects or 64 MiB of them, the
writer (§5.0) repacks: once after the open, and after a checkpoint's commit is reachable (the
actor moved the branch, §5.13 C5), never during a sync. The loose objects are counted by listing
the object folders once, at the first check of a session; what the writer writes after is added,
so a checkpoint does not list them again. The repack runs on a thread of its own: neither a save
nor a materialization waits for it, and the project's close never does. It starts only when the
project is not closing, and a run that is going when the person closes is stopped (libgit2 asks
a progress callback twice a second while it looks for deltas), removes nothing it has not
packed, and is finished by the next repack. One run packs at most 96 MiB of loose objects
(counted before compression) with two threads; the rest is still due at the next check. The
search for deltas between the versions of a derived file is what keeps three years of history
near 400 MiB, and it took 26 s for the loose objects of 250 checkpoints of a whole Bible.

1. Walk every reference under `refs/heads/`, `refs/remotes/` and `refs/tc/` (and a detached
   HEAD), and collect the commits, trees and blobs they reach. A walk that fails (a reference that
   does not read, an object that is missing) stops the repack: nothing is pruned unless every
   reference was read. No reference keeps replaced carried versions (carried-files ruling rule
   2): a replaced version that no commit reaches may be pruned after the 14 days of step 3. A
   version that is still a head (in conflict) stays: its file is in the folder, and a checkpoint
   commits it within 30 minutes (§5.13). An existing `refs/tc/carried` (only projects made by
   development builds have one) is deleted at the next open.
   Storing a carried file's bytes holds off the removal of unreachable objects as a commit does,
   so bytes stored again are never removed by a repack that listed their old loose file.
2. `repo.packbuilder()`; insert the loose objects the walk reached that no pack holds, and write
   the pack and its index into `.git/objects/pack/`. The history already in packs is not written
   again, so a repack adds the size of the new objects to the disk, not the size of the history.
   With `core.fsyncObjectFiles=true` (§6.1 O3) libgit2 flushes the `.pack`, the `.idx` and their
   folder itself (`git_packbuilder_write`, `indexer.c`), so the app flushes nothing here. The
   index is read back and must name every object about to lose its loose file.
3. Delete the loose objects that are now in a pack (and the loose copies of objects a pack held
   already), and the unreachable loose objects older than 14 days; remove the two-letter folders
   this empties. A file that cannot be removed now (Windows keeps loose objects read-only, and a
   scanner may hold one) is removed by a later repack.

A repack that stops between steps 2 and 3 leaves objects twice and never fewer (§6.4 R1): the
next repack finds them in a pack and only removes the loose copies. An unreachable object younger
than 14 days is neither packed nor removed.

**A commit being made.** The walk decides once what is reachable, and the unreachable objects
older than 14 days go later; a checkpoint's tree can name an old object nothing references (a
blob an abandoned checkpoint stored, which the manifest or `ensure_blob` names again) before the
actor has moved the branch to the commit. Three things keep it:

- `ensure_blob` writes through the object database when the object exists, which only brings its
  time up to date (libgit2 does what git does) and counts no second write.
- The writer marks the span from the start of the tree build to the branch's move (the actor ends
  it), and every handle on the repository
  shares the mark. A repack removes unreachable objects in batches of 128, each batch under the
  lock the mark is kept under, and a batch is removed only if no commit is being made, none was
  being made when the run started and none has begun since. Beginning a commit waits for the
  batch that is being removed, never for the run, and every batch after it finds the commit and
  stops the removals; what a commit names (a blob it takes from the manifest without writing it
  again, one `ensure_blob` freshened) is therefore never removed after the commit began, and what
  went before it was gone when the tree build looked.
- Before each unlink the repack looks at the file's time: an object written again since the run
  listed it (a blob stored outside a commit, such as a carried file's) is left.

The objects that stay are pruned by the next run.

### 9.9 Editing during a sync

The worker never holds the project. A verse that receives a teammate's change while it is open
in the editor shows "Juan changed this verse while you were typing. Your text will be kept as a
second version." The pending save still carries the token of what the editor displayed, so it
becomes a conflict, not a revert (tC4 F5, F14). Saves that land before S7 are in that sync's
merge commit; later ones go in the next.

---

## 10. USB and file exchange

An exchange file is a Scripture Burrito zip. It serves a teammate (merge), a newcomer (create
the project), a backup and any other burrito tool.

### 10.1 Export ("Save work for a teammate")

```
E1  [actor] Checkpoint "before export".
E2  Entries from HEAD's tree: metadata.json first, then every path under ingredients/ in path
    order (derived files, every pack this device holds from every teammate, carried files),
    with explicit directory entries (Pankosmia needs them, PN #29). Carried files at the
    repository root and foreign entries (§9.4) are stored under `.tc-root/<path>`, a dot path
    that burrito readers skip. Never .git, .tc, .gitignore, .gitattributes. Entry names use
    `/`. Deflate level 6.
E3  Zip archive comment (informational; import works without it):
    tc-exchange 1 {"project":"…","exporter":{"device":"d…","name":"Juan"},"commit":"<oid>","created":<ms>}
E4  Write <destination>/.~<name>.<rand>.tmp; sync_all; rename to
    `<abbreviation>-<YYYY-MM-DD>-<person>.zip` (add -2, -3 on a clash); flush the rename as in
    §5.0 step 4 where the platform allows. `abbreviation` and `person` are cleaned first: each
    is reduced to characters the path grammar of §3.3 allows (letters, digits, `-`, `_`),
    trimmed of trailing dots and spaces, cut to 40 characters, and checked against the Windows
    device names; an empty result falls back to the first 8 characters of the project id or
    of the device id. A person's display name can never make the export fail.
E5  Verify from the device, not from the page cache: re-open the written file with the cache
    bypassed (Windows `custom_flags(FILE_FLAG_NO_BUFFERING)` with sector-aligned reads into an
    aligned slice of an owned buffer; Linux `posix_fadvise(DONTNEED)` after the flush; macOS
    `F_NOCACHE`; through a safe wrapper such as `rustix`) and check the central directory, the
    CRC of every entry, tc_burrito::check on the entries' bytes and P2–P4 on every pack. Where
    the cache cannot be bypassed, the app asks the person to remove and re-insert the stick and
    verifies then. Only after a verify from the device: "Saved to E:\…. You can remove the
    stick." On failure the file is deleted and the person is told.
```

The project folder is not changed by an export except for the checkpoint commit (tC4 J7). The
J7 "Scripture Burrito" export is the same file under the name `<project>-<YYYY-MM-DD>.zip`.

### 10.2 Import ("Receive work from a file")

```
R1  Open the zip. Refuse entries with an absolute path, a drive letter, `..`, or a symlink
    flag; normalize `\` to `/`; caps: 512 MiB per entry, 4 GiB in total, 200,000 entries.
    A zip whose central directory cannot be read is refused: "This file is damaged. Ask for a
    new copy." Nothing is written.
R2  Identify by ingredients/tc/project.json:
      - the open project → "Add Juan's work (saved 3 Oct) to <project>?";
      - an id no local project has → "Create the project from this file" (a newcomer joins: the
        project is built in a staging folder from the file's packs, then opened as in §9.6);
      - another local project → "This file is for <name>, not <name>. Open that project?";
      - no tc/project.json → the importers (§13): a tC4 project if it has
        ingredients/checking/journal/, otherwise a foreign burrito.
R3  Format gate from the file's tc/format.json (§12).
R4  Every entry R4 to R6 read is checked first: its size against the central directory and
    the CRC-32 of its inflated bytes. An entry that fails is damaged: it is never compared,
    never reported as an edit and never written (a damaged carried file is reported, and the
    version already here stays). Each pack entry not present locally, or present with different
    bytes (rules of §9.2 N3): P2–P4. Valid → taken. Damaged → its verified lines are salvaged
    into a `salvage` pending batch (`commit_received`); the damaged file is not written into
    this project; the report says "1 file from Juan's computer is damaged: 212 of 230 changes
    were recovered. Ask Juan for a new export." Every other entry is still taken. A
    newer-format pack is taken unread.
R5  Write every carried file's bytes (including `.tc-root/` entries) whose CRC passed into the
    git object database (repo.blob), so the projection can use any version. Files the file's
    format.json lists as derived but this app does not derive are taken verbatim (§12 rule 4).
R6  A derived file in the zip that differs from the projection of the zip's own packs is
    reported once (§11.3); nothing is taken from it.
R7  [actor] atomic_write(.tc, "intake.json", {source, started}); atomic_write each taken pack
    into place (directory fsync deferred, then once per directory), store each as a blob, and
    rewrite metadata.json (§5.10 M6, closing the write group, I7); commit_received the salvage
    changes exactly as computed (§5.0); fold incrementally; checkpoint "Received work
    from Juan (file of 3 Oct) (tC)" with one parent; atomic_remove(.tc/intake.json).
R8  Report as for a sync (§9.7).
```

Any device can import any file, in any order, any number of times. Importing a file again adds
nothing. A file relayed through a third person carries everyone's packs. When a device that
received work by USB syncs with Door43 later, packs Door43 already has have the same path and
bytes, so git stores them once.

---

## 11. Edits made outside the app

The code follows this section from S3 step 9; until then S2's recording of outside edits runs.

The rule (I8, I11, I13): a derived or carried file that is not the app's own (§6.2) is kept
before the app writes that file again. A book file or story waits for the person's answer. Any
other derived file is copied and written again. Nothing in a kept file enters the project
unless a person imports it (I13). A file that cannot be read is copied for the person and never
silently lost. Rulings: `rulings/2026-10-21-outside-edits.md`, `rulings/2026-10-21-carried-files.md`,
and `rulings/2026-10-22-H18.md` to `-H22.md`.

### 11.1 A file edited on this computer (Paratext, a text editor, a cloud-sync client)

**Detection** at open (§6.2, O9), by the compare-and-swap before every rewrite (§5.10 M1), by
the storage check (O14), and on window focus (the md5 of every path that is not clean by stat
by the rule of §6.2: a size or mtime that differs from the placement's, an unsettled placement,
a held path, every path on a network drive). A file watcher (`notify`, on `ingredients/` and the
root, 1 s debounce) only marks a derived path for the next look; it decides nothing. The
watcher also watches `.tc/pending/` and `ingredients/tc/changes/`: a path there tells the actor
to look for a journal file it does not know (a second writer, §5.0). The watcher is stopped
before the project closes, so no job reaches the writer after the last write. Neither the
watcher nor the focus projects a file: they stat, read what is not clean by stat, and keep what
differs. Every look decides by the table of §6.2. The question is asked at open and at focus; a
changed book file that M1 or the watcher finds while the window has focus raises it at once
(the person is in the app, and the file is held from that moment).

**The flow for a changed book file or story `p` with bytes `b`:**

```
Q1  Keep: the copy, the heads of the base, the manifest (Held, kept), in that order (§5.10
    Keep and hold).
Q2  Decode b with the codec (§13.5) and compare it with the base, as §13.6 does (no batch).
    - Nothing differs but line ends and blanks at line ends: no question. The file is written
      again, with the WhitespaceOnlyEdit notice ("Line endings in ROM.usfm were changed by
      another program; translationCore keeps the file's line endings"). The copy stays.
    - b cannot be decoded (a verse number twice, broken markers): the question says why and
      offers only Not now: "This book has been changed externally, and translationCore cannot
      read it: TIT 1:3 appears twice (line 212). A copy is kept at …. Fix the file, or choose
      Not now to keep the project's text." A later save of the file is looked at again.
    - Otherwise the book goes into the question.
Q3  Ask, once for every book found together:
      "This book has been changed externally. Would you like to import it into this project?"
      With several books: "These books have been changed externally: Titus, Ruth. Would you like
      to import them into this project?"   [Import]  [Not now]
Q4  Import: §13.6 on each kept copy, with the copy's base. Then Imported{md5}, and the file is
    written from the project. The report says "Titus: 12 verses imported, 2 to review."
Q5  Not now: NotNow; the file is written from the project. The kept copy stays in the conflict
    review's notices with Import and Dismiss until it is imported or dismissed.
```

- **A book file for a book the project lacks** (L67). It is kept and asked about ("A book file
  RUT.usfm was added outside translationCore. Would you like to import it into this project?").
  *Import* adds the book (§13.6 U1). On *Not now* the file moves out of `ingredients/`: its copy
  is already kept, and a file for a book the project does not hold would leave `currentScope`
  without it.
- **Another save of a held file.** It is kept again under a new copy, never replacing the first,
  with an entry of its own and the same base; the state becomes `Held` of the newest bytes. The
  question still names the book once, and *Import* there imports the newest copy. Each earlier
  copy keeps its own entry, so it stays in the notices with *Import* and *Dismiss* until it is
  imported or dismissed. The second save need not continue the first: a cloud client can put
  back an older file after a Paratext save, and the Paratext save must not drop off every
  screen.
- **A read-only open** writes nothing, so it keeps no copy and asks nothing. It shows a notice:
  "Titus was changed outside translationCore. Open the project normally to import it."
- **A full disk** leaves a changed file exactly as it is until the app can make its copy (§5.10).
- **Bytes the app wrote that show work the journal lost** (§6.2 N4). A copy is kept with the
  notice "Some recent work was missing from this project's records (a file of saved changes
  was damaged). A copy of Titus is kept. [Bring it back] [Dismiss]". *Bring it back* runs the
  import of §13.6 against the N4 base; a clash with newer work goes to the conflict review.
  Nothing comes back on its own. It works for every derived file kind that has a decoder
  (`rulings/2026-10-22-H22.md` part 1).
- **No *Undo*** after an import: History keeps every version and the conflict review handles
  mistakes (`rulings/2026-10-22-H22.md` part 4).

The small behaviours above (a change of line ends only asks nothing; a file that cannot be read
is asked with *Not now* only; the question is raised at once when the app has focus; a read-only
open shows a notice; a book file for a book the project lacks is moved out on *Not now*; a full
disk leaves the file as it is) are `rulings/2026-10-22-H22.md` part 4, which names the ones the
owner did not see as following this section until the owner says otherwise.

**Other generated files** (`metadata.json`, `checking/settings.json`, `checking/resources.json`,
the alignment and decision sidecars, `vrs.json`, `tc/comments/*.json`; `conflicts.json`,
`format.json` and `project.json` as §3.5) are rewritten from the record, with a kept copy and a
one-line notice (`GeneratedFileRewritten`, outside-edits ruling rule 4). No question is asked
and nothing is imported from them. The alignment and check-decision files are among them
(`rulings/2026-10-22-H21.md`): the supported way to bring alignments in from elsewhere is an
aligned USFM file through "Update a book" (§13.6 U4). An outside edit of `resources.json` adds
no `ResourcesExtra` members (§5.6). An OBS template edit enters only through "Update a story"
(§5.15). The decoders of every derived file kind stay in `tc-journal` for §13.6 and *Bring it
back*; the "more than 50 keys" rule of the old recording does not apply to an import a person
chose.

This is a declared difference from tC4, which reconciles an out-of-band USFM edit on its own at
open (`reconcileUsfm`); the owner ruled for the question (outside-edits ruling rule 3). It is
recorded where APP-DESIGN lists declared differences when D1 builds the dialog.

### 11.2 A file edited on Door43 (web editor, gatewayEdit, tcCreate, git users)

Unsupported (`rulings/2026-10-22-H18.md`, answer B), the same as a project folder pushed to
Door43 by another tool (outside-edits ruling rule 7). The sync keeps the project's own text and
merges nothing from such a commit: the merge tree takes the fold's projection for every derived
file, as for any commit (§9.4). There is no intake walk and no decoding of Door43's files.

One check stays (§9.2 N4). At intake, for each commit after the last app commit (one with the
`Tc-Frontier` trailer) that changed a derived or carried file, the app says once, remembered by
commit id so it is not said at every sync (`OutsideToolEdit`): "Titus was changed on Door43 by
jdoe (commit 3fa2c1d) with another tool. translationCore kept its own text. That version is in
Door43's history." To use it, a person downloads the file and runs "Update a book from USFM"
(§13.6). A file deleted on Door43 is restored by the merge commit, with the notice "TIT.usfm
was deleted on Door43 by jdoe; translationCore kept it". Nothing is lost: every such version
stays in Door43's history.

The device kind `Door43Edit` and `ExternalSource::Door43` are no longer written; they can leave
the format before the freeze (§12.2).

### 11.3 A file edited inside an exchange file

Treated as §11.2 treats a Door43 edit (`rulings/2026-10-22-H22.md` part 3). An export takes
every derived file from HEAD's tree, which is the fold's projection (§10.1 E2, §9.4), so a
derived file in the zip that differs from the projection of the zip's own packs was edited by
hand. The receiving app keeps its own text and says once (§10.2 R6, `OutsideToolEdit`):
"Juan's file holds a changed Titus that was not made in translationCore. It was not taken in;
the file on the stick still has it." Nothing is kept as a copy in this project and nothing is
offered for import; the exchange file is never changed, so nothing is lost. An entry whose size
or CRC fails is never compared (§10.2 R4). The device kind `ExchangeEdit` and
`ExternalSource::ExchangeFile` are no longer written (§12.2).

### 11.4 Carried files

*Owner ruling H5 (2026-10-24):* two versions of a carried file are both kept, with no question. The newest keeps the file's name; each other version is placed beside it as `<stem>-<author>.<ext>` (the author's Door43 user name or display name, or the version's short id without one), with `-2`, `-3` while that name collides with a file the project holds; the fold gives out the names, so every computer gives the same ones. The shown token of a carried file names only its shown version, so an edit or a removal of it leaves the other.

A carried file is any file the app does not derive and does not own, whose path obeys the
grammar of §3.3: anything under `ingredients/` outside the derived table, `tc/` and the format's
`derivedPaths`, and any file at the repository root other than `.gitignore`, `.gitattributes`
and `metadata.json`. Root files are carried in commits but are not ingredients. Files whose
paths break the grammar or have a dot segment are foreign entries (§9.4), kept verbatim.

- **Record** (carried-files ruling rule 1; `rulings/2026-10-22-H19.md`: built now, without
  the git backups of old versions). `CarriedFile(path)` with `Present { oid, size, md5 }` or
  `Removed`. The record holds every field the ruling names: the version's identity (`oid` and
  `md5`; a screen shows its number, its depth in the chain of `parents`), the version it
  replaced (the change's `parents`), the Door43 user who made it (the change's `device`, whose
  `DeviceProfile` holds the username once D98 consent was given) and when (`at`). No old bytes
  are kept in the project on purpose (rule 2): a replaced version's bytes may go when nobody else
  changed that file; the versions that were synced stay in Door43's history, and in the local
  `.git` as far as commits reach them. Transports bring the versions their trees hold: a fetch
  brings every committed version in R's history; an exchange file brings only current versions
  (§10.2 R5). A version recorded elsewhere and never committed may therefore be missing here:
  History shows it with "The file of this version is not on this computer", *Restore this
  version* is offered only for versions whose bytes are here (§5.9), and a projected head whose
  blob is missing never stops a materialization or a commit (§5.10 M0, §9.4).
- **Change detection.** As for derived files, against the manifest, by the open (the entries
  not clean by stat are read once the actor runs), the watcher and the window focus (§11.1). A
  file is never held in memory: it is read in chunks of 64 KiB to take its md5 and size; when
  they are none the app placed, it is read again in chunks into a blob (`blob_writer`) while the
  md5 is taken once more, and the file is recorded only if the two agree, else it changed
  meanwhile and the next look finds it. The blob is the copy of the person's bytes, so no copy
  in `.tc/external/` is made. Then a change (§6.2 N11) whose `parents` are the head of the version the app placed
  there: the change(s) of the record whose blob is the one the path's manifest entry names (the
  heads among them by the token rule of §2.7, else the newest of them), not the newest head of
  the record, which may be a teammate's version the folder never showed; a project that holds
  no such version records it by the rules for a new file below; or `[]` by the revert rule (the
  bytes are an older version's).
  A new file nobody tracked: a change with no parents; where a removed file was, it
  replaces the removal. This is the one way a file's content enters the project without a
  person's choice (I13): a carried file is not derived, so a new version in the folder is the
  person's version. A session with no repository (`HistoryUnavailable`) cannot keep the
  bytes and records nothing; the file is left as it is.
- **Placing.** The materialization of the carried group (and the first job of an open that
  finds a placement the manifest does not know) writes each placement whose manifest entry is
  not its blob and clean by stat: the file is gone, or holds the bytes the app placed before.
  It is written from the blob in chunks by `atomic_write_streamed`, whose last look before the
  rename checks the bytes written against the record's md5 and the file against what was read
  (I8), before every attempt including sharing retries. Bytes whose version is still a head of
  its record (a conflict side that moves between the path and its `.conflict-` copy when the
  provisional head changes, or the version *Keep both* moves to its own path) are written over
  or removed only after they are written and flushed at the place the projection now gives
  them. When that version's blob is missing, it is first stored again from the file (as L73
  does). Only the bytes of a version that a newer version replaced, and that is no head, may go
  without this (L151). The reason the narrower rule stays: a translator records `jon_01.wav`
  (V1); a power cut loses the loose blob, which nothing flushes; a teammate's V2, made from the
  same V0, arrives by sync and becomes provisional. The group must write V1 to its `.conflict-`
  copy and V2 to the path; with the blob gone, replacing the path would destroy the only bytes
  of V1, a version nobody chose to drop. The rule needs no git flushes or backup packs: the
  atomic write at the new place is the durable copy. Preparation
  produces a typed carried write plan without replacing ingredients; the group's projected
  metadata is validated and any outside metadata is preserved before that plan executes.
  Refused writes retain their actual bytes in the final ingredient listing. Bytes the app did
  not place are the person's: they are recorded, not overwritten. A blob missing here leaves
  the file as it is and lists `CarriedBytesMissing`. A blob that is here and cannot be read
  (a damaged object, a cloud placeholder) leaves the file as it is too: the path waits and is
  tried again (R19, `FileUnreadable`), the rest of the project is written, and a commit keeps
  the version it had at the path (§9.4). A file the project no longer places at that path
  (the copy of a version that was kept apart, moved or removed) is removed after the same
  compare-and-swap. Each carried path remembers the
  sequence number of the fold it was last placed, found equal or recorded from (a recorded
  version stands for the newest snapshot, which holds its batch). A checkpoint's job, which
  runs on the older snapshot its packs stand for (§5.13 C2), leaves such a path as it is:
  it neither places an older version over it nor removes a file its fold has no place for,
  and `metadata.json` lists the path as its entry says, with the entry's blob. The entry's
  blob is the base of the person's next edit, so it never names an older snapshot's version
  of bytes it does not hold. The carried group is marked for a new listing only when what
  `metadata.json` says of the path changed.
- **A missing file is never a removal.** It is restored from the object database, or reported as
  `CarriedBytesMissing` if the blob is not there. Removal is the explicit `RemoveCarriedFile`
  command (§5.16). An antivirus quarantine or a cloud client freeing space never deletes a file
  for the team.
- **Two versions** (rule 3). Two versions that replaced the same version, or a version that
  replaced none beside another, are a `CarriedFile` conflict, with the choices *Keep this one*
  and *Keep both* (§8.3). A version that is a head is never removed by the old-bytes rule
  (L151). The provisional head's bytes
  are at the path; each other head's bytes are at `<stem>.conflict-<oid8>.<ext>`, which the
  projection derives (so those paths are never recorded as carried files themselves). Every
  device places the same bytes at the same paths. A person who edits a conflict copy (the
  natural way to merge two texts) continues that version: the edit is recorded as a new
  version of `CarriedFile(original path)` whose parents are the head whose bytes were placed at
  that copy, with a notice ("Your edit of intro.conflict-1a2b3c4d.md was kept as a new version
  of intro.md"). Deleting a conflict copy is not a choice; it is rebuilt (I11).
- **Letter case.** Two carried paths, or a carried and a derived path, that collide (§3.3) are
  never written over each other: one is placed at a `.case-` path and a `PathCase` conflict is
  listed (§7.7). Nothing reads one file's bytes as an edit of the other.
- A carried file over 256 MiB is not recorded and not committed; it stays on this computer, is
  listed in the working folder's `metadata.json`, and the person is told to keep such files
  outside the project.

---

## 12. Format versioning

*Owner ruling H5 (2026-10-24)* changed which concurrent heads conflict. Format 1.0 is not frozen (H9), so builds from before and after it are not compatible, and no team used the earlier ones.

### 12.1 What carries a version

| Thing | Field | Changes when |
|---|---|---|
| Project | `ingredients/tc/format.json`: `major`, `minor`, `minReaderMinor`, `derivations`, `derivedPaths` | below |
| Change | `v`: the minor that introduced the kinds and fields the change uses | a record kind or field is added |
| Pack | header `tcpack` | the pack framing changes (never so far) |
| Exchange file | zip comment `tc-exchange <n>` | the comment changes |
| tC4-shaped sidecars | their `schemaVersion` (1, and 2 for `resources.json`) | their shape changes |
| Fold cache (`.tc/index.bin`, local) | `INDEX_VERSION`, the format minor and the probe stamp (§3.6); any other value makes it no cache. Not the app build: every build of one version, minor and stamp reads the cache, so an update that changes none of them keeps it | its layout, or what the fold keeps of a change or what the per-change rules decide for bytes the probe does not hold, a bug fix included; a change the probe shows changes the stamp by itself, and the test `the_encoding_is_pinned` fails until `INDEX_VERSION` is raised and the new stamp pinned |

```json
{"format":"tc-project","major":1,"minor":0,"minReaderMinor":0,
 "derivations":{"plainText":1,"verseSplit":1,"tokenizer":1,"selections":1},
 "derivedPaths":["ingredients/*.usfm","ingredients/vrs.json","ingredients/checking/**",
                 "ingredients/content/*.md","ingredients/tc/comments/*.json",
                 "ingredients/tc/conflicts.json","ingredients/tc/format.json"]}
```

`format.json` holds no app version, so two builds write the same bytes. It is derived: on every
intake it becomes the maximum of the local and incoming documents (`minor` and `minReaderMinor`
by maximum; `derivations` and `derivedPaths` from the document with the larger minor), so two
teammates on different minors converge to the higher one. Its bytes are written like
`metadata.json`: keys sorted, two-space indentation, one trailing line feed. Members a newer
minor adds, and derivation names this app does not know, are kept and written back; a merge
takes them, too, from the document with the larger minor, and between two documents of one minor
from the one whose canonical text is larger, so the merge is commutative. `major`, `minor` and
`minReaderMinor` are non-negative integers, compared as numbers whatever their size (and so
are derivation versions); a member a newer minor adds is written back exactly, its numbers in
the digits they were written with, and only a member nested deeper than 64 levels, which no app
writes, makes the document unreadable. A
`derivedPaths` entry is a `/`-separated pattern in which `*` matches any run of characters
inside one segment and a `**` segment matches one or more whole segments; the file comes from
any app or person, so matching must take polynomial time.

### 12.2 Rules

1. **Major.** A larger `major` than the app knows is refused before anything is written, at open,
   at sync intake and at file import: "This project was saved by a newer translationCore
   (project format 2). This version reads format 1. Install the newest translationCore. Nothing
   was changed." A smaller major is migrated by an explicit, confirmed step that first writes an
   exchange file as a backup and then commits the migration.
2. **Readable minor.** When `minReaderMinor` ≤ the app's minor, the app opens and edits the
   project. A change whose `v` is above the app's minor and whose kind it does not know parses as
   `Record::Unknown`: it takes part in heads and conflicts like any record, projects nothing, and
   is counted ("12 changes from a newer translationCore are kept but not shown"). A change of a
   known kind with unknown fields keeps that kind's own key (§4.5 V1): it joins the same heads
   as the older app's changes of that record, its known fields project as usual, its unknown
   fields are compared byte for byte (§7.3) and ignored by projection, and the record shows
   "changed by a newer translationCore". An unknown cause parses as `Cause::Newer`. Any other
   unknown enum variant cannot be held in the typed value: the change is quarantined as made by a
   newer translationCore (§4.5 V1), and rule 7 makes such an addition raise `minReaderMinor`, so
   only a read-only app meets it. So two app versions holding the same changes compute the same
   heads, conflicts and derived bytes.
   Because ids are hashes of stored bytes and the app never re-serializes another device's
   change, nothing is lost when data passes through an older app.
3. **Derived bytes.** `plainText`, `verseSplit`, `tokenizer` and `selections` decide derived
   bytes and computed validity. A new version of one changes derived bytes, so it raises
   `minReaderMinor`, and an older app opens the project read-only: "This project needs
   translationCore 5.2 or newer to save changes. You can read and export it." Two app versions
   therefore never flip `invalid` flags back and forth on Door43.
4. **Paths.** Each format version derives the paths in its `derivedPaths`. A path that the
   project's `format.json` lists but this app does not derive is kept verbatim: never rewritten,
   never removed, never recorded as a carried file. A newer app's new sidecar survives an older
   app's checkpoint and sync.
5. **Packs.** A pack with an unknown `tcpack` is carried unread, reported as "made by a newer
   translationCore", never as "damaged" (§4.4 step 0); a pending file with a newer `pending` is
   kept unread. Pack and pending headers may gain members in a newer minor; readers ignore
   members they do not know.
6. **Writing.** An app never lowers `minor` or `minReaderMinor`. Writing a change kind introduced
   in minor M raises `minor` to at least M. The first release ships the newer-format refusal, so
   tC4's risk 10 (a newer journal looks "damaged") cannot recur.
7. **What needs a new minor:** a record kind, a field, a cause, a derived path. The new minor
   also raises `minReaderMinor` (rule 3), so older apps become read-only, when the addition is
   not **inert** for older apps: when it changes the bytes of a path an older minor already
   derives (including `tc/conflicts.json`), changes which heads merge or conflict (a new merge
   unit, a new value variant), or changes what a kind's known fields mean. An inert field or
   cause (one an older app can ignore, by rule 2, and still compute the same heads, conflicts
   and derived bytes) keeps `minReaderMinor`. **What needs a new major:** any change to the
   meaning of an existing kind, field or rule, the canonical encoding, or the line framing. An
   existing kind is never reinterpreted.

**Before the 1.0 freeze** (the format is not frozen, H9), these go into 1.0 with no minor raise
(rulings of 2026-10-21 and 2026-10-22, applied 2026-10-23): "Update a book" writes
`Import(Usfm { file_name })`; `ImportSource::Markdown { file_name }` is added for a story; the
import cause carries an `unknown_base` mark, ranked 0 by `CauseClass::rank`, so a change an
import makes with no parents is never provisional (§13.6 U3). `Cause::External { source:
LocalFile }` is written only for a carried file's version (§11.4); `ExternalSource::Door43`,
`ExchangeFile` and the device kinds `Door43Edit` = 4 and `ExchangeEdit` = 5 are no longer
written, and V removes them before the freeze if no frozen sample project holds one. The pending
header `reconciles {path, seenMd5}` keeps a narrower meaning (§4.2). The carried-versions reference is no
longer written (§9.8). `.tc/materialized.json` (local, pinned by V): `held` loses its cause, a
manifest from a development build that says `damage` opens as a held change and is asked
about, `kept` is added, and the meaning of `Writing.old` changes while its bytes do not.
`.tc/heads/*.bin` keeps the same bytes and also the heads of the bases `kept` entries name. Fold
notices: `OutsideEditHeld` means "book changed outside, waiting", `GeneratedFileRewritten` is
the one notice for any rewritten generated file (§2.6); none is stored in the project.
`format.json` `derivedPaths` does not change.

---

## 13. Import

Common rules for every importer (`tc-import`):

- The source is never modified. The new project is built in `<projects>/.staging-import-<16 hex>/` (§6.4 N1) and
  renamed into place at the end (§6.4 N1, N2). Any failure deletes the staging folder.
- After conversion the importer materializes, runs `tc_burrito::check`, and compares the text
  byte for byte with the source, and every non-derived field of the source's `metadata.json`
  (`localizedNames`, identification authorities, `upstream`, agencies and the rest) with the
  projected document (P9). A mismatch refuses the import, lists the keys, and creates nothing.
- An import never stores a pin without a sha (tC4 D82).
- `.bak` files (tC4's single-level undo) are copied to `.tc/external/tc4-bak/`, never imported.

### 13.1 tC4 4.0 projects with a journal

The import converts tC4's events one by one, with the parents tC4's per-key rules give, then
checks the result against tC4's own derived files. It does not port tC4's structural chains or
generations.

```
T1  Copy the tC4 project folder, .git included, into staging.
T2  Read ingredients/checking/journal/**. Validate each segment with the ported tC4 validator
    (container 1, sha256 of the body, schema v1–v3, actor binding, file name = first ts).
    Invalid segments are listed and the person confirms: "3 saved changes in this
    translationCore 4 project are damaged and cannot be imported. Import the rest?"
T3  Read tC4's derived files from the copied working tree, not from HEAD (tC4 rewrites them at
    every save and commits only at checkpoints, so HEAD can be older than the journal): every
    <BOOK>.usfm, the alignment and decision sidecars, resources.json, settings.json, vrs.json,
    metadata.json. They are the oracle.
T4  ProjectId = UUID (version 8) from sha256("tc4-import\0" ‖ ts of the earliest event ‖
    sha256 of its segment file)[..16]. Every teammate's import of the same project gets the
    same id, so the imports merge.
T5  One Tc4Actor device per tC4 actor, with a DeviceProfile written by the Importer device:
    `legacy = Tc4Actor { actor }` and `door43_username` from the actor's v3 `actor.name.set`
    events (`{v:3, op, actor, ts, base:null, username}`), the first in `ts` order, as tC4's
    first-wins rule takes it. Later ones are listed in the import report, as tC4's
    `namesRejected` lists them; they are never a conflict. A username that breaks the
    Door43Username grammar (§2.3) is listed and not set. A project that was only shared has
    no such event.
T6  Convert the events in tC4 ts order. Each event is one batch (BatchId =
    sha256("tc4-batch\0" ‖ ts ‖ sha256 of its segment file)[..16], so two different events
    with one ts stay apart); device = Tc4Actor(actor); seq = index in the batch; at = the ts's
    milliseconds; cause = Import(Tc4 { actor, ts, op }). Every record the event sets gets a
    change, even when the value is unchanged; a record the event only implies (the `Covered`
    verses of a span slot) gets one only when its value changes.
    Parents of a change on record k: for each tC4 event E named in the event's `base`,
    `supersedes` or structural `sources`: E's change on k if E wrote k, otherwise the heads of k
    in the replay right after E was applied (what E's author saw on k). Nothing else. tC4's
    same-actor rule is not replayed, so work it hid between two machines with one actor becomes
    a conflict, and a rootless write (`base: null` on a live key) becomes a conflict instead of
    tC4's hidden `rootless-base`.
T7  Oracle and reconcile. Fold the converted changes and project in memory. Compare with T3:
    every book's USFM byte for byte, and every decision, alignment, pin group and setting by
    their stored fields. For each record that differs, one reconcile change with tC4's
    value: device = Importer("tc4-reconcile"), parents = [], at = the largest
    converted `at` + 1, cause = Import(Tc4Reconcile), all in one batch. The reconcile value is
    the newest head, so it is the provisional value and every USFM file is byte-identical to
    tC4's. Work tC4 hid (E1–E3 of the research: unselected structural branches, pending
    structure actions, orphaned verse edits; non-text forks tC4 decided by newest-wins) is a
    head too, so it appears as a conflict: "Kept from translationCore 4: this change was hidden
    by translationCore 4's rules."
T8  Seal the converted and reconcile changes into packs of the importing device's sealer
    directory (a new Local device for this folder). Materialize, check, compare (P9).
T9  Commit "Import from translationCore 4 (tC)" on the existing history with the trailer
    `Tc-Import-Source: <tC4 HEAD oid>`. The commit removes ingredients/checking/journal/**
    (its bytes stay in git history) and adds ingredients/tc/**.
T10 Rename the staging folder into place. The original folder is left as it was.
```

| tC4 event | Changes |
|---|---|
| `book.add {book, scope, skeleton, initialVerses}` | `BookPresence::Present { scope, how: Added }`, `BookHeader`, a `ChapterFront` per chapter, a `Verse` per slot: `Text { last }` for the slot (`4-5` → verse 4 with `last: 5`), `Covered` for the inner verses. When `base` names a `book.remove` (a re-add), parents = the replay heads of every record of the book: tC4 had no "restore", so the re-add was a deliberate start-over; the import report says "Titus was removed and added again in translationCore 4; the earlier text is in History". |
| `text.verse.set {chapter, verse, text}` | `Verse(c, first)` = `Text` with `split_verse(text)` and `last` from the slot |
| `text.skeleton.set` | `BookHeader` and `ChapterFront` changes for the parts that differ from the base skeleton; a changed verse marker's whitespace changes that verse's `marker`, with parents = the verse's replay heads |
| `text.structure.apply {skeleton, transitions, dispositions}` | per destination slot: `Text` from the transition; newly covered verses → `Covered`; vanished slots → `Absent`; changed fronts → `ChapterFront`. Dispositions need no change: validity is computed. |
| `book.remove` | `BookPresence::Removed { seen }`, `seen` = the replay heads of the book's records at that point |
| `align.verse.set` | `Alignment(c, first verse)`: the record's fields; `span_end` from the slot; `marked_invalid` = the record's `invalid`; `source` = the original-language pin of the replay at that point when its label equals the record's `sourceVersion`, else none |
| `check.decision.set` | `Check(Decision)`: `context`, `category`, selections, `nothingToSelect`, `status`, `comment` from `comments`, `bookmarked` from `reminders`, `resource` from the decision file's resolution record, `imported_invalid` = `invalidated`, and `verse_text_md5` = md5(plain_text_v1) of the replay's verse at that point |
| `note.add` | `Comment` with `CommentId = sha256("tc4-note\0" ‖ ts ‖ sha256 of its segment file)[..16]`; the target's `generation` is ignored (comments bind to frame verses) |
| `resource.pin.set` | the `Pins` group holding that slot, with the slot set |
| `settings.set` (dotted path) | `Setting(top-level key)` with the path set in the replay's value |
| `project.meta.set`, `project.vrs.set` | `ProjectMeta(Extra(path))`, `Versification` |
| `text.frame.set`, `text.story.ref.set` | `Frame`, `StoryRef` |
| `actor.name.set` | `DeviceProfile(Tc4Actor(actor)).door43_username`, from the actor's first such event in `ts` order (T5); later ones are only reported |

Alignment hashes: `plain_text_v1` is byte-compatible with tC4's `verseTextMd5`, so an alignment
valid in tC4 is valid here, and one tC4 marked invalid stays invalid until it is aligned again.

Two teammates who import their own copies of the same tC4 project produce identical changes for
every event both copies hold. Their unsynced events are extra changes, so the two imports merge
like any exchange. If their derived files differ, their reconcile changes differ, and the
difference is a visible conflict, not a silent pick.

### 13.2 tC4 projects without a journal (Phase-1 sidecars)

A state seed, one batch per book plus one for the project, device Importer("tc4-seed"),
`cause: Import`: `ProjectMeta` from `metadata.json`, `Versification` from `vrs.json`, books
through the codec, pins from `resources.json` (with tC4's schemaVersion 1 → 2 migration),
settings per top-level key, decisions per record (`imported_invalid` from `invalidated`;
`verse_text_md5` from the current verse) and alignments per record (`marked_invalid` from
`invalid`). Verified before it is kept: the projection reproduces every USFM file and `vrs.json`
byte for byte and every sidecar record field for field (tC4 R-8.8.2). Any mismatch refuses the
import and changes nothing.

### 13.3 Moving a Door43 team from tC4 4.x

**The move is one way** (owner ruling, 2026-10-16: option B of
`docs/JOURNAL-DECISION-2026-10-16.md`). Rebuild users and tC4 4.0 users never share a Door43
project: once a team moves, its repository holds this format, and tC4 4.x refuses to sync it.
No way back is built: there is no export of a moved project to tC4 4.0 (the memo's plan step 8
is dropped). No straggler's work is lost by this: a tC4 4.x sync of a moved repository refuses
before it pushes, so it overwrites nothing, and the bridge below converts the tC4 segments a
straggler pushes after the move.

A Door43 repository with `ingredients/checking/journal/` and no `ingredients/tc/format.json` is a
tC4 team repository. The new app does not sync with it silently. It says: "This project is shared
by translationCore 4. Move the team to the new translationCore? Teammates who still use
translationCore 4 cannot sync until they update; their unsynced work comes along when they do."

- **The move:** import the local tC4 copy (§13.1), then sync. The merge commit removes
  `checking/journal/**` and adds `tc/**`.
- **Teammates:** their new app imports their local tC4 project (same project id, same change ids
  for shared events) and then syncs. Unsynced tC4 events become extra changes.
- **tC4 4.x stragglers:** tC4's round-1 sync refuses a repository with files it does not derive
  under `ingredients/` (`sync.verify-failed`), so a 4.x client cannot overwrite the moved
  repository; its work stays local until it updates.
- **Bridge:** if R nevertheless contains new files under `ingredients/checking/journal/` (pushed
  by hand or by a tolerant 4.x build), intake converts the new segments with the T6 converter
  over the full set of tC4 events, ignores the derived-file changes of commits whose message ends
  in "(tC4)" (they are projections of the journal just converted), and says "Juan still uses
  translationCore 4; ask them to update." The merge commit drops `checking/journal/` again.

### 13.4 tC3 projects

A tC3 project (zip, CRC-checked; one or several zips of one language, one book each) becomes a
new project; it is never merged into an existing one (tC4 D53). `tc_import::tc3` follows tC4's
`tc3.ts` (d310bf3, with e79a2795's #796 rule) and imports the current state, as tC4 does:

- **Text:** `<book>/<chapter>.json` → USFM through tC3's own export, usfm-js 3.4.3 `toUSFM`
  with forced new lines (`tc_usfm::tc3_book_usfm`, byte for byte; tC4's definition of "text
  unchanged" for tC3, J9a), then the codec. The records must give that text back byte for
  byte, or nothing is imported (P9).
- **Alignments:** `alignmentData` → one tC4 §5.1 record per verse (occurrences as integers,
  NFC, tC3's `type` kept on a banked word), read with `decode_alignment_record` as tC4 wrote
  it. `marked_invalid` is tC3's invalid marker, or a record that holds a link and whose words
  are not the verse's words (#796); the record is kept as tC3 held it.
- **Checks:** the check index items that hold the translator's work become decisions, read
  with `decode_decisions`, their `verseTextMd5` the imported verse's and tC3's `invalidated`
  their `imported_invalid`. tC3's per-check history files (`checkData`) give no change of
  their own, as in tC4: History starts at the import. A decision on a translationNotes row
  tied to every occurrence (`-1`) has no check in the rebuild (`tc-check`
  `EveryOccurrence`); it is not imported, the report lists it, and the source keeps it.
- **Pins:** tC3 `externalResources` tags are listed for the app to resolve to shas (an
  explicit internet step, or the installed versions); the decisions are stored against the
  pins the app passes. An alignment's `source` is none.
- **Write:** one `Store::create_imported` with cause `Import(Tc3)` on every record, built in a
  staging folder and placed whole (§6.4 N1, N2).

### 13.5 Raw USFM, foreign burritos, and the codec

The codec (`tc-usfm`) decomposes a book into records and recomposes it:

- Verse marker: `\v[ \t]+(\d+)(?:-(\d+))?` followed by one terminator (space, tab, CR or LF).
  Chapter marker: `\c[ \t]+\d+` followed by whitespace or the end of the file. Digits are
  ASCII. Every `\v` or `\c` followed by a space, a tab, a line break or the end of the file
  (the pattern of V3) must be one of these markers, except that verse markers before the first
  chapter marker are header bytes, kept as written; so must a `\v` or `\c` right before one of
  these markers, which would end its part, where V3 reads it as a marker. A verse's content runs to the next verse or
  chapter marker anywhere, or the end of the file (tC4 R-8.4.1).
- `BookHeader` is every byte before the first `\c`, verbatim. `ChapterFront` runs from `\c N` to
  the byte before that chapter's first verse marker.
- Out-of-order verses or chapters are kept in a `BookLayout` record that lists every chapter
  and verse marker in file order.
- Refused, with the line and tC4's `import.damaged.usfm-parse` code: a verse number twice in a
  chapter ("TIT 1:3 appears twice; fix the file and import again"), a chapter twice, a span
  whose end is not after its start, a letter-part label such as `\v 2a` (as tC4 4.0), invalid
  UTF-8, U+0000; also a verse label with a leading zero (`\v 01`: the label is computed from
  the verse numbers, so it could not be given back; a chapter number keeps its zeros in the
  chapter front, as many as it has), verse or chapter 0, a number above 65,535, a verse marker
  at the end of the file with no terminator, any other `\v` or `\c` marker that is not well
  formed, and a book of more than 65,535 verses, counting each verse of a span (Psalms has
  2,461). Each verse is a record, so without that limit a file of a few kilobytes of
  `\c N` and `\v 1-65535` lines would stand for millions of records and stop every device that
  decodes it, at an import or an "Update a book" (§13.6).
- `recompose(decompose(x)) == x` for every accepted input, property-tested and checked on tC4's
  USFM identity corpus.

A raw USFM import makes a new project, one `Import` batch per book, the text taken as is (no
stubs), except for the alignment markup of an aligned book.

**Aligned USFM** (a book tC4 or the ULT wrote with `\zaln-s … \zaln-e\*` milestones around `\w`
words). Kept in the verse text, that markup would leave `plain_text_v1` only the punctuation of
the verse, so no check could select a word and the aligner could not open it. Each verse that
holds alignment markup is taken apart instead (`tc_journal::alignment::split_aligned`, over
`tc_usfm::parse_aligned`):

- When it reads apart and its runs woven back into the plain verse (`tc_usfm::weave` of each
  run of words under the same milestones, its words as the value writes them) give the verse's
  content byte for byte, the import writes the
  plain verse as the `Verse` value (`words` and `layout` split as in §2.4) and one `Alignment`
  record keyed by the verse (its anchor, §7.7): `span_end` is the verse's `last`; each
  `\zaln-s` is a source word (`x-content` is `word`; `x-strong`, `x-lemma`, `x-morph`,
  `x-occurrence` and `x-occurrences` are the named fields; every other attribute is in `extra`
  under its own name). The cards are tC4's (word-aligner's `unmerge`,
  `tc_journal::alignment::tc4_cards`): runs under the same outermost source word join one card,
  whose target words are the runs' `\w` words, so a source word is never on two cards. Where
  the file writes one source word over two runs (en_ult TIT 1:1, ἐκλεκτῶν), an export writes
  that card as one run: the same links, in tC4's bytes. The verse's words on no card are the
  word bank, in verse order; `target_verse_md5` is the md5 of the plain
  verse's `plain_text_v1`; `source` is none and `source_version` empty, since a USFM file names
  no pin (validity then does not read the original-language pin, and the aligner checks the
  source words against the pinned text when it opens the verse); `done` is set when every word
  is placed (D73); `marked_invalid` is false. The P9 comparison of such a verse compares the
  woven projection with the source.
- Otherwise (malformed markup, a word that is not one word of the plain verse, attributes in
  another order, renumbered occurrences: whatever `weave` would not give back exactly) the
  verse is kept as written, markup and all, and the import report lists it. Nothing is lost
  either way. A foreign Scripture Burrito is validated (§3.4 check); its text ingredients go through
the codec; tC4-shaped `x-` sidecars become records as in §13.2. A burrito with
`ingredients/tc/project.json` is an exchange file (§10.2); one with
`ingredients/checking/journal/` is a tC4 project (§13.1).

### 13.6 Update a book from USFM, or a story from Markdown

§13.5 makes a new project from raw USFM; this section changes an existing project with §13.5's
codec and its aligned-USFM split. Two entries reach it: *Import* of a kept copy (§11.1, and
*Bring it back*), and the menu item "Update a book from USFM" (or "Update a story from
Markdown") with a file chosen anywhere (outside-edits ruling rule 5; F1 adds the menu item).
Rulings: `rulings/2026-10-22-H20.md` (the no-base rule and the empty-verse rule),
`rulings/2026-10-22-H22.md` part 1 (*Bring it back*).

```
U1  Read and decode the file with the codec (§13.5; the same refusals, with the line). A story
    decodes into StoryBase, Frame and StoryRef. A file whose book the project lacks adds the book:
    BookPresence::Present { scope from the file, how: Added } with no parents, so a concurrent
    removal is a visible BookPresence conflict (§7.6).
U2  Find the base, the version of the project the file was written from:
    a. a kept copy: the base its entry names (placed:<md5> gives the heads of that placement in
       the path's heads file; head gives what HEAD's packs hold; none, or heads that are not
       there, give no base);
    b. a file chosen from anywhere: no base.
U3  Compare each record of the file with the base, as the planner does
    (tc_journal::outside::plan_outside_edit): the whitespace rule (line endings normalized,
    blanks at line ends removed; a changed record whose base had CRLF gets CRLF back), the
    shown-digest prefilter (only records that differ are read). Then:
    - same as the base: nothing;
    - with a base, different from the base and the project not changed since (the record's heads
      are still the base heads): one change marked as imported, parents = the base heads;
    - with a base, different, and the project changed since (the heads moved): one change with
      parents = the base heads. It is a sibling of the newer head, so a conflict. An import
      never overwrites a verse changed on both sides (L145);
    - equal to an older version in that record's own history (a revert, for example a cloud
      client restoring a copy): no parents, so a conflict unless equal;
    - a verse or record the file lacks that had a value: a conflict, never a removal (I11). For
      a verse, with no base (as built, DOM lane 2026-10-24): left as it was and listed ("not in
      the file"), as is a verse the file holds inside a span; a removal would win over an empty
      stub (§7.3), and as a conflict it makes a passage with the verses around it, so the
      file's other verses would not show;
    - no base (H20): a verse equal in the file and the project imports nothing. The project's
      verse empty and the file's with text: the file's text is imported directly, no conflict.
      The file's verse empty and the project's with text: the project's text stays, no
      conflict; the report lists the verse as "left as it was: empty in the file". Both with
      text, differing: one change with no parents, so a conflict. "Empty" is a verse with no
      words (Home's drafted test); formatting or markers with no words count as empty. Known
      edge: a teammate who deliberately cleared a verse that the file still holds gets the old
      text back without a question; the cleared version stays in History (accepted as rare).
      A record other than a verse: equal to the project's text, or to any version in its
      History, imports nothing; any other difference is a change with no parents, so a
      conflict unless equal. All such conflicts form the import's one grouped row (§8.1).
    Every change made with no parents (the revert case and the no-base case) carries the
    import's unknown_base mark, so it is never provisional (§12.2): until the person chooses,
    the project's text stays the text shown and written.
    The "more than 50 keys" rule of the old recording does not apply: the person chose the
    import, a removal is a conflict anyway, and the revert rule keeps a restored old copy from
    replacing newer work.
U4  Alignments, in the same batch, verse by verse:
    - a verse imported without conflict whose words are unchanged (only layout changed) keeps
      its alignment as it is;
    - a verse imported without conflict whose words changed: the alignment is carried over by
      tc_align::reflow, exactly as §5.1 step 5 does for a save. Links to words the verse still
      has stay, new words go to the bank, words that are gone are dropped. The earlier
      alignment stays in History;
    - a verse that became a conflict: its alignment is not written. Validity is computed (§7.7);
    - a file with alignment markup (\zaln-s, from gatewayEdit, tcCreate or tC4): each verse
      whose markup reads apart (§13.5 "Aligned USFM") imports its alignment as a change by the
      rules of U3. A verse whose markup does not read apart is imported as text with markup,
      and the report lists it.
U5  Two batches (as built): first what comes in with no question (the book, verses empty in the
    project, their alignments), then the changes to review, planned again on the new fold. The
    fold makes one passage conflict of a batch's verse changes, so in one batch a verse to
    review would take the filled verses into its conflict. Cause Import(Usfm { file_name }) or
    Import(Markdown { file_name }), this device,
    the pending header reconciles {path, seenMd5} for a kept copy (§4.2). Changes that rewrite
    verse structure (spans, numbers) form a passage over their range (§0 item 3), as a section
    save does. Plain text changes are verse by verse.
U6  Fold, end the kept entry, send Imported{md5} when the path still holds those bytes (§6.2),
    and materialize the book. Report the counts (BookImported): imported, in conflict, not
    readable.
```

How the base is known, in short. For the question flow the base is exact: it is the placement
whose bytes the file differed from, and its heads are kept for as long as the `kept` entry
lives (§3.6). For a file chosen from anywhere there is no base, and U3's no-base rule keeps
every disagreement visible except where one side is empty. In the supported flow of rule 5
(edit a copy of the book file, then Update a book), every verse edited on both sides is a
conflict, to accept in one click with *Accept all from the file*; verses drafted into an empty
verse come in with no review (H20).

---

## 14. Scale: a fully checked whole Bible, three translators, three years

**The reference machine and the open budgets** (owner rulings of 2026-10-16,
`docs/JOURNAL-DECISION-2026-10-16.md` "Decisions"; `docs/PERFORMANCE.md` "Budgets" holds the
same figures). Budgets are p95 on a five-year-old Windows laptop: 4 cores, 8 GB, a SATA SSD,
Windows 10 or 11 with Defender real-time protection on. Many laptops of that age (Intel
10th-generation Core U-series, for example) have no SHA instructions, so every budget must hold
without them; a measurement on a CPU with them does not alone show a budget met. An ordinary
cold open of a whole Bible, with a usable fold cache (§3.6), takes at most **3 s**. The first
open of a project with no usable fold cache (one received from a teammate, imported, or opened
after the cache encoding changed, §18 item 15) and the first open after a restart, when the
disk cache is cold, take at most **10 s**. Every open shows determinate progress within 300 ms
(PERFORMANCE.md rule 4). The owner's quiet laptop (Ice Lake, with SHA instructions,
`docs/LOCAL-BENCH-FINDINGS-2026-10-16.md` and `docs/evidence/journal-head-to-head-2026-10-16.md`)
hashed 6.5 times faster than the VM below, opened the whole Bible without a cache in 3.8 s and
projected Psalms in 269 ms; the VM rows below are the CPU class without SHA instructions.

Assumptions: 66 books, 1,189 chapters, 31,102 verses; 215,767 check items (tC4's benchmark
corpus); about 8 verse saves per verse across the team (autosave on blur and after 2 s idle), 1.5
alignment saves per verse, 1.3 saves per check item, 5,000 comments; three devices, 250 working
days a year.

| Quantity | Estimate or measurement | Basis |
|---|---:|---|
| Changes | ≈ 620,000; measured corpus 617,143 in 553,978 batches | 250k verse, 47k alignment, 280k decision, 32k seed, 10k other; the corpus is `tc-corpus` at these rates (seed 1) |
| Raw change bytes | measured 674 MiB on disk (664 MB of canonical JSON) | verse ≈ 470 B, alignment ≈ 2.7 KB, decision ≈ 1.2 KB (the real tN and TWL `contextId`s are twice the first estimate of 600 B), plus 65 B line hash each |
| Packs | ≈ 200 files; measured corpus 131 packs and 172 pending files | consolidated: 3 devices × 36 months, plus at most a few dozen per device in the current month |
| Derived files | ≈ 400 | 66 USFM, 66 alignment, 132 decision, 66 comment, 6 project files |
| Files under `ingredients/` | ≈ 600 | |
| `metadata.json` | ≈ 120 KB | 600 entries × ≈ 200 B, pretty-printed |
| Working tree | ≈ 880 MiB | packs 674 MiB (measured) + derived ≈ 210 MiB (compact sidecars: alignments ≈ 90 MiB, decisions ≈ 110 MiB, USFM ≈ 5 MiB; the corpus of unit S1's benchmark has 334 derived files of 277 MiB) |
| `.git` after 3 years | ≈ 250–400 MiB | packs ≈ 70 MiB zlib, stored two to three times because consolidation writes new blobs; derived file history ≈ 110 MiB as deltas; ≈ 7,500 commits of ≈ 15 KB trees |
| Cold open, 4 cores, SSD | budget 3 s with a usable fold cache, 10 s without one or with a cold disk cache; the whole open (O1 to O13) measured by unit S1 (`docs/evidence/storage-open-save-2026-10-17.md`, a 4-vCPU VM **with** SHA instructions, other jobs running, load 1.5 to 5): with the fold cache 1.6 s median, p95 2.1 s with the page cache warm (the p95 is the first open after a session that wrote files, whose unsettled entries it reads), 1.7 s median, p95 1.8 s with the guest's dropped, and as fast with a cache another build of the same encoding wrote (1.6 s, p95 1.7 s); without a cache 3.4 s median, p95 3.5 s, about 3.8 to 4.3 s estimated for a CPU of that speed without SHA instructions; first progress report at 300 to 308 ms; an open after no change opens 132 to 133 files and projects and reads no derived file before the project is usable; its storage check (O14, §6.1) then reads every derived file once, in the background: on a whole Bible 466 files opened and 1,062 MB read by the end, against 133 files and 771 MB when the project is usable, so the check is 334 derived files and 290 MB (0.5 to 0.6 s later with the page cache warm, 0.8 to 1.0 s after it was dropped); on the Psalms corpus 143 files and 72 MB against 133 and 54 MB. On Windows with Defender that is 334 file scans per open, after the project is usable (not measured here). Earlier, load and fold alone (O8, with O7's rules on the files' facts): with the fold cache 1.6 s (median of 9, p95 1.7 s) with the files in the page cache and 1.7 s (p95 1.9 s) with the guest's page cache cold, 2.5 to 2.6 s of CPU over all threads, on a 4-vCPU VM without SHA instructions and no other job running; as fast with a cache another build of the same `INDEX_VERSION` and probe stamp wrote; the first open without a cache (§18 item 15) 5.4 to 5.5 s (p95 5.4 to 5.6 s; 5.9 to 6.3 s the evening before), 19 s of CPU with glibc malloc and 13.5 s with jemalloc: within the 10 s budget of an open without a usable cache; on the owner's Ice Lake laptop with SHA instructions 3.8 s (median of 5, p95 3.9 s) | with the fold cache (§3.6, §6.1 O8) the open reads and fingerprints every file (705 MB) and takes each file's prepared changes from its section instead of hashing and parsing its lines; the fold indexes them as they arrive, and its work after the last file is 0.5 to 0.6 s; O7's rules take 1 to 2 ms. Without the cache every byte is hashed once (P4, with `ring`: one pass over every file takes 1 s of the four threads' time) and every change is parsed and checked; `docs/evidence/storage-fold-2026-10-16.md`. A corpus of 277,880 changes (the size of the probe's 278,041 actions, which it folded in 1.13 s) folds in 0.92 s (0.59 s on a faster VM) |
| Save one verse | 1–10 ms | one `atomic_write` of ≈ 1 KB (measured 0.41 ms on the VM, fsyncbench) |
| Materialize one book | 1 s p95 on the reference laptop (owner ruling 2026-10-18; PERFORMANCE.md), on the writer, shown by no indicator; measured by unit S2 (`docs/evidence/storage-materialize-2026-10-18.md`, an i7-12700 with SHA instructions, other jobs running, load 3 to 11): Psalms after one verse save 185 to 257 ms median, p95 231 to 333 ms (nine rounds of 20; 1.28 s in one the drive stalled), after one decision 184 to 259 ms, p95 215 to 305 ms, Jude 57 to 81 ms, p95 63 to 84 ms; it projects and opens only that book's files (5 files projected, 2 read, 1 to 3 written; 34 MB read, 25 MB hashed, 11 flushes); estimated on the laptop at p95 0.5 to 1.3 s, **not shown to meet the budget**. Earlier, the projection alone: 20–60 ms estimated; measured projection of Psalms 150 to 164 ms (medians of 20 in each of 5 processes, p95 177 to 195 ms) on the VM with no other job running, 205 to 217 ms (p95 247 to 278 ms) with one other job, within the 300 ms budget; 348 to 376 ms when other jobs loaded it (the build before `1665eba`: 341 to 521 ms in the same rounds); not measured on the reference laptop | projecting every Psalms file (USFM, alignments, tN and TWL decisions, comments; 18.7 MB) with values read from disk and checked against their fingerprints (§2.6), on 4 threads, about 0.5 s of CPU (2.3 G instructions), about half of it decoding the book's records from JSON; the book's files are listed from its own records, and decisions and alignments are decoded, written and freed in chunks on the fold's one worker pool (rayon), however the steps nest; the USFM alone 16 ms, one chapter 0.13 ms; the write and `metadata.json` are `tc-store`'s, and with §6.2 an open after no change projects no file |
| Checkpoint | 500 ms (PERFORMANCE.md); measured by unit S2 on the same machine: after one Psalms save 460 to 591 ms median, p95 586 to 761 ms; with the materialization already done 266 to 319 ms median, p95 316 to 525 ms; estimated on the laptop at 0.8 to 1.3 s and 1.3 to 2.4 s: **missed** | seal ≤ 1 MiB (22 to 35 ms, in steps the actor serves saves between), the writer's materialization (30 to 60 ms), the blobs of the changed derived files (80 to 130 ms: SHA-1, zlib and two flushes each), `metadata.json` of the tree and its validation (25 ms), the tree of changed directories (45 to 85 ms: five loose objects, two flushes each), one commit object and the branch (14 ms) |
| Whole-project materialize (a pins change, a resource upgrade, a book added) | background, with a progress bar; measured by unit S2: a pins change over the whole Bible 2.8 to 4.7 s (10 to 11 s for runs the drive stalled), the first progress report 315 to 325 ms after the job began, 23 to 43 progress events, saves and decisions at p95 14 to 24 ms meanwhile (190 to 244 ms in two rounds the drive stalled); the review estimated 10 to 25 s on the laptop for the same work on the actor | 334 files projected book by book (400 to 456 MB read, 300 to 318 MB hashed, 318 to 370 flushes), each book its own write group, the writer's flushes paced behind saves (116 to 242 ms waited in all) |
| Close a project | 3 s on the laptop (PERFORMANCE.md); measured by unit S2: 0.9 to 1.4 s (median 0.92 to 1.05 s) after a Psalms save and a decision, estimated on the laptop at 2.7 to 4.2 s: **not shown** | the checkpoint, the fold cache written piece by piece (65 MB), the draft writer stopped; a repack that runs is stopped and never waited for |
| Save while background work runs | 25 ms p95 (PERFORMANCE.md); measured by unit S2: 14 to 24 ms during a whole-project materialize (190 to 244 ms in two rounds the drive stalled), 23.9 to 26.8 ms during a checkpoint (19.7 to 44.9 over thirteen rounds), 14.9 to 16.2 ms beside a repack, 13.3 to 17.6 ms idle (median 7.4 to 8.7 ms) | two flushes of the save's pending file; the drive's flush queue is what a save waits for (`docs/evidence/storage-materialize-2026-10-18.md`) |
| Three Psalms verses changed in `PSA.usfm` and imported (§13.6; measured on the automatic recording path it replaces) | no budget; measured by unit S2: 30 to 49 ms median from the file being written to the notice, p95 46 to 78 ms | the file read once (28 MB read, 19 MB hashed with the rewrite that follows), one decode, three keys compared, three changes written |
| Daily sync | 1–3 s + network | incremental fetch of a few hundred KB; push ≈ 0.3–1 MB (non-thin, §9.5) |
| Join | ≈ 250–400 MiB full clone, or ≈ 120 MiB exchange file | packs deflate ≈ 5x in the zip |
| USB export | ≈ 120 MiB, 5–15 s on USB 2 | |
| Memory | ≈ 100 MiB typical, ≈ 350 MiB peak; measured folded state 170 MB of heap with 4 MB of allocator overhead (205 MB with 15 MB before the compaction of 2026-10-17); with glibc malloc an open with the fold cache peaks at 281–294 MiB RSS and leaves 217–232 MiB with the fold open, the first open without a cache peaks at 317–324 MiB (450–456 MiB before), and the app's case (one open, then saves, decisions, alignments, resolutions, conflict lists, 20 projections of Psalms and snapshots, in a process of its own) peaks at and keeps 302–310 MiB; with jemalloc 315–319, 256–261, 348–356 and 340–373 MiB; the peak is met, the steady 150 MiB is **missed** by the fold's tables alone (`docs/evidence/storage-fold-2026-10-16.md`). The owner's rows of 2026-10-18 (400 MiB steady, 450 MiB peak, private bytes, the close and the whole-project materialize included) were measured by unit S2 on the whole Bible (`docs/evidence/storage-materialize-2026-10-18.md`): with one heap 302 MiB a minute after 200 saves and 200 decisions, 323 MiB at the peak (the whole-project job, then the close); with glibc's default heap 404 MiB and 457 MiB, which misses by 4 and 7 MiB; the laptop is estimated at the one-heap figures | ≈ 280 B of tables per change: each change's entry 96 B (its first parent, sequence number, value's place and fingerprint, and shape inline; what few changes have beside the table), each batch's 28 B, each record's 144 B (identity strings of up to 15 bytes inline), three position-index slots; values are byte ranges read on demand and dropped when a book is idle; the open folds each file as it arrives and parses a file's lines one at a time, so its peak holds the tables, the cache's bytes and a few files' inputs; the fold's parallel steps run on one worker pool; glibc keeps each worker thread's heap at its high-water mark (41–50 MiB after an open, about 120 MiB after the app's case); meeting 150 MiB needs the fold to keep only the heads and the open books' histories in memory |

**Rows of the project store that are not met or not shown** (unit S2, 2026-10-18;
`docs/PERFORMANCE.md` "Rows of the project store that are open" has the table and what would change
each): the checkpoint (500 ms) is missed by the laptop estimate; a save during a checkpoint is at
the edge of 25 ms p95, and the month's consolidation is still one actor step; one book's
materialize (1 s) and the close (3 s) are estimated at both sides of their budgets; and the
memory (400 MiB steady, 450 MiB peak) is met with one heap and missed by 3 to 11 MiB with glibc's
default heap, which makes the whole-project job 3.5 times slower. They stay open until the owner
chooses a remedy, or a measurement on the reference laptop shows them met.

An aligned NT project is about a quarter of these figures. Rows that say "measured" were
measured on this format with the `tc-corpus` whole-Bible corpus, release builds
(`docs/evidence/storage-fold-2026-10-16.md`); the other rows are still estimates from probes,
until `tc-sync` measures sync and export (§16.5). With the fold
cache `.tc/index.bin` (§3.6, §6.1 O8), load and fold take 1.6 to 1.9 s; the rest of the open (O1
to O7, O9 to O13) is measured by `tc-store` (`docs/evidence/storage-open-save-2026-10-17.md`; unit S2
measured it again with the writer, git and the watcher: 0.78 to 0.86 s with the cache and 1.12 to
1.41 s without, on an i7-12700). The cache holds a section per file, keyed by the file's path, size and fingerprint,
not one key for the whole set of files: a save, a seal, a consolidation or a sync costs the next
open only the parse of the files it added, a file another program changed is read again, and a
file that is gone is never asked for. The first open without a cache verifies and parses every
change (§18 item 15).

---

## 15. Every silent-loss case and what prevents it

"Lost" means gone from every screen, from the projected text or from Door43, even if the bytes
survive somewhere. Each row is a named regression test (§16.3) that asserts what the person sees
and the on-disk end state, not only what the fold returns.

### 15.1 The cases in `research/sync-design.md`

| # | Case (source) | What tC4 did | What prevents it here | Test |
|---|---|---|---|---|
| L1 | Two concurrent verse spans in one book (E1, E1 reversed, F1) | the older structure action applied; the newer one pended `incomplete`; every later edit by its author in that book was anchored on the pending branch and vanished; 0 conflicts shown | a span writes only its own verse records (`Text { last }` and `Covered`); spans on different verses touch different keys; there is no structural chain, so a later edit is a plain verse change that always projects | `loss_e1_two_spans_one_book`, `loss_e1_reversed_order` |
| L2 | Spans that share a verse | as L1 | the shared verse has two heads; both spans are passage batches, so it is one `Passage` conflict over both spans (§7.3, §8.1), described in words, with every word shown once | `loss_e1_overlapping_spans_conflict` |
| L3 | A span while a teammate edits any verse of the book with an older timestamp (E2a) | the span pended `conflicted` forever and hid its author's later edits | different keys, no pending state, no timestamps in any rule | `loss_e2a_span_vs_edit_elsewhere` |
| L4 | A phantom conflict the span author never caused (E2b) | the span re-headed every slot | a span never writes verses outside its range | `loss_e2b_no_phantom_conflict` |
| L5 | An edit to a verse a teammate merged away (E3, F2) | retained `orphaned-text`, shown nowhere | verse 5 has `Covered` and `Text` heads: a conflict; the coherence pass keeps verse 5's words in the provisional USFM; *Keep 4 and 5 separate* restores both verses in one action | `loss_e3_edit_vs_merge_away`, `loss_span_resolution_restores_both_verses` |
| L6 | A renumber against an edit of the moved verse; the "branches do not meet" dead end (F12) | `resolveVerse` could refuse; the losing text became unreachable | a renumber is a passage batch: an edit of any verse it rewrote is one `Passage` conflict shown from one side (§7.3); a resolution names the current heads and needs no ancestry condition | `loss_renumber_vs_edit`, `loss_renumber_shift_vs_edit_by_number`, property `resolution_always_possible` |
| L7 | The same verse edited two ways | a fork, newest provisional, the review showed it | the same, for every record kind; the base text is shown with a word diff | `loss_same_verse_two_ways` (J18) |
| L8 | Silent newest-wins for alignments and check results (E4, F3, G7) | only text forks reached a person; the rest kept the newest head with a count | every record kind has the same conflict state and a review screen (§8); decisions merge by unit first, so a bookmark against a status is no conflict | `loss_e4_alignment_newest_wins_the_other_in_history`, `loss_check_two_ways_newest_wins`, `merge_bookmark_and_status` |
| L9 | Pins and settings mixed by concurrent slot events (sync-design §7 table) | two gateway changes could mix slots | pins are grouped; a disputed slot makes the whole group a conflict and the projection never mixes two heads | `loss_two_gateway_changes_never_mix` |
| L10 | Two `book.add` of one book with different payloads | structural fork, newest wins silently | per-record merge; seed stubs settle by the stub rule; real differences are conflicts per verse | `loss_two_book_adds` |
| L11 | Two people resolve one conflict differently | a new fork | a new conflict, labelled "resolved differently"; equal resolutions merge | `resolve_differently_reconflicts` |
| L12 | A poison segment that passes the validator but makes the fold throw (E5, F6) | every teammate's sync failed and the project could not open; teammates re-pushed it | the fold is total; every rule is local to one change or one file; a bad change is quarantined and listed | `loss_e5_poison_never_blocks`, property `fold_never_panics` |
| L13 | A fast clock wins every newest-wins and drags everyone's clock forward (E6) | yes | no rule that decides survival reads a clock; `at` chooses only the provisional text; nothing ratchets; `ClockAhead` is shown | `loss_e6_fast_clock_cannot_hide` |
| L14 | A file changed on Door43 by another tool (web editor, gatewayEdit, tcCreate) is overwritten by the next sync without a word (F4, R-8.7.9) | replaced without a report | unsupported (`rulings/2026-10-22-H18.md`): a Door43 edit made with another tool is reported once, naming the commit and person, and the version stays in Door43's history; it reaches the project through "Update a book" (§11.2, §13.6) | `a_door43_edit_by_another_tool_is_reported_once` |
| L15 | A stale whole-book save reverts teammates' work (F5) | silent linear revert, or "make your last change again" | no whole-book writes; every change names the heads its screen showed (tokens with a MAC); a stale screen makes a conflict; a no-op save writes nothing | `loss_f5_stale_buffer_never_reverts`, `a_save_of_the_displayed_value_writes_nothing` |
| L16 | Two machines with one identity (F7) | same-actor writes overwrote each other linearly; one duplicate `ts` stopped the fold | the fold never treats one device specially; concurrent edits are conflicts; ids are content hashes, so nothing collides; fingerprint, ledger and `DuplicateDevice` mint new devices and say so | `loss_f7_one_identity_two_machines`, `loss_f7_restored_backup_warns` |
| L17 | A lost installation secret orphans the outbox (F8) | staged actions never replayed | pending saves are files in the project folder; a lost secret only means a new device | `loss_f8_secret_lost_pending_survives` |
| L18 | Non-atomic writes; a torn `metadata.json` (F9, RISKS 13) | the project would not open | `atomic_write` for every file; `metadata.json` is derived and regenerated at open | crash matrix §16.2, rows W, M |
| L19 | A damaged scratch copy locks the project for good (F10) | forward-only apply refused every open | no scratch copy and no marker; intake writes immutable files; open never refuses because of content | `loss_f10_nothing_can_lock_open` |
| L20 | A false "synced" status (F11) | a ref rejected after the pack could read as success | `push_update_reference` rejections are errors; an `ls-remote` must show the pushed commit before "on Door43" | `sync_protected_branch_reported`, `a_push_whose_reply_is_cut_off_is_sent_not_confirmed_and_the_next_sync_confirms` |
| L21 | One extra file on Door43 blocks every sync (F13) | `sync.verify-failed` for the whole team | extra files are carried files; newer derived paths are kept verbatim | `a_file_the_app_does_not_own_follows_door43` |
| L22 | One invalid incoming journal file refuses the whole sync (G2, "one bad file blocks sync forever") | the whole team blocked until hand surgery | each file is validated alone; a damaged pack is salvaged line by line, kept on the tip, and repaired by its owner | `loss_bad_pack_from_teammate_sync_continues`, `a_damaged_remote_pack_is_salvaged_and_kept_in_the_tree` |
| L23 | The project is locked while a sync runs; queued edits refused after it (F14, `sync.project-replaced`) | saves refused | the sync works on a snapshot and immutable files; saves continue | `loss_f14_edits_during_sync_kept` |
| L24 | `retained` and `pendingStructural` shown on no screen (sync-design §3.4, sync-design §9.3) | conserved in the journal only | those states do not exist; I2 lists every state and each one is visible | property `conservation` |
| L25 | Version skew: a newer `v` fails older readers (F6, RISKS 10) | "damaged" | majors refused in words before writing; newer kinds carried as `Unknown`; newer packs carried unread | `loss_newer_minor_carried`, `loss_newer_major_refused_before_write` |
| L26 | Sync livelock: a full clone per retry (sync-design §9.3) | three full re-clones, then refusal | incremental fetch, intake of new files only, six attempts with jittered backoff | `three_devices_converge_with_byte_identical_derived_files`, `sync_contention_three_devices_converge` |
| L27 | Decision files depend on installed resources; Door43 flips between teammates (sync-design §9.3) | yes | `resolution(tool, book)` reads only the projected pins with recorded coverage (I12) | `projection_ignores_installed_resources` |
| L28 | The Door43 username is written into public history automatically (sync-design §9.3) | at the first sync | only after the author notice (D98), in this device's `DeviceProfile` | `username_written_once_after_first_successful_fetch` |
| L29 | Audio never reaches Door43 (sync-design §9.1) | own audio never synced | carried files travel in every exchange as records | `loss_carried_file_travels` |
| L30 | A default branch other than `main` refused forever (sync-design §9.2) | `sync.not-main` | `tc.branch` records the branch at share or join | `sync_master_branch` |
| L31 | Private repositories cannot sync (sync-design §9.1) | the clone sent no credentials | fetch and join send the token | `sync_private_repository` |
| L32 | OBS projects refused by sync; no USB exchange (sync-design §9.1, sync-design §10) | refused | OBS syncs like Bible projects; USB exchange is §10 | `team_sync_obs_project`, journey `usb_both_directions` |
| L33 | Locks only in-process (sync-design §6.3); tC4 4.0 runs once per computer (D39) | a process left running after a crash, or another program, could write into the folder | `.tc/lock` is an OS lock that refuses the open of a project another process holds, naming the holder; translationCore runs once per computer, so no second app, process or window works on a project on one computer (several people work only from separate computers, through sync: L64, `OpenOnAnotherComputer`, `SecondWriter`) | `a_second_open_of_a_project_is_refused_and_writes_nothing`, `a_lock_another_program_holds_refuses_the_open_without_a_name` |
| L34 | A protected branch unmeasured (sync-design §9.1); no write access is measured: HTTP 200 and a declined ref, not 403 (§9.5) | heuristic for the protected branch | `NoWriteAccess` and `RefRejected` carry Door43's own text; the exchange file is offered | `no_write_access_is_refused_before_the_fetch_with_nothing_changed`, `sync_protected_branch_reported` |

### 15.2 Cases found by the design panel

| # | Case | What prevents it | Test |
|---|---|---|---|
| L35 | A carry-over re-keys a decision while a teammate checks it offline: the teammate's verdict lands on an identity that projects nothing | `MovedTo` is a value of the same record, so the concurrent edit is a `CheckMoved` conflict; *Apply Pedro's change to the moved check* | `loss_carryover_move_vs_offline_check` |
| L36 | A re-key moves the verdict but leaves check comments and bookmarks behind | the whole `DecisionRecord` (judgement, comment, bookmark) moves | `loss_carryover_keeps_comment_and_bookmark` |
| L37 | A book removed while a teammate edits it: the edits are hidden | `Removed { seen }`; unseen changes raise `BookRemovedWhileEdited` and keep the book projected | `loss_book_removed_while_edited` |
| L38 | Typing into a conflicted verse silently resolves it | the token of a conflicted record lists only the displayed candidate; a resolution is refused when the heads moved | `loss_edit_conflicted_verse_keeps_conflict`, `loss_resolution_refused_when_heads_moved` |
| L39 | Two rewordings of different poetry lines of one verse are combined into a verse neither wrote | `words` is never split; only trailing marker-only lines form `layout` | `loss_poetry_lines_never_auto_merged` |
| L40 | The materializer overwrites a file another program saved in the idle window | compare-and-swap against the manifest before every write and removal | `loss_materializer_never_overwrites_outside_edit` |
| L41 | Select-all-delete or truncation in another editor deletes verses one by one | Nothing enters the project without *Import*. On import, a verse the file lacks is a conflict, never a removal (I11) | `loss_outside_mass_delete_is_conflict` |
| L42 | A cloud-sync client restores an older copy and reverts verses linearly | Nothing enters without *Import*. On import, a value equal to an older version of the record has no parents, so it is a conflict (§13.6 U3) | `loss_outside_revert_is_conflict` |
| L43 | An antivirus quarantine or "free up space" deletes an audio file for the team | a missing file is never a removal; it is restored | `loss_carried_file_deletion_not_propagated` |
| L44 | Two versions of a binary file placed differently on two devices | `CarriedFile` conflict; projection places the same bytes at the same paths everywhere | `loss_carried_file_two_versions_both_kept` |
| L45 | A quarantined or newer incoming pack is dropped from the Door43 tip by a device that cannot read it | every file of R is taken; only the redundancy rule removes a pack | `a_damaged_remote_pack_is_salvaged_and_kept_in_the_tree`, `loss_bad_pack_from_teammate_sync_continues` |
| L47 | A seal deletes pending files before the pack is known to be readable | the pack is re-read and verified before any pending file is removed | crash rows K2, K3; `crash_seal_verifies_before_delete` |
| L48 | An append to a file inside the burrito tears acknowledged saves | no file is appended or written in place; each save is a new atomic file outside the burrito | crash rows W1–W4 |
| L49 | One damaged file in a USB zip refuses the whole exchange | each pack is validated and salvaged alone | `loss_usb_one_bad_pack_rest_taken` |
| L50 | The provisional USFM drops a verse's words while a span conflict is open | the coherence pass (§7.5) | `loss_span_conflict_never_drops_words` |
| L51 | Restoring a book detaches its checks and comments ("generations") | no generations: restoring the book projects every record again | `loss_restore_book_keeps_checks` |
| L52 | Adding a removed book again stubs over its old text | the restore dialog offers "Restore the text" first | `a_book_is_added_with_stubs_removed_with_what_was_seen_and_restored_either_way` |
| L53 | Written invalidation events conflict with checkers' judgements | invalidation is computed, never written | `merge_text_edit_never_conflicts_with_check` |
| L54 | A damaged pack from a teammate whose computer is gone is lost whole | every line carries its own hash; any device salvages it | `a_truncated_pack_salvages_every_whole_line` |
| L55 | Concurrent removals of the last books leave a burrito with no books | the last-book rule keeps one present and lists `LastBookRemoved` | `loss_last_book_removed_concurrently` |
| L56 | Git rewrites line endings on Windows, so every file looks edited outside the app | `core.autocrlf=false`, `* -text`, and no checkout: the app writes every working-tree byte | `loss_autocrlf_never_rewrites_bytes` |
| L57 | A crash inside the save debounce loses the last sentence | the browser-side buffer and `.tc/drafts.json`, offered at open | `drafts_offered_after_reopen` |
| L58 | Two teammates upgrade one resource to different versions; the carry-overs mix | a `Pins` conflict plus the check conflicts it caused, grouped as one row and resolved in one batch | `loss_upgrade_two_versions_grouped` |
| L59 | A bulk "Keep newest" hands the decision back to the clock | there is none; bulk choices are by a chosen set of computers, and skip conflicts whose candidates share that set (§5.9) | `review_has_no_keep_newest`, `bulk_choice_by_computer_skips_shared_username` |
| L60 | tC4 4.0 data hid work (E1–E3, same-actor overwrites) before the import | the converter has no structural chains and no same-actor rule; the reconcile keeps tC4's text provisional and the hidden work becomes conflicts | none: importing tC4 4.0 projects waits until after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md), so no translator reaches it |
| L61 | An outside edit that cannot be read is overwritten | a byte copy is kept until the person dismisses the notice | `loss_unreadable_outside_edit_copy_kept` |
| L62 | A file whose name cannot be an ingredient (a cloud client's "conflicted copy") makes the burrito invalid or is deleted | it is moved to `.tc/external/` and the person is told | `loss_invalid_name_file_moved_not_deleted` |
| L63 | Half of an action arrives (a damaged pack held part of an upgrade) | batch completeness holds the batch until it is whole, and says so | `loss_incomplete_batch_held_and_listed` |

### 15.3 Cases found by the adversarial review

| # | Case | What prevents it | Test |
|---|---|---|---|
| L64 | Two installations write into one cloud-synced folder; both name a pending file `…07.batch`, one save replaces the other, or a renamed `…07-DESKTOP.batch` is never read | pending files are named `<device>-<hash32>`, so no other save can replace one; every file in `.tc/pending/` is read whatever its name; cloud-synced folders open read-only with a copy offer; a second writer is detected and stops writing (§4.2, §5.0) | `an_unknown_journal_file_appearing_turns_the_project_read_only`, `open_cloud_synced_folder_read_only` |
| L65 | A pack is deleted outside the app (antivirus, cloud client, a copy that skipped long paths); the fold silently loses a month, and a checkpoint fast-forwards the loss to Door43 | the expected pack set is state: missing or damaged packs are restored from HEAD, the confirmed commit, R or the stored blob at open and before every tree; trees hold the expected set; no tree is built while an expected pack is unreadable (§4.4, §5.13 C3, §9.4) | `loss_pack_deleted_outside_app_restored`, `loss_pack_deleted_while_open_never_pushed`, `loss_pack_cloud_placeholder_unreadable_not_damaged` |
| L66 | A damaged local copy of a teammate's pack is committed over Door43's good copy | restore from any valid blob, in every sealer directory; a pack failing P2–P4 never enters a tree while a valid blob exists; when every copy is damaged, the union of their salvaged lines is sealed (§4.4, §9.2 N3) | `a_damaged_local_pack_is_repaired_from_door43s_copy`, `loss_all_copies_damaged_union_salvaged` |
| L67 | A book file uploaded on Door43 or copied into `ingredients/` becomes records that project nowhere, and the merge commit deletes it from the tip | Local half: a book file for a book the project lacks is kept and asked about; *Import* adds the book, *Not now* moves the file out with its copy kept (§11.1). Door43 half: a book file uploaded with another tool is reported once and stays in Door43's history (§11.2); no entry of R is dropped silently (§9.4) | `loss_local_book_file_added_projects`, `a_book_file_for_a_book_the_project_lacks_is_asked_and_not_now_moves_it_out` (pending: S3), `a_door43_edit_by_another_tool_is_reported_once` |
| L68 | A renumber that shifts verses against an edit by verse number shows one verse twice and drops another, and alignments and comments stay on the old numbers | a renumber is a passage batch: one `Passage` conflict, the provisional text takes one side whole, *Combine* applies the edit where the text now is; `from` anchors move alignments and comments with the text (§7.3, §7.7) | `loss_renumber_shift_vs_edit_by_number`, `loss_renumber_alignment_and_comments_follow_text` |
| L69 | A section save that moves a word from verse 5 to 4 against an edit of verse 4 drops the word from the projection; a per-verse choice makes it permanent; a concurrent paragraph break merges onto the wrong words | the save is a passage batch; no unit merge inside a passage; one grouped conflict described in words; provisional text from one side (§7.3, §8.3) | `loss_section_move_vs_verse_edit`, `loss_section_move_vs_paragraph_break_not_merged` |
| L70 | *Start empty* silently beats a concurrent *Restore the text*, or is silently absorbed by a concurrent edit | `Present` carries `how`, so the race is a `BookPresence` conflict; *Start empty* writes `StartOver` changes, never seed stubs; the stub rules are defined for one head plus stubs (§5.7, §7.3, §7.6) | `loss_restore_text_vs_start_empty_conflict`, `loss_start_empty_vs_concurrent_edit_conflict`, `stub_rules_one_head_plus_stubs` |
| L71 | A comment or alignment made concurrently with a carry-over, a renumber or a join appears on no screen | the projection places attached records by anchor, follows `MovedTo` and `moved_to`, shows inner-verse alignments in the run's aligner, and lists the rest under Detached (§7.7) | `loss_comment_follows_moved_check`, `loss_comment_follows_renumber`, `loss_alignment_inside_join_reachable`, property `every_head_reachable_from_a_screen` |
| L72 | A field from a newer minor makes an older app fold the change under another key; the two versions write different derived bytes and trade false Door43 edits | a known kind with unknown fields keeps its own key; unknown fields are compared, not projected; non-inert additions raise `minReaderMinor` (§4.5 V1, §12.2) | `loss_newer_field_keeps_known_key`, property `mixed_minor_versions_converge` |
| L73 | Restoring a carried version whose blob is not local makes every checkpoint, sync and export fail | *Restore* is offered only for versions whose bytes are here. A missing blob is stored again from the folder when the file holds its bytes, else the tree keeps the previous blob. A commit never fails for it (§9.4, §11.4) | `loss_carried_version_missing_blob_never_blocks_commit`, `a_placed_file_whose_blob_is_gone_is_stored_again_from_the_folder` |
| L74 | A section save is refused on every retry because a teammate's span arrived after the screen was drawn | tokens for every displayed verse; coherence is checked on the author's view; overlap is a `Passage` conflict; the section buffer is a draft (§5.2, §5.14) | `loss_section_save_after_teammate_join_conflicts` |
| L75 | A verse added to a book with a `BookLayout` is skipped by the projection | the save updates `BookLayout`; the projection places every unlisted record after its numeric predecessor (§5.2, §7.5) | `loss_booklayout_new_verse_projected` |
| L76 | A batch that can never be completed hides its recovered changes forever, and its descendants show false conflicts | *Apply what was recovered* writes a `BatchRelease`; conflicts with a missing parent are marked incomplete history (§7.4) | `loss_unrecoverable_batch_release`, `incomplete_history_conflict_labelled`, `a_restore_race_verse_continuing_a_held_change_marks_the_presence_conflict`, `a_restore_race_candidate_with_a_missing_parent_is_incomplete_history` (properties), `a_start_empty_verse_head_with_a_missing_parent_marks_the_presence_conflict`, `a_restore_race_lists_the_batches_of_its_verse_candidates`, `an_incomplete_restore_race_offers_every_choice` |
| L77 | Deterministic ingests and salvaged lines are re-stamped with this device's seq, time and batch | `commit_received` writes them exactly as received (§5.0) | none for ingests, which no longer exist: a Door43 edit made with another tool is reported, not taken in (rulings/2026-10-22-H18.md); `a_damaged_remote_pack_is_salvaged_and_kept_in_the_tree` |
| L78 | "Keep all of mine" discards a teammate's versions when two computers share one Door43 account | bulk choices by computer; conflicts within the chosen set are left for a choice of their own (§5.9, §8.5) | `bulk_choice_by_computer_skips_shared_username` |
| L79 | Paths that differ only in letter case overwrite each other on Windows or macOS, and one is recorded as an outside edit of the other | collisions are found by case folding; a colliding file is placed at a `.case-` path or kept out of the working tree; a `PathCase` conflict (§3.3, §7.7) | `loss_case_colliding_carried_paths_kept`, `loss_door43_case_alias_never_recorded` |
| L80 | On Windows a power cut keeps a complete pending temp file and drops the rename after "Saved"; open deletes it | the rename is flushed before "Saved"; open renames a verifying temp into place instead of deleting it (§5.0 step 4, §6.1 O2) | crash rows W1, W1a; `crash_lost_rename_rolled_forward` |
| L81 | A metadata value that is valid JSON but invalid in a burrito, edited on Door43, stops every materialization, commit, sync and export for the team | `build_metadata` is total: each field is validated and falls back to its newest valid value with a listed anomaly; commands validate first (§3.4) | `loss_invalid_meta_value_never_blocks_sync`, property `arbitrary_meta_never_blocks_commit` |
| L82 | The rename retry writes over a file another program saved during the retry | the compare-and-swap is repeated before every retry (§5.10) | `loss_rename_retry_never_overwrites_outside_save` |
| L83 | A bit flip on a stick or on disk is recorded as a person's edit and synced to the team | Every zip entry's CRC and size are checked (§10.2 R4). On disk: nothing is taken into the project without the person. A changed book file is kept and asked about. Any other file is written again from the record, with a copy (§6.2) | `loss_usb_damaged_carried_entry_not_recorded`, `loss_bit_rot_not_recorded_as_edit` |
| L84 | Rebuilding a damaged `.git` throws away carried blobs, so no commit can ever be made | G4 copies blobs from the broken repository and re-stores matching working-tree files; a missing blob never fails a commit (§6.4 G4, §9.4) | `crash_g4_rebuild_keeps_carried_blobs` |
| L85 | The export check reads the zip back from the page cache and calls a failing stick verified | the verify bypasses the cache, or asks for the stick to be re-inserted (§10.1 E5) | `usb_export_verify_reads_device` (pending: USB-VERIFY) (probe §16.5); until then the export's verify reads the file back through the page cache, `an_export_ends_verified_or_deleted` |
| L86 | A disk error after the rename reports "Not saved" for a save that exists; the retry reuses its seq and raises a false duplicate-device alarm | `WrittenNotDurable` counts as written, with a disk warning (§5.0) | `save_disk_error_after_rename_counts_as_saved` |
| L87 | A read-only open deletes the writer's temp and lock files; a leftover `config.lock` blocks share and connect forever | a read-only open writes nothing; a writable open removes every `*.lock` under `.git` (§6.1) | `a_read_only_open_writes_nothing_in_git`, `an_open_removes_stale_git_lock_files` |
| L88 | Files on Door43 the app does not own (dot files, names with spaces) are dropped from the tip by the next sync | foreign entries are kept verbatim; a badly named ingredient moves to `.tc-moved/` with a notice on every device (§9.4) | `a_checkpoint_keeps_foreign_entries_of_head`, `a_file_the_app_does_not_own_follows_door43`, `loss_invalid_name_file_moved_not_deleted` |
| L89 | `project.json` deleted on Door43 refuses every device's sync forever | a missing or unreadable file on R is not a mismatch; the merge commit restores it (§9.2 N1, N2) | `sync_project_json_deleted_on_door43_restored` |
| L91 | A one-verse edit in another program that strips CR becomes a whole-book conflict | line-ending and trailing-whitespace differences are not changes at an import (§13.6 U3); the file's line endings are kept (§11.1 Q2) | `a_whitespace_only_change_asks_nothing_and_keeps_a_copy` (pending: S3) |
| L92 | `localizedNames`, other id authorities and OBS template edits are dropped at import and after outside edits | Every non-derived metadata field is a record at import (P9). An outside edit of `metadata.json` is copied, not recorded. An OBS template edit enters through Update a story (§3.4, §11.1, §13) | `loss_metadata_unmodelled_fields_kept`, `outside_edit_reconcile_conserves` (its OBS template edit) |
| L93 | An edit of a carried file's `.conflict-` copy cannot be recorded and is overwritten | it continues that version of the original record (§11.4) | `loss_conflict_copy_edit_kept` |
| L94 | The working folder is an invalid burrito during seals and intakes for longer than I7 said | every write group ends with a `metadata.json` rewrite; I7 states the windows; the crash tests assert validity after each (§1 I7, §16.2) | `every_written_state_is_a_valid_burrito` |
| L95 | A display name such as `Ana/Pedro` makes every USB export fail | the file name is cleaned and falls back to ids (§10.1 E4) | `a_display_name_never_makes_the_file_name_fail` |
| L96 | A large push cannot resume, so an upgrade never reaches Door43 on a poor link | large pushes are a chain of valid commits pushed one at a time; the exchange file is offered after repeated failures (§9.5) | `sync_large_push_chain_resumes`, `a_large_push_that_keeps_breaking_offers_the_exchange_file` (`crates/tc-sync/tests/sync/door43.rs`); `a_large_commit_is_a_chain_of_valid_commits` (`crates/tc-store/tests/store/intake.rs`) |

### 15.4 Cases found by the review of the fold

| # | Case | What prevents it | Test |
|---|---|---|---|
| L97 | Keeping one candidate of a passage conflict writes another person's version of the same record, the one a later clock prefers, and supersedes the chosen one; *Keep separate* picks between two concurrent texts of a verse by clock | a choice writes exactly the version picked; where a side, or a verse's own history, holds two different versions the person did not pick between, the choice asks (§8.3) | `loss_passage_keeps_the_picked_version`, `loss_passage_side_with_two_versions_asks`, `loss_keep_separate_never_picks_by_clock`, `loss_keep_joined_keeps_the_picked_span`; property `resolution_always_possible` |
| L98 | A version the fold's source cannot read (a locked file, a cloud placeholder) is taken as absent: two unread versions settle as equal, or an unread base lets the one readable head merge over a version nobody saw, and the next edit's token supersedes it | an unreadable value is unknown, never absent or equal: the record is listed as a conflict marked unreadable, no token names an unread version, no choice is made while one is unread, and `retry_unreadable` judges it again (§2.6) | `unreadable_versions_are_listed_never_settled`, `unreadable_equal_versions_wait_to_be_read`, `unreadable_base_never_merges_away_a_version`, `unreadable_settled_version_is_not_in_a_token` |
| L99 | Applying offline changes to a moved check applies only the newest editor's changes (by clock) and supersedes the others, and leaves out the units other than judgement, comment and bookmark | every decided version's changed units are applied, every unit of §7.3; two different changes of one unit refuse the choice (§8.3) | `loss_carryover_two_offline_editors`,  |
| L100 | *Choose per line* starts from one head's value and skips the units that merged cleanly, so the other person's agreed change (a comment) is lost; with no folded head it panics | the record starts from the merged agreed units and takes only the disputed units from the picks; it is offered only for a record's own conflict with disputed units (§8.3) | `loss_choose_per_line_keeps_agreed_units`, `per_unit_not_offered_without_units` |
| L101 | Two changes under one comment id that target different books file the comment under the book of whichever arrived first, so devices disagree on removal visibility, `RecordsWithoutBook` and the comments files | the comment is a record of every book a change under its id targets (§7.6); a comments file lists it with the book its shown version targets | `duplicated_comment_id_across_books_converges`; property `fold_converges_under_any_union` |
| L102 | A *Start empty* rewound after *Restore the text* rewinds only its own `StartOver` changes: a verse typed after starting empty shows beside restored verses, a mix of the two sides, and is on no screen once rewound | every verse change that is or descends from a rewound `StartOver` change is rewound, and listed as a candidate of the book's presence conflict; *Restore the text* restores each such verse (§7.6) | `loss_start_empty_descendants_rewound_and_listed` |
| L103 | A save the disk refused (a scanner held the file for longer than the retries) leaves a complete temp file; the person saves again at the same seq; the next open rolls the failed attempt forward beside the later save, and the project says *another computer was saving as this one* and shows a conflict between the person's two texts (Windows hazard 2) | a rename or removal another program holds is retried with a bounded back-off (§5.0 step 3); O2b decides a pending temp by `(device, seq)` after O8: a temp that collides with a later save is kept as a copy in `.tc/external/`, never folded, never removed (§6.1 O2b, §6.3) | `a_held_save_past_the_retries_is_not_saved_and_leaves_no_false_conflict`, `a_held_save_never_retried_is_saved_at_the_next_open`, `of_two_failed_attempts_the_later_one_is_kept`, `a_complete_temp_that_cannot_be_placed_rotates_the_device`, `an_open_never_removes_a_temp_it_cannot_read`, `crash_held_save_and_retry`, `a_save_whose_rename_is_held_a_few_times_is_durable` |

### 15.5 Cases found by the reviews of unit S2

| # | Case | What prevents it | Test |
|---|---|---|---|
| L104 | A pending file of this session's save is cut inside a line or changed in a line (a cloud client, a disk error): the seal refuses it, the close leaves it, the next open quarantines it with nothing salvaged, and the save is gone from the project and from the book file | the seal takes the lines that still verify from the file and the lines it lost from the fold's bytes, checked against their ids, and quarantines the file with its bytes (§5.11 K1) | `a_pending_file_cut_inside_a_line_is_sealed_from_memory`, `a_pending_file_changed_inside_a_line_is_sealed_from_memory_and_kept_in_quarantine` |
| L105 | A pending file loses a save the fold cannot give back (cut in place, or deleted before a crash) while the book file shows it: the next open takes the book file for the app's own stale bytes and writes it over, the last copy of the save | The ledger names every change the bytes show. When the fold no longer holds one applied and readable, a copy is kept with *Bring it back* before the file is written over (§6.2 N4) | `a_book_file_holding_a_save_its_pending_file_lost_in_place_is_recorded_at_the_next_open`, `a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded`, `a_book_file_whose_heads_were_put_back_is_recorded_not_written_over` |
| L107 | A group that cannot read a path drops the path's intent at M8, so a later retry takes the app's own bytes for an outside edit and lists the person's two saves as a conflict | A path keeps its `Writing` state until a read decides it, so a retry never asks a false question about the app's own bytes (§6.2) | `a_group_keeps_the_intent_of_a_path_it_could_not_read` |
| L108 | A pending file holding one batch of several changes is cut at a line while the app is closed after a crash: the batch waits for its lost member, and the open takes the book file that shows the whole batch for the app's own stale bytes and writes it over, with no notice | The fold does not hold a member of an incomplete batch as applied, so `holds` is false and the book file is kept with *Bring it back* (§6.2 N4) | `a_batch_cut_at_a_line_while_closed_is_kept_from_the_book_file` (`crates/tc-store/tests/store/model.rs`) |
| L109 | The same batch cut inside a line: the change salvaged from the file counts as held, so the kept copy's base keeps its digest, skips its record, and the book file is written over without it | a base keeps only the heads the fold holds applied and readable; others make the record be compared in full at *Bring it back* (§6.2 N4) | `a_salvaged_change_whose_batch_lost_a_member_is_kept_from_the_book_file` |
| L110 | A pending file or pack the open read is cut in place: the value of a head can no longer be read, the projection shows the verse as absent, and the job writes the book without it | a rendering that read a value the source could not give is not written; the file keeps its bytes (§6.2 R6) | `a_head_whose_value_can_no_longer_be_read_is_never_written_as_absent` |
| L113 | A save's journal line cut while the app runs and the version it saved replaced by a newer save: the job writes the book file, the last copy of the older version, over, because the fold's index still holds the change | `holds` reads the value of every version the bytes show that the projection no longer reads (§6.2) | `random_histories_lose_no_work` |
| L114 | A project detail whose value can no longer be read: the token rule leaves its head out of `metadata.json`'s heads, so the ledger names nothing to ask the fold about, and a later write drops the detail | the ledger names the version the projection shows when the token rule leaves it out (§3.6) | `random_histories_lose_no_work` |
| L115 | Bytes recorded once (a save the fold still held was skipped by its digest), then the journal loses that save and the next look records the same bytes again: a batch that reconciles the same path and md5 makes the second recording write nothing, and the file is written over without the save | The case stays, through *Bring it back*: an earlier batch reconciled the same path and md5, then the journal lost a save the file shows. A `why: lost` entry is never ended by an older reconciling batch (§6.4 X3), and *Bring it back* leaves out only the records that batch changed and the fold still holds applied and readable. The copy goes only after that | `random_histories_lose_no_work_long` |
| L116 | A process killed after a rename and before its directory flush; the next open takes the new bytes as placed and writes nothing, so the rename stays in the page cache; the journal line of a save the file shows is cut, and a power cut puts the bytes before the rename back: the save is gone | a read that settles `Writing` on its new bytes flushes the file and its directory before the ledger names them placed (§6.2, §6.4 M7) | `random_histories_lose_no_work_long` |
| L118 | `metadata.json` showing a project detail whose pending line was cut while the app ran, then a crash: at the next open the file was clean by stat (settled), so M1 took its stat for the bytes placed, never asked the table, and the job wrote it over without the detail | bytes about to be written over are decided even when clean by stat, and read when the table keeps them (§6.2 N4: a copy with *Bring it back*) | `a_settled_metadata_json_holding_a_lost_name_is_recorded_before_it_is_written_over` (`crates/tc-store/tests/store/model.rs`) |
| L119 | The only book file held by another program at an open that finds no ledger entry for it (`.tc/` lost), or its path held where a rebuild must go: `metadata.json` could list no book, the build failed as a bug, and the project turned read-only, refusing every save | a listing a blocked file leaves invalid writes nothing and waits for the file (§6.2) | `a_held_book_file_with_its_manifest_lost_leaves_the_project_writable`, `a_deleted_book_file_whose_path_is_held_leaves_the_project_writable` |
| L120 | A pending file damaged while the app was closed and held by a scanner that lets it be read: the open could not move it to quarantine, failed the write, and turned the project read-only | a file another program holds stays where it is for the next open, as the seal leaves one, and is never taken for a second writer's (§6.1 O2, §5.11 K4) | `a_damaged_pending_file_a_scanner_holds_leaves_the_project_writable` (`crates/tc-store/tests/store/model.rs`) |
| L121 | A seal of a new device killed after it made the device's pack folder and before it flushed the folder's entry; a later session's seal made the month folder inside it, flushed only the month folder's entry, wrote the pack, removed the pending files, and a power cut took the device folder with the pack: four saves gone | `secure_dir_chain` flushes the entry of every directory on the way that the session did not make, past the ones it just made (§6.1, "A directory found existing is not trusted") | `a_pack_in_a_folder_a_killed_seal_made_survives_a_power_cut` (`crates/tc-store/tests/store/model.rs`) |
| L122 | A held book file with no ledger entry, or its path held where a rebuild must go, in a job whose files of no book are a group of their own: that group found the listing invalid and failed as a bug, turning the project read-only (L119's fix counted only its own group's blocked paths) | any path the job could not read or write excuses an invalid listing (§6.2) | `a_held_edited_book_file_with_its_manifest_lost_leaves_the_project_writable` |
| L123 | `metadata.json` written by a group killed right after its rename, taken by the next session as the write's new bytes with the old placement's scope digest; the journal then lost the name the file shows, and the next open, whose fold no longer had it, found the stale digest equal to its own and took the file for converged: the name stayed hidden in the file, and the fold showed the old one | an entry's scope stands for the bytes it places or holds and is dropped when those change (§6.2) | `a_metadata_json_found_after_a_kill_past_its_rename_is_never_taken_for_converged` (`crates/tc-store/tests/store/model.rs`) |
| L124 | A pending file the open read cut in place, so a value of the book can no longer be read, and the book file deleted: the book file waits for the value (R15), and a later checkpoint's job (or the close's) found no book to list, failed the build of `metadata.json` as a bug, and the checkpoint and the close failed | a book file the folder lacks and the group does not write excuses an invalid listing like a blocked path (§6.2) | `a_deleted_book_file_waiting_for_an_unreadable_value_fails_no_checkpoint` (`crates/tc-store/tests/store/model.rs`) |
| L125 | `metadata.json` renamed by a group killed right after the rename, then held unreadable at the next open while a checkpoint's job ran: the job began a second write of it, so the ledger no longer named the first write's bytes, and the next read recorded the app's own bytes as an outside edit with a copy | No second write begins before a read settles the first, so the app's own first bytes are never taken for a change made outside (a false copy and notice) (§6.2) | `a_held_metadata_json_after_a_kill_past_its_rename_is_not_taken_for_an_edit` (`crates/tc-store/tests/store/model.rs`) |
| L126 | An unreadable projection cached without its readability fact is later written as complete | only complete projections enter the cache | `an_unreadable_prerender_never_replaces_the_last_copy_of_a_save`, `a_rendering_that_could_not_read_a_save_never_replaces_its_last_readable_copy` |
| L127 | A surviving unapplied outside-batch member suppresses its recovery record and spins the writer | An incomplete *Import* batch never marks its kept copy imported: `reconciles` counts only for folded, readable, applied changes (§6.4 X3a) | `crash_at_every_write_point_of_an_import_imports_once` |
| L128 | Salvaging an earlier pending path replaces a later path's source slot, then stale cleanup removes its readable source | cleanup relocates the current pending entry, including salvage aliases | both path-order regressions of `crates/tc-store/tests/store/salvage_sources.rs` |
| L129 | Unreadable or unpreservable metadata permits carried replacement and breaks the old listing | metadata denial defers the group; typed carried preparation precedes metadata preservation and execution | `carried_restore_waits_with_its_old_listing_while_metadata_is_unreadable`, `a_carried_restore_waits_until_outside_metadata_can_be_preserved` |
| L130 | Failed metadata preservation is deduplicated as kept, or an occupied final copy name is replaced | remember only durable copies; every new copy uses atomic no-replace creation | `metadata_copy_failure_is_retried_after_writes_resume`, `a_carried_restore_waits_until_outside_metadata_can_be_preserved` |
| L131 | A carried removal or same-stat replacement loses a save between comparison and filesystem action, including a sharing wait; a refused settled replacement keeps an obsolete listing | compare actual bytes immediately before every attempt; preserve changed bytes through the ledger and clear settled after refusal | `a_carried_edit_after_removal_check_is_preserved_before_unlink`, `a_book_file_saved_during_the_sharing_wait_of_its_removal_is_kept`, all regressions of `crates/tc-store/tests/store/last_look.rs`, `a_checked_write_rechecks_bytes_after_a_sharing_wait` |
| L132 | A missing, corrupt or cached-only old carried blob lets replacement or removal destroy its only readable History bytes; listing a refused placement attaches the new blob to the old bytes | The old-bytes half applies only to a version that is still a head: its bytes are written and flushed at their new place first, and its blob is stored again from the file when missing (§11.4). A replaced version that is no head may go (carried-files ruling rule 2). The listing half stays: a refused placement is listed with the blob of the bytes that are there | carried missing-blob and sharing-wait regressions of a head's bytes (see L151) |
| L133 | A temporary stat denial on a carried ingredient fails the whole materialization instead of waiting for access | both stat and content denial take R19 and keep the path dirty until retry | `a_carried_file_that_cannot_be_read_waits_for_its_replacement` |
| L137 | A placed carried version whose blob is there but cannot be read (a damaged object): the job failed, the project turned read-only at every open, and a checkpoint could not build its tree | the path waits (R19, `FileUnreadable`) and the rest is written; a tree takes in only a blob whose header reads, else keeps the version it had (§9.4, §11.4) | `a_restored_version_whose_blob_is_damaged_waits_and_the_project_opens_writable`, `a_checkpoint_keeps_the_committed_version_while_the_placed_one_is_damaged` |
| L139 | A save of this session whose pending file the seal cannot read (a scanner holds it) is left unsealed; if the file is cut meanwhile, the next open quarantines it and the save is gone | K1 seals from the fold's own bytes the changes it holds of a file it cannot read, and keeps the file | `a_save_whose_pending_file_the_seal_cannot_read_is_sealed_from_memory`, model history `a_pending_file_cut_twice_around_a_revert_and_a_held_open` |
| L140 | A placement's record named a head a journal loss took and a head the fold kept: the file edited back to the kept head's value compares equal to it, records nothing, and is written over with the project's newer text | An import of a file edited back to a kept head's value, after a lost head, compares that record with the fold's heads. The edit is a revert, so a conflict, never silently dropped (§13.6 U3) | `an_edit_back_to_a_kept_head_after_a_lost_one_is_kept_and_asked` |
| L144 | A changed book file is lost or written over while the question is open | The copy is durable before the state names it, and a file whose copy cannot be made is left as it is (§5.10). The file is left as it is until *Import* or *Not now*. Another save keeps another copy with its own entry. The state and the copy survive a restart, and the question is asked again (§6.2, §11.1) | `a_changed_book_file_is_kept_and_left_until_the_answer` (pending: S3), `a_question_left_unanswered_is_asked_again_at_the_next_open` (pending: S3), `a_second_outside_save_while_asked_keeps_both_copies` (pending: S3), crash rows X1, X2 |
| L145 | *Import* overwrites a verse changed on both sides since the file left the project | Parents are the base heads; a moved head makes a sibling, so a conflict (§13.6 U3) | `update_a_book_conflicts_a_verse_changed_on_both_sides` |
| L146 | The app's own interrupted write is taken for a change made outside (a false question or copy after a crash) | `Placed`, `Writing` old and new, and the projection are the app's own (§6.2) | `no_crash_of_the_apps_own_write_asks_a_question` (pending: S3) (kill at every write point of M3 to M7, with and without a power cut) |
| L147 | A changed `metadata.json` or sidecar is written over before its copy is durable, then a power cut | The copy and its folder are flushed before the manifest names it and before the rename (§5.10 *Keep and write*) | `an_outside_edit_of_metadata_json_is_copied_durably_before_it_is_rewritten` (pending: S3) |
| L148 | Updating from a file that left the project long ago reverts newer work | Base from the kept entry or the ledger; with no base, every disagreement between two non-empty verses is a conflict and unchanged verses stay (§13.6 U2, U3) | `updating_from_an_old_export_reverts_nothing` |
| L149 | An import leaves alignments on words the verse no longer has, or drops alignments of unchanged words | §13.6 U4: kept when the words are unchanged, reflowed in the same batch when they changed | `update_a_book_keeps_alignments_of_unchanged_words_and_reflows_changed_ones` (pending: S3) |
| L150 | A crash after the import batch imports the copy twice | `reconciles` ends the entry (§6.4 X3) | `an_import_interrupted_after_its_batch_is_not_imported_twice` (pending: S3) |
| L151 | Removing an old carried version's bytes removes a version still in conflict, or a version that moves (a provisional change, *Keep both*) whose blob was lost | Only bytes of a version that is no head may go; a head's bytes are written at their new place, from the file if the blob is missing, before the old place is written over or removed (§11.4) | `an_old_carried_version_in_a_conflict_is_never_removed` (pending: S3), `a_conflict_side_whose_blob_was_lost_moves_to_its_conflict_copy_before_the_path_is_replaced` (pending: S3), `keep_both_with_a_lost_blob_keeps_the_version` (pending: S3) |
| L152 | A Door43 edit made with another tool is replaced without a word | The intake reports it once, naming the commit and person (§9.2 N4, §11.2) | `a_door43_edit_by_another_tool_is_reported_once` |
| L153 | After a kill past a book file's rename, another program holds the file, and a checkpoint lists bytes that are not there: a valid folder made invalid, or a commit or exchange file built from the stale list | I7: the holder is another program, so only a read decides which bytes are there; an undecided path keeps the entry `metadata.json` has for it (§5.10 M2), the folder heals at the first open or checkpoint that reads it, and commits and exchange files come from the projection (§9.4) | `a_checkpoint_that_cannot_read_a_book_file_keeps_a_valid_folder_valid`, `a_checkpoint_while_a_book_file_is_undecided_commits_a_valid_burrito`, `a_checkpoint_after_the_held_book_file_reads_again_heals_the_folder`, `an_open_after_the_held_book_file_reads_again_heals_the_folder`, `an_export_while_a_book_file_is_undecided_is_a_valid_burrito` |

---

## 16. Test plan

Every test calls the public API the way `tc-app` does and asserts an observable result. Each
property and scenario below names the defect that makes it fail; a review checks that the test
does fail when that defect is introduced (§16.7). Tests never touch the network: Door43 is a local
smart-HTTP git server (a small Rust server around `git http-backend`, so server hooks run) plus
`tc-door43-fake` for the Gitea REST API.

### 16.1 Property tests (`proptest`)

The generators must reach the states they claim. Each run counts the states it produced
(conflicts per kind, criss-cross bases, structure issues, book removals with unseen work,
incomplete batches, duplicate devices, damaged packs) and fails if a counter is zero over the
run (tC4 D51: a generator that re-folds after every event never builds a fork). A state counts
only when it is in a state a property asserts on, never when it was only passed on the way.
Devices keep their own stale views on purpose: a property of one device's fold asserts on the
view, before the last exchange, of the device holding the most defects (held batches, missing
parents), and again once every device holds everything. The team generator lives in `tc-journal/tests/sim`; the fold
properties run there over the in-memory union, and its `Transport` trait is how `tc-store` (the
exchange zip) and `tc-sync` (the local git server) run the same scripts through their transports.
Its scripts write every record kind but OBS frames, whose records go through the same merge code
and are covered by the projection tests.

| Name | Generator | Assertion | Fails if |
|---|---|---|---|
| `fold_converges_under_any_union` | 3–5 devices make random edits of every record kind (verse words and layout, spans, joins, renumbers that shift ranges, marker drags that move words, chapters, `BookLayout` books, alignments, decisions, comments, pins, settings, project details with fields from a newer minor, OBS frames, carried files, book remove, restore with text and start empty, batch releases, resolutions) from their own stale views, and exchange random subsets through three transports: in-memory union, exchange zip, and the local git server through the real sync code | once two devices hold the same set: identical `frontier`, conflict list, anomaly list and derived bytes; independent of order, grouping and duplication | any rule reads arrival order, a map's iteration order, the clock or the machine |
| `conservation` | as above | every valid change is in exactly one state of I2, and every state-(c) change was named as a parent by a change whose author held it | a change disappears, or an automated writer names a parent it did not display |
| `concurrent_differences_are_visible` | as above | for every pair of changes on one key that are not ancestor-related and have different values: they are in a listed conflict, or a merge in which every agreed unit was changed by at most one value relative to every base | a merge rule picks between two different changes silently |
| `stale_tokens_never_supersede_unseen` | screens that hold tokens across exchanges | no head disappears unless the writing screen displayed it | the actor uses current heads instead of the token |
| `incremental_equals_full` | every step of the generators above, including batches completed late and parents arriving after children | the incremental state equals a fold from scratch | an incremental update forgets a key, a chapter or a book |
| `fold_never_panics` | arbitrary bytes as packs and pending files; well-formed but odd sets (unknown parents, parents on other keys, duplicate devices, incoherent spans, unknown kinds, giant batches) | no panic; every defect is an anomaly | any step can fail |
| `heads_and_text_conflicts_read_no_clock` | the same history with permuted `at` values and device ids | identical heads, merges and text conflicts; only the provisional choice and the newest-wins settlement (H5) may differ | a rule reads `at` or the device |
| `resolution_always_possible` | any fold state | resolving every conflict, with any choice, yields a state with no conflict on those keys | a resolution needs an ancestry condition |
| `merge_units_sound` | records with criss-cross histories and random unit changes | an agreed unit's value equals the value written by every head that changed that unit relative to every base | the base choice lets one head override another |
| `provisional_usfm_never_drops_words` | concurrent spans, splits, joins, renumbers that shift ranges across a chapter boundary, marker drags, *Start empty*, verse edits and paragraph breaks, in books with and without a `BookLayout` | per chapter, the multiset of words in the projected USFM equals the multiset of the chapter state built from the provisional side of every `Passage` conflict; every record whose projected value is `Text` has its words in the USFM exactly once; an incoherent covered verse shows its newest own text; every incoherence is listed | the coherence pass drops a verse, a passage is judged verse by verse, or the layout skips a record |
| `every_head_reachable_from_a_screen` | the generators above, with carry-overs, renumbers, joins and removals concurrent with comments, alignments and decisions | every head of every record kind is placed on a screen (a verse, a run's aligner, a check, a settings page, History of a removed book), is a conflict candidate, or is listed under Detached or as an anomaly | an attached record is left on an identity no screen shows |
| `mixed_minor_versions_converge` | the same change set folded by the current app and by a build of the previous minor, with changes that carry inert newer fields and causes. Until a previous minor is released, `tc-journal` emulates it: the team history written again with inert newer-minor fields on a third of its changes (an older device's edit of such a record drops them, as it would) | identical heads, conflict lists and derived bytes; emulated: every change on its own key, the same heads, every conflict of the plain history still listed (a passage group may grow, by its passage's records, when one of them differs in a newer field: such versions are not equal values, F6), an added conflict only where versions differ in a newer field, and no newer field in any derived file | an unknown field moves a change to another key |
| `arbitrary_meta_never_blocks_commit` | arbitrary JSON as `ProjectMeta`, pin and scope values, from commands and from imported files | `build_metadata` succeeds, the document passes `tc_burrito::check`, a checkpoint commits, and every invalid value is listed as `InvalidForBurrito` | any value can make M2 or C3 fail |
| `a_newer_removal_removes_and_a_newer_edit_keeps` | removals concurrent with edits of every record kind of the book | the newest of the removal and the unseen edits decides; every change the remover did not hold stays in History; a Bible project always projects a book | removal visibility ignores a record kind |
| `update_a_book_conserves` (to write: S3 step 9) (replaces `outside_edit_reconcile_conserves` and `outside_edit_reconcile_conserves_nightly`) | random files imported through §13.6, with and without a base: small edits, reverts to older values, verse deletions, more than 50 keys, line-ending and trailing-space rewrites, book files for absent books, aligned verses, empty verses on either side | no head lost; reverts and removals are conflicts; whitespace-only differences import nothing; a file for an absent book adds it; with no base every disagreement between two non-empty verses is a conflict and the empty-verse rule holds (§13.6 U3); no no-base change is provisional | a known-base import replaces a value it did not see, or a no-base import replaces a teammate's text |
| `a_door43_edit_by_another_tool_is_reported_once` (to write: S3 step 9) (replaces `door43_ingest_deterministic`) | commits by other tools fetched by several devices in different orders | each such commit is reported once on each device, by commit id; the merge tree takes the fold's projection (§11.2) | the intake merges a file from such a commit, or reports it at every sync |
| `exchange_idempotent_and_relayed` | exports and imports in random order, repeated, relayed A→B→C | importing twice equals importing once; a relay equals direct exchange | intake writes something that depends on the importer |
| `consolidation_loses_nothing` | seals and consolidations interleaved with crashes, exchanges, and packs deleted or damaged in the working tree between steps; the census counts consolidations completed, kills between K2 and K4 and kills between Q2 and Q4 | the union of change ids never shrinks; every tree holds the expected pack set and passes the redundancy rule's safety check | a replaced pack held an id the new pack lacks, or a tree is built from the working tree alone |
| `codec_round_trip` | the tC4 USFM identity corpus plus generated USFM (spans, poetry, headings, CRLF, tabs, odd marker whitespace, out-of-order verses) | `recompose(decompose(x)) == x`; `join_verse(split_verse(c)) == c` | a byte is normalized |
| `canonical_change_round_trip` | arbitrary `ChangeBody` values | canonical bytes parse back to the same value and re-hash to the same id; the line verifies | the encoder and parser disagree |
| `pack_round_trip_and_salvage` | packs with random bit flips | an intact pack passes P2–P4; a damaged one salvages exactly the lines whose bytes are intact; no salvaged change differs from what its author wrote | a damaged line is accepted |
| `a_pack_with_lines_moved_or_replaced_is_damaged` | packs with a line dropped, duplicated, swapped with another or replaced by another valid line, or with a header replaced by another valid one or damaged | the pack is damaged at its path (no byte of a line is changed, so only the name of §4.3 can tell); salvage takes exactly the change lines the file holds intact | the name leaves out the header, a line, or the order of the lines |
| `any_changes_come_back_from_the_cache_unchanged` | arbitrary `ChangeBody` values in packs and pending files, cached at one file number and read back at another | every prepared change from the fold cache equals the one reading the file gives | the cache drops or alters a field of what the fold keeps |
| `a_damaged_index_never_gives_other_changes` | every single-bit flip and every truncation of a fold cache; arbitrary bytes after a valid header | the cache is unreadable or the section unused, never a section that gives other changes; no panic | a section is used without its fingerprint, or the decoder trusts a length |
| `a_changed_byte_in_the_corpus_opens_as_without_the_cache` | a fold cache of the operation-count corpus, then any byte of any of its files changed (the deterministic tests beside it: every byte and every length of every file of a small project, a file replaced by one of the same size, files added, removed, renamed and grown, a consolidation, a damaged, truncated, foreign, older or newer cache) | the open with the cache gives the full fold output (heads, conflicts, anomalies, Detached, every derived file) and the per-file reports of the open without it; the changed file is read | a section stands for bytes other than those it was made from, or hides a damaged file |
| `sidecar_decode_render` | random decisions and alignments | decoding a rendered sidecar gives the same records | an import or *Bring it back* decode would invent a change |
| `every_written_state_is_a_valid_burrito` | every state any test writes (§16.6) | `tc_burrito::check` passes | a writer skips the check or lists a wrong md5 |
| `random_histories_lose_no_work` (`tc-store/tests/store/model.rs`; the named histories the reviews found, then 200 histories of up to 7 chunks in `cargo test` (about 20 s); `random_histories_lose_no_work_long` with 5,000 of up to 15 chunks nightly, which must take every row of the ledger's table the model can reach, `ROWS_OUT_OF_REACH` listing the rest) | a reference model of a Titus project run against the real store on a `FaultFs`: saves of a verse, of a section (one batch of two changes) and of the project's name; leaves, checkpoints and closes, each killable before any of their first 80 write points, with or without a power cut, torn or not, or right after the rename of a file; outside edits and reverts of the book file and of `metadata.json`, told to the watcher or not, reverts that keep the old mtime, deletions and touches, while open and while closed; pending files and packs cut at a line or inside one, changed in a byte or deleted, in place while open or while closed, and a pending file deleted while a seal runs (at each of its steps, through the seal hook); files and pending files another program holds; a lost manifest or heads files; a step `Answer { import: bool }` (*Import* or *Not now*) on a kept book file; restarts; the window's focus; the clock. The generator favours a crash followed by damage while closed, a kill after a rename followed by a revert, and a file held around an open | after every step and every open: every save is a readable version in History, or waits in a batch that lost a member no file in the folder showed when the open read the journal; every put is in the folder, is what the project writes there (a `metadata.json` put is retired once it lists the folder as it is or as the ledger places it, and any put once the ledger names its bytes placed), is copied to `.tc/external/`, or is bytes of a write the app had begun (`Writing.old`) whose records are all versions in History; a put of a book file is kept until the answer, and after *Import* every verse the put showed is a version in History; a put of another file is copied, with a notice, never saved; a save a held file shows counts as kept by the hold's copy; unless damage took every copy (a save of the running session counts as held until its next seal). With no outside step, no question, no notice and no copy in `.tc/external/`. Saves succeed unless the process died, or an outside step or a held pending file explains a refusal by a rule; checkpoints too, and skip only for no changes, or for a seal or pack an outside step or a held file stops | the ledger lets bytes be written over that are the only record of a person's work; the app keeps its own bytes twice or turns the project read-only by mistake |
| `every_pair_of_the_table_keeps_every_byte` (`tc-store/src/ledger.rs`) | every state × event × facts of the transition table (§6.2) | a write or removal only of bytes that are the app's own (§6.2) whose changes the fold holds and can read, or of the projection; bytes that are not the app's own are kept (a copy, and for a book file a hold); a held file is written over only after *Import* or *Not now*; the hit list is N1 to N11, R15 to R19 and the events, with `the_reviews_cases_are_kept` | a row drops bytes |

### 16.2 Crash injection (P4)

`FaultFs` implements `Vfs` with a model filesystem that tracks, per file and per directory entry,
what is durable and what is only in the page cache. The harness can stop the writer before or
after any `write`, `sync_all`, `rename`, `remove_file`, `create_dir` or directory fsync, and can
simulate power loss by dropping every non-durable byte and directory entry (a lost rename on
Windows included). libgit2 writes outside the `Vfs`, so git steps are injected by kill hooks before
and after each git2 call, and power loss for `.git` is simulated by restoring a copy of `.git` taken
before the call or by truncating its newest loose objects.

For each action below, kill at every point, with and without power loss, then open the project
and assert (the power-loss model decides what each flush makes durable, so it is set before the
action runs, and the action runs once under each model; a save the disk answered with
`WrittenNotDurable` counts as saved except after a power cut under the strict model): the state is the action's before or after state (§6.4); every save that returned
"Saved" is present (the model drops an unflushed rename, so this fails if `atomic_write` skips
the rename flush of §5.0 step 4 or open deletes a complete temp file); no batch is half applied;
the folder passes `tc_burrito::check` after open, including after a crash inside every write
group of I7 (seal, consolidation, intake packs, materialization); no path is left in the ledger's
`Writing` state; a
second open changes nothing. The restart's own work must survive a power cut as well: after
each kill the project is opened again and closed (which seals and materializes what the kill
left), and a power cut is taken after that; every change the folder held before the close must
still be held (a kill between the creation of a folder and the flush of its entry, then a
restart that writes into it, then a power cut).

| Action | Crash rows (§6.4) |
|---|---|
| save, section save, draft | W1, W1a, W2–W4, D1 |
| seal, consolidation | K1–K4, Q1–Q2 |
| materialize (including the compare-and-swap path) | M1–M6 |
| checkpoint commit | G1–G4 |
| sync against the local git server (fetch, intake, merge commit, push, confirm) | Y1–Y6 |
| USB export and import | U1–U2, I1–I2 |
| a changed file kept, asked and imported | X1–X6 and X3a |
| resource upgrade | A1–A2 |
| create, join, tC4 import, tC3 import | N1–N2 |
| repack | R1 |

A soak test runs the real binary as a subprocess, drives scripted saves, syncs and imports, sends
`SIGKILL` at random times for an hour, and checks the same assertions after every restart.

### 16.3 Named regression scenarios

Each scenario of §15 is a test with that name in `tc-sync/tests/loss/` or `tc-store/tests/loss/`.
Each asserts what the person sees (the conflict list, the review rows, the notices, the projected
verse) and the on-disk end state (the packs, the derived files, `metadata.json`, the commit tree).
The scenarios the fold decides also have a test of the same name in `tc-journal/tests/loss.rs`
that asserts the fold's half: the conflict list, the projected values, the placements, the
anomalies and the derived bytes, the same for every fold order, incremental or whole.
The tC4 team-sync fixtures (`different-verses`, `verse-two-ways`, `check-two-ways`,
`invalid-file`, `changed-own-file`) are ported first. Scenarios that need two or three devices use
separate project folders with separate app-data directories and installation secrets.

Scenarios of the rulings of 2026-10-21 and 2026-10-22, besides those §15 names for L144 to
L152, each (to write: S3 step 9): `a_changed_book_file_is_asked_once_with_every_book_found_together`,
`not_now_keeps_the_project_text_and_lists_the_copy_with_import`,
`import_marks_changes_as_imported_and_writes_the_book_file_from_the_project`,
`a_book_file_for_a_book_the_project_lacks_is_asked_and_not_now_moves_it_out`,
`an_undecodable_change_is_kept_and_offers_only_not_now`,
`a_whitespace_only_change_asks_nothing_and_keeps_a_copy`,
`an_outside_edit_of_a_sidecar_is_copied_and_rewritten_with_one_notice`,
`a_read_only_open_asks_nothing_and_writes_nothing`, `keep_this_one_removes_only_the_other_file`,
`not_now_with_the_book_file_held_open_writes_it_later_with_no_second_copy_or_notice` (N3a),
`a_second_outside_save_of_a_held_file_keeps_the_first_in_the_notices`,
`a_full_disk_leaves_a_changed_file_as_it_is_until_its_copy_is_made`,
`a_no_base_import_never_becomes_the_provisional_text`, `an_empty_verse_imports_without_a_conflict`
(H20, both directions), the crash tests for rows X1 to X6 and X3a of §6.4 (with
`kill_everywhere` and power cuts), and the model's step `Answer { import }` with an oracle that
checks that a put of a book file is in the folder or in a kept copy until it is imported or
dismissed. The tests of the old recording that these change keep their names and now assert
keep, question and import: `loss_materializer_never_overwrites_outside_edit`,
`loss_outside_mass_delete_is_conflict`, `loss_outside_revert_is_conflict`,
`loss_unreadable_outside_edit_copy_kept`,
`an_edit_made_while_the_app_was_closed_is_recorded_and_agrees_with_the_project`,
`take_in_held_records_as_conflicts_and_ends_the_hold`,
`an_edit_with_no_heads_file_is_an_unknown_base_conflict`,
`a_heads_file_without_digests_still_records_an_outside_edit`,
`an_outside_edit_found_without_a_manifest_uses_the_head_base`,
`two_saves_from_a_program_that_keeps_the_file_open_are_no_conflict`,
`an_edit_back_to_a_kept_head_after_a_lost_one_is_recorded`,
`the_watcher_records_an_edit_without_a_focus`, `the_watcher_on_a_real_folder_records_an_edit`,
`loss_local_book_file_added_projects`; copied and written again with one notice, nothing
recorded: `an_outside_edit_of_a_sidecar_records_decisions_and_ignores_flags`,
`an_outside_edit_of_an_alignment_file_is_recorded`, `loss_metadata_unmodelled_fields_kept`
(outside half); Update a story: `loss_obs_template_edit_recorded`; kept and asked (a book) or
copied (`metadata.json`), the import's base the stopped group's placement:
`an_outside_revert_after_a_stopped_group_is_recorded_as_a_conflict`,
`an_outside_revert_of_metadata_json_after_a_stopped_group_is_an_edit`,
`an_outside_edit_of_the_bytes_a_stopped_group_left_continues_them`; N4's copy with *Bring it
back*: `a_book_file_holding_a_save_its_pending_file_lost_in_place_is_recorded_at_the_next_open`,
`a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded`,
`a_book_file_whose_heads_were_put_back_is_recorded_not_written_over`,
`metadata_json_holding_a_project_detail_whose_pending_file_was_deleted_is_recorded`,
`a_commit_takes_a_file_waiting_for_an_outside_edit_from_the_snapshot`; the copy before the
replacement, nothing recorded:
`unreadable_but_writable_metadata_keeps_the_outside_name_until_it_can_be_read`,
`a_same_size_same_mtime_edit_of_settled_metadata_is_kept_before_replacement`; no question and no
copy after any crash of the app's own write:
`crash_materialize_ledger_tells_its_bytes_from_an_outside_edit`,
`crash_materialize_with_a_carried_file_and_a_held_file`; a same-stat change kept and asked (a
book) or copied and written again: `loss_bit_rot_not_recorded_as_edit`; the
`<stem>-<Door43 user>.<ext>` name: `loss_carried_file_two_versions_both_kept` and `a_kept_copy_never_takes_a_held_name` (tc-journal).

### 16.4 Journeys (P1, P2)

Playwright drives the real UI against the real server for:

- **J17** Receive a project from Door43 (and from an exchange file), continue offline.
- **J18** Keep one version of a conflicted verse; the next sync carries the resolution.
- **Team sync:** three users edit the same book offline (same verses, different verses, spans,
  checks, alignments), then sync in every order through the local git server, with two pushes
  racing; all three end with identical projects and the same conflict list.
- **USB both directions** while both sides keep editing; then a relay through a third person.
- J8 checkpoints (exactly one commit when leaving with changes, none without), J11 share, J12 and
  J13 with a teammate's concurrent checks, J9a, J9c and J9d imports with byte-identical text (P9),
  and the tC4 4.0 import of every tC4 conformance fixture.
- **Opening a project** (tC4's J15 cases, PERFORMANCE.md rule 4): a small project shows no
  indicator before 300 ms; the whole-Bible corpus shows determinate progress from 300 ms on,
  with phases in order and counts that only grow, and the indicator leaves no earlier than the
  project's text is on screen; a folder that stops being readable shows its error and leaves no
  indicator. Unit S1's `open_shows_determinate_progress_by_300_ms` proves the store's side.

### 16.5 Probes and measurements before the format is frozen

1. **Scale corpus.** Generate the §14 corpus; measure cold open, save, materialize, checkpoint,
   sync and USB export on the reference laptop (§14); the open budgets are 3 s with a usable
   fold cache and 10 s without one or with a cold disk cache.
2. **Shallow join.** Fetch, commit and push from a depth-1 clone against Gitea (a container where
   available; the local server otherwise). Until it passes, joins are full clones.
3. **Ref rejection.** A server hook and a protected branch reject the ref after the pack;
   `push_update_reference` must surface it (`sync_f11_rejected_ref_not_success`).
4. **Derivations.** `plain_text_v1`, `tokenize_v1` and `selections_v1` against golden outputs
   produced once by usfm-js 3.4.3, string-punctuation-tokenizer 2.0.0 and word-aligner-lib over
   tC4's fixtures and a generated corpus, including tC4's sample hash
   `f8aee622daff1496bccc7c2e3b358b43`; tC4's draft test (`codec::tc4_indexer_drafted`) and `shown_text_v1`
   against tC4's own `verseTextIndex` run over every verse of its plain drafts and of drafts
   written for poetry, lists, notes and markers inside a line
   (`fixtures/usfm/check-text-vectors.json`).
5. **Windows.** Rename and flush behaviour on NTFS with a real power cut, on the release
   pipeline's Windows machine: that `sync_all` on the directory handle opened with
   `FILE_FLAG_BACKUP_SEMANTICS` and on the renamed file makes the rename durable (§5.0 step 4).
   The release is blocked until it passes; if neither call is enough, the format is not frozen
   and the save protocol is redesigned before release.
6. **Fuzzing** (`cargo-fuzz`): the change-line parser, the pack parser, the codec, the sidecar
   decoder and the zip reader. **Bit flips:** 10,000 random flips in packs and pending files;
   every one is rejected or salvaged exactly.
7. **Cloud folders.** Detection of OneDrive (including Known Folder Move of Documents),
   Dropbox, iCloud Drive and Google Drive folders and of placeholder attributes on each
   platform, and the second-writer detection with two machines sharing one OneDrive folder.
8. **Export verify.** That the E5 read bypasses the cache on each platform, checked with a
   stick whose contents are changed behind the cache.

### 16.5a Operation counts of the open (PERFORMANCE.md rule 2)

`tc-store` (units S1 and S2) counts, in its tests, the derived files read and their bytes, the
files projected, the heads files read and the packs read at each open, on the Psalms corpus and
on the corpus ten times larger:

| Test | Asserts |
|---|---|
| `an_open_after_no_change_projects_no_file_and_reads_no_derived_file` | open, close, open, close, open: the second open reads at most the files the first session's last materialization wrote (their entries are unsettled), projects none of them, settles them and writes the manifest again; the third open, counted at the moment it is usable, projects 0 files and reads 0 derived files (`derived_read`) and 0 heads files, on both corpora; its storage check (O14) then reads each derived file once (`check_read` equals the number of derived files). A `clean_by_stat` that is never true, which reads every derived file at open, fails it. The second open finds no fold cache and the third the one it wrote, and each opens every pack and pending file once: the reading thread hands its handle to the fold's `ValueSource`, so a value the fold reads later never opens the file again |
| `a_record_entering_a_files_scope_projects_it` | the first comment on a book and a teammate's new check decision, folded from a pack received between two opens, whose files on disk are clean by stat: the next open projects that book's comment and decision files and no other file |
| `an_open_after_one_save_projects_only_that_books_files` | a verse saved and materialized before close: the next open projects nothing; a verse saved but not materialized (a crash after B3): the next open projects only that book's files and `metadata.json`, the same count on both corpora |
| `a_racy_or_mtime_restored_file_is_never_overwritten` | on a `FaultFs` with 2 s mtimes: a same-size edit within the tick of the app's write; the same edit followed, before the next open, by another book's materialization that writes the manifest again; and an edit whose old mtime is put back after its entry settled. The first two are read at the next open (and at window focus), the third by the storage check; all three are kept and asked about: the file keeps its bytes, nothing is recorded, a materialization of that book leaves it, *Import* imports the copy through §13.6, *Not now* rewrites the file. No file is written over before its copy is durable and the person chooses |
| `a_save_reads_no_pack_and_writes_a_bounded_number_of_files` | a verse save opens no file, reads only the values of the records it changes from where they lie (under 64 KiB, never a whole pack), creates one pending file and flushes twice (the file and its folder), the same write points on both corpora |

Unit S2 adds the counts of the writer (§5.10), the checkpoint (§5.13) and the import of a
changed book (§13.6), on the same two corpora; each asserts the same counts on both:

| Test | Asserts |
|---|---|
| `a_book_change_marks_only_that_books_files` | adding a book projects its USFM and reads and writes no other book's file (on the larger corpus only, the one with other books) |
| `an_unchanged_file_is_not_read_back_and_each_buffer_is_hashed_once` | one Psalms verse saved, then `leave`: the files opened and renamed are that book's files, their heads files, `metadata.json`, the manifest and the intent, no journal file and no other book's; a decisions file that renders equal to its entry is neither read nor written; the bytes hashed are the bytes rendered plus the bytes read for the compare-and-swap, once each (`bytes_hashed`) |
| `a_checkpoint_after_one_save_writes_only_changed_trees` | a Psalms save and a checkpoint write the same number of trees (at most ten) and blobs on both corpora: the changed files, the new pack and `metadata.json`; no unchanged derived file is read |
| `updating_a_book_costs_what_it_changed` (to write: S3 step 9) (replaces `an_outside_edit_costs_what_it_changed_at_the_open_and_by_the_watcher`) | three Psalms verses replaced inside `PSA.usfm`, kept and imported: one derived file and its heads file read, one decode, three keys compared value by value (`outside_keys_examined`), three changes written, on both corpora (D10: a record whose shown digest is the heads file's is not looked at) |
| `a_focus_reads_only_what_is_not_clean_by_stat` | the window focus stats each path the app wrote once and reads those whose size or time differ |

Where an earlier unit's row says "recorded" for a changed file, the file is now kept and asked
about (a book file) or copied and written again (§6.2), whatever its stat. A pack a teammate sealed is put into the
folder between two opens, since sync is unit X's.

### 16.6 Burrito validity everywhere (P5)

Every state a test writes runs through `tc_burrito::check`: the working folder after every
completed write group (I7) and every open, every commit tree read back from git, every exchange
file, every export. The check is the vendored SB 1.0.0 schema plus the code checks of §3.4 and
the exact-set, md5 and size check. The 48 schema controls of the research are a table test.
There are zero allowed failures.

In `tc-store` the check is not left to the tests that remember it. With the `test-hooks`
feature, the writer calls a probe the test installed (`Hooks::check_burrito`) with the project
folder after every job that wrote files, when no write group is open (`Shared::group_open`: the
actor raises it before a checkpoint's restore and seal and the writer lowers it when the
materialization that lists the new packs ends) and no kept book file waits for its answer (the
folder then lists its bytes as they are, I7). The rig of every store test installs a probe that
validates the folder with `tc_burrito::validate_project` over the files of the test
filesystem, read without moving the operation count that kill points use, and fails its test
at drop when the probe named a problem, unless the test killed the filesystem on purpose or
took a file out of the folder for good (`damaged_on_purpose`). That is
`every_written_state_is_a_valid_burrito` for every test of the crate, and the test of that
name runs each command of §2.7 on a project with every record kind, then a checkpoint and a
close, and asserts the folder and every commit after each. `assert_commits_valid` reads every
commit of the history back through git2 and validates each tree; every checkpoint test and
every reopen of the crash tests calls it.

### 16.7 Tests that can fail

For each mechanism, a mutation that removes it must turn a named test red:

| Remove | Must fail |
|---|---|
| the token rule (use current heads as parents) | `loss_f5_stale_buffer_never_reverts`, `stale_tokens_never_supersede_unseen` |
| the conflicted-token rule (list all heads) | `loss_edit_conflicted_verse_keeps_conflict` |
| the compare-and-swap in materialize | `loss_materializer_never_overwrites_outside_edit` |
| the no-base and revert rules of the import (§13.6 U3) | `loss_outside_mass_delete_is_conflict`, `loss_outside_revert_is_conflict` |
| removal visibility | `loss_book_removed_while_edited` |
| batch completeness | `loss_incomplete_batch_held_and_listed` |
| the coherence pass | `provisional_usfm_never_drops_words` |
| the base agreement check in merge units | `merge_units_sound` |
| the re-read before removing pending files | `crash_seal_verifies_before_delete` |
| the one notice per commit by another tool at intake (merge its files, or say nothing) | `a_door43_edit_by_another_tool_is_reported_once` (to write: S3 step 9) |
| the push confirmation | `sync_f11_unconfirmed_push_reported` |
| passage conflicts (judge each verse alone) | `loss_section_move_vs_verse_edit`, `loss_renumber_shift_vs_edit_by_number`, `provisional_usfm_never_drops_words` |
| per-device content-hash pending names (use a counter) | `an_unknown_journal_file_appearing_turns_the_project_read_only` |
| the expected pack set (build trees from the working tree) | `loss_pack_deleted_outside_app_restored`, `loss_pack_deleted_while_open_never_pushed` |
| the known key for newer fields (parse as `Unknown`) | `mixed_minor_versions_converge` |
| total `build_metadata` (check the whole document only) | `arbitrary_meta_never_blocks_commit` |
| the rename flush and the roll-forward of complete temp files | `crash_lost_rename_rolled_forward` |
| the roll-forward of every complete pending temp at O2 (decide them by `(device, seq)` after O8) | `a_held_save_past_the_retries_is_not_saved_and_leaves_no_false_conflict`, `of_two_failed_attempts_the_later_one_is_kept` |
| the retry of a held rename or removal (fail at the first refusal) | `a_save_whose_rename_is_held_a_few_times_is_durable`, `a_held_rename_is_retried_within_the_bound` (`atomic.rs`) |
| the removal of a temp the open cannot read (remove it as damaged) | `an_open_never_removes_a_temp_it_cannot_read` |
| the flush of a directory found existing (trust it) | `crash_checkpoint_into_a_months_folder_that_does_not_exist_yet`, `a_directory_found_existing_is_made_durable_once` (`atomic.rs`) |
| the compare-and-swap before each rename retry | `loss_rename_retry_never_overwrites_outside_save` |
| anchors and Detached (place attached records by number) | `every_head_reachable_from_a_screen` |
| foreign entries (build merge trees from owned paths only) | `loss_foreign_entries_kept_on_tip` |
| `commit_received` (stamp received changes) | `door43_ingest_written_verbatim` |
| the fold cache's key (take a section by path alone, or by path and size) | `a_section_stands_only_for_the_bytes_it_was_made_from`, `an_open_from_the_cache_is_the_open_from_the_files`, `a_changed_byte_anywhere_opens_as_without_the_cache`, `a_file_replaced_by_one_of_the_same_size_opens_as_without_the_cache` |
| the fold cache's validity (cache a damaged, foreign or newer file, or one with a quarantined line) | `a_file_the_cache_cannot_stand_for_has_no_section`, `a_changed_byte_anywhere_opens_as_without_the_cache` |
| the fold cache's section fingerprint | `a_damaged_index_never_gives_other_changes` |
| the fold cache's probe stamp (read a cache whatever summaries its build made) | `an_index_of_a_build_that_summarizes_otherwise_is_not_read`, `the_encoding_is_pinned` (a stub rule that drops a class: `the_probe_covers_every_class`) |
| settled entries (trust an entry once the manifest's own mtime is 2 s past the file's) | `a_racy_or_mtime_restored_file_is_never_overwritten` |
| the scope digest from the fold (compare only the records the heads file lists) | `a_record_entering_a_files_scope_projects_it` |
| the change ids in a loaded file's facts (move the changes out of what the redundancy rule and O7 read) | `the_open_s_rules_read_the_same_ids_from_a_read_and_from_the_cache` |
| one owner of the validity rules (a screen that hashes its shown text, or reads a pin the ladder stops at as absent) | `the_psalms_corpus_files_are_what_the_screens_compute`, `edits_joins_renumbers_and_pins_change_files_and_screens_alike` (`tc-journal/tests/screens_and_fold.rs`: what `tc_check::status::CheckState` and `tc_align::verse_status` compute from the projected USFM and `resources.json`, read as the screens read them, equals every `invalidated`, `verseEdits`, `status`, `userInvalid` and `invalid` the fold writes, over tC4's sample project, the aligned en_ult Titus, joins, renumbers, cleared verses, unusable and unrecorded pins, and the Psalms corpus) |

| settled entries read by O9 (a `clean_by_stat` that is never true: every derived file read before the open is usable) | `an_open_after_no_change_projects_no_file_and_reads_no_derived_file` |
| the release of a hold whose file is gone (M0, O9) | `a_held_file_deleted_while_the_project_is_closed_is_rebuilt`, `a_held_file_deleted_while_the_project_is_open_is_rebuilt` |
| a removed carried file in X | `a_removed_carried_file_leaves_the_folder_and_the_burrito_stays_valid`, `a_removed_carried_file_that_was_edited_outside_is_not_deleted` |
| listing fixed, carried and held files from their bytes (a held book file as it is) | `a_carried_file_edited_outside_is_recorded_and_listed_as_it_is`, `a_fixed_file_edited_outside_is_restored_and_a_copy_is_kept`, `a_held_file_that_changes_again_is_recorded_and_both_copies_stay` |
| no copy replaces another in `.tc/external/` | `two_outside_edits_in_one_second_keep_two_copies` |
| the copy of changed bytes of `metadata.json` when no manifest entry names them | `an_outside_edit_of_metadata_json_is_copied_when_the_manifest_is_gone`, `an_outside_edit_of_metadata_json_is_copied_when_the_manifest_does_not_parse` |
| a best-effort open (a write failure → read-only, an unreadable derived file reported, a failed stat listed) | `an_open_on_a_full_disk_is_read_only_and_never_refused`, `a_disk_that_fills_during_the_opens_materialization_leaves_a_read_only_project`, `an_open_goes_on_when_a_derived_file_cannot_be_read`, `loss_pack_cloud_placeholder_unreadable_not_damaged` |
| no wait on the actor for a derived file another program holds | `a_derived_file_another_program_holds_never_stalls_a_save` |
| the notices of holds listed again at open | `a_held_files_notice_comes_back_after_a_restart`, `a_dismissed_notice_of_a_held_file_keeps_its_copy` |
| a new device when a pending file cannot be read | `an_unreadable_pending_file_makes_this_device_start_a_new_one` |
| the materialization after a failed seal | `a_seal_that_fails_after_its_pack_leaves_a_valid_burrito` |
| the draft writer ending before the lock is released | `nothing_is_written_into_the_folder_after_the_lock_is_released` |
| the coherence view of an unverified section draft (current values, no parents) | `an_unverified_section_draft_that_holds_a_span_can_be_kept` |
| the session's `Placed{new}` after a rename of a group that stopped (keep `Writing`) | `a_group_that_fails_after_a_rename_does_not_mistake_its_bytes_for_an_outside_edit`, `a_focus_after_a_group_stopped_does_not_take_its_bytes_for_an_outside_edit`, `an_outside_revert_after_a_stopped_group_is_recorded_as_a_conflict`, `a_retry_after_a_stopped_group_does_not_write_over_an_outside_revert`, `an_outside_revert_of_metadata_json_after_a_stopped_group_is_an_edit` |
| a `Writing` state kept until a read decides it (drop it when a group could not read the path) | `a_group_keeps_the_intent_of_a_path_it_could_not_read` |
| `Writing` at the open (read its md5s as no one's) | `crash_materialize_ledger_tells_its_bytes_from_an_outside_edit` |
| the heads of new bytes written before their rename (after it) | `a_retry_that_finds_the_file_converged_writes_its_heads_file`, `a_crash_after_a_rename_leaves_no_stale_heads_file_for_a_later_edit`, `a_close_while_the_heads_file_is_held_leaves_no_stale_heads_file_for_a_later_edit`, `a_heads_file_the_open_could_not_write_is_written_by_a_later_open`, `metadata_json_is_not_renamed_before_its_heads_are_written` |
| the ledger's transition table (any row that writes over bytes it does not name, or that the fold does not hold) | `every_pair_of_the_table_keeps_every_byte`, `the_reviews_cases_are_kept`, `random_histories_lose_no_work` |
| the stored size of a carried file waiting to be recorded (the size of the bytes kept in memory, none) | `a_carried_file_waiting_to_be_recorded_is_listed_as_it_is` |
| the listing of a carried file the fold no longer places (list the fold's placements only) | `a_removed_carried_file_is_listed_until_its_job_removes_it` |
| the entry of `metadata.json` naming the bytes M6 wrote (the md5 M2 checked) | `the_manifest_names_the_metadata_json_a_group_that_skipped_a_file_wrote` |
| the commit after a seal that left a file unsealed (commit the snapshot) | `a_pending_file_read_at_the_open_and_cut_short_in_place_stops_the_commit` |
| K1 sealing from the fold's bytes the saves of a pending file it cannot read (leave the file unsealed, so a later cut takes the saves) | `a_save_whose_pending_file_the_seal_cannot_read_is_sealed_from_memory`, `a_pending_file_cut_twice_around_a_revert_and_a_held_open_is_kept` |
| K1 taking the lines a file that no longer verifies lost from the fold's bytes (leave such a file unsealed, which the next open drops) | `a_pending_file_cut_inside_a_line_is_sealed_from_memory`, `a_pending_file_changed_inside_a_line_is_sealed_from_memory_and_kept_in_quarantine`, `a_pending_file_replaced_by_one_of_a_newer_format_is_sealed_from_memory_and_kept` |
| K1's comparison with the ids the fold took from a pending file (drop a removed file, seal what a cut file holds) | `a_pending_file_deleted_outside_is_sealed_from_memory`, `a_pending_file_cut_short_at_a_line_is_sealed_from_memory`, `a_pending_file_read_at_the_open_and_deleted_is_sealed_from_the_open_handle` |
| the app's own bytes that show changes no journal file holds kept with *Bring it back*, not rewritten (N4; O9, M1, M6) | `a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded`, `a_book_file_the_open_could_not_read_is_asked_the_same_at_its_compare_and_swap`, `metadata_json_holding_a_project_detail_whose_pending_file_was_deleted_is_recorded` |
| the heads of a kept copy's base cut to those the fold holds (compare with a digest of a version the journal lost) | `a_book_file_holding_a_save_its_pending_file_lost_in_place_is_recorded_at_the_next_open`, `a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded` |
| a write's `old` bytes as the app's own (treat `d = old` as a change made outside) | `no_crash_of_the_apps_own_write_asks_a_question` (to write: S3 step 9), `random_histories_lose_no_work` |
| WriteFailed for the renames a group that stopped with an error never tried or left unchanged (keep `Writing`) | `a_focus_after_a_group_stopped_does_not_take_its_bytes_for_an_outside_edit`, `an_outside_revert_after_a_stopped_group_is_recorded_as_a_conflict` |
| M1 deciding `metadata.json` bytes about to be written over when clean by stat (trust the stat) | `a_settled_metadata_json_holding_a_lost_name_is_recorded_before_it_is_written_over` |
| a listing a blocked file leaves invalid treated as blocked (fail the group) | `a_held_book_file_with_its_manifest_lost_leaves_the_project_writable`, `a_held_edited_book_file_with_its_manifest_lost_leaves_the_project_writable`, `a_deleted_book_file_whose_path_is_held_leaves_the_project_writable`, `a_deleted_book_file_waiting_for_an_unreadable_value_fails_no_checkpoint` |
| the open leaving a held damaged pending file for the next open (fail the open's write) | `a_damaged_pending_file_a_scanner_holds_leaves_the_project_writable` |
| `secure_dir_chain` walking past the directories just made (stop at the first one) | `a_pack_in_a_folder_a_killed_seal_made_survives_a_power_cut` |
| an entry's scope dropped when the entry comes to stand for other bytes (keep it) | `a_metadata_json_found_after_a_kill_past_its_rename_is_never_taken_for_converged` |
| `metadata.json` waiting when it cannot be read during an unsettled write (begin a second write) | `a_held_metadata_json_after_a_kill_past_its_rename_is_not_taken_for_an_edit` |
| `holds` asking for applied and readable changes (ask only whether the fold's index holds them) | `a_batch_cut_at_a_line_while_closed_is_kept_from_the_book_file`, `a_salvaged_change_whose_batch_lost_a_member_is_kept_from_the_book_file`, `a_book_file_whose_heads_were_put_back_is_recorded_not_written_over` |
| R6, a rendering that read an unreadable value (write it) | `a_head_whose_value_can_no_longer_be_read_is_never_written_as_absent` |
| the journal files a session left because another program held them (list them as unknown: a second writer) | `a_pending_file_the_seal_could_not_remove_is_no_second_writer`, `a_pending_file_the_open_could_not_remove_is_no_second_writer` |
| K4 taking a pending file another program removed after K1 as removed (fail the seal with the removal's error) | `a_pending_file_removed_outside_during_the_seal_ends_the_seal_as_removed` |
| the values of a pending file that lost lines left where the fold reads them (move them to the pack's offsets) | `a_pending_file_read_at_the_open_and_replaced_cut_short_is_sealed_from_the_open_handle` |
| the checkpoint job's carried placements leaving a path a newer snapshot placed or recorded (place the older version) | `a_checkpoint_job_never_places_an_older_version_over_a_newer_edit` |
| the checkpoint job's carried listing from the entries of a newer snapshot (list the older placement's blob) | `a_checkpoint_listing_keeps_the_placed_version_of_a_newer_edit`, `a_checkpoint_job_never_places_an_older_version_over_a_newer_edit` |
| the checkpoint job's removals leaving a carried file a newer snapshot recorded (remove what the older fold does not place) | `a_checkpoint_job_keeps_a_carried_file_recorded_during_its_seal` |
| a pending file of a newer format left out of the unsealed (stop the commit for it) | `a_pending_file_of_a_newer_format_does_not_stop_a_commit` |
| the `reconciles` path of a pending header read as the path of a kept copy's file (an ingredient's; a header of an older build that names `metadata.json` still verifies) | `a_pending_file_that_reconciles_metadata_json_verifies_and_caches_like_any_other` (`tc-journal/tests/index.rs`), `arbitrary_meta_never_blocks_commit` |
| the heads file written when an import is durable (`Imported`) | `two_saves_from_a_program_that_keeps_the_file_open_are_no_conflict` |
| the parents of a carried file's edit (the newest head instead of the placed version) | `an_edit_of_a_placed_file_does_not_replace_a_teammates_newer_version` |
| the snapshot taken when the seal starts (hand the writer the newest) | `a_commit_made_while_a_save_is_served_shows_only_what_its_packs_hold` |
| the checkpoint job's M1 leaving a path a newer snapshot wrote (write the older projection) | `a_checkpoint_never_writes_an_older_projection_over_a_save_a_retry_wrote` |
| the tree's paths from the snapshot (a held book file, a path written from a newer snapshot) | `a_commit_takes_a_file_waiting_for_an_outside_edit_from_the_snapshot`, `a_retry_that_wrote_a_newer_snapshot_does_not_reach_the_tree` |
| the freshening of a reused blob and the repack's guard while a commit is made | `a_blob_a_new_commit_names_again_is_kept_young`, `a_repack_prunes_nothing_while_a_commit_is_being_made` (`git.rs`) |
| the marks of the commits the repack looks at (remove whatever it listed) | `a_repack_prunes_nothing_while_a_commit_is_being_made`, `a_commit_that_begins_during_the_removals_loses_no_object` (`git.rs`) |
| the lock the marks are kept under while a batch is removed (let go of it first) | `a_commit_cannot_begin_while_a_batch_of_removals_runs` (`git.rs`) |
| the look at an object's time before its unlink (remove what was listed) | `an_object_written_again_since_the_listing_is_not_pruned` (`git.rs`) |
| the notice of a rewrite whose every digest equals its base | `a_whitespace_only_rewrite_records_nothing` |
| the durable copy of a changed `metadata.json` or sidecar before it is written again | `an_outside_edit_of_metadata_json_is_copied_durably_before_it_is_rewritten` (to write: S3 step 9), `an_outside_edit_of_a_sidecar_is_copied_and_rewritten_with_one_notice` (to write: S3 step 9) |
| the removal of a `Failed` notice by a job that wrote | `a_group_that_fails_after_a_rename_does_not_mistake_its_bytes_for_an_outside_edit` |
| the revert rule and the removal rule of the import planner (switch either off) | `update_a_book_conserves` (to write: S3 step 9) |
| the look at a file just before its rename (rename over what M1 read) | `an_outside_save_during_the_rename_window_is_never_overwritten` |
| the compare-and-swap reading the bytes (trust an entry's size and mtime) | `the_compare_and_swap_reads_a_file_whose_size_and_mtime_are_unchanged` |
| the writer thread (materialize on the actor, as unit S1 did) | `a_save_is_durable_while_a_materialize_is_held`, `reads_never_wait_behind_a_held_checkpoint`, `a_save_is_durable_while_the_tree_is_built`, `the_opens_first_materialization_runs_after_the_open_returns` |
| a job never goes back to an older snapshot | `a_job_from_an_older_snapshot_never_writes_over_newer_bytes` (`writer.rs`) |
| marking by book (mark every file of the project) | `a_book_change_marks_only_that_books_files`, `an_unchanged_file_is_not_read_back_and_each_buffer_is_hashed_once` |
| one hash for each buffer and no read-back of an unchanged file | `an_unchanged_file_is_not_read_back_and_each_buffer_is_hashed_once` |
| the `reconciles` header ending a kept entry (import again what an earlier batch imported) | `an_import_interrupted_after_its_batch_is_not_imported_twice` (to write: S3 step 9) |
| the digest prefilter (compare every record of the file) | `updating_a_book_costs_what_it_changed` (to write: S3 step 9), `update_a_book_conserves` (to write: S3 step 9) |
| the raw commit object (libgit2's signature, which refuses an empty email) | `the_author_is_translationcore_user_with_no_email` |
| a missing carried blob stored again from the folder (leave the file out of the commit) | `a_placed_file_whose_blob_is_gone_is_stored_again_from_the_folder` |
| the read-back of a repack's index before it removes a loose object, and the 14 days | `repack_keeps_every_reachable_object`, `repack_prunes_unreachable_objects_by_age_and_keeps_what_a_ref_reaches`, `a_close_stops_the_repack_that_runs_and_the_next_repack_finishes_it` |
| G4's copy of the carried blobs (start a new repository empty) | `crash_g4_rebuild_keeps_carried_blobs` |
| the choices of a conflict marked as incomplete history (offer only *Apply what was recovered*) | `an_incomplete_restore_race_offers_every_choice`, `release_batch_is_accepted_beside_the_other_choices` |
| the path of a kept copy (H5: no choice, the fold gives it) | `a_kept_copy_never_takes_a_held_name`, `carried_keeps_both` |
| a head's carried bytes written at their new place before the old place goes (remove first) | `an_old_carried_version_in_a_conflict_is_never_removed` (to write: S3 step 9), `a_conflict_side_whose_blob_was_lost_moves_to_its_conflict_copy_before_the_path_is_replaced` (to write: S3 step 9) |
| the copy of a changed book file durable before the manifest names it (name it first) | `a_changed_book_file_is_kept_and_left_until_the_answer` (to write: S3 step 9), crash rows X1, X2 |
| the look at the folder as a burrito after a job (§16.6) | `every_written_state_is_a_valid_burrito`, `the_states_after_outside_changes_are_valid_burritos` |
---

## 17. Crates

| Crate | Owns in this design |
|---|---|
| `tc-core` | `BookId`, references (exists); `GitOid` and `Md5`, the two digests every crate shares (the journal, the burrito, the Door43 client, the helps, the resource store and the screens hold one type each) |
| `tc-usfm` | the codec (`decompose`, `recompose`, `split_verse`, `join_verse`, `PARA_MARKERS` and the layout rule, the drafted rule `words_drafted` and tC4's `tc4_indexer_drafted`), the derivations `plain_text_v1`, `tokenize_v1`, `selections_v1` and the shown text `shown_text_v1`, the OBS story splice |
| `tc-burrito` | `build_metadata`, `check`, the vendored SB 1.0.0 schema, the extension table, `collision_form` (§3.3) |
| `tc-journal` | identifiers, `RecordKey`, `Record`, `ChangeBody`, canonical encoding, the change line, pack and pending codecs (bytes in, values out), the fold cache's codec (`index`: sections of prepared changes keyed by path, size and fingerprint), the per-change rules, consolidation and redundancy as functions of pack listings, `format.json`, the fold, merge units, coherence, validity, projection to bytes, `ShownToken` minting and checking (edit and review kinds), the command types, the scope digests and heads lists of derived files (§6.2), the per-chapter range reads the screens and the store use (`chapters_of`, `chapter_keys`, `decisions_of`, `alignments_of`, `comments_of`, `book_keys`, APP-DESIGN R1), each costing what it returns. It is the one owner of the types and rules the domain crates share: `CheckKey`, `ResourceRef`, `DecisionRecord`, `AlignmentValue` (and it re-exports `tc-core`'s `GitOid` and `Md5`); the pin slots and the D30 ladder (`pins`); a decision's edit and computed state (`decision`); an alignment's validity, its verse order and the aligned-USFM split (`alignment`); person labels. No I/O. |
| `tc-store` | `Vfs`, `StdFs`, `FaultFs`, `atomic_write`, the project lock, the device identity and the ledger, `Store` (open, create in a staging folder renamed without replacing, the start cleanup, a copy of a read-only project), the project actor and its commands, the `ProjectHandle` surface (§5.0), the draft writer, open and recovery, reading and writing the fold cache `.tc/index.bin` (§6.1 O8), the fold's `ValueSource` over pack and pending files and their blobs, materialize, seal, consolidate, the detection of changed files, their kept copies and the import of a kept copy (§11.1, §13.6), drafts, identity, local git (tree build, commit, index, repack, object database access; no TLS: it comes with `tc-door43`'s fetch and push) |
| `tc-door43` | the internet switch, the Gitea REST client, and git2 network calls (`fetch`, `push`, `list`) on a repository `tc-store` opened |
| `tc-sync` | the Door43 sync (§9), the exchange file export and import (§10), the notice of a file changed on Door43 or in an exchange file by another tool (§11.2, §11.3), `ExchangeReport` |
| `tc-import` | the tC4 4.0 converter and oracle check, the tC4 seed, tC3, raw USFM, foreign burritos |
| `tc-check`, `tc-align` | building check items and alignments for the screens, over `tc-journal`'s types and rules and the derivations of `tc-usfm`; they hold no second copy of a record type, a ladder or a validity rule |
| `tc-resources` | installing, verifying, loading and upgrading resources; its pins read and write `tc-journal`'s `Pins` and `ResourcesExtra` values (`ProjectPins::group_value`, `resources_extra`), its rungs, slots and groups are `tc-journal`'s, and it resolves with `tc-journal`'s ladder (`impl Ladder for ProjectPins`; installed coverage only when it opens a tool on this computer). It counts no decision: the cost of a gateway change is `tc-check`'s |
| `tc-corpus` | the deterministic scale corpus of `docs/PERFORMANCE.md` (Psalms, ten times Psalms, the whole Bible of §14) as journal packs and pending files and as plain domain inputs, every part labelled; for tests and benchmarks only (`publish = false`) |

Dependencies: `git2 =0.21.0` (default features off; `vendored-libgit2` in `tc-store`; `https`,
and `vendored-openssl` on Linux, only in `tc-door43`, for its fetch and push), `jsonschema =0.58.6` (default features off), `sha2`, `md-5`,
`hmac`, `im`, `smallvec`, `rayon`, `postcard`, `zip`, `getrandom`, `notify`, `machine-uid`,
`sysinfo`, `uuid`, `unicode-normalization`, `caseless` or an equivalent default case folding
(path collisions, §3.3), `rustix` (cache-bypassing reads, §10.1 E5), `similar` (display-only
word diffs). Shared versions
go in `[workspace.dependencies]`. No CRDT library and no git content merge (`docs/rebuild.tsv`,
2026-10-14).

---

## 18. Known limits

1. **The working folder is not a valid burrito inside a write group** (I7): milliseconds for one
   book's group (§5.10: a whole-project materialization is one group for each book), and,
   after a crash inside one, until the next open. A plain folder cannot replace many files at
   once. Commits, exchange files and exports never have this gap.
2. **A race of milliseconds** remains between each compare-and-swap check and the rename that
   follows it (§5.10). The next look keeps the other program's next save.
3. **No signatures.** Anyone with write access to the Door43 repository can add a pack under any
   sealer directory. Hashes detect damage, not forgery; content-addressed parents stop a forged
   change from hiding versions its author could not name, but not from adding a wrong version.
   Per-device Ed25519 signatures can be added later as a new header field (a new minor).
4. **Growth.** Changes are never pruned. A very active whole-Bible team adds roughly 150 MiB of
   packs a year, and consolidation stores each month's packs two or three times in git. A Door43
   username in a `DeviceProfile` stays in history.
5. **More decisions reach people than in tC4**, because alignment, check, pin and settings
   conflicts are shown. Merge units, the stub rule, grouping by batch and bulk choices by person
   keep the count down.
6. **The provisional text is the newest by the author's clock.** A wrong clock can change which
   version other tools and exports show until a person decides; it cannot change what is kept.
7. **Computed validity binds four derivation functions.** Changing one raises `minReaderMinor`
   and makes older apps read-only until they update.
8. **One person on two computers is two devices.** Screens group devices by Door43 username; a
   person who never signs in shows as two teammates.
9. **Root-level files** added on Door43 are carried in commits but are not burrito ingredients.
10. **A lost ref update costs one recommit.** libgit2 1.9 flushes a reference it writes when
    core.fsyncObjectFiles is on, but a power cut can still leave the earlier commit as HEAD, and
    git is not the source of truth (§6.4 G3).
11. **Letter-part verse labels** (`\v 2a`) are refused at import, as in tC4 4.0.
12. **Cloud-synced project folders are read-only.** Two computers work on one project through
    Door43 or exchange files, not through a shared folder. Detection covers the common clients
    (§5.0); an unknown client is caught by the second-writer check after its first sync.
13. **A replaced carried version is not kept on this computer.** A version that was synced is
    in Door43's history (carried-files ruling rule 2).
14. **Whitespace at line ends and line endings** changed by an outside editor are not imported
    (§11.1 Q2, §13.6 U3); the app keeps the file's previous line endings.
15. **The first open without a fold cache** (a project just received or copied without its
    `.tc/`, or the first open after an update that raised `INDEX_VERSION` or the format minor or
    changed the probe stamp, §3.6)
    verifies and parses every change: for a whole Bible on a processor without SHA
    instructions, 19 to 20 s of CPU over four threads with glibc malloc (13.5 s with jemalloc)
    and 5.4 to 6.3 s of wall time on the quiet VM (§14), within the 10 s budget for such an
    open (§14, PERFORMANCE.md); 3.8 s on the owner's laptop with SHA instructions. It shows determinate progress and writes the cache, so the
    next open's load and fold take about 1.7 s.
16. **A whole Bible open keeps 270 to 330 MiB resident**, against the 150 MiB steady budget
    (`docs/evidence/storage-open-save-2026-10-17.md`; the 350 MiB peak is met). The fold holds
    an entry for every change and record of the project's history (about 170 MB of tables for
    617,000 changes, §14), and glibc keeps each worker thread's heap at its high-water mark.
    Meeting the budget needs the fold to keep only the heads and the open books' histories in
    memory and read the rest on demand, a change to `tc-journal`'s tables.
17. **A change that keeps a file's size and time** is found only by the storage check (O14),
    after the open.

---

## Appendix A. Where each part came from

The storage design panel produced three candidates: A (causal per-key registers over per-device
segments), B (the tC4 journal ported and repaired) and C (per-record version graphs in
content-addressed packs). Three judges scored them with different lenses (silent loss; simplicity
and the translator's experience; scale, git and burrito validity). C had the highest total and is
the base.

**Kept from C:** content-hash change ids and per-record parents; heads as the merge rule; no
clock, device or author in any survival rule; verse records with spans as values; computed
invalidation; merge units; pending files outside the burrito; immutable packs; commits built in
memory; the total fold; `MovedTo` on the decision's own record; the heads files for outside
edits; the crash table's shape; the team move from tC4.

**Fixed in C (the judges' findings):** book removal now records what the remover saw and never
hides unseen work (§7.6); the token of a conflicted record lists only the displayed candidate
(§2.7, §8.2); the layout split no longer splits words, so poetry lines never auto-merge (§2.4); the
materializer checks the disk before every write (§5.10); a missing carried file is never a
deletion, and carried files are records (§11.4); every file of the Door43 tip is kept (§9.2); the
Door43 anchor is found by graph order and checked by content (§11.2); the seal re-reads its pack
before removing pending files (§5.11); packs are plain NDJSON with a hash on every line and can be
salvaged by anyone (§4); temporary files are dot-prefixed (§5.0); the exchange manifest moved into
the zip comment (§10.1); `format.json` holds no app version (§12); completed months are
consolidated (§4.7); the provisional choice is the newest by `at`, never an unknown-base
outside edit, instead of the deepest (§7.2).

**Grafted from A:** the stub rule; the coherence pass so provisional USFM never drops a verse; the
unknown-base rule for deletions and large outside edits, grouped as one decision; deterministic
ingestion of Door43 edits; the restored-backup warning from an app-data ledger; the FAT32 and
exFAT refusal; the tC4 import as an event-by-event conversion plus a reconcile change checked
against tC4's own derived files; Combine with a preview; the outside-edit toast with Undo; per-device
UI preferences.

**Grafted from B:** the refusal of a resolution when the heads moved; *Edit and resolve*;
`drafts.json`; the re-read before deleting (K2); self-healing own packs from HEAD; keeping invalid
incoming files on the tip; grouping conflicts by batch; the export warning; `core.autocrlf=false`;
dot-prefixed temporary names; `metadata.json` generated from the format version; the batch
completeness idea (B's groups) applied to every batch; the measured open and fsync numbers.

**Rejected:** A's in-place append to a segment inside the burrito (it writes a project file in
place and exposes acknowledged saves to a torn sector); A's version vectors and causality check
(content-hash parents give the same causality for free); A's "Keep newest" bulk action; B's
generations (they hid checks and comments after "Restore book"); B's written invalidation events
(they made translators' edits conflict with checkers' work); B's permanent tC4 4.0 reader and
JavaScript key-order byte ports; C's gzip packs (opaque on Door43; git and zip compress anyway);
C's four-hour seal window; C's full port of tC4's structural fold for import.

---

## Review log

Adversarial review of this document, 2026-10-15. Each finding was checked by walking its
scenario through the text as it stood. All 40 were real, some reported twice; none was rejected.
The regression tests are rows L64 to L96 of §15.3 and the property tests added to §16.1.

1. **Pending batch names collide in a cloud-synced folder** (blocker). Real: §4.2's
   "largest counter on disk + 1" and rename-with-replace let two writers replace each other's
   save. Changed: pending files are `<device>-<hash32>.batch` (§4.2); every file in
   `.tc/pending/` is read whatever its name; cloud-synced folders open read-only with a copy
   offer, and a live second writer is detected (§5.0, O5). L64.
2. **A missing pack is dropped from the fold, the commit and the tip.** Real: no step compared
   the working tree with HEAD. Changed: the expected pack set (§4.4) is restored at open (O7)
   and before every tree (C3); `build_tree` uses that set; no tree while a pack is unreadable.
   L65.
3. **A book file added on Door43 projects nowhere and is deleted from the tip.** Real (X2 never
   wrote `BookPresence`). Changed: decoding a book file of an absent book adds the book (§7.6,
   X2, D2); `RecordsWithoutBook` anomaly; no entry of R is dropped silently (§9.4). L67.
4. **A shifting renumber collides with an edit by verse number.** Real. Changed: passage batches
   and `Passage` conflicts with one-side provisional text and rewinding (§7.3); `from` anchors
   so alignments and comments follow the text (§2.3, §5.2, §7.7); *Combine* over the passage
   text (§8.3). L68, L6.
5. **"Start empty" beats or is absorbed by concurrent work.** Real. Changed: `PresentHow` in
   `Present`, `Cause::StartOver` (never a seed stub), provisional book text follows the
   provisional presence head, the stub rules define "one head plus stubs" and absorb a stub
   against any non-stub head (§5.7, §7.3, §7.6). L70.
6. **Comments and alignments cut off by a concurrent structure change.** Real. Changed: anchors,
   `MovedTo` and `moved_to` following, inner-verse alignments shown in the run's aligner, a
   Detached list with *Dismiss* (`dismissed` field), and a reachability property (§7.7, §16.1).
   L71.
7. **A section save that moves words loses them under a per-verse merge.** Real. Changed: the
   same passage mechanism as 4; no unit merge inside a passage; the review describes the move in
   words (§7.3, §8.3). L69.
8. **An unknown field in a known kind parses under a different key.** Real (V1 contradicted
   §12.2 rule 2). Changed: a known kind keeps its key; unknown fields are compared byte for byte
   and not projected; unknown causes parse as `Cause::Newer`; rule 7 defines inert additions
   and raises `minReaderMinor` for the rest (§2.3, §4.5, §7.3, §12.2). L72.
9. **Restoring a carried version whose blob is not local stops every commit.** Real.
   Changed: a carried-versions reference (deleted again on 2026-10-23) kept recorded versions; Restore only for local bytes; a missing
   blob keeps the first parent's blob and never fails a commit; §11.4's claim corrected (§5.9,
   §9.4, §9.8, §11.4). L73.
10. **SaveSection is refused forever after a teammate's span.** Real. Changed: tokens for every
    displayed record, coherence checked on the author's view only, overlap becomes a `Passage`
    conflict, the section buffer is a draft (§2.7, §5.2, §5.14). L74.
11. **BookLayout is never updated.** Real. Changed: saves that add a verse or chapter update
    `BookLayout`; the projection places unlisted records after their numeric predecessor (§5.2,
    §7.5). L75.
12. **A batch that can never be completed hides its lines; descendants show false conflicts.**
    Real. Changed: `BatchRelease` record and *Apply what was recovered*; `incomplete_history`
    conflicts. Choices are held back only while the missing parent is in a releasable batch;
    when the parent is gone for good the choices stay, so a conflict can always be resolved
    (§2.2, §7.2 F2, §7.4). L76.
13. **`commit_batch` re-stamps received changes.** Real (contradiction with §2.5 and D3).
    Changed: `commit_received` (§5.0), named in T2, R4, R7, D2, §11.3 and §4.4. L77.
14. **"Keep all of mine" with one shared username.** Real. Changed: bulk choices by a chosen set
    of computers; conflicts inside the set are left out; versions sharing a username are
    labelled by computer (§5.9, §8.5). L78, L59.
15. **Carried paths that differ only in case.** Real. Changed: collision by case folding,
    `.case-` placement and `PathCase` conflicts; set-level, so not a per-change rule V9 (§3.3,
    §4.5, §7.7, §11.4). L79.
16. **A missing pack reverts text and leaves Door43 by fast-forward** (blocker). Real; same root
    as 2, fixed there. Also: read errors are `FileUnreadable`, never damage, and unreadable
    pending files are never removed (§4.4, K4). L65.
17. **On Windows "Saved" precedes a durable rename; O2 deletes the complete temp.** Real
    (blocker). Changed: the rename is flushed (directory handle with
    `FILE_FLAG_BACKUP_SEMANTICS`, then the renamed file) before "Saved", proven by a blocking
    power-cut probe; O2 rolls verifying temp files forward; W1 split into W1 and W1a; §4.4's
    "lying hardware" claim corrected (§5.0, §6.1, §6.4, §16.2, §16.5). L80.
18. **A burrito-invalid metadata value blocks every commit** (blocker). Real. Changed:
    `build_metadata` is total, per-field validation with fallback to the newest valid value and
    `InvalidForBurrito`; commands validate first; property `arbitrary_meta_never_blocks_commit`
    (§3.4, X2). L81.
19. **The M4 retry overwrites a just-saved outside edit.** Real. Changed: M1 is repeated before
    every retry; the watcher does not count a waiting path's intent md5 as the app's own write
    (§5.10, §18 item 2). L82.
20. **A damaged local copy of a teammate's pack is pushed over the good one.** Real. Changed:
    restore from any valid blob in every sealer directory; a P2-failing pack never enters a tree
    while a valid blob exists; all-damaged copies are salvaged and unioned; the notice no longer
    promises a repair by the owner (§4.4, §8.5, §9.2 N3, §9.4). L66.
21. **Damaged bytes are recorded as edits.** Real. Changed: every zip entry's size and CRC are
    checked before use; same size and mtime with different bytes was a storage-damage hold (retired 2026-10-23), rebuilt,
    never recorded (§6.2, §10.2 R4, §11.3). L83.
22. **A missing carried blob blocks commits; G4 discards blobs.** Real. Changed: G4 reads the
    broken object database through a disk alternate and re-stores verified working-tree files;
    `build_tree` checks every blob and never fails on a missing carried one (§6.4 G4, §9.4).
    L84.
23. **Case-only path differences rewrite verses on Windows.** Real; same root as 15. Also: a
    colliding verbatim file from R stays out of the working tree, and intake writes go through
    the compare-and-swap (§3.3, §9.2 N5, §9.3 T1, §9.6). L79.
24. **A cloud-synced folder gets two writers the lock cannot see.** Real; same root as 1. The
    proposed "open read-only while another live host holds the lock" cannot be decided across
    machines, so `.tc/lock` content only prompts the person, and the data-based second-writer
    check makes the decision (§3.6, §5.0, O1). L64.
25. **E5 verifies from the page cache.** Real. Changed: the verify bypasses the cache through
    safe calls, else asks to re-insert the stick; probe 8 (§10.1, §16.5). L85.
26. **`atomic_write` reports "unchanged" after the rename.** Real. Changed: `WriteError` splits
    `NotWritten` from `WrittenNotDurable`; the latter counts as saved with a disk warning
    (§5.0). L86.
27. **O2 runs in read-only opens; `config.lock` is never removed.** Real. Changed: a read-only
    open writes nothing; a writable open removes every `*.lock` under `.git` (§6.1). L87.
28. **A schema-invalid metadata value from Door43 blocks the team** (blocker). Real; same as 18.
29. **A USFM file for a book the project never had projects nowhere.** Real; same as 3. Also an
    explicit I2 state for records without a book (§1 I2, §7.6).
30. **Badly named and dot files on Door43 are dropped from the tip.** Real. Changed: foreign
    entries kept verbatim by raw path; a badly named ingredient moves to `.tc-moved/` with a
    notice on every device; exchange files carry them under `.tc-root/` (§3.1, §3.3, §9.4,
    §10.1). L88.
31. **Pending names are not unique per device.** Real; same as 1.
32. **A deleted `project.json` on Door43 refuses every sync.** Real. Changed: missing or
    unreadable `project.json` or `format.json` in R is restored by the merge, not a mismatch
    (§3.5, §9.2 N1, N2). L89.
33. **Moving a tC4 team creates phantom whole-book conflicts.** Real. Changed: D1 anchors at a
    commit without packs, so nothing is diffed against an empty projection, and the
    "no anchor" case is defined (§11.2 D1). L90.
34. **A one-verse web edit that strips CR becomes a whole-book conflict.** Real. The Gitea source
    claim was not re-checked here, but editors that rewrite line endings or trim lines exist on
    every platform. Changed: whitespace-only differences at line ends are not changes, CRLF is
    restored on the changed keys, `WhitespaceOnlyEdit` notice (§11.1 X3, §18 item 14). L91.
35. **A new field or cause diverges old and new apps.** Real; same as 8. Also: a commit with app
    trailers that fails the anchor check is labelled as a version or bug mismatch, never shown
    as a person's edit (§11.2 D1).
36. **Unmodelled `metadata.json` fields and OBS template edits are dropped.** Real. Changed:
    every non-derived metadata field is a `ProjectMeta` record (leaf-merged); the OBS decoder
    yields `StoryBase`; decoders are complete; P9 compares metadata (§3.4, §5.15, §7.3, §11.1,
    §13). L92.
37. **Edits of a `.conflict-` copy are overwritten.** Real. Changed: recorded as a change of the
    original record continuing that candidate; M1 always terminates (§5.10, §11.1, §11.4). L93.
38. **The folder is invalid longer than I7 admits.** Real. Changed: I7 defines write groups, each
    ending with a `metadata.json` rewrite (T1, R7); crash tests assert validity after each
    (§1, §9.3, §10.2, §16.2, §18 item 1). L94.
39. **The export file name uses unchecked strings.** Real. Changed: names are cleaned with an
    id fallback (§10.1 E4). L95.
40. **A large push cannot resume.** Real. Changed: large pushes become a chain of valid commits
    pushed one at a time; the exchange file is offered after repeated failures (§9.1 S7, §9.5).
    L96.

### Review of the fold (unit J2), 2026-10-16

An independent review of the fold, its conflicts and its projections. Each finding was checked
with a test that failed before the change. The regression tests are rows L97 to L102 of §15.4.

1. **Keeping a passage candidate writes another person's version, chosen by clock**
   (blocker). Real: *Keep* chose among every version on the kept side by `(rank, at, id)`, and
   *Keep separate* and *Keep joined* chose between concurrent texts the same way. Changed: a
   choice writes exactly the version picked; where a side or a verse's history still holds two
   versions, the choice asks (§8.3). L97.
2. **Unreadable values hide a conflict or merge it away.** Real. Changed: an unreadable value
   is unknown, never absent or equal; such records are listed as conflicts marked unreadable,
   tokens never name an unread version, choices are refused, `retry_unreadable` judges them
   again, and `tc-store` writes no derived file meanwhile (§2.6). L98.
3. **Applying offline changes to a moved check applies only the newest editor's.** Real.
   Changed: every editor's changed units, every unit of §7.3; two different changes of one unit
   refuse the choice (§8.3). L99.
4. **Choose per line drops units that merged cleanly; panics with no folded head.** Real.
   Changed: it starts from the merged agreed units and is offered only with disputed units
   (§8.3). L100.
5. **Four rules could be deleted without a failing test.** Real. Changed: direct tests for each,
   and the properties now catch each mutation (§16.1).
6. **The property census counted states its assertions never saw; two record kinds were never
   generated.** Real. Changed: the census counts only asserted views, team properties assert
   before the last exchange too, and the scripts write versification and device profiles
   (§16.1). The stronger generator found three more defects, fixed here: a comment id written
   twice across books (L101), a structure issue that listed a verse with only a held change,
   and conflicts of other kinds that did not say a head continues a missing change. It also
   showed that a `Comment` conflict could never be resolved under V6, which now lets a
   resolution name parents.
7. **Budgets missed: cold open, Psalms projection, memory.** Real. Changed: changes are
   folded as the reading threads hand them over, so the fold's own work after the last file is
   about 0.7 s and the open's heap peak fell from 413 MB to 233 MB. The cold open's read
   (hashing and parsing every change, 16.8 s of single-thread work) is now done once per file:
   the fold cache `.tc/index.bin` (§3.6, §6.1 O8, §14) keeps each file's prepared changes,
   keyed by the file's path, size and fingerprint, and the open takes them from it (2.6 s on
   the shared VM, against 9.9 to 25.7 s). This replaces the cache §14 sketched, one key for
   the whole set of files holding heads and value locations: any save would have invalidated
   it, and heads alone cannot fold later changes. Still missed, with the evidence and the
   remaining work in `docs/evidence/storage-fold-2026-10-16.md`: the Psalms projection
   (decoding and writing out again every decision's context), and the steady memory (the
   resident history is about 200 MiB). The budgets are unchanged.
8. **A comment's book depends on which change arrived first.** Real. Changed: a comment id
   held by changes that target several books is a record of each (§7.6). L101.
9. **Every step scans the whole anomaly list.** Real. Changed: each anomaly is replaced by its
   exact value; anomaly writes are counted by the operation-count tests.
10. **A save while a snapshot is alive costs more than 2 ms.** Real (1.5 ms median, 2.7 ms p95).
    Changed: smaller shared chunks and shards, and each book's entry shared on its own (0.26 ms,
    p95 0.40 ms).
11. **Start empty is rewound only when the shown change is its own.** Real, and the spec had the
    gap the review named. Changed: every change that is or descends from a rewound `StartOver`
    change is rewound and listed in the presence conflict; *Restore the text* restores such
    verses (§7.6). L102.
12. **`per_unit` panics on a conflict whose record has no folded heads.** Real; fixed with 4.

### Review of the Windows build, 2026-10-18 (unit S2)

The owner's Windows build (`docs/evidence/windows-build-2026-10-17.md`, "Open") listed four
mismatches of the `incomplete_history` mark with this document, all fixed in unit S2. (1) §7.4
said a conflict marked incomplete offers only *Apply what was recovered*; read literally that
would have hidden the choices of a restore race. It is now a warning: every choice stays
offered (§7.4). (2) A *Start empty* `StartOver` head with a missing parent marked its verse's
conflict but not the book's presence conflict; it marks both (§7.6). (3) The property counted
unreadable bases as heads; it counts heads only, and the restore race's verse heads
(`differences_are_visible`). (4) A conflict's `batches` left out the batches of a restore
race's verse heads; they are listed (§2.6).

The same review found **hazard 2**: on Windows a scanner holds a file just written, and the
rename that makes a save durable fails for a moment. S1 failed the save at the first refusal
and left the temp; the next open rolled the temp forward beside the person's next save at the
same seq. Fixed in unit S2: every rename and removal of a project file is retried within a
bound (§5.0 step 3), and a pending temp is decided by `(device, seq)` after the journal is
read (§6.1 O2b). L103.

2026-10-23: the delta of 2026-10-21 applied by the S2 merge; H18-H22 as ruled on 2026-10-22.
