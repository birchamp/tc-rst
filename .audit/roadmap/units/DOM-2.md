---
id: DOM-2
title: One reader per job and one set of identifiers
track: A
lane: domain
milestone: M1
state: gating
owner: domain lane (Claude Code 9f599c21)
branch: track-DOM-2
worktree: .worktrees/track-DOM-2
depends: [S2]
needs_owner: []
proves: [P9]
no_loss: true
size: 2-3 d
merged: none
updated: 2026-10-24
next: Integration lane merges it in batch 1 (rulings/2026-10-24-H3-H4-H7-fast-release.md); in the merge commit set state done and merged to this branch's tip.
blocked_by: none
---

# DOM-2: One reader per job and one set of identifiers

## Goal

Before `tc-import` is written, each import job has one reader: one for aligned USFM, one for
tC4's alignment sidecar, one for translation-question tables (the lenient one tC4 uses).
Chapter, verse, story and frame numbers and git object ids are defined once, in `tc-core`,
and every small text parser round-trips. The JavaScript whitespace rule and the USFM scans
that live outside `tc-usfm` move to one place each.

## Why it matters to a translator

Carry-over (P9) must read a tC4 or USFM project exactly as tC4 does: an aligned verse must
show the cards tC4 shows, and one bad row in a questions table must not hide every question
of a book or a story. A verse numbered 0 or a reference written `TIT +1:1` must never reach a
saved record.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| S2 '` shows `done`.
- `grep -rn "split_aligned\|decode_alignments" crates/*/src | grep -v "^crates/tc-journal/src/alignment.rs\|^crates/tc-journal/src/fold/sidecar.rs"`
  lists the production callers. On resume-build-lane: `tc-store/src/commands.rs:1033`
  (`add_book_text`, Add books). On s2-proof-executor: `tc-journal/src/outside.rs` (outside
  edits). Write what main shows in the log.
- `grep -rn "from_aligned(\|from_aligned_usfm(\|record::from_record(" crates/`: only test
  callers of `from_aligned` (`crates/tc-align/src/usfm.rs:58`), `from_aligned_usfm`
  (`suggest/mod.rs:114`) and `from_record` (`record.rs:51`).
- `grep -rn "parse_story_questions" crates/` prints only `tc-obs` (no production caller);
  `crates/tc-resources/src/load.rs:417` reads every questions part with the strict
  `tc_helps::tsv::parse_questions`.
- `grep -rn "pub struct StoryNo\|pub struct FrameNo\|pub struct ChapterNo\|pub struct VerseNo" crates/`
  prints `tc-api/src/ids.rs:433`, `tc-journal/src/ids.rs:289, 293, 319, 333`,
  `tc-obs/src/ids.rs:14, 46`.
- `grep -rn "impl FromStr for\|impl std::str::FromStr for" crates/*/src | wc -l` prints 27
  on resume-build-lane.
- If `S3` is in an active state on the board, run
  `git diff --stat main...<S3's branch> -- crates/tc-store crates/tc-journal` and keep the
  list: steps 2, 3, 5 and 8 edit those crates (see "Stop and ask when").

## Funnel card

- **Invariants**:
  - INV-ONE-READER-PER-JOB: each import input (aligned USFM, tC4's alignment sidecar, a
    questions table) has one production reader, and it reads as tC4 reads.
  - INV-IDS-ROUND-TRIP: for every small `FromStr` type, `parse(x.to_string()) == Ok(x)`; for
    identifiers the app writes itself, a parsed text prints back exactly as written.
  - INV-ZERO-IS-NOT-A-VERSE: no chapter, verse or story number 0 can be built; frame 0 is a
    story's title, by name (`FrameNo::TITLE`).
