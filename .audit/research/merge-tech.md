# Merge technology for the Rust rebuild of tC4

**Date:** 2026-10-14. **Machine:** Linux x86_64, 4 cores, 15 GiB RAM, rustc 1.97.0, release builds.
**Probe code:** `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/merge-probe/`
(a cargo workspace: `crdt/`, `journal/`, `gitx/` (gix), `gitx2/` (git2). The raw outputs are the `*.txt` files beside it.)
**Inputs read:** BURRITO-SPEC 1.23 §8 and Appendix A 2.4; `docs/evidence/automerge-proof-2026-08-19.md`,
`bench-fold-2026-08-25.md`, `fold-worker-2026-09-14.md`, `pankosmia-sync-model-2026-07-08.md`,
`sync-transport-spike-2026-10-13.md`; DECISIONS D52, D53, D55, D67, D98; `journal/fold.mjs` (`joinHead`).

## 1. Recommendation

Port the existing BURRITO-SPEC §8 journal to Rust, byte-compatible with the v1–v3 format, and use
**git2** (vendored libgit2) as a byte transport only. Git never merges project content. The sync and
the USB import are both "union the immutable journal files, fold, regenerate the derived files".
A verse that two people changed differently is a **fork** that keeps both texts verbatim and goes to
a person. A resolution is a journal event, so it travels like any other change.

Reasons, each measured below:

1. **Every CRDT text type silently interleaves one verse edited two ways.** In Rust, automerge 0.12,
   yrs 0.28 and loro 1.16 all produced a sentence that neither translator wrote (§3.1). Only one of
   the five candidates' built-in primitives makes a same-verse conflict visible: automerge's
   multi-value map register (`get_all`). Used that way, it is the §8 fork model with a binary
   container around it.
2. **The §8 fork model already does what the owner requires,** and the Rust port proves it. Two
   replicas converge to one digest. A same-verse fork carries both texts and the base text. One
   resolution clears it on every device. Two competing resolutions fork again. 1,000 permuted,
   duplicated and partitioned unions folded to the same result (§3.4).
3. **Rust removes the JS performance problem.** The checked-whole-Bible fold (278,041 events) takes
   **1.13 s** in Rust, against 9.28 s in JS. An open (scan, validate and fold of 278k segment files)
   takes **~3 s**, against 41 s for the JS reference reader and 3–4 min on the production HTTP path (§3.5).
4. **Projects already carry §8 journals.** tC4 4.0.0 ships 2026-10-16 with §8 journals, and
   team sync round 1 (D98) is normative. Any other model strands every existing project's history
   or needs a lossy migration.
5. **Git's own merge is the wrong tool, and the probe shows why.** libgit2's line merge reports false
   conflicts on adjacent verses and on verses that share a line, and writes `<<<<<<<` into the
   USFM. Both libgit2 and gix conflict on the derived files (`TIT.usfm`, `metadata.json`) and
   never on the journal files (§3.6). A journal-aware sync makes a two-parent commit whose tree it
   computes itself, so Door43 `main` only moves forward and no git conflict can happen.
6. **git2, not gix:** gix 0.89 has no push (only the `push.default` config enum), and Door43 sync
   must push. git2 builds vendored in 1m42s, cloned over HTTPS through the proxy, and is the same
   libgit2 that pankosmia-web uses. Neither crate can read or write a `git bundle`, so the USB path
   is a zip of journal files, not a bundle.

## 2. What the requirements force

| Requirement | Consequence for the merge model |
|---|---|
| Offline peers keep working after they send | Merge must be commutative, associative and idempotent over *sets* of changes (send twice, receive late, receive twice). |
| Nobody's work is ever silently lost | A same-verse conflict must keep both versions verbatim and show them. Character-merging two rewrites is silent loss (the result is text nobody wrote). LWW without a record is silent loss. |
| Door43 stores only git repos | Whatever is canonical must be plain files in a git tree, and a push must be a fast-forward (D98 G4). |
| USB / file exchange | The same merge must work without git history or a common merge base (a zip of files). |
| The project on disk is always a valid Scripture Burrito | Merge state (conflicts) must never be written *into* USFM; derived files must be regenerated with correct md5s at every commit. |
| Months of work, partial or corrupt files | Every unit of change must be self-checking, and a bad unit must be refused and reported, never half-applied. |

