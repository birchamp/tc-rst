# The tC4 state seed and its check on the parity scenarios (unit B1a)

Unit B1a of `docs/APP-DESIGN.md` §10 builds STORAGE §13.2's state seed in
`crates/tc-app/src/backend/seed.rs` (`tc4_state_seed`, feature `fixture-backend`) and runs its
check on every project of the parity `.state` scenarios. This file is the write-up the row
asks for: every mismatch with the record, the bytes and the rule involved, for the storage
team, who own `tc-journal` and `docs/STORAGE.md`. Nothing in `tc-journal`, `tc-store` or
STORAGE.md was changed by this unit.

## 1. What ran

- **Code:** branch `app-b1a`, from `app` at `647ec77` (units C and S1 merged). The seed and its
  check are in `crates/tc-app/src/backend/seed.rs`, `seed/check.rs` and `seed/strict.rs`.
- **Scenarios:** `sample`, `en-pins` and `shared`, as
  `parity/tc4-baseline/fixtures/build-state.mjs` builds them from tC4 at `d310bf3` (the row
  said "two": `shared` came with unit A6a). Each has two projects, `sample_burrito` (Titus and
  Jonah) and `sample_burrito_obs` (Open Bible Stories).
- **The bytes:** cargo tests run no node. `crates/tc-app/tests/app/state_seed.rs` assembles
  each scenario's project from `fixtures/tc4/sample-burrito{,-obs}` and the committed overlay
  `fixtures/parity-state/` (the four files `en-pins` rewrites and the two `.remotes.json` of
  `shared`), and `scenario_projects_are_the_recorded_state_bytes` checks every file, both ways,
  against `parity/app/state.sha256.json`, the sha256 of each file as built from the tC4
  checkout. The overlay was copied from `node parity/app/state.mjs --scenario <name> --out
  target/b1a/state-<name>`.
- **Command:** `cargo test -p tc-app --test app state_seed` (31 tests, 1.45 s in a debug build)
  and `cargo test -p tc-app --features fixture-backend --lib backend` (14 tests).

## 2. Outcome

| Scenario | Project | Outcome | Batches | Changes |
|---|---|---|---|---|
| `sample` | `sample_burrito` | passes | 3 | 127 |
| `sample` | `sample_burrito_obs` | refused: 53 mismatches | 51 | 66 |
| `en-pins` | `sample_burrito` | passes | 3 | 127 |
| `en-pins` | `sample_burrito_obs` | refused: the same 53 | 51 | 66 |
| `shared` | `sample_burrito` | passes, the same change ids as `sample` | 3 | 127 |
| `shared` | `sample_burrito_obs` | refused: the same 53 | 51 | 66 |

`sample_burrito`'s 127 changes: the project's batch of 17 (9 free fields of `metadata.json`,
`vrs.json`, 5 pin groups, 2 settings), Jonah's 54 (presence, header, 4 chapter fronts, 48
verses, among them `2:9` holding the span `9-10` and `2:10` covered by it) and Titus's 56
(presence, header, 3 chapter fronts, 46 verses, 1 alignment, 4 decisions).
`sample_burrito_obs`'s 66: the project's batch of 16 (9 free fields, 2 pin groups, 1 setting,
4 carried files) and one `StoryBase` per story. The OBS counts come from the seed before its
check (`obs_stories_are_kept_as_the_files_are`), since the public seed refuses the project.

**Consequence for unit B1b:** a mismatch fails a scenario's load (APP-DESIGN §4.2), so the
`MemoryBackend` cannot load the OBS project of any scenario until the storage team fixes
mismatches 4.1 to 4.3. The Bible project loads in every scenario.

## 3. The rules of the check

The seed is checked by what tc-journal's fold projects, through tc-journal's own functions
(`render`, `derived_files`, `check_texts`, `carried_files`, `metadata_source`, `projected`,
`heads`), and never through `FoldState::record_keys` (APP-DESIGN §4.3). Every rule runs and
every mismatch is collected; any mismatch refuses the seed and nothing is made.