- **Wrong states**:
  - Aligned USFM read in two shapes: `split_aligned` gives one group per run of words, tC4
    one card per outermost source word (`crates/tc-align/src/usfm.rs:7-14`; they differ in 5
    of Titus' 46 verses), so a translator could see a source word on two cards after Add
    books.
  - One bad row (an `x` in the Occurrence column) in a questions table hides all its
    questions (strict reader at load.rs:417), where tC4 reads past it
    (`crates/tc-obs/src/questions.rs:17-19`).
  - `Reference` parses `TIT +1:1` (`crates/tc-core/src/lib.rs:130`, `u16::parse` takes `+`);
    `VerseSpan::single(0)` builds a value that prints `0`, which its own parser refuses
    (lib.rs:32-37).
  - Three `StoryNo` types and two `FrameNo` types, joined by fallible converters
    (`crates/tc-app/src/story.rs:276`, `create.rs:132-133`, `conflicts.rs:124`).
- **Code**:
  - Types: in `crates/tc-core/src/ids.rs` (to create): `ChapterNo(NonZeroU16)`,
    `VerseNo(NonZeroU16)`, `StoryNo(u8)` with 1 to 50 (as all three copies have it today;
    serde as a number, as tc-api's), `FrameNo(u16)` with a private field and `TITLE`.
    `Reference` and `ChapterVerse` (`crates/tc-helps/src/refs.rs:9-12`) get private fields
    and typed numbers. `VerseSpan::single` takes a `VerseNo`. `tc_core::js::{is_space, trim}`
    (to create).
  - One entry point: one reader per job. Aligned USFM: `tc_journal::alignment::split_aligned`
    (tc-journal is below both tc-align and tc-store), giving tC4's cards. Sidecar: a
    per-record reader factored out of `tc_journal::fold::decode_alignments`. Questions: the
    lenient reader moved from `tc_obs::questions` into `tc_helps::tsv` (tc-resources depends
    on tc-helps, not on tc-obs).
  - Removed: `from_aligned` and `from_record` (tc-align); the duplicate id types and the
    three converters; `tc_door43::ObjectId` (`crates/tc-door43/src/git_tree.rs:18`); the
    whitespace copies and the USFM scans outside tc-usfm (each listed in its step).
  - Left alone: tc-usfm's UTF-16 whitespace (`crates/tc-usfm/src/derivations/js.rs:40`, a
    usfm-js port); tc-versification's `u16` verse numbers, where 0 is legal and documented
    (`crates/tc-versification/src/spec.rs:10-11`); the bit-exact ports (`usfmjs.rs`,
    `wordmap.rs`).
  - Left to checks, and why: "no `tc_usfm::Document`/`Node` outside tc-usfm" cannot be carried
    by visibility, because tc-usfm's own integration tests need the types public; step 8 moves
    them into `pub mod raw` (the Code-layer half), and CHK-1's
    `low_level_types_stay_in_their_owning_crate` holds the rule across crates
    (INV-ONE-PARSER-OWNER).
- **Checks**:
  - `the_sidecar_readers_agree_on_every_tc4_record` and
    `the_aligned_usfm_readers_give_tc4s_cards_on_en_ult_titus` in
    `crates/tc-align/tests/one_reader_per_job.rs` (to create); kind: differential over
    `fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json`,
    `fixtures/tc4/conformance-fixtures/import/tc3/cfm_fbt_tit_book.zip` and
    `fixtures/tc4/test-fixtures/usfm-corpus/en_ult-TIT-aligned.usfm`; runs in: CI job `rust`,
    `cargo test --workspace`.
  - `questions_with_one_bad_row_still_load` in `crates/tc-resources/tests/load.rs` (to
    create); kind: unit through the public API; runs in: CI `rust`.
  - `ids_round_trip` (to create) in each crate with a small `FromStr` (its
    `tests/properties.rs`, else `tests/ids.rs`) and `zero_is_not_a_chapter_verse_or_story`
    in `crates/tc-core/tests/ids.rs` (to create); kind: property and unit; runs in: CI `rust`.
  - Wire forms and no loss: `crates/tc-api/tests/golden.rs`, tc-journal's `canonical.rs`,
    `versioning.rs`, `properties.rs`, `loss.rs`, `outside.rs` and `cargo test -p tc-store`
    pass with no change to `fixtures/`.
  - Proof it can fail: (1) route OBS questions back to `parse_questions`: the load test
    fails. (2) Let `split_aligned` keep per-run groups: the en_ult differential fails on TIT
    1:1. (3) Put `u16::parse` back in `Reference::from_str`: `ids_round_trip` in tc-core fails
    on `TIT +1:1`. Record each output.
- **Prose**: `docs/STORAGE.md` §13.5: the aligned-USFM reader gives tC4's cards.
- **Human**: none. Reading as tC4 reads follows `AGENTS.md` "Matching tC4".

## Steps

### Step 1: Write the differential tests and record what they find

- Do:
  1. Create `crates/tc-align/tests/one_reader_per_job.rs`. Sidecar: for every verse record
     of the sample burrito's `TIT.json`, compare `decode_alignments(BookId::Titus, bytes)`'s
     value with `record::from_record(record, basis)` (build `basis` as
     `crates/tc-align/tests/tc4_vectors.rs:251` does); then do the same for every tC3 record
     of the zip (read as `crates/tc-align/tests/tc3_import.rs:55` `imported()` does), each
     wrapped as a one-verse sidecar `{"chapters": {c: {v: record}}}`. Aligned USFM: for every
     verse of en_ult Titus, compare `split_aligned`'s value with `from_aligned`'s cards, as
     links (each source word to its target words) and as card groupings.
  2. Count, for TIT 1:1, how many of `split_aligned`'s groups name ἐκλεκτῶν: more than one
     means the aligner would show that source word on two cards.
  3. Run it. Copy every disagreement (verse, field, both values) into the log.
- Files: `crates/tc-align/tests/one_reader_per_job.rs`.
- Done when: `cargo test -p tc-align --test one_reader_per_job -- --nocapture` runs, and the
  log lists the disagreements. Expected from `crates/tc-align/src/usfm.rs:10-14`: equal links
  in all 46 verses, different groupings in 5.
- Record: a log line `step 1` with the counts; set `next` to step 2.

### Step 2: Keep one aligned-USFM reader, with tC4's cards

- Do:
  1. In `crates/tc-journal/src/alignment.rs`, make `split_aligned` join runs under one
     outermost source word into one card, as `from_aligned` does
     (`crates/tc-align/src/usfm.rs:7-14`, and its code). Keep STORAGE §13.5's no-loss rule:
     the verse is taken apart only when weaving the per-run groups back gives the content
     byte for byte; otherwise it stays as written.
  2. Make the step 1 aligned-USFM test assert equality, and see it pass.
  3. Delete `from_aligned`; point its tests (`tc-align/tests/psalms.rs`, `rtl.rs`,
     `tc4_vectors.rs`) at `split_aligned`. Rewrite `TrainingVerse::from_aligned_usfm` over
     `split_aligned` so it no longer walks `Document`; it stays public because
     `tests/suggestion_budget.rs` and tc-app's `suggest_tests.rs` use it.
  4. Update `docs/STORAGE.md` §13.5's bullet on aligned USFM: groups are tC4's cards.
- Files: `crates/tc-journal/src/alignment.rs`, `crates/tc-align/src/usfm.rs`,
  `crates/tc-align/src/suggest/mod.rs`, `crates/tc-align/tests/*.rs` named above,
  `docs/STORAGE.md`.
- Done when: `cargo test -p tc-journal -p tc-align -p tc-store` passes;
  `cargo test -p tc-align --release --test suggestion_budget -- --ignored` passes (the CI
  `alignment-budget` job); `grep -rn "fn from_aligned(" crates/` prints nothing.
- Record: a log line `step 2`; set `next` to step 3.

### Step 3: Keep one alignment-sidecar reader

- Do:
  1. Factor the per-record body of `decode_alignments` (`crates/tc-journal/src/fold/sidecar.rs:132`)
     into `pub fn decode_alignment_record(record: &Value, at: &str, chapter: u16, first: u16,
     last: u16) -> Result<AlignmentValue, SidecarError>` (to create; take the arguments the
     loop has).
  2. For each disagreement step 1 found, port `from_record`'s rule
     (`crates/tc-align/src/record.rs:1-30` lists them: digit-string occurrences read as
     numbers, fractions refused, unknown fields kept in `extra`, tC3's `"type"` kept on the
     word) into the kept reader. Never drop a field: a field the reader does not know goes to
     `extra`.
  3. Delete `from_record`; point `tests/tc3_import.rs` and `tests/tc4_vectors.rs` at
     `decode_alignment_record`. Make the step 1 sidecar test assert agreement with the
     previous `from_record` results saved as expected values.
- Files: `crates/tc-journal/src/fold/sidecar.rs`, `crates/tc-journal/src/fold.rs`,
  `crates/tc-align/src/record.rs`, `crates/tc-align/src/lib.rs`, `crates/tc-align/tests/*.rs`.
- Done when: `cargo test -p tc-journal -p tc-align` passes; `grep -rn "fn from_record" crates/`
  prints nothing.
- Record: a log line `step 3` that names the kept readers (`split_aligned`,
  `decode_alignments` with `decode_alignment_record`); I's card reads them from this log; set
  `next` to step 4.

### Step 4: Read questions tables as tC4 does

- Do:
  1. Move `parse_story_questions` and its helpers from `crates/tc-obs/src/questions.rs` into
     `crates/tc-helps/src/tsv.rs` as `parse_questions_leniently(text, scope)`; keep
     `tc_obs::questions` as a call to it, or delete it if nothing else needs it.
  2. In `crates/tc-resources/src/load.rs:417`, read `HelpsPart::ObsQuestions` with it.
  3. Sibling: tC4 reads Bible questions with the same `deriveTqItems`
     (`src/data/derive.ts:323` and `:670` in the tC4 clone). Read lines 323-348 there. If it
     reads past a bad row for Bible questions too, read `HelpsPart::Questions` with the
     lenient reader (`TsvScope::Book`); record what you found in the log either way.
  4. Add `questions_with_one_bad_row_still_load` to `crates/tc-resources/tests/load.rs`: a
     questions table with one row whose Occurrence is `x` loads every other question, for
     OBS (and for a Bible book if 4.3 applies).
- Files: `crates/tc-helps/src/tsv.rs`, `crates/tc-obs/src/questions.rs`,
  `crates/tc-resources/src/load.rs`, `crates/tc-resources/tests/load.rs`.
- Done when: `cargo test -p tc-helps -p tc-obs -p tc-resources` passes; proof mutation (1)
  fails the new test.
- Record: a log line `step 4`; set `next` to step 5.

### Step 5: Define the numbers and the git object id once, in tc-core

- Do:
  1. Create `crates/tc-core/src/ids.rs` with `ChapterNo`, `VerseNo`, `StoryNo`, `FrameNo` as
     the funnel card says; their `Display` forms are today's. `StoryNo` serializes as a
     number with `try_from = "u8"`, as `crates/tc-api/src/ids.rs:431-433` does.
  2. Replace the copies: `crates/tc-journal/src/ids.rs:286-336` and
     `crates/tc-obs/src/ids.rs:14-60` become `pub use tc_core::{…}`;
     `crates/tc-api/src/ids.rs:431-470` becomes `pub use tc_core::StoryNo`. Add `tc-core` to
     `crates/tc-obs/Cargo.toml`. Keep each crate's helper functions (`StoryNo::all`,
     `story_ipath`, `story_frame`) where they are, over the shared types.
  3. Delete the converters `journal_story` (`crates/tc-app/src/story.rs:276`), the
     conversion at `crates/tc-app/src/create.rs:132-133` and `story`
     (`crates/tc-app/src/conflicts.rs:124`).
  4. Replace `tc_door43::ObjectId` with `tc_core::GitOid`: `git_tree.rs` keeps its SHA-1
     hashing and returns `GitOid::from_bytes`; where text is parsed, use
     `GitOid::parse_any_case`, which accepts what `ObjectId::from_str` accepted.