## 3. Probe results (executed 2026-10-14)

### 3.1 CRDTs: one verse, two concurrent rewrites (`crdt/`, output `crdt-run-5k.txt`)

Base `Paul, a servant of God and an apostle of Jesus Christ, for the faith of God's chosen people`;
Maria writes `Paul, a slave of God and an apostle of Jesus the Messiah, for the faith of the people God chose`;
Juan writes `I, Paul, serve God; Jesus Christ sent me to strengthen the faith of God's chosen people`.
Each replica edits offline from the same base, then the replicas exchange state.

| Library, primitive | Merged result | Verdict |
|---|---|---|
| automerge 0.12 `Text` (`update_text`) | `I, Paul, sleve God; Jesus the Messiah sent me to strengthen the faith of the people God chose` | silent interleave |
| yrs 0.28 `Text` (prefix/suffix diff) | `I, Paul, serve God; Jesus Christ sent me to strengthenlave of God and an apostle of Jesus the Messiah, for the faith of the people God chose` | silent interleave |
| loro 1.16 `Text` (`update`) | `I, Paul, slavee God; Jesus the Messiah sent me to strengthen the faith of the people God chose` | silent interleave |
| automerge map `put` of the whole verse string | `get_all` returns **2** values; `get` shows Juan's | conflict **detectable** (multi-value register) |
| yrs `Map::insert` | both replicas show Juan's text; Maria's is gone; no API exposes it | silent loss (LWW by client id) |
| loro `LoroMap::insert` | Juan's text; no API exposes Maria's | silent loss (LWW) |

This reproduces the D52 negative control in native Rust. Text CRDTs are unusable for verse text.
A CRDT is only safe as a store of whole, immutable values with multi-value conflict semantics, and
then the domain rules (generations, structural actions, I-3 alignment validity, review classes,
conservation reporting) still have to be written on top.

### 3.2 CRDT storage growth (output `crdt-run-50k.txt`)

50,000 whole-verse saves over 1,292 verses. The text is pseudo-natural: about 22 words per verse,
and each re-edit keeps about 70 % of the words. That is 7.64 MB of raw text written.