1. **The fold.** No conflict, no anomaly, and every seeded change is the one head of its
   record and the value the fold shows.
2. **The file set.** Every file of the folder that the format derives must be among
   `derived_files()`; one that only `render()` gives would not be written by a materialize.
   A derived file the folder lacks is listed in the report as added (nothing is lost by it).
3. **Bytes.** Every USFM file, `vrs.json` and story file equals its projection byte for byte;
   every carried file is placed at its path with the source's git blob id, size and md5, and
   its bytes are kept under that blob id.
4. **Frames.** Each frame of a story reads through the fold (`check_texts(Stories)`) as tC4
   reads it from the file (`parseStory`, `tc_obs::StoryFile`): what the story screens and the
   frame checks see.
5. **Alignments,** read as tC4 reads them: `alignments` and `wordBank` equal as ordered lists
   (word order is the translator's work); `targetVerseMd5`, `sourceVersion` and `source` equal;
   `done` absent reads as false; `invalid` reads with I-3, so a record whose hash is not the
   verse's is invalid whatever its flag says (the verse's hash is that of the projected verse
   value the label's first verse holds); every other field equal both ways; the journal's own
   fields (`markedInvalid`, `dismissed`, `anchor`, `spanEnd`) compared only where the source
   has them. The document holds only `schemaVersion`, `book` and `chapters`. Decoding the
   projected file gives every seeded record back.
6. **Decisions,** read as tC4 reads them, keyed by canonical `contextId` (record order is not
   compared): `contextId` and `category` equal; `selections` with `[]` read as `false`
   (`normalizeDecision`) but an absent one kept apart from `false`, since tC4's `isDecided`
   reads it as a selection (`src/data/derive.ts:738`); `comments`, `reminders`,
   `nothingToSelect`, `invalidated` and `userInvalid` absent read as false; the status as tC4
   reads it (`valid` on an invalidated record reads `invalid`, and an absent one is derived:
   invalidated is `invalid`, done is `valid`, else `todo`, BURRITO-SPEC §5.2);
   `modifiedTimestamp` as an instant; every other field equal both ways; the journal's own
   fields (`userStatus`, `verseTextMd5`, `pinnedResource`, `importedInvalid`, `dismissed`,
   `checkKey`) only where the source has them. The document's `schemaVersion`, `tool`, `book`
   and `resource` are equal, and it has no other member. Decoding gives every record back.
7. **`resources.json`** equals the source (or its migration, for `schemaVersion: 1`) as JSON,
   member order aside.
8. **`settings.json`** equals the source as JSON.
9. **`metadata.json`:** every free field is what `metadata_source()` builds the document from.
   The derived fields (`format`, `meta.version`, `meta.category`, `meta.generator`,
   `meta.normalization`, `idAuthorities.local`, `idAuthorities.dcs`,
   `identification.primary.local`, `type`, `relationships`, `ingredients`;
   `tc_burrito::DERIVED_FIELDS`) are not compared: the builder makes them (STORAGE §3.4), as
   tC4's server owns them (tC4 passes `meta: null` to its own seed).

**Declared differences** (in the report, never silent): a decision's `verseEdits` is recomputed
from the hash §13.2 takes from the current verse; a decision file's `summary` is dropped (tC4's
own seed drops it as a cache, `journalingStore.ts:1827-1835`); `resources.json` gains an empty
`resources` or `languageSets` member; a `schemaVersion: 1` `resources.json` is migrated; a
timestamp is rewritten in tC4's own form.

## 4. The mismatches

### 4.1 Frame 0 reads with the story's number (all 50 stories of `sample_burrito_obs`)

- **Files and records:** `ingredients/content/01.md` to `50.md`, frame 0 (the title) of each
  story.
- **Bytes:** the first line of `01.md` and of `02.md`:

  ```text
  # 1. La Creación
  # 2.
  ```

  | Story | tC4 reads frame 0 as | the fold reads frame 0 as |
  |---|---|---|
  | 1 | `La Creación` | `1. La Creación` |
  | 2 to 50 | `` (empty) | `2.` … `50.` |