- Files: `crates/tc-core/src/ids.rs`, `crates/tc-core/src/lib.rs`, the files named above,
  and every caller the compiler names (on resume-build-lane `ObjectId` is used in
  `tc-door43/src/{client,error,lib,verify}.rs` and `tc-resources/src/{fetched,lexicon}.rs`).
- Done when: `grep -rn "pub struct StoryNo\|pub struct FrameNo\|pub struct ChapterNo\|pub struct VerseNo\|pub struct ObjectId" crates/`
  prints only `crates/tc-core/src/ids.rs`; `cargo test -p tc-api --test golden` and
  `cargo test -p tc-journal --test canonical --test versioning` pass and
  `git diff --exit-code -- fixtures/ crates/tc-api/tests/` prints nothing.
- Record: a log line `step 5`; set `next` to step 6.

### Step 6: Make the reference types strict and round-trip every small parser

- Do:
  1. `crates/tc-core/src/lib.rs`: `Reference`'s fields become private with getters, its
     chapter a `ChapterNo`; `from_str` accepts digits only (no `+`), as `VerseSpan` does
     (lib.rs:80-85). `VerseSpan::single` takes a `VerseNo` (3 non-test callers).
     `crates/tc-helps/src/refs.rs:9-12`: `ChapterVerse`'s fields become private.
  2. Add `proptest` to `crates/tc-core/Cargo.toml` `[dev-dependencies]` (workspace
     version). For each of the 27 `FromStr` types (list them with the grep in "Before you
     start"), add `ids_round_trip`: `parse(x.to_string()) == Ok(x)` over generated values.
     For identifiers the app writes itself (tc-core, tc-api, `tc-door43/src/names.rs`), also
     assert that a text that parses prints back unchanged. Parsers of outside files
     (`tc-versification`, `tc-burrito`, `tc-resources/src/pin.rs`) keep the spellings they
     accept today; give them the first property only.
  3. Add `zero_is_not_a_chapter_verse_or_story` in `crates/tc-core/tests/ids.rs`.
- Files: `crates/tc-core/src/lib.rs`, `crates/tc-core/Cargo.toml`, `crates/tc-helps/src/refs.rs`,
  the test files named in the funnel card, and callers the compiler names.
- Done when: `cargo test --workspace` passes; proof mutation (3) fails tc-core's
  `ids_round_trip` (paste it in the log).
- Record: a log line `step 6`; set `next` to step 7.

### Step 7: Define the JavaScript whitespace rule once

- Do: create `crates/tc-core/src/js.rs` with `pub fn is_space(c: char) -> bool` and `pub fn
  trim(text: &str) -> &str` (ECMAScript `WhiteSpace` and `LineTerminator`, the table of
  `crates/tc-app/src/js.rs:7-24`) and one table test over every code point that any copy
  treats as space. Replace the copies: `tc-api/src/model.rs:96, 111`, `tc-app/src/js.rs:7,
  22`, `tc-check/src/js.rs:7`, `tc-helps/src/blocks.rs:126, 141`,
  `tc-resources/src/strongs.rs:171`, `tc-obs/src/js.rs:8, 23`, and
  `tc-usfm/src/codec.rs:261` once a test shows its formula agrees on every `char`.
- Files: `crates/tc-core/src/js.rs`, `crates/tc-core/src/lib.rs`, the files above.
- Done when: `grep -rn "fn is_space\|fn js_space\|fn is_js_space\|fn is_whitespace\|fn is_js_whitespace\|fn js_trim" crates/*/src`
  prints only `crates/tc-core/src/js.rs` and `crates/tc-usfm/src/derivations/js.rs`;
  `cargo test --workspace` passes.
- Record: a log line `step 7`; set `next` to step 8.

### Step 8: Move the USFM scans into tc-usfm

- Do:
  1. Move `source_book_name`, `markers` and `seed_book` (`crates/tc-store/src/seed.rs:31,
     46, 97`) and `skeleton` and `id_book` (`crates/tc-app/src/create.rs:143, 334`) into
     tc-usfm as pure functions with their tests; callers call tc-usfm.
  2. Move tc-align's walks of `tc_usfm::Document`/`Node` (`crates/tc-align/src/words.rs:16`
     and the functions that use them; `crates/tc-align/src/suggest/mod.rs:118-215`, if step 2
     left any) into tc-usfm functions that return the source words and the aligned walk.
  3. The Code-layer half of INV-ONE-PARSER-OWNER: move the five raw types out of the root
     re-export (`crates/tc-usfm/src/lib.rs:44`) into `pub mod raw`, so the only import path is
     `tc_usfm::raw::{Document, Node, Marker, Milestone, Span}` and a reader sees what it is
     reaching for; tc-usfm's own integration tests import from `raw`. Then remove the tc-align
     line from `crates/tc-arch/allow/imports.txt` (CHK-1's `low_level_types_stay_in_their_owning_crate`
     fails on a stale line, so this is forced). If `crates/tc-arch` does not exist yet, log a
     `note` that CHK-1's imports allow list needs no tc-align line.