| Store | Saved size | Author time | Load | Note |
|---|---:|---:|---:|---|
| automerge map register | 2.00 MB | 2 s | 281 ms | full history kept; native Rust is ~200x faster than the 418 s WASM figure in the JS proof |
| automerge Text (diff) | 11.16 MB | 82 s | 4 s | superlinear authoring |
| yrs Map (GC on, default) | 0.50 MB | 0.16 s | – | **history discarded** (overwritten values GC'd) |
| yrs Text (`skip_gc`) | 7.62 MB | 0.42 s | – | |
| loro map register | 4.27 MB | 0.21 s | 3 ms | |
| loro Text (`update`) | 10.45 MB | 72 s | – | |
| §8 journal (for comparison) | ~1.1 KB per event raw; 17.6x smaller as one solid stream (JS evidence) | 5 s per 100k segment files written | – | plain files; git packs delta-compress them |

### 3.3 Corruption: 1,000 random single-bit flips of a saved document

| Format | Rejected | Loaded identical | Loaded **wrong, silently** |
|---|---:|---:|---:|
| automerge `save_nocompress` | 1000 | 0 | 0 |
| loro snapshot | 998 | 2 | 0 |
| yrs update v1 | 181 | 2 | **817** |
| §8 sealed segment (sha256 over body) | every flip (checksum) | – | 0 (by construction; the JS proof ran 8,640 exhaustive flips) |

yrs has no integrity check: four out of five corrupted updates decode and apply as wrong data.

### 3.4 The §8 journal in Rust: same-verse conflict is detected, never interleaved (`journal/`, output `journal-scenario.txt`)

`journal/src/lib.rs` (538 lines) ports the §8 core: the HLC `Ts` (its derived `Ord` equals the §8.2
string order), sealed segments `{"container":1,"body","sha256"}` with R-8.1.2 filenames, write-once
publish (temp file plus rename; identical bytes are an idempotent accept), `read_union` with
parse-and-checksum, name and actor binding, and the fold. The fold ports `joinHead` from
`journal/fold.mjs`: resolve-all-heads, base-matched advance, same-actor linear advance, else fork;
plus rootless-base retention, identical-payload auto-merge, supersedes conservation, and the
absent-book quarantine. Structural actions, generations and NFC/grammar checks are out of scope for
the probe.

Scenario: Maria creates TIT. Juan clones. Both work offline: Maria writes 1:1, 1:2 and 1:4; Juan
writes 1:1, 1:3 (twice) and 1:4 with the same text as Maria. They exchange by copying the journal
files each side lacks (the USB semantics):

```
exchange: 3 files maria->juan, 4 files juan->maria (re-running copies 0: 0 0)
  [maria after exchange] events=8 forks=1 autoMerged=1 retained=0 digest=45ea7d8d58ef35c5
    FORK text|TIT 1:1  (provisional = 2026-10-03T04:00:00.006Z|0000|juan-x2)
      base : "Paul, a servant of God and an apostle of Jesus Christ, for the faith of God's chosen people"
      maria-x1 : "Paul, a slave of God and an apostle of Jesus the Messiah, for the faith of the people God chose"
      juan-x2  : "I, Paul, serve God; Jesus Christ sent me to strengthen the faith of God's chosen people"
    AUTO-MERGED identical heads on text|TIT 1:4: 2 heads
  [juan  after exchange] ... digest=45ea7d8d58ef35c5            (identical)
corrupted copy: 1 invalid segment(s) reported: [("2026-10-03T04_00_00.006Z,0000,juan-x2.action.json", "Checksum")]
  [juan receives maria's resolution] forks=0 retained=2 (both originals: Superseded)
  [both resolved independently, then exchanged] forks=1   (the two resolutions fork again)
DETERMINISM: 1000 permuted/duplicated/partitioned unions folded to the reference digest (1017 forks across the 200 reference folds)
```

**A defect the probe found in its own first run, and why R-8.2.4 is load-bearing.** Before the
receive ratchet was added, Maria's clock lagged Juan's. Her resolution got a `ts` lower than
Juan's head, so it sorted *before* the head it superseded, and the fork stayed open. The Rust writer
MUST ratchet the HLC to the maximum `ts` it receives (sync, import and open). The writer SHOULD
also refuse to seal a `supersedes` whose target `ts` is not lower than the new event's `ts`.

### 3.5 Fold and open performance (output `journal-bench.txt`)

Corpus: the `bench-fold --bible` shape (66 books from `eng.json`, 31,104 verses, one text event and
one alignment event per verse, 215,767 decisions = 278,041 events, avg 814 B sealed). The fold borrows
a loaded `&[Event]`. Each figure is the median of 5 runs.

| Measurement | Rust (this probe) | JS (bench-fold 2026-08-25, M2 Pro) |
|---|---:|---:|
| Fold, checked whole Bible, 278,041 events | **1.13 s** | 9.28 s |
| Fold, first 100k of those events | **0.26 s** | – |
| Fold, 100k re-edits of 1,292 verses | **33 ms** | – |
| Open: scan, validate and fold of 100k segment files (rayon x4) | **1.37 s** (sequential 3.16 s) | – |
| Open: scan, validate and fold of 278,041 segment files (rayon x4) | **BIBLE_PAR** (sequential BIBLE_SEQ) | 41.3 s reference reader; 3–4 min production HTTP |
| Directory listing only, 278k names | BIBLE_LS | – |
| Disk: 278k segments, content / allocated (ext4, 4 KiB blocks) | 215.9 MiB / 1,086.5 MiB | 247.6 MiB / ~1.1 GiB (APFS) |

A fold cache is unnecessary at Bible scale. Recommended practice: apply local saves incrementally to
the in-memory fold state. This is O(1) per event and valid because, after the ratchet, a local event
always sorts last. Run a full re-fold after a sync or an import. Run it off the UI thread, as in
fold-worker-2026-09-14.

### 3.6 Git: build, transport, and what git's merge would do (`gitx2/`, `gitx/`)

**Builds (both PASS):**
- `git2 = { version = "0.21", default-features = false, features = ["https", "vendored-libgit2", "vendored-openssl"] }`:
  1m42s cold release build, libgit2 1.9.7, 7.8 MiB stripped probe binary. HTTPS clone of
  `github.com/octocat/Hello-World` through the agent proxy with `ProxyOptions::auto()`: **0.49 s**.
  Per `libgit2-sys` `build.rs`, Windows uses WinHTTP and macOS uses Secure Transport. OpenSSL is used
  only on Linux.
- `gix = { version = "0.89", default-features = false, features = ["blocking-http-transport-reqwest-rust-tls", "merge", "worktree-mutation", "revision", "status", "max-performance-safe", "sha1"] }`:
  5m10s cold build. The `sha1` feature is required: without it gix-hash fails with E0004. 12.2 MiB
  stripped. HTTPS clone (rustls) through the proxy: **0.67 s**.
- git.door43.org is blocked (403) by this sandbox's egress proxy, so the transport was proven against
  GitHub over HTTPS and against a bare `file://` repository (the D98 rig stand-in).

**Door43 sync simulation over git2 (output `gitx2-sync.txt`).** Maria and Juan each commit offline.
Maria pushes. Juan's plain push is refused:
`cannot push because a reference that you are trying to update on the remote contains commits that are not present locally`.
Juan then runs the journal-aware sync (§5.4): fetch, validate the incoming journal blobs *from the
fetched commit*, union, fold, regenerate, commit with parents `[local, origin/main]`, push.
Result: `sync done in 1 attempt(s); forks=1; burrito valid=true`. Maria's sync then fast-forwards.
`G6: maria and juan derived files byte-identical: true`. `door43 main ... only moved forward: true`.

**git's own merge of the same two commits:**
- git2 `merge_commits` conflicts: `["ingredients/TIT.usfm", "metadata.json"]`, journal files conflicting: 0.
- gix `merge_commits` (2.0 ms): `unresolved=true conflicts=["metadata.json", "ingredients/TIT.usfm"]`.

**Line merge (libgit2 xdiff) vs a verse-level 3-way merge on USFM:**

| Case | git line merge | verse-level 3-way (probe) |
|---|---|---|
| different, non-adjacent verses | clean | clean |
| different, **adjacent** verses | **CONFLICT**, `<<<<<<<` written into the USFM | clean |
| same verse, different text | CONFLICT, markers in the USFM | CONFLICT `["verse 1"]` |
| different verses on **one line** (`\v 1 … \v 2 …`) | **CONFLICT** | clean |

**Tree cost of one file per action (output `gitx2-tree.txt`).** 100,000 segments from 3 actors (about
33k files per `segments/` directory):

| Layout | First commit tree objects | **Growth of `.git` per later commit (+1 segment), loose** | Pack a push would send |
|---|---:|---:|---:|
| flat `segments/` (§8.1 today) | 7.3 MiB raw | **708.5 KiB** (grows with segments per actor; about 6 MiB at 278k per actor) | 4.8 MiB |
| sharded `segments/dNNN/` (730 buckets) | 7.3 MiB raw | **9.0 KiB** | 4.8 MiB |

libgit2 has no `gc`. A Rust app that commits a flat 100k-entry directory daily accumulates GBs of
loose tree objects unless it repacks (git2 `PackBuilder`). Packs delta-compress the tree versions,
so Door43's side is fine.

**Bundles:** libgit2 1.9.7 exposes no bundle API (header grep), and gix 0.89 has none either. A
`git bundle` USB story would require shipping the git CLI.

**USB archive of 100k segments:** `zip -r` 65.3 MiB; `tar | xz -6` 5.6 MiB. Synthetic data is more
repetitive than real data; the JS evidence on real-shaped segments is 2.33x per-file deflate and
17.6x as one solid stream (whole Bible: ~106 MiB zip, ~14 MiB solid).

## 4. Evaluation of the five options

| | (1) automerge 0.12 | (2) yrs 0.28 | (3) loro 1.16 | (4) custom journal (§8) | (5) git-native 3-way |
|---|---|---|---|---|---|
| **Storage growth** | register: 2.0 MB per 50k saves (full history); Text 11.2 MB | GC'd map 0.5 MB but **history lost**; Text no-GC 7.6 MB | map 4.3 MB; Text 10.4 MB | ~0.8–1.1 KB per event raw; 216 MiB raw at 278k events; ~14 MiB solid-compressed; git packs delta it | smallest (git deltas of plain files); no per-change record beyond commits |
| **Determinism** | convergent | convergent | convergent | proven set-function (1,000 unions; JC-3/JC-32 in JS) | **no**: the result depends on the merge base and the integration order (criss-cross gives virtual bases) |
| **Same-verse concurrent edits** | Text: **interleave**. Map register: `get_all` shows both | Text: **interleave**. Map: **silent LWW** | Text: **interleave**. Map: **silent LWW** | **Fork**: both texts verbatim + base text; provisional = newest; resolution event travels; competing resolutions re-fork | line merge: false conflicts on adjacent verses / one line, markers inside USFM; verse-level 3-way: correct but needs a merge base |
| **Partial/corrupt files** | chunk checksums (1000/1000 rejected) | **817/1000 flips applied silently** | 998 rejected, 0 wrong | sha256 per segment; an invalid segment is invisible *as a whole* and reported; sync refuses (G2) | git object ids are content hashes (robust); a worktree conflict state can be left half-merged |
| **Plain git transport (Door43)** | binary chunks; needs per-actor immutable files to avoid git conflicts; not readable on Door43 | binary updates; same | binary; same | plain JSON files, disjoint per actor: git never conflicts on them; derived files are rebuilt, never merged | native |
| **USB exchange** | copy chunk files | copy updates | copy snapshots/updates | zip of journal files; the union is idempotent and needs no common base | needs a bundle (unsupported in libgit2/gix) or a bare repo on the stick; zip of the tree loses the merge base |
| **Maturity (Rust)** | Rust core is the reference impl; 0.x versioning; format stable since 2.0 | widely used, Yjs wire-compatible | 1.x; smaller ecosystem; shallow snapshots can strand old peers (D52) | normative spec + 368 JS conformance checks; a **Rust port is new code** (JS `journal/` is ~2.7k+ lines) | git2: 120M downloads, libgit2 1.9.7; gix: no push |
| **Fit with tC4 4.x projects** | parallel format; the domain fold is still needed; JS proof: +415 lines, 0 deleted | fails no-silent-loss | needs a custom conflict layer | **is** the 4.x format | drops the journal: existing history unusable; alignments and decisions need per-record 3-way |

## 5. Recommended design

### 5.1 Crates

- `git2 0.21` (`default-features = false`, `["https", "vendored-libgit2"]`, plus `"vendored-openssl"` on Linux only): fetch, commit, push, packbuilder. Same libgit2 as pankosmia-web.
- `serde`/`serde_json` for the segment container and bodies; `sha2` for the seal; `md-5` for `metadata.json`; `rayon` for the open scan.
- `zip` for USB exchange files; `similar` for display-only word diffs in the conflict view.
- **Not used:** automerge, yrs, loro, gix, git's content merge, `merge=union`.

### 5.2 Rust types

```rust
/// [a-z0-9-]{4,32}; one per (installation, project) — D53(c). Derived from the installation secret.
pub struct ActorId(Arc<str>);

/// HLC event identity. Derived Ord == §8.2 string order (both leading fields are fixed width).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ts { ms: u64, ctr: u16, actor: ActorId }

pub struct Hlc { actor: ActorId, last_ms: u64, ctr: u16 }   // tick(now) R-8.2.2/3; observe(&Ts) R-8.2.4

pub struct Event {
    v: u8,                       // 1 | 2 | 3; unknown -> fold refuses (R-8.3.9)
    actor: ActorId,
    ts: Ts,
    base: Option<Ts>,            // per-key observed head; None = "no prior state I know of" (D53d)
    generation: Option<Ts>,      // mandatory on align/decision/note (R-8.5.6)
    supersedes: Vec<Ts>,         // resolution only
    batch: Option<Ts>,
    seed: Option<Seed>,
    op: Op,
}

pub enum Op {                    // exhaustive per §8.5 (v1..v3)
    BookAdd { book: BookId, scope: Scope, skeleton: String, initial_verses: BTreeMap<SlotKey, String> },
    BookRemove { book: BookId },
    TextSkeletonSet { book: BookId, skeleton: String },
    TextStructureApply { book: BookId, skeleton: String, transitions: BTreeMap<SlotKey, Transition>, dispositions: Vec<Disposition> },
    TextVerseSet { book: BookId, chapter: String, verse: VerseKey, text: String },
    AlignVerseSet { book: BookId, chapter: String, verse: VerseKey, alignments: Vec<Alignment>, word_bank: Vec<Word>, target_verse_md5: Md5Hex },
    CheckDecisionSet { tool_id: ToolId, decision: Decision },
    NoteAdd { target: NoteTarget, text: String },
    ResourcePinSet { slot: PinSlot, entry: Option<PinEntry> },
    ProjectVrsSet { vrs: Box<RawValue> },
    ProjectMetaSet { path: DottedPath, value: Option<Value> },
    SettingsSet { path: DottedPath, value: Option<Value> },
    TextFrameSet { story: u8, frame: u16, text: String },          // v2 (OBS)
    TextStoryRefSet { story: u8, text: String },                   // v2
    ActorNameSet { username: Door43Username },                     // v3
}

/// One file = one user action = one immutable sealed segment (R-8.1.1..9).
pub struct Segment { path: SegmentPath, events: Vec<Event> }   // bytes: {"container":1,"body":"<json>","sha256":"<hex>"}

pub enum RegisterKey {
    Verse(BookId, SlotKey), Align(BookId, SlotKey), Decision(ToolId, DecisionIdentity),
    Pin(PinSlot), Setting(DottedPath), Meta(DottedPath), Skeleton(BookId), Book(BookId),
    Frame(u8, u16), StoryRef(u8),
}

pub struct FoldOutput {
    projection: Projection,               // books (skeleton + slot heads), alignments, decisions, notes, pins, settings, vrs, names
    forks: Vec<Fork>,
    auto_merged: Vec<AutoMerge>,          // key, all head ts, winner (R-8.6.4)
    retained: Vec<Retained>,              // closed reason vocabulary (R-8.6.9)
    invalid: Vec<InvalidRecord>,          // e.g. stale alignments (I-3), orphaned
    pending_structural: Vec<Ts>,
    names: BTreeMap<ActorId, Door43Username>,
    names_rejected: Vec<Ts>,
}

/// What the UI needs to let a translator SEE and CHOOSE.
pub struct Fork {
    key: RegisterKey,
    base: Option<Ts>,                     // the head both sides started from -> base text for the diff
    candidates: Vec<Candidate>,           // every live head, ts order
    provisional: Ts,                      // highest ts; projected, flagged provisional
    review: ReviewClass,
}
pub enum ReviewClass { Person /* text.verse.set — G7 */, NewestWins /* counted once in the report */ }
pub struct Candidate { ts: Ts, actor: ActorId, username: Option<Door43Username>, value: CandidateValue }

pub enum Resolution { Take(Ts), Edited(String) }
// -> Event { op: TextVerseSet{..}, base: Some(fork.provisional), supersedes: fork.candidates.ts, batch }

pub struct ExchangeReport {               // sync and USB import share it
    source: ExchangeSource,               // Door43 { remote, main_before, main_after } | File { path, exporter }
    added: BTreeMap<ActorId, usize>,
    new_verse_forks: Vec<RegisterKey>,
    newest_wins: usize,
    outcome: Result<(), Refusal>,
}
pub enum Refusal { OwnFileDiffers(Vec<PathBuf>), IncomingInvalid(Vec<(PathBuf, SegmentError)>), ProjectMismatch,
                   MetadataOverlay, ObsUnsupported, Offline, AuthFailed, NoWriteAccess, NonFastForwardExhausted }
```

### 5.3 On-disk layout (inside the burrito, unchanged from §2)

```
<project>/                                   git work tree; a valid Scripture Burrito at every commit
  metadata.json                              derived (ingredients + md5 + size; currentScope from the fold)
  ingredients/<BOOK>.usfm                    derived (recomposed from skeleton + verse slots)
  ingredients/vrs.json                       derived (project.vrs.set register)
  ingredients/checking/alignments/<BOOK>.json, <toolId>/<BOOK>.json, resources.json, settings.json   derived
  ingredients/checking/journal/<actorId>/actor.json
  ingredients/checking/journal/<actorId>/segments/<ts with : -> _ and | -> ,>.action.json          canonical, immutable
<app-data>/tc4/projects/<projectId>/         NOT in the repo, never pushed
  outbox/<ts>.action.json                    staged intent; republished after a crash (R-8.1.8)
  fold.state                                 optional cache, MUST equal the full fold
  peers.json                                 USB export watermarks (optional)
```

Writes: seal, write to a temp file, `fsync`, `rename`, `fsync` the directory, then delete the outbox
entry. Native Rust can do a real `fsync`, so D50's "power-loss durability detected but not
guaranteed" can become guaranteed. Commit at checkpoints (session close, book done, pre-sync), not
per save. After each sync, repack loose objects with `PackBuilder`; libgit2 has no `gc`.

**Format question for the owner (measured, not decided here).** The flat `segments/` directory costs
O(segments) of new loose tree object per commit: 708 KiB at 33k segments per actor, about 6 MiB at
278k. A sharded path (`segments/<YYYY-MM>/…`) costs O(1), 9 KiB. This changes R-8.1.2 and the
G2 path validator, so it needs a §9 version bump, and every deployed reader must accept it before
any writer emits it. Until then, the Rust app writes the flat layout and repacks.

### 5.4 Merge on Door43 sync (git2; replaces D98's clone-and-swap)

1. Refuse early: OBS project, internet off, no token, a `project.meta.set` overlay (R-8.7.10, while
   the server owns `metadata.json`).
2. Checkpoint and commit any local work, so the work tree is clean.
3. `fetch +refs/heads/main:refs/remotes/origin/main`.
4. If `origin/main` is HEAD or an ancestor of HEAD: push and stop.
5. Walk the **fetched commit's tree** from the object database, never a work tree (PLATFORM-NOTES #20).
   Run every blob under `ingredients/checking/journal/` through the one validator (G2). Check that
   each own-actor path is byte-identical (G1). Any failure refuses the sync, and nothing has been
   written yet.
6. Write the missing segment files into the work tree. They are new immutable files, so this step is
   idempotent. Ratchet the HLC to the maximum incoming `ts`.
7. Run a full fold over the union, regenerate every derived file (G3), and check that the burrito is
   valid. Derived bytes from Door43 are never inputs.
8. Commit with parents `[HEAD, origin/main]` and the computed tree. Git's merge machinery never runs.
9. Push `main`. On `NotFastForward`, go back to step 3 (G4). Never force-push.
10. Return the `ExchangeReport`.

Compared with D98's design, there is no clone at a fixed path, no copy-and-delete swap, and no
second project copy. That removes the G8 crash window, and G5 holds trivially. Local history is
kept (two-parent commit). A crash between steps 6 and 8 leaves extra immutable segments and stale
derived files, which the next open regenerates (R-8.7.5).

### 5.5 Merge on USB / file import

- **Export:** a `.tc4sync` zip holding `exchange.json` (project id, exporter actor, created, and the
  list of `{path, sha256}`) plus `ingredients/checking/journal/**` for every actor the exporter has.
  It optionally holds the whole burrito, so a newcomer can start from it. Full exports are cheap:
  ~106 MiB per-file deflate, ~14 MiB solid, for a checked whole Bible. Per-peer watermarks can make
  exports incremental later.
- **Import:** the same steps 5–8 with the zip as source. The project id must match; joining is not
  merging (D53b). Validate every entry (zip CRC plus segment sha256); one bad entry refuses the
  import and names the paths. Copy in the missing segments, fold, regenerate, then commit locally
  ("Import from Juan's file"). The next Door43 sync pushes the union.
- Any device can import any other device's file, in any order, any number of times. The union is
  commutative and idempotent, so D67(4b)'s "one integrator per offline team" is unnecessary.
- No `git bundle` (no libgit2 or gix support). A bare repository on the stick works with git2
  `file://` (it must be bare, per the 2026-10-13 spike), but it is optional and not needed for
  correctness.

### 5.6 Conflict representation and resolution in the UI

- **Representation:** the fold computes conflicts on every run. Nothing is persisted into USFM, so
  the burrito stays valid and other tools see the provisional (newest) text. A `Fork` carries the
  base text, every candidate verbatim, author usernames (`actor.name.set`) and timestamps.
- **After a sync or import:** a non-modal banner says "3 verses were changed by you and Juan.
  Review." The project stays usable (D53e). Forked verses carry a marker in drafting and checking.
- **Review screen** (translationStudio design, D98 point 4): per verse, it shows the base text, then
  each candidate shaded by its author, not by its position, with a word-level diff against the base.
  The diff is display only; nothing is auto-applied. Actions: *Use Maria's*, *Use Juan's*,
  *Edit* (opens an editor prefilled with one candidate), bulk *Use all of mine* / *Use all of
  theirs*, and *Later*. Each choice writes one `text.verse.set` with `supersedes` = all candidate
  `ts` and `base` = provisional. Bulk choices share a `batch`.
- **Alignments and check decisions** (D98 point 8): the newest wins and is counted once in the report.
  The losing record stays in the journal and in `forks[]`, so the checking UI can show "another
  version exists (Juan, 2 days ago)" and re-apply it as a new event. Nothing is silent. A resolved
  text fork re-evaluates I-3, so stale alignments flag themselves.

### 5.7 Corrupt or partial files

- A local torn write leaves an invalid segment. It is invisible as a whole and reported (R-8.1.6/7);
  the app republishes it from the outbox (R-8.1.8).
- Incoming invalid files (Door43 or zip) refuse the whole exchange, name each path, and change
  nothing (G2).
- A derived file edited or deleted out of band is detected as divergence at open and reconciled
  through `seed: out-of-band-usfm` events (R-8.8.1). It is never silently overwritten.

## 6. Risks and open items

- **The port is the work.** The JS reference (`journal/*.mjs`, 368 conformance checks) is the
  contract. Run the JS conformance fixtures against the Rust fold, digest for digest, in CI before the
  Rust fold writes any project. The probe covers only the LWW/fork core.
- **HLC ratchet discipline** (§3.4) must be enforced at every receive path: open, sync, import.
- **`metadata.json` ownership:** while the app runs on pankosmia-web, the server rewrites it per
  commit (R-8.7.10). If the Rust app owns git directly, `metadata.json` becomes a derived file and
  must be deterministic, which the probe shows is possible (G6 byte-identical).
- **Flat segment directory** costs, as measured in §5.3. This is an owner decision with a spec bump,
  or else repack.
- **Door43 403/protected-branch replies** were not measurable here (Door43 is blocked by the sandbox
  proxy). Set git2's `push_update_reference` callback; the probe does, because a server-side refusal
  otherwise does not surface as an error (PLATFORM-NOTES #46).
- **Numbers are from one Linux VM** with synthetic corpora, as in the JS evidence. Treat the trend
  and the order of magnitude as evidence, not a universal prediction.

## 7. Reproduce

```
cd /tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/merge-probe
cargo build --release
N=50000 ./target/release/crdt          # CRDT interleave, register conflicts, growth, bit flips
./target/release/journal scenario      # two-replica fork, resolution, re-fork, corruption, determinism
FULL=1 ./target/release/journal bench  # fold + open benchmarks (writes run/disk-*)
./target/release/gitx2 sync            # Door43 stand-in sync, git merge comparison, line vs verse merge
./target/release/gitx2 tree            # flat vs sharded tree cost (needs run/disk-100k)
./target/release/gitx2 https           # git2 HTTPS clone
./target/release/gitx                  # gix HTTPS clone + merge_commits
```