- **The rule:** STORAGE §7.7 (`docs/STORAGE.md:2982-2984`): "the template's first `# ` line is
  frame 0". tc-journal follows it: `template_frame` (`crates/tc-journal/src/fold/project.rs:2004`)
  reads frame 0 as everything after `# `, and `splice_story` (`project.rs:2054`, the title at
  `:2085`) writes `# ` and the record's text. tC4 reads the title with `TITLE_RE`
  (`journal/story.mjs:15`, `/^# (\d+)\.( (.*))?$/`), the text after `# N. `, and writes it with
  `writeTitle` (`journal/story.mjs:151-158`) as `# N. title`, or `# N.` when empty. tc-obs
  follows tC4 (`title_line`, `crates/tc-obs/src/story.rs:65`; `title_line_of`, `:54`).
- **What a translator would see:** the story screen's title box shows `1. La Creación` where
  tC4 shows `La Creación`, and `2.` where tC4 shows an empty title; a check on a story title
  (frame 0) is assessed against the text with the number. A title edit then loses the number
  (5.1).
- **Owner:** storage (tc-journal and STORAGE §7.7).
- **Fix:** read and write frame 0 as tC4 does: frame 0 is the title text of `TITLE_RE`, and the
  splice writes `# N. ` and the title (`# N.` for an empty one). STORAGE §7.7's sentence would
  say so.
- **Done (B7, 2026-10-19):** `template_frame` reads frame 0 as tC4's `TITLE_RE` does, and
  `splice_story` writes it as `writeTitle` does, keeping the line's own number. STORAGE §7.7
  and §3.2 still carry the old sentence; the storage branch owns that file.

### 4.2 `checking/settings.json` is not a derived file of an OBS project

- **File:** `ingredients/checking/settings.json` of `sample_burrito_obs` (177 bytes):

  ```json
  {
    "schemaVersion": 1,
    "checkCategories": {
      "translationWords": [
        "kt",
        "names",
        "other"
      ],
      "translationNotes": [
        "translate"
      ]
    }
  }
  ```

- **Rule:** the seed makes its `Setting` record and `render(&DerivedFile::Settings)` gives the
  file back, but `derived_files()` lists `Settings` only for a Bible project
  (`crates/tc-journal/src/fold/project.rs:526-528`), so a materialize would not write it and
  the file would leave the project. STORAGE §3.2 (`docs/STORAGE.md:984-996`) lists no
  `settings.json` for an OBS project. tC4 derives it for every project:
  `journalingStore.ts:716-723` ("resources.json and settings.json are ALWAYS derivable") and
  `journal/checkpoint.mjs:252`.
- **Owner:** storage.
- **Fix:** list `DerivedFile::Settings` for both kinds, and `checking/settings.json` in STORAGE
  §3.2.
- **Done (B7, 2026-10-19), with C6's identical change:** the same line.

### 4.3 The OBS text pin's relationship makes `metadata.json` invalid (pin groups `Primary` and `Fallback`)

- **File and records:** `ingredients/checking/resources.json`, the pin groups `Primary` and
  `Fallback` of `sample_burrito_obs`; in `en-pins` both groups are the English OBS suite, with
  the same result.
- **Bytes:** the slot that causes it, in both groups:

  ```json
  "obs": {
    "repoPath": "git.door43.org/es-419_gl/es-419_obs",
    "version": "v2",
    "sha": "4a239590d543df59f77d5ee624d475a7488b5fc3",
    "flavor": "gloss/textStories"
  }
  ```

  The fold lists two anomalies:

  ```text
  InvalidForBurrito { key: Pins(Primary), reason: "\"gloss\" is not one of \"source\", \"target\" or 3 other candidates (at \"/relationships/0/relationType\", schema keyword enum)" }
  InvalidForBurrito { key: Pins(Fallback), … the same reason }
  ```