- Files: `crates/tc-usfm/src/`, `crates/tc-store/src/seed.rs`, `crates/tc-app/src/create.rs`,
  `crates/tc-align/src/words.rs`, `crates/tc-align/src/suggest/mod.rs`, `crates/tc-arch/`
  (if present).
- Done when: `grep -rn "tc_usfm::Document\|tc_usfm::Node\|use tc_usfm::{[^}]*Document" crates/*/src | grep -v "^crates/tc-usfm/"`
  prints nothing; `cargo test --workspace` passes.
- Record: a log line `step 8`; set `next` to step 9.

### Step 9: Run the full gates

- Do: run the full gate list and the extra gates below.
- Files: none.
- Done when: every gate passes.
- Record: a log line `gate`; set `state: verifying` and `next`.

## Gates before merge

Run `scripts/gates.sh` (INT-3 creates it; it lands before this item can start). If it is
missing, run every command of `AGENTS.md` "Tests" and the Windows step:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and
  again with `--features gallery`, `--features app` and `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`
- `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`

Plus:

- `cargo test -p tc-align --release --test suggestion_budget -- --ignored --nocapture` (CI's
  `alignment-budget` job).
- `git diff --exit-code <claim commit> -- fixtures/` prints nothing: no fixture changed.
- This is a no-loss item: the review runs the three verification lenses of `FUNNEL.md` §8
  item 5, and `cargo test -p tc-journal --test properties --test loss --test outside` passes.

## Stop and ask when

- S3 is active and its branch changes a file that step 2, 3, 5 or 8 edits (the list from
  "Before you start"): ask the owner which item goes first. Steps 1, 4, 6 and 7 do not touch
  tc-store and can go first.
- Step 1 shows the readers disagree on the plain text of a verse, or on a link (not only on
  grouping): that is a loss risk; stop.
- Making `split_aligned` give tC4's cards would leave a verse with its markup in the text
  where it is taken apart today.
- A gate fails twice and you cannot tell why.

## References

- `.audit/roadmap/evidence/2026-10-21/health-domain.md` §3, §6 items 2, 3, 5-8, F4-F8, F10;
  `checks-inventory.md` C5.
- `crates/tc-journal/src/alignment.rs:235`; `crates/tc-journal/src/fold/sidecar.rs:132`,
  `:342`; `crates/tc-align/src/usfm.rs:1-58`; `crates/tc-align/src/record.rs:1-51`;
  `crates/tc-align/src/suggest/mod.rs:114-215`; `crates/tc-obs/src/questions.rs:1-60`;
  `crates/tc-resources/src/load.rs:405-420`; `crates/tc-core/src/lib.rs`,
  `crates/tc-core/src/digest.rs:48-125`; `crates/tc-door43/src/git_tree.rs:18`.