- **Rule:** tc-journal's `relationships` (`crates/tc-journal/src/fold/metadata.rs:136-171`)
  mirrors every pin with a repository and a flavor, taking the flavor's type as the
  relationship's type (`scripture` becomes `source`), so the OBS text pin becomes
  `{"relationType": "gloss", "flavor": "textStories", …}`, which the Scripture Burrito schema
  refuses. The fold then uses no value for the whole group (`choose`, `fold/metadata.rs:80-125`), and
  `metadata.json` gets none of the group's relationships (tW, tA, OBS notes, OBS words). tC4's
  `relationshipsFromPins` (`journal/relationships.mjs:13-19, 37-38`) maps only `scripture`,
  `parascriptural` and `peripheral` and leaves any other pin out ("A pin of any other type
  (`gloss/textStories`, the OBS source text) has no relationship the bundled schema accepts …
  so it has no row"). Smaller differences in the same function, which the check does not see
  because `relationships` is a derived field: a flavor without `/` becomes `source` where tC4
  leaves the pin out; rows are deduplicated by the whole row where tC4 keeps the first row of
  each repository; and the extra scripture comes last (`GROUP_ORDER`, `fold/metadata.rs:173`)
  where tC4 puts it second.
- **Owner:** storage.
- **Fix:** skip a pin whose flavor type is not `scripture`, `parascriptural` or `peripheral`
  (or has no `/`), as tC4 does; then follow tC4's row order and first-row-per-repository rule.
- **Done in part (B7, 2026-10-19):** `relationships` skips a pin whose flavor is no scripture,
  parascriptural or peripheral, or has no `/`, as tC4 does, so the OBS pin groups hold no
  anomaly. The row order and the first-row-per-repository rule are still the storage team's.

## 5. Findings outside the check

Both are asserted as they stand today by
`backend::seed::tests::obs_frame_edits_project_other_bytes_than_tc4_writes_today`, which
folds one frame edit by a person onto the OBS sample's seed, so the test fails the day storage
changes them.

### 5.1 A title edit loses the story number

A `Frame` change of story 1, frame 0, with the text `La Creación` (what tC4's title box sends)
projects the first line

```text
# La Creación
```

where tC4's `writeTitle` writes `# 1. La Creación`. The number is lost, and tC4's `TITLE_RE`
no longer reads the line as a title: `tc_obs::StoryFile::parse` refuses the file, as tC4's
`parseStory` would. Same rule and fix as 4.1.

### 5.2 An edit of an empty frame is spliced in another layout

A `Frame` change of story 1, frame 3 (empty in the sample), with the text `Texto nuevo.`
projects, between the image lines of frames 3 and 4:

```text
![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-03.jpg)\n\n\n\nTexto nuevo.\n![OBS Image](…-01-04.jpg)
```

tC4's `writeFrame` (`journal/story.mjs:135-150`, and `tc_obs::story::write_frame`) replaces the
whole region with one blank line on each side:

```text
![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-03.jpg)\n\nTexto nuevo.\n\n![OBS Image](…-01-04.jpg)
```

`splice_story` (`project.rs:2054-2120`) keeps the template's blank lines after the image and
puts the text after them, ending at the next image line. tC4 still reads the frame's text, but
reading the file back (`tc_obs::decode_story`, as an outside edit is read) names frame 3 as
reformatted, and the bytes differ from what tC4 or a teammate's tC4-compatible writer makes.
**Fix:** splice a frame as `writeFrame` does (the region replaced by `['', …text lines, '']`,
or the template's empty form for an empty text).

5.1 is fixed (B7, with 4.1); 5.2 is still open.

## 6. Declared differences

Per project, in every scenario:

- `sample_burrito`: `SummaryDropped` for `checking/translationNotes/TIT.json` and
  `checking/translationWords/TIT.json`; `VerseEditsRecomputed` for tN `gr8c` at TIT 1:4
  (`true` in the file, `false` projected: the fold computes it from the hash §13.2 takes from
  the current verse; the decision stays invalidated through `imported_invalid`). Ignored
  files: `.gitignore` (written by the app), and `.remotes.json` in `shared` (the mock
  server's remote list).
- `sample_burrito_obs` (its check fails, so its report is not issued): `EmptyMemberAdded`
  `resources` in `resources.json`, since tc-journal always writes the member and tC4 omits an
  empty one (`journal/checkpoint.mjs:133`).

Declared differences from tC4 4.0 (AGENTS.md, "Matching tC4"):

- A `schemaVersion: 1` `resources.json` is migrated as BURRITO-SPEC §5.3 describes (STORAGE
  §13.2); tC4 4.0's seed refuses such a project, since it compares the projected v2 document
  with the file. The translator gets their project with the English links pinned instead of a
  refusal they cannot act on. No public tC4 commit or fixture holds a v1 file; BURRITO-SPEC
  §5.3 is the only definition, and the added `translationWordsLinks` is the bundled `en_twl`
  pin with its flavor (tC4 requires a flavor of every pin).
- `verseEdits` is not carried: the format computes it, and no tC4 4.0 view reads it.
- A decision with no `modifiedTimestamp` is written with the time its change carries,
  `meta.dateCreated` or else 2020-01-01T00:00:00.000Z (S-3), and the report declares it
  (`TimestampAdded`). tC4 4.0 writes such a decision back without one and reads it as changed
  at the moment it reads it (`normalizeDecision`, `src/data/httpStore.ts:307`), a time that
  moves at every read and that no change can hold. No tC4 4.0 screen shows the time. None of
  the scenarios' decisions lacks one.
- The check compares tC4's reading of each record, where tC4's compares whole documents; the
  rebuild writes computed flags (`status`, `done`, `invalid`) explicitly, with the same meaning
  for every tC4 reader.
- `metadata.json`'s free fields are seeded and checked; tC4's seed passes `meta: null` and
  leaves the file to its server. More is kept, nothing is lost.

## 7. Spec questions for the storage team

- **S-1.** A §13.2 seed's `ProjectId`: STORAGE gives none (§13.1 T4 is for a project with a
  journal). The fixture backend uses `fixture_project_id`, a version-8 UUID from
  sha256("tc4-state-seed\0" ‖ the folder's name), which is not an import rule.
- **S-2.** Its `ImportSource`: there is no seed variant; the seed writes
  `Burrito { file_name: <the folder's name> }`.
- **S-3.** The `at` of a seed change other than a decision's (whose `at` is its
  `modifiedTimestamp`, R-8.8.2): the seed uses `meta.dateCreated`, else tC4's floor
  2020-01-01T00:00:00.000Z (`journal/reconcile.mjs:137`).
- **S-4.** `VersificationValue.name` for a seed: tC4 records the name `unrecorded`
  (`journalingStore.ts:1693`); the seed stores `None` and readers fingerprint the bytes.
- **S-5.** `source` of a seeded alignment: §13.1 sets it from the original-language pin whose
  label equals `sourceVersion`; §13.2 says nothing, and the seed leaves it `None` (a tC4
  Phase-1 record names only `sourceVersion`).
- **S-6.** Whether `verseEdits: true` should survive a seed: it cannot, with §13.2's
  "`verse_text_md5` from the current verse"; the seed declares it.
- **S-7.** `DeviceSeq` of a non-Local change counts from 0 or 1 ("the index of the change
  inside its batch", `ids.rs`); the seed counts from 0.
- **S-8.** The empty `resources` member tc-journal always writes in `resources.json`
  (`project.rs:1391, 1400`), where tC4 omits an empty one.
- **S-9.** An OBS project's licence and front and back matter at a seed: STORAGE §13 covers
  OBS nowhere; the seed makes them `CarriedFile` records, as §3.2 does at creation.
- **S-10.** A pin without `sha` in a seeded `resources.json`: STORAGE §13 says an import never
  stores one (tC4 D82); the seed keeps it as tC4 kept it, and the pins reader shows it
  unusable. That rule seems meant for pins an importer resolves.

## 8. Measurements

Release build, `crates/tc-app/examples/state_seed_bench.rs` at the commit that adds this file,
20 seeds per project after one read of its folder:

```text
cargo run --release -p tc-app --features fixture-backend --example state_seed_bench -- --state target/b1a/state-<scenario> --runs 20
```

| Scenario | Project | Read | Seed median | Seed p95 | Slowest (the first) |
|---|---|---|---|---|---|
| `sample` | `sample_burrito` (passes) | 0.31 ms | 5.78 ms | 6.05 ms | 20.7 ms |
| `sample` | `sample_burrito_obs` (refused) | 0.23 ms | 4.79 ms | 4.93 ms | 5.0 ms |
| `en-pins` | `sample_burrito` | 0.26 ms | 5.16 ms | 5.97 ms | 27.8 ms |
| `en-pins` | `sample_burrito_obs` | 0.24 ms | 5.01 ms | 5.34 ms | 5.8 ms |
| `shared` | `sample_burrito` | 0.16 ms | 6.06 ms | 6.19 ms | 19.0 ms |
| `shared` | `sample_burrito_obs` | 0.24 ms | 5.19 ms | 5.47 ms | 5.5 ms |

- **The first seed in a process** takes about 20 ms. It is not the fold's thread pool (with
  `RAYON_NUM_THREADS=1` it is the same) but the Scripture Burrito schema, which
  `tc_burrito::check_field` compiles once per process (12.7 ms measured alone); with the schema
  compiled first, the first seed takes 7.3-8.0 ms. `tc4 serve --backend memory` (B1b) would pay
  it once at start, plus about 5 ms per project: under 100 ms for the six projects of the three
  scenarios.
- **Machine:** the owner's Linux PC, Intel Core i7-12700 (20 threads), 61 GB; the CPU **has**
  SHA instructions (`sha_ni`) and no AVX-512 (`avx512f` absent). Other agents were building on
  it (load average 5.6 to 6.1). No budget applies to this fixture path.

## 9. Seen while running the gates: a storage property test failed once

Not a finding of the seed check, and in code this unit did not touch: in one of three runs of
`cargo test --workspace` on this branch, tc-journal's property test
`mixed_minor_versions_converge` (`crates/tc-journal/tests/properties.rs:1179`) failed with
"VerseText {78, 79} is lost with newer fields at crates/tc-journal/tests/properties.rs:1253".
Two reruns of the test passed: its cases are random (`failure_persistence: None`,
`tests/common/mod.rs:858-861`), so it fails only on some seeds. The shrunk input, for a
regression test:

```text
Script { devices: 3, last_books_removed: true, ops: [
  Edit { dev: 0, at: 0, stale: false, newer: false }, Renumber { dev: 0, at: 49 },
  Carried { dev: 0, path: 1, value: 205 }, Drag { dev: 177, at: 204 },
  Fork { a: 241, b: 57, what: 208, at: 198 }, Profile { dev: 172, value: 137 },
  Layout { dev: 115, at: 29 }, Fork { a: 182, b: 183, what: 155, at: 229 },
  Edit { dev: 255, at: 179, stale: false, newer: true }, Renumber { dev: 29, at: 242 },
  Renumber { dev: 251, at: 191 }, Exchange { from: 3, to: 4, mode: 105, seed: 208 },
  Setting { dev: 161, path: 167, value: 63 }, Renumber { dev: 10, at: 14 },
  Fork { a: 160, b: 92, what: 23, at: 101 }, Resolve { dev: 163, pick: 117, how: 215 },
  Layout { dev: 226, at: 123 }, CarryOver { dev: 252, check: 223 },
  Stray { dev: 242, at: 7, covered: true }, Fork { a: 144, b: 130, what: 235, at: 179 } ] }
```

The assertion (`properties.rs:1244-1253`) means that a verse-text conflict the plain history
lists, between its changes 78 and 79, is neither listed nor grown into a larger conflict when
the same history carries newer-minor fields: a conflict a person should see would not be shown.
It goes to the storage team with the rest of this file.