- `docs/STORAGE.md` §13.5 (aligned USFM); tC4 `src/data/derive.ts:323-348`, `:585-597`.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/health-domain.md
- 2026-10-24 07:37 EDT | domain lane (Claude Code 9f599c21) | claim | none | Claimed in .worktrees/track-DOM-2 from main 29048562. Production callers on main: split_aligned in tc-store/src/commands.rs:1052 (Add books); decode_alignments in tc-journal/src/outside.rs:270 (outside edits), tc-app/src/backend/seed/records.rs:415 and seed/check.rs:596. from_aligned, from_aligned_usfm and from_record have only test callers (and tc-app's suggest_tests.rs). parse_story_questions only in tc-obs; load.rs:417 reads questions with the strict parse_questions. Id types at tc-api/src/ids.rs:433, tc-journal/src/ids.rs:289,293,319,333, tc-obs/src/ids.rs:14,46. 27 FromStr impls. S3 not active | python3 -I .audit/tools/board.py
- 2026-10-24 07:41 EDT | domain lane (Claude Code 9f599c21) | step 1 | none | crates/tc-align/tests/one_reader_per_job.rs (tC3 fixture helpers moved from tc3_import.rs to tests/common). Sidecar: 47 records (the sample's TIT 1:1 and the 46 tC3 records) read the same through decode_alignments and from_record: 0 disagreements. Aligned USFM, en_ult Titus: 46 verses, plain text and links equal in all 46, the bank equal in all 46; card groupings differ in 29 (1:1-1:6, 1:9, 1:10, 1:12, 1:15, 1:16, 2:1, 2:3, 2:5, 2:7, 2:8, 2:10, 2:13-2:15, 3:1, 3:3-3:8, 3:10, 3:14), split_aligned always giving more cards (1:1: 19 vs tC4's 17). The card expected 5: that is the count whose exported bytes differ, not the count whose cards differ. TIT 1:1: split_aligned gives 2 cards naming ἐκλεκτῶν. No plain-text or link disagreement, so no stop | cargo test -p tc-align --test one_reader_per_job -- --nocapture
- 2026-10-24 08:02 EDT | domain lane (Claude Code 9f599c21) | step 2 | none | split_aligned gives tC4's cards through tc_journal::alignment::tc4_cards (public: the one card rule); the no-loss check weaves each run with its words as the value writes them (from_usfm/to_usfm), so a verse whose attributes would reorder is still kept as written (shared_rules' an_aligned_verse_weave_would_rewrite_is_kept_as_written caught a first draft that skipped the word round trip). Bank still in verse order (§5.3). from_aligned, Extracted, order_cards deleted. Tests moved: tc4_vectors' 92 withOriginal vectors now read through split_aligned and Aligner::open (which fits cards to UGNT) and match tC4's cards and bank; the 136 partly aligned vectors match tC4's cards, and the bank holds tC4's words in verse order (tC4 groups it by occurrence; the rebuild keeps every value in verse order, §5.3, and the aligner shows it so); export test reads through split_aligned; rtl through split_aligned and Aligner::open; psalms' model of a tC4-saved value uses tc4_cards plus a test-only model of word-aligner's orderAlignments (tests/common in_tc4_source_order), because split_aligned keeps 35 Psalms verses as written (punctuation inside a milestone). one_reader_per_job now asserts: links of the runs kept, no source word on two cards, TIT 1:1 has 17 cards with ἐκλεκτῶν on one. shared_rules: weaving the split value gives en_ult's bytes in 41 of 46 verses and the same links in all. Not done as the card says: TrainingVerse::from_aligned_usfm stays a walk of tc_usfm::Document, because it reads one source token per \zaln-s as tC4's bench does (tc4_suggest.rs's four tC4 vectors fail when it reads cards); step 8 moves the walk into tc-usfm. STORAGE §13.5 says the cards are tC4's. Done-when: cargo test -p tc-journal -p tc-align -p tc-store passes; suggestion_budget --release --ignored passes (26 s); grep "fn from_aligned(" prints nothing | cargo test -p tc-align --test tc4_vectors --test one_reader_per_job
- 2026-10-24 08:12 EDT | domain lane (Claude Code 9f599c21) | step 3 | none | Kept readers: aligned USFM is tc_journal::alignment::split_aligned (cards by tc4_cards); sidecar and tC3/tC4 records are tc_journal::fold::decode_alignments with decode_alignment_record(record, at, chapter, first, last, writer). It takes a Writer enum (ThisApp, Tc4) the card did not name: this app's projection writes `invalid` computed and the mark as `markedInvalid`, while tC4's stored `invalid` is the mark, so one reader must know who wrote the record (Tc4 also keeps anchor, spanEnd, source, dismissed and markedInvalid as unknown fields, as from_record did). Ported from from_record: digit-string occurrences read as numbers; a non-string strong, lemma or morph and a non-boolean done, invalid, markedInvalid or dismissed are refused, where the sidecar reader read them as nothing. Fractions were already refused and unknown fields (tC3's "type") already kept. from_record and tc_align::record deleted; tests read through tests/common tc4_record (decode as Tc4, then Alignment::well_formed, the rules every edit keeps). The sidecar test now asserts that all 47 records read as Tc4 give from_record's values, saved before the deletion as crates/tc-align/tests/expected/tc4_records.txt, and that this app's reader gives the same; each_writers_invalid_reads_as_that_writer_meant_it covers the ported rules. cargo test -p tc-journal -p tc-align passes; grep "fn from_record" prints nothing | cargo test -p tc-align --test one_reader_per_job
- 2026-10-24 08:18 EDT | domain lane (Claude Code 9f599c21) | step 4 | none | tc_helps::tsv::parse_questions_leniently(text, scope) holds tC4's reading (moved from tc_obs::questions; tc_obs::parse_story_questions is now one call to it, its tests unchanged). 4.3: tC4 reads Bible questions with the same deriveTqItems (translationcore4 d310bf3, src/data/derive.ts:323-348, called at :590 for OBS and :670 for books), which keeps a row whose Occurrence is `x` (Number gives NaN), so load.rs reads HelpsPart::Questions and ObsQuestions leniently; a row whose Reference does not address the book's verse or a frame is left out, as before for OBS. questions_with_one_bad_row_still_load (tests/load.rs): en_obs-tq v10 with one Occurrence `x` loads all 672 questions, the bad one Unrecorded; a TIT table with one bad row loads its 3. Proof mutation (1), load.rs back on parse_questions: fails with `assertion failed: obs.parts[&HelpsPart::ObsQuestions].is_loaded()` (load.rs:668) | cargo test -p tc-resources --test load questions_with_one_bad_row_still_load
- 2026-10-24 08:50 EDT | domain lane (Claude Code 9f599c21) | step 5 | none | crates/tc-core/src/ids.rs defines ChapterNo, VerseNo (NonZeroU16), StoryNo (1 to 50, serde as a number with try_from u8, error NotAStory; COUNT and all() moved here) and FrameNo (private field, new, get, TITLE, is_title). tc-journal and tc-obs re-export them, tc-api's StoryNo is `pub use tc_core::StoryNo`; tc-obs's StoryNo::frame became tc_obs::ids::obs_ref(story, frame). tc-core gets an optional `proptest` feature (tc-api's turns it on) for StoryNo's Arbitrary, which tc-api's derived wire types need. Converters deleted: tc-app story.rs journal_story, conflicts.rs story, create.rs's conversion, and the same story-to-story conversions in comments.rs, check/item.rs, export.rs, wire/scope.rs. Frame 0 is FrameNo::TITLE by name everywhere. tc_door43::ObjectId is gone: git_tree.rs hashes into GitOid::from_bytes, and the tree ids Door43 and the lock file give are read with GitOid::parse_any_case. Done-when: the struct grep prints only crates/tc-core/src/ids.rs; tc-api golden, tc-journal canonical and versioning, and cargo test -p tc-core -p tc-api -p tc-journal -p tc-obs -p tc-door43 -p tc-resources -p tc-check -p tc-helps -p tc-store -p tc-app -p tc-ui pass; fixtures/ unchanged. Not as the card says: crates/tc-api/tests/ids.rs changes one line (StoryNo::new now answers None, not an error) | cargo test -p tc-api --test golden
- 2026-10-24 09:00 EDT | domain lane (Claude Code 9f599c21) | step 6 | none | tc_core::Reference has private fields with getters (book, chapter -> ChapterNo, verses) and Reference::new; its chapter reads digits only. VerseSpan::single takes a VerseNo. tc_helps::refs::ChapterVerse holds a ChapterNo and a VerseNo, private, with getters. Callers converted (reads of .chapter became .chapter().get(), so callers keep their u16); tc-versification's verse_zero test drops its SchemeReference built with VerseSpan::single(0), a value that can no longer be made, and keeps the text-row cases. ids_round_trip for all 27 FromStr types: tc-core tests/ids.rs (BookId, VerseSpan, Reference, GitOid, Md5, with prints-back), tc-api tests/ids.rs (CheckPlace, with prints-back), tc-door43 tests/ids.rs (AccountName, RepoName, NewRepoName, RepoRef, TagName, TokenName with prints-back; ServerUrl round trip), tc-burrito, tc-resources and tc-versification tests/properties.rs (outside-file parsers: round trip only). Spellings read but written otherwise, left out of prints-back on purpose: a lower-case book code (BookId, CheckPlace; tC4's files have both), a span with equal ends (`40-40` reads as 40), a leading zero in a verse label (USFM `\v 01`). zero_is_not_a_chapter_verse_or_story in tc-core tests/ids.rs. Proof mutation (3), u16::parse back in Reference::from_str: ids_round_trip fails with `left: "TIT 1:1" right: "TIT +1:1"`, minimal input reference = "TIT +1:1". cargo clippy --workspace --all-targets passes | cargo test -p tc-core --test ids
- 2026-10-24 09:09 EDT | domain lane (Claude Code 9f599c21) | step 7 | none | tc_core::js::{is_space, trim} with the_spaces_are_ecmascripts: the set over every char is exactly ECMAScript's 25 code points, and equals tc-usfm codec's formula (Rust whitespace minus U+0085 plus U+FEFF) on every char, so codec.rs's js_trim now uses it. Copies replaced by imports: tc-api model.rs (js_space, js_trim stay public names as a re-export, used by tc-ui's shell), tc-app js.rs, tc-check js.rs, tc-helps blocks.rs, tc-obs js.rs, tc-resources strongs.rs, tc-ui understand.rs, tc-usfm codec.rs. tc-usfm words/mod.rs's is_whitespace is the tokenizer's own table (string-punctuation-tokenizer), not `\s`; renamed in_whitespace_table so the two are not mistaken. The card's grep prints only crates/tc-core/src/js.rs and crates/tc-usfm/src/derivations/js.rs; clippy --workspace --all-targets passes | cargo test -p tc-core js
- 2026-10-24 09:17 EDT | domain lane (Claude Code 9f599c21) | step 8 | none | tc-store's seed.rs moved whole into tc_usfm::seed (source_book_name, markers, seed_book and their tests); tc-app create.rs's skeleton (now given the verse counts, so tc-usfm needs no versification) and id_book moved there too. tc-align's walks moved into tc_usfm::source: source_words (usfm-js's verse-object rules, Phrases, HIDDEN_SPANS and SHOWN_SPANS), push_leading, and aligned_walk (the suggestion bench's per-milestone reading, which step 2 kept); tc-align converts their words with SourceWord::from_usfm. Document, Node, Marker, Milestone and Span are reached only through tc_usfm::raw; tc-usfm's own tests import from raw. The card's grep prints nothing; clippy --workspace --all-targets passes; cargo test -p tc-usfm -p tc-align pass. crates/tc-arch does not exist yet, so there is no imports allow line to remove: CHK-1's imports allow list needs no tc-align line | grep -rn "tc_usfm::Document" crates/*/src
- 2026-10-24 09:17 EDT | domain lane (Claude Code 9f599c21) | note | none | For CHK-1: crates/tc-arch is absent; its imports.txt needs no tc-align line for tc_usfm's raw types (no crate outside tc-usfm names them now) | .audit/roadmap/units/CHK-1.md
- 2026-10-24 09:35 EDT | domain lane (Claude Code 9f599c21) | verify | 245e0ad1 | Lens loss (fresh subagent, read only, ran tc-journal and tc-align tests and a scratch probe of split_aligned): 1 Code: good types, but nothing tied the stored cards to the checked runs. 2 Checks: no test of runs sharing an outer word with other inner words. 3 Reach: Add books with aligned USFM from a tool other than tC3/tC4. 4 Siblings: tC4 project open (tc-app seed/records.rs) rewrites `invalid` to `markedInvalid` and reads as ThisApp, a second path for tC4 records. 5 No loss: MAJOR, runs `A B: x` and `A C: y` became one card linking x to C and y to B, silently, where the old split kept the links; every target word is on a card or banked; extra kept. 6 Prose: alignment.rs's "same links" and "never on two cards" were false for such shapes. Minors: a second run's other lemma dropped by the merge; `"done": null` or a non-string strong now refuses a tC4 project open; a bad questions ID row dropped and undeclared | review report quoted in this line
- 2026-10-24 09:35 EDT | domain lane (Claude Code 9f599c21) | verify | 245e0ad1 | Lens races and protocol (fresh subagent, ran tc-api golden and ids, tc-journal canonical, versioning, outside): no blocker, no major. Wire and stored forms unchanged; the deleted story converters' failure branches could never run, so tokens, scopes and conflict places name the same records; this app's sidecar cannot be refused by the stricter reader (the projection writes only booleans and strings); a retried Add books gives the same records. Minors: the bad-story refusal text changed; a hand-edited `"1"` occurrence in this app's sidecar reads as 1 and is rewritten as a number; a non-boolean dismissed or markedInvalid kept as extra by Writer::Tc4 would be refused on read-back (no production caller of Tc4) | review report quoted in this line
- 2026-10-24 09:35 EDT | domain lane (Claude Code 9f599c21) | verify | 245e0ad1 | Lens browser and parity (fresh subagent, read tC4 d310bf3 and the journeys): BLOCKER, the wasm app build failed, tc-ui web/query.rs:330 and :359 still called StoryNo::new(n).ok() (app-e2e run 40097524494 red, no journey ran). tC4 4.0's Add books keeps aligned USFM in the text, so splitting it out is the rebuild's §13.5 design; within it tc4_cards matches word-aligner's addAlignment grouping. Bible questions keep any Reference as written; Reference parsing refuses only a sign. Minors: a bad-ID questions row and an OBS `1:2:3` row are dropped where tC4 shows them, undeclared; imported cards stand in first-run order, which the aligner re-sorts on open as tC4 does | review report quoted in this line
- 2026-10-24 09:35 EDT | domain lane (Claude Code 9f599c21) | fix | none | Blocker: web/query.rs lists stories with StoryNo::all(); all five wasm clippy gates (tc-ui plain, gallery, app, test-hooks; tc-api) pass. Major: split_aligned keeps a verse as written unless the cards make exactly the runs' links, source words compared with every attribute (so the other-lemma minor is refused too); test an_aligned_verse_whose_cards_would_change_its_links_is_kept_as_written (crossed runs and other lemma kept as written; en_ult's shape still one card); with the guard removed it fails, `left: Some(("x y", … top_words: [α, β, γ] …)) right: None`. The doc comment now says what holds. A `null` flag reads as false, so a tC4 `"done": null` no longer refuses an open. The two question-row differences are declared in docs/APP-DESIGN.md (beside the shared helps reads) and in tsv.rs's doc. The other minors are in .audit/minors.tsv | cargo test -p tc-journal --test shared_rules
- 2026-10-24 14:26 EDT | domain lane (Claude Code 9f599c21) | gate | fa955ad3 | Branch CI green on fa955ad3: app-e2e run 40098778816; ci run 40098778867 green on its rerun (attempt 2; attempt 1 failed only tc-store's review::a_held_file_deleted_while_the_project_is_open_is_rebuilt, the race the orchestrator gave the storage lane, which also failed DOM-1's first gate run) | gh run view 40098778867
- 2026-10-24 14:26 EDT | domain lane (Claude Code 9f599c21) | note | none | Main 0189cee4 merged into track-DOM-2 so the batch starts from a tree that builds: tc-door43's client.rs and lib.rs conflicts resolved to DOM-1's imports less ObjectId (tc_core::GitOid now). crates/tc-arch is on main, so step 8.3's allow line goes: the tc_usfm Document,Node,… line of crates/tc-arch/allow/imports.txt is removed; tc-arch's edge table lets tc-obs depend on tc-core, as step 5 adds. cargo test -p tc-arch passes; the claims check passes with this card marked done | node --test .audit/tools/claims.test.mjs
