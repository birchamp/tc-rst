# Plan for design unit B1a: the seed spike

Planner's revision 1, 2026-10-18. Commit this file with your first commit; it is the only file
the planner wrote.

Worktree `/home/bot/tc-rst/.claude/worktrees/app-b1a`, branch `app-b1a`, cut from the integration
branch `app` at `647ec77` (branch `app` was created from `claude/focused-ride-5uzj7p` for this
unit). Storage units C and S1 are merged there (`de3f164`, `2d0cb2b`), so `tc-journal` and
`tc-store` are on this branch. Before you start: `git -C /home/bot/tc-rst log --oneline -3 app`;
if `app` moved, merge it into `app-b1a` first.

The unit's row (docs/APP-DESIGN.md §10, phase 1, line 2395):

> B1a Seed spike *(A-16)* | `crates/tc-app/src/backend/seed.rs` only: `tc4_state_seed` and §13.2's
> check on the two `.state` scenarios (`build-state.mjs` run on the tC4 checkout): `ProjectMeta`,
> `Versification`, books through the codec, pins with the schema 1 → 2 migration, settings,
> decisions, alignments, OBS story records | `storage` (tc-journal); rebased when C merges | C, if
> a mismatch is a spec question | the seed check on both scenarios: USFM and `vrs.json` byte for
> byte, sidecars field for field | the check passes on both scenarios, or each mismatch is written
> up with the record, the bytes and the rule involved and handed to unit C

No owner question (Q-n) and no storage request (R-n) gates this row. Unit C is merged; its
successors on `tc-journal` are the storage team's units on branch `storage` (S2 is running there).
"Handed to unit C" now means: the write-up of §11 below, which the orchestrator relays to the
storage team. **Never edit `crates/tc-journal`, `crates/tc-store`, `docs/STORAGE.md`, branch
`storage` or `.claude/worktrees/storage`.** A storage bug is written up, not fixed here.

What the planner read (re-read the parts the steps cite): AGENTS.md; docs/PLAN.md;
docs/ARCHITECTURE.md; docs/PERFORMANCE.md; APP-DESIGN "Owner's answers", §0–§2 (AD6, AD7, AD32,
AD35, AD36), §4.1, §4.2 (the seam, `MemoryBackend`, the contract suite), §4.3 (the rule "nothing in
tc-app calls `FoldState::record_keys`"), §7.1–7.2, §10 (introduction, phase 1, order paragraph),
§11 risk 2, §12 Q-16 and Q-23, RT-16 and its answer; docs/STORAGE.md §2.1 (hash formulas), §2.2–2.3,
§3.1–3.4, §5.15, §7.7 (derived files), §13 (common rules, §13.1 T4–T8, §13.2, §13.5);
`parity/tc4-baseline/fixtures/build-state.mjs`, `parity/app/{state.mjs,state.test.mjs,
state.sha256.json}`; `fixtures/README.md`, `fixtures/tc4/sample-burrito{,-obs}/**`; tc-journal
(`lib.rs`, `change.rs`, `ids.rs`, `keys.rs`, `record.rs`, `pending.rs`, `pack.rs`, `rules.rs`,
`format.rs`, `decision.rs`, `pins.rs`, `fold.rs`, `fold/{state,sidecar,project,metadata,reads}.rs`,
`tests/{projection.rs,screens_and_fold.rs,support/mod.rs,rules.rs}`); tc-obs (`story.rs`, `ids.rs`,
`tests/codec.rs`); tc-burrito (`build.rs`); tc-resources (`suite.rs`, `pin.rs`, `sets.rs`); tc-app
(`Cargo.toml`, `lib.rs`, `seed.rs` (A9's), `tests/app.rs`, `tests/app/common.rs`, `examples/`);
tC4 at `d310bf3`: `journal/reconcile.mjs:135-205` (`seedFromSidecars`),
`src/data/journal/journalingStore.ts:1665-1900` (`seedStateProblems` and the five problem
finders, and `:716-723` "resources.json and settings.json are ALWAYS derivable"),
`journal/checkpoint.mjs:106-140` (`projectResources`), `journal/story.mjs:1-40` (`TITLE_RE`),
`docs/BURRITO-SPEC.md:180` (I-3), `:286-290` (done, `status`, `userInvalid`), `:353` (the
schemaVersion 1 → 2 migration), `:561` (R-8.8.2).

---

## 1. What B1a is, and what it is not

**Is.** A pure function in tc-app, `backend::seed::tc4_state_seed`, that turns one tC4 project
folder with no journal (tC4's "Phase-1 sidecars" state, STORAGE §13.2) into the changes of a
STORAGE-format project: one batch for the project and one per book (Bible) or per story (OBS),
made by the device `Importer("tc4-seed")`, sealed into real packs, read back and folded with
tc-journal's real fold. Then the check: what the fold projects must give back the folder, USFM,
`vrs.json` and story files byte for byte, every sidecar record field for field, every non-derived
`metadata.json` field, every carried file. Any mismatch refuses the seed and nothing is made. The
check runs on every project of the parity `.state` scenarios. Each mismatch it finds is written up
(record, bytes, rule) for the storage team. This is the copy that `MemoryBackend` (B1b) loads and
that unit I's importer later replaces (AD7: `memory_seed_equals_tc_import_seed`, then deletion).

**Not B1a.** The seam (`ProjectStore`, `ProjectHandle`, `EditToken`, `ReviewToken`), the
`MemoryBackend`, the scenario loader with resources, sessions, tokens, `wire`, counters (all B1b);
any endpoint; any screen (B1a builds none, so "the screens match tC4 4.0" holds trivially; the
check is what later keeps `MemoryBackend` screens honest, AD7); writing anything to disk (C1a
writes the parity projects through tc-store later); importing tC4 projects with a journal (§13.1,
unit I); any change to tc-journal, tc-store or STORAGE.md.

---

## 2. Facts in the code that the steps rely on

Each of these was read; several contradict the design text, and §12 fixes the design.

1. **`encode_pending` refuses the seed's device.** `tc_journal::pending::encode_pending`
   (`crates/tc-journal/src/pending.rs:223-229`) returns `PendingWriteError::NotLocal` unless the
   header's device is `DeviceKind::Local`. §13.2 makes the seed's author `Importer("tc4-seed")`.
   Packs, not pending files, carry other devices' changes: `tc_journal::pack::seal(&PackSpec {
   project, sealer, sealed }, &lines)` (`pack.rs:596`) needs only the *sealer* to be Local, and
   `read_pack(path, bytes, project)` (`pack.rs:304`) reads it back verified. §13.1 T8 does the same
   ("Seal the converted and reconcile changes into packs of the importing device's sealer").
   APP-DESIGN §4.2 says `encode_pending`: wrong, fixed in §12.
2. **The sidecar decoders read the format's own fields, not tC4's flags.**
   `decode_alignments` (`fold/sidecar.rs:132`) ignores tC4's `invalid` and reads `markedInvalid`;
   §13.2 wants `marked_invalid` from `invalid`. `decode_decisions` (`:230`) *requires*
   `verseTextMd5` (tC4 records have none), reads `importedInvalid` (tC4 writes `invalidated`) and
   reads `status` only when the record is not invalidated, so a tC4 "To do" on an invalidated
   decision would be lost. The seed must rewrite each tC4 record before decoding it. The rule is
   already written once, in `crates/tc-journal/tests/screens_and_fold.rs:471-504` (`seeded()`):
   copy it (D6).
3. **Decision flags are computed by the projection.** `decision_record`
   (`fold/project.rs:1761-1830`) writes `verseEdits`, `invalidated`, `status` and `userInvalid`
   from `decision::assess` (`decision.rs:126-159`), writes `modifiedTimestamp` as the `at` of the
   record's newest head (`project.rs:942-948`, `rfc3339` at `:122`, form
   `YYYY-MM-DDTHH:MM:SS.sssZ`), and writes `status: "invalid"` for an invalidated decision with no
   status of its own. It adds the journal's own fields (`userStatus`, `verseTextMd5`,
   `pinnedResource`, `importedInvalid`, `dismissed`, `checkKey`) "so a decoder reads the record
   back".
4. **Alignment `invalid` is computed** (`project.rs:775-800`, `alignment_record` at `:1691`) from
   the record's hash against the current verse and its `source` pin (I-3), and `done` is always
   written. tC4 reads an absent `done` as false and treats a hash mismatch as invalid whatever the
   flag (BURRITO-SPEC:180).
5. **The sample's alignment already round-trips.**
   `tc-journal/tests/screens_and_fold.rs:546-571` imports the sample's Titus sidecar and asserts the
   projected `chapters` equal tC4's except the added `done: false`.
6. **`derived_files()` lists `checking/settings.json` only for Bible projects**
   (`project.rs:526-528`). The OBS sample has `ingredients/checking/settings.json`
   (`checkCategories`), and tC4 always projects it for both kinds (`journalingStore.ts:716-723`,
   `journal/checkpoint.mjs:252`). `render(&DerivedFile::Settings)` still works for an OBS fold
   (`project.rs:699`), so only a file-set comparison sees that a materialize would drop the file.
   Expected mismatch; do not hide it.
7. **`resources.json` always gets `"languageSets"` and `"resources"` members**
   (`project.rs:1391`, `:1400`), empty or not. tC4 omits an empty `resources`
   (`checkpoint.mjs:131`), and the OBS sample has none. Equal for every reader; a declared byte
   difference (D7), reported.
8. **Story files: the fold and tc-obs model a story differently.** tc-obs's `decode_story`
   (`tc-obs/src/story.rs:738`) gives `StoryRecords` whose base has every recorded part in the
   writers' empty form and whose title excludes `N. ` (`title_line_of`, `:54`; tC4's `TITLE_RE`,
   `journal/story.mjs:15`). The fold's `splice_story` (`project.rs:2054`) keeps the base's lines
   and replaces only a frame's paragraph lines between blank lines, writes the title line as
   `"# " + frame 0`, and reads frame 0 of a base as everything after `# ` (`template_frame`,
   `:2004`), number included. Seeding `decode_story`'s records therefore cannot reproduce the files:
   every frame region and every title line would change. What does reproduce them is tC4's own
   model of a project without a journal: the story's base **is** the committed file and no frame is
   journaled until it is edited (tc-obs `decode_story` docs: "tC4 keeps a frame's bytes until its
   journal holds that frame; its base is the committed file"; tC4 `seedFromSidecars` seeds no
   story at all). D5 seeds `StoryBase` = the file. The frame-0 difference still shows on the
   screens (the fold reads the title as `1. La Creación`, tC4 as `La Creación`), and the check's
   frame-text rule finds it (expected mismatch, §7).
9. **OBS carried files.** STORAGE §3.2: `ingredients/LICENSE.md`, `content/front/title.md`,
   `content/front/intro.md`, `content/back/intro.md` are CARRIED files (`CarriedFile` records with
   `Present { oid, size, md5 }`, the bytes kept by blob oid). The format's derived patterns
   (`format.rs:106-114`) and owned paths (`rules.rs:276`) make any other file under
   `ingredients/checking/`, `ingredients/content/*.md`, `ingredients/*.usfm` or `ingredients/tc/`
   impossible to carry (`Change::new` refuses it, rule V8).
10. **`metadata.json` is derived except its free fields.** `tc_burrito::DERIVED_FIELDS`
    (`tc-burrito/src/build.rs:16-28`, `is_derived_field` at `:31`): `format`, `meta.version`,
    `meta.category`, `meta.generator`, `meta.normalization`, `idAuthorities.local`,
    `idAuthorities.dcs`, `identification.primary.local`, `type`, `relationships`, `ingredients`.
    Every other field is a `ProjectMeta` record: `MetaField::{Name, Abbreviation, Description,
    Languages, Copyright}` for the modelled five (`keys.rs:562-580`), `MetaField::Extra(MetaPath)`
    for the rest (`meta.defaultLocale`, `meta.dateCreated`, `confidential`, `localizedNames`,
    `idAuthorities.<other>`, `identification.primary.<other>`, `identification.upstream`, other
    top-level names), exactly as `crates/tc-corpus/src/history.rs:973-1004` does.
    `FoldState::metadata_source((FORMAT_MAJOR, FORMAT_MINOR))` (`fold/metadata.rs:276`) returns the
    fields it would project, keyed by dotted path; a value that fails `check_field` is left out
    and listed as `Anomaly::InvalidForBurrito`.
11. **tC4's own seed compares whole documents** (`journalingStore.ts:1798-1897`), drops a decision
    file's `summary` ("derived cache ... disposable"), and refuses an unknown top-level field or two
    records on one identity key. Its resource check compares the projected v2 document with the
    file, so tC4 4.0 refuses to seed a `schemaVersion: 1` `resources.json`; no v1 file exists in
    any public tC4 commit or fixture (the oldest public commit, `e4ada8b`, already writes v2). The
    only definition of v1 is BURRITO-SPEC:353.
12. **The scenarios.** `build-state.mjs` now has three scenarios: `sample`, `en-pins`, `shared`
    (the row says "two"; `shared` came with A6a). Their project folders are:
    - `sample`: `fixtures/tc4/sample-burrito` and `fixtures/tc4/sample-burrito-obs`, byte for byte
      (verified: `diff -r` of a build against the fixtures is empty);
    - `en-pins`: the same with four files rewritten by `pinEnglishSuite`/`pinEnglishObsSuite`:
      `sample_burrito/ingredients/checking/{resources.json, translationNotes/TIT.json,
      translationWords/TIT.json}` and `sample_burrito_obs/ingredients/checking/resources.json`;
    - `shared`: `sample` plus a `.remotes.json` at each project's root (the mock Pankosmia server's
      remote list).
    `parity/app/state.sha256.json` records the sha256 of every file of every scenario as built from
    tC4 `d310bf3`; `node --test parity/app/state.test.mjs` (6 tests, passing here) proves a build
    from this repository equals it. `build-state.mjs`'s `DEFAULT_TC4` is the cloud path
    `/home/user/unfoldingword/translationcore4`: here pass `TC4_ROOT=/home/bot/unfoldingword/translationcore4`.
13. **Hash formulas.** STORAGE §2.1: `‖` is byte concatenation; strings are UTF-8, ids and digests
    (`ProjectId`, `GitOid`, `Md5`) their raw bytes. `DeviceKind::Importer` =
    sha256(`"import\0"` ‖ kind ‖ projectId)[..15] (`ids.rs:138-140`); build it with
    `DeviceId::derived(DeviceKind::Importer, &input)` (`ids.rs:171`). Local devices number their
    changes 1, 2, 3…; every other kind uses "the index of the change inside its batch".
14. **No rule binds `at` or `seq` of a non-Local change** (`rules.rs` checks V1–V8 only). Each
    change's `at` is free; the decision projection reads it (fact 3).
15. **tc-app module names.** `crate::seed` is A9's first-launch resource seeding and
    `tests/app/seed.rs` its tests. The new module is `crate::backend::seed`; its integration tests
    go in a new `tests/app/state_seed.rs`. Do not rename A9's.
16. **APP-DESIGN §4.3: "Nothing in tc-app calls `FoldState::record_keys`".** The check must not
    either: it walks the records the seed made and the files the source has.

---

## 3. Decisions (record each in the B1a row's annotation, §12)

- **D1. Shape.** `backend::seed` is pure apart from one reader: `Tc4Folder::read(dir)` reads a
  folder into memory; everything else takes `&Tc4Folder`. `tc4_state_seed(&Tc4Folder, ProjectId)`
  builds, seals, folds and checks, and returns the seed only when the check passes; otherwise
  `SeedRefusal::Mismatches` with every mismatch. Nothing partial is returned. The module is
  compiled only with the new tc-app feature `fixture-backend` (AD6, AD35: never in release
  builds).
- **D2. Who wrote it.** Device `Importer("tc4-seed")` = `DeviceId::derived(DeviceKind::Importer,
  b"import\0" ‖ b"tc4-seed" ‖ project.as_bytes())`. Cause `Cause::Import(ImportSource::Burrito {
  file_name: <the folder's own name> })` (STORAGE names no seed variant of `ImportSource`; spec
  question S-2). `parents: []` (every record is new). `v: FORMAT_MINOR`. `seq`: the change's index
  in its batch, from 0. `at`: the decision's own `modifiedTimestamp` for a decision (fact 3, and
  tC4 R-8.8.2: "`modifiedTimestamp` maps into the ts"), else the project's `meta.dateCreated`, else
  tC4's seed floor `2020-01-01T00:00:00.000Z` = 1,577,836,800,000 ms (`reconcile.mjs:136`)
  (spec question S-3).
- **D3. Batches.** One batch `Project` (ProjectMeta, Versification, Pins, ResourcesExtra, Setting,
  CarriedFile records), then one `Book(book)` per Bible book in canonical order (`BookId`'s
  order; the batch holds presence, header, chapter fronts, verses, layout, its alignments, its
  decisions, which is `RecordKey`'s derived order, `keys.rs:13-44`), or one
  `Story(n)` per OBS story in number order (its `StoryBase`, and its frame decisions if
  `checking/<tool>/OBS.json` has any). Within a batch, records are sorted by `RecordKey` (derived
  `Ord`), so the seed does not depend on the order of records in a source file (tC4 R-8.8.2:
  record order is byte form) and is deterministic. `BatchId` = sha256(`"tc4-seed-batch\0"` ‖
  projectId ‖ label)[..16], label `project`, the book code, or `story-NN`.
- **D4. Real format.** Seal all changes with `tc_journal::pack::seal(&PackSpec { project, sealer,
  sealed }, &lines)`, sealer `DeviceId::local(sha256("tc4-seed-sealer\0" ‖ projectId)[..15])`,
  `sealed` = the project `at` of D2; refuse (`SeedRefusal::Internal`) if `set_aside` is not empty.
  Read each pack back with `read_pack(&pack.path.to_string(), &pack.bytes, project)`, require
  `is_valid()` and every change back, and fold `FoldState::fold(kind, entries.map(|v|
  FoldInput::new(v, Some(read.id))))`. AD36: changes only through tc-journal's encoders.
- **D5. Records (STORAGE §13.2, with the mappings this plan fixes):**
  - `ProjectMeta` per non-derived field (fact 10, §5 step 3).
  - `Versification { value: VersificationValue { name: None, bytes } }` from `ingredients/vrs.json`
    read as exact UTF-8 (`String::from_utf8`, never lossy). tC4 records the name `unrecorded` for a
    seed (`journalingStore.ts:1693`) and readers fingerprint the bytes
    (`tc_versification::resolve_scheme`); `None` is that here (spec question S-4).
  - Books through the codec: `tc_usfm::codec::decompose` → `records_from_usfm(book, &parts)`
    (`sidecar.rs:342`) plus `BookPresence::Present { scope, how: PresentHow::Added }`; `scope`
    from `metadata.json` `type.flavorType.currentScope[BOOK]` (`[]` → `Scope::Whole`, ranges →
    `Scope::Ranges` through `tc_burrito::ScopeRange`), a book file the scope does not list →
    `Scope::Whole`, reported in `SeedReport::notes`; a scope entry with no file refuses.
  - Pins: `Record::Pins` per group through `PinGroupValue::from_json` (`record.rs:415`): `primary`,
    `fallback` (under `languageSets`), `originalLanguage`, `lexicon` (under `resources`),
    `ExtraScripture` as `{"extraScripture": [...]}` (as `screens_and_fold.rs:440-469` does);
    every other member as `Record::ResourcesExtra { key: ResourcesKey::new(level, name)?, value }`
    (`keys.rs:312-360`). `schemaVersion` 2 is taken as it is; `schemaVersion` 1 is migrated first
    (D8); any other or a missing one refuses. A pin without `sha` is kept as tC4 kept it (the
    pins reader shows it unusable): STORAGE §13's "an import never stores a pin without a sha"
    is about pins an importer resolves (spec question S-10).
  - Settings: one `Record::Setting { key: SettingKey::parse(name)?, value: Some(json) }` per
    top-level key except `schemaVersion`, which must be 1.
  - Decisions: each tC4 record rewritten as fact 2 says (D6), decoded with `decode_decisions(tool,
    book, bytes)`, then its `judgement.verse_text_md5` set from the fold of the book's text (D6).
  - Alignments: each record with `invalid: true` gets `markedInvalid: true` (unless it already has
    the field), then `decode_alignments(book, bytes)`; `source` stays `None` (a tC4 Phase-1 record
    names only `sourceVersion`; spec question S-5).
  - OBS stories: `Record::StoryBase { story, value: <the file as UTF-8> }` and **no** `Frame` or
    `StoryRef` record (fact 8). The story number comes from the file name:
    `tc_obs::ids::story_number_of` takes the path *below* `ingredients/` (`content/01.md`) and,
    like tC4, also answers 0 and 51–99, which `tc_journal::StoryNo::new` refuses →
    `UnexpectedFile`. `decode_story` must succeed on the file (a story tC4 could not read refuses)
    and give the same number. `tc_obs::ids::is_obs_base_ingredient` names the files tC4 never
    derives (`LICENSE.md`, `content/front/*`, `content/back/*`).
  - Carried files: every other file under `ingredients/` that `RepoPath::parse` accepts →
    `Record::CarriedFile { path, value: CarriedValue::Present { oid, size, md5 } }`, `oid` the git
    blob id = SHA-1 of `"blob <len>\0"` ‖ bytes (`ring::digest::SHA1_FOR_LEGACY_USE_ONLY`; tc-app
    already depends on `ring`), the bytes kept in `StateSeed::carried` by oid.
- **D6. Decision rewriting** (copy of `screens_and_fold.rs:471-504`, rule by rule):
  `importedInvalid` = tC4 `invalidated`; `userStatus` = tC4 `status` when not invalidated; when
  invalidated: `"todo"` if `status` is `"todo"`, `"invalid"` if `userInvalid` is true, else none;
  `verseTextMd5` = a placeholder before decoding, then replaced by
  `interim.check_texts(Collection::Book(book)).text(&key.target).map(|t| t.md5())`, or
  `verse_text_md5_v1("")` when the target names no text (stub `___`, `front`, a verse the book
  lacks); a frame decision uses `check_texts(Collection::Stories)` (`tc_journal::pins::Collection`).
  `interim` is `FoldState::fold(kind, <the book or story batches' text changes, unsealed>)`: the same
  text functions the projection uses (`project.rs:989`, `:223`, `decision.rs:37-70`), so the
  stored hash is exactly what the projection compares. Never compute it from your own USFM
  parse. Every other tC4 field (`contextId`, `category`, `selections`, `comments`, `reminders`,
  `nothingToSelect`, unknown fields) passes through untouched; `verseEdits`, `invalidated`,
  `status`, `userInvalid` and `modifiedTimestamp` are computed fields the decoder ignores.
- **D7. The check reads as tC4 reads** (full rules in §6): absent flags read as false; a decision's
  absent `status` reads as tC4 derives it (`invalidated` ⇒ invalid; done ⇒ valid; else todo,
  BURRITO-SPEC:288); an alignment's `invalid` reads with I-3; `modifiedTimestamp` compares as an
  instant; record order is not compared; the journal's own extra fields are compared only when the
  source has them. Differences the format makes by design and that no tC4 screen shows are
  **declared** (listed in the report, never silent): `verseEdits` recomputed, a decision file's
  `summary` dropped, an empty `resources`/`languageSets` member added to `resources.json`, a
  `schemaVersion: 1` `resources.json` migrated, a timestamp rewritten in tC4's own form. Anything
  else is a mismatch.
- **D8. The v1 → v2 pins migration** (BURRITO-SPEC:353, the only definition): from
  `{schemaVersion: 1, gatewayLanguage, resources: {translationNotes, translationWords,
  translationAcademy, [translationWordsLinks], [translationQuestions], [simplifiedText],
  [originalLanguage], [lexicon], ...}, [extraScripture], ...}` make the set F =
  `{gatewayLanguage, translationNotes, translationWordsLinks, translationWords,
  translationAcademy}` plus the optional set slots present; `translationWordsLinks`, when the v1
  file has none, is `tc_resources::suite::english_twl().journal_pin().to_json()` (the fallback
  pins the installed English suite, and D97 requires a `_twl` repository); one the file already has
  is kept as it is (never silently rewritten; the pins reader shows it unusable if it is the
  retired one-repo form). Then `languageSets: {primary: F, fallback: F}`, `resources:
  {originalLanguage, lexicon, <other resources members>}`, `extraScripture` and every other
  top-level member unchanged, `schemaVersion: 2`. `gatewayLanguage` is read at the top level or
  under `resources`; both present and different, or neither, refuses (no v1 file exists to tell us
  more, fact 11). The check compares the projection with the migrated document and declares
  `ResourcesMigrated { from: 1 }`. Declared difference from tC4 4.0, which refuses such a project.
- **D9. Strict JSON.** Every JSON file is read with a reader that refuses a duplicate object key at
  any depth (`SeedRefusal::DuplicateKey { path, at, key }`); `serde_json::from_slice` silently keeps
  the last one, which would drop a record before any comparison could see it.
- **D10. Project id.** The caller gives the `ProjectId` (STORAGE gives §13.2 no rule; spec question
  S-1). `fixture_project_id(folder_name)` = UUID version 8 of sha256(`"tc4-state-seed\0"` ‖
  name)[..16] (set byte 6 to `(b & 0x0f) | 0x80`, byte 8 to `(b & 0x3f) | 0x80`) is the rule the
  fixture backend uses, documented as such and not as an import rule.
- **D11. Scenario bytes in cargo tests.** No cargo test runs node. The Rust test assembles each
  scenario project from `fixtures/tc4/sample-burrito{,-obs}` plus a committed overlay
  `fixtures/parity-state/<scenario>/<project>/...` (the 4 `en-pins` files and the 2 `shared`
  `.remotes.json`), and asserts every file's sha256, both ways, against
  `parity/app/state.sha256.json` under `repos/_local_/_local_/<project>/`. So the test runs on
  exactly the bytes `build-state.mjs` makes from the tC4 checkout.
- **D12. Known mismatches are tests.** A mismatch that belongs to the storage team is asserted
  exactly (`obs_project_outcome_is_exactly_the_written_up_mismatches`), so the test fails the day
  storage fixes it and the next builder updates the list and the write-up. Never relax a rule to
  make a scenario pass.

---

## 4. Public API (`crates/tc-app/src/backend/seed.rs`)

```rust
//! module doc: STORAGE §13.2's state seed (AD7), what it reads, the check (§6 of the plan, in
//! words), the declared differences, and that unit I's importer replaces it.

pub struct Tc4Folder { /* name: String, metadata: Vec<u8>,
                          files: BTreeMap<String, Vec<u8>> (repo paths under ingredients/),
                          ignored: Vec<Ignored> */ }
impl Tc4Folder {
    /// Reads `dir` (never writes). Skips `.git/` whole. Each file is classified as §5 step 1 says.
    pub fn read(dir: &Path) -> Result<Self, SeedRefusal>;
    /// The same classification over files given in memory (`/`-separated paths relative to the
    /// folder), for tests and for B1b.
    pub fn from_files(name: &str, files: impl IntoIterator<Item = (String, Vec<u8>)>)
        -> Result<Self, SeedRefusal>;
    pub fn name(&self) -> &str;
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>>;
    pub fn ignored(&self) -> &[Ignored];
}

pub fn tc4_state_seed(folder: &Tc4Folder, project: ProjectId) -> Result<StateSeed, SeedRefusal>;
pub fn fixture_project_id(folder_name: &str) -> ProjectId;

pub struct StateSeed {
    pub project: ProjectId,                 // tc_journal::ProjectId, not tc_api::ids::ProjectId
    pub kind: ProjectKind,                  // tc_journal::fold::ProjectKind
    pub device: DeviceId,
    pub batches: Vec<SeedBatch>,            // D3 order
    pub fold: FoldState,                    // the fold of the sealed, re-read changes
    pub carried: BTreeMap<GitOid, Vec<u8>>, // carried files' bytes by blob oid
    pub report: SeedReport,
}
pub struct SeedBatch { pub label: BatchLabel, pub changes: Vec<Change> }
pub enum BatchLabel { Project, Book(BookId), Story(StoryNo) }

pub struct SeedReport {
    pub declared: Vec<DeclaredDifference>,
    pub ignored: Vec<Ignored>,
    pub added_files: Vec<String>,           // derived files the source lacked (no loss)
    pub notes: Vec<String>,                 // e.g. a book file the scope did not list
    pub counts: SeedCounts,                 // batches, changes, records by kind name
}
pub struct Ignored { pub path: String, pub why: IgnoredWhy }
pub enum IgnoredWhy { GitHistory, AppOwned /* .gitignore, .gitattributes */,
                      MockServerRemotes /* .remotes.json */, Tc4Backup /* *.bak, §13 */,
                      ForeignRootEntry /* other root entries, STORAGE §3.1 FOREIGN */ }

pub enum DeclaredDifference {
    VerseEditsRecomputed { path: String, check: String, source: bool, projected: bool },
    SummaryDropped { path: String },
    EmptyMemberAdded { path: String, member: String },
    ResourcesMigrated { from: u64 },
    TimestampRewritten { path: String, check: String, source: String, projected: String },
}

pub struct Mismatch {
    pub path: String,              // repo path of the file
    pub record: Option<String>,    // "TIT 1:1", "translationNotes gr8c TIT 1:4 #1", "story 3 frame 0"
    pub field: Option<String>,     // "invalid", "resource", "identification.name", ...
    pub source: String,            // the source bytes or compact JSON (with line number for text)
    pub projected: String,         // what the fold gives (or "absent")
    pub rule: CheckRule,
}
pub enum CheckRule { UsfmBytes, VrsBytes, StoryBytes, FrameText, CarriedFile, FileNotDerived,
                     RecordMissing, RecordAdded, RecordField, DocumentField, RoundTrip,
                     MetadataField, FoldAnomaly, NotProvisional }

#[derive(Debug, thiserror::Error)]
pub enum SeedRefusal {   // each message in plain words; the path always named
    Io { path: String, problem: String },
    NoMetadata,
    HasJournal,          // "...the translationCore 4 importer (STORAGE §13.1) imports it"
    NewFormat,           // ingredients/tc/ present: an exchange file or a rebuild project
    Flavor { found: String },
    Unreadable { path: String, problem: String },     // UTF-8, JSON, USFM (with its line), story
    Schema { path: String, found: String },
    DuplicateKey { path: String, at: String, key: String },
    DuplicateRecord { path: String, key: String },
    UnexpectedFile { path: String, why: String },     // lower-case book file, unknown sidecar...
    ScopeWithoutFile { book: String },
    BadValue { path: String, problem: String },       // identity grammar, Change::new refusal
    Mismatches(Vec<Mismatch>),
    Internal(String),                                 // sealing or reading back failed: a bug
}
```

`Mismatch` and `DeclaredDifference` get `Display` (one line each) for the write-up and the
`SeedRefusal::Mismatches` message. Keep every `Err` variant small (clippy `result_large_err`):
the `Vec` is 24 bytes, fine.

`tc4_state_seed` is two private steps: `build(&Tc4Folder, ProjectId) -> Result<Built,
SeedRefusal>` (records, batches, seal, read back, fold; `Built` holds what `StateSeed` holds plus
the expected documents: the source or migrated `resources.json`, and the declared differences
found while building) and `check(&Tc4Folder, &Built) -> Checked` (mismatches, declared
differences, added files). Unit tests call `build` directly to look inside a project the public
seed refuses.

Private, pure (names are suggestions; keep each one job):
`classify(path) -> Class`, `read_json_strict(path, bytes) -> Result<Value, SeedRefusal>`,
`kind_of(&Value)`, `meta_records(&Value) -> Result<Vec<Record>, _>`, `scope_of`, `book_records`,
`pins_records(&Value) -> Result<(Vec<Record>, Value /* expected doc */, Option<DeclaredDifference>), _>`,
`migrate_pins_v1`, `settings_records`, `alignment_records`, `decision_records(.., &CheckTexts)`,
`story_record`, `carried_record`, `batch(label, Vec<(Record, WallTime)>, &Ctx) -> Result<SeedBatch, _>`,
`seal_and_fold`, `parse_utc_millis(&str) -> Option<i64>` (tC4's `toISOString` form, and the
same without milliseconds; round-trips with tc-journal's `rfc3339`, which is `pub(crate)` there,
so write the formatter too, ~20 lines, days-from-civil), `check(&Tc4Folder, &Built) -> Checked`.
tc-burrito's `get_path`/`set_path` are private: write two small dotted-path helpers.

If `seed.rs` passes ~1,500 lines, move the check into `backend/seed/check.rs` (a child module of
`seed.rs`) and say so in the commit. Tests go in `tests/app/state_seed.rs` and a `#[cfg(test)] mod
tests` at the end of `seed.rs`.

---

## 5. The seed, step by step

1. **Classify the folder** (`Tc4Folder::read`/`from_files`):
   - `.git/` → one `Ignored(GitHistory)`; root `.gitignore`, `.gitattributes` → `AppOwned`;
     root `.remotes.json` → `MockServerRemotes`; any `*.bak` → `Tc4Backup` (§13: copied to
     `.tc/external/tc4-bak/` by the importer, never imported); any other root entry except
     `metadata.json` and `ingredients/` → `ForeignRootEntry`. A link or other non-file → `Io`.
   - No `metadata.json` → `NoMetadata`. Anything under `ingredients/checking/journal/` →
     `HasJournal`. Anything under `ingredients/tc/` → `NewFormat`.
2. **Kind.** `tc_obs::ids::is_obs_metadata(&meta)` → `ProjectKind::Stories`; `type.flavorType` =
   `scripture`/`textTranslation` → `Bible`; else `Flavor`.
3. **Project records.** `meta_records` walks the parsed `metadata.json`: skip every
   `is_derived_field` path; break `meta`, `idAuthorities`, `identification` and
   `identification.primary` into their members; map the five modelled paths to their
   `MetaField`; every other path to `MetaField::Extra(MetaPath::parse(path)?)` (a `MetaPath`
   refusal → `BadValue`). Self-check: setting every record's value at its path into an empty
   object gives the source minus its derived fields, exactly; else `BadValue` (catches a key with
   a dot in it). Then `vrs.json` (if present), pins (+ D8), `ResourcesExtra`, settings, carried
   files. `at` = the project time (D2).
4. **Book batches (Bible).** For each `ingredients/<CODE>.usfm` whose name is exactly
   `BookId::code()` (any other `*.usfm` name → `UnexpectedFile`): text as exact UTF-8, decompose
   (a `DecomposeError` → `Unreadable` with its line), records, presence with scope. Each
   `currentScope` key with no file → `ScopeWithoutFile`. Fold these book batches into `interim`
   (no sealing needed for the interim).
5. **Sidecars.** `ingredients/checking/alignments/<CODE>.json` → that book's batch;
   `ingredients/checking/<tool>/<CODE>.json` and `.../<tool>/OBS.json` (tool one of
   `ToolId::ALL` names) → the book's batch, or the batch of the story each decision targets.
   `resources.json`, `settings.json` → project batch. Any other path under `ingredients/checking/`
   → `UnexpectedFile` (it cannot be carried, fact 9). Check each sidecar's `schemaVersion` is 1
   (`Schema` otherwise). Duplicate `RecordKey`s inside one file → `DuplicateRecord` (tC4 refuses
   "co-present records on one §5.2 identity key"; the fold would show one of them).
6. **OBS.** `ingredients/content/NN.md` → `Story(n)` batch with its `StoryBase`; any other
   `content/*.md` directly under `content/` → `UnexpectedFile`; `content/front/*`,
   `content/back/*`, `LICENSE.md` and every other non-derived file under `ingredients/` →
   carried. A Bible project with `content/*.md` → `UnexpectedFile`.
7. **Changes.** `Change::new(ChangeBody { v: FORMAT_MINOR, record, parents: vec![], device, seq,
   at, batch: BatchRef::new(batch_id, n)?, cause })` per record; a `ChangeError` → `BadValue` with
   the record's key and the rule's words.
8. **Seal, read back, fold** (D4).
9. **Check** (§6). Mismatches → `SeedRefusal::Mismatches`; else `Ok(StateSeed)` with the report.

Invariants the code keeps, each tested in §9:
- nothing in the source folder is written; the reader opens files read-only;
- every source file is either a record, a carried file, `metadata.json` (fields compared) or
  listed in `ignored` with a reason: never silently skipped;
- deterministic: the same folder and id give byte-identical changes;
- no `unwrap`/`expect`/`panic` outside tests; no `HashMap` iteration feeds an output;
- no `FoldState::record_keys` (APP-DESIGN §4.3).

---

## 6. The check, rule by rule

Run in this order; collect every mismatch (do not stop at the first).

1. **Fold invariants.** `fold.conflicts().count() == 0`, `fold.anomalies().count() == 0` (each
   anomaly → `FoldAnomaly` in words; an `InvalidForBurrito` anomaly names the metadata field). For
   every seeded change: `fold.projected(&record.key()) == Some(record)` and
   `fold.key_state(&key).heads == [change.id()]`; else `NotProvisional`.
2. **File set.** Each source file that the format derives maps to one `DerivedFile`:
   `<CODE>.usfm` → `Usfm(book)`, `vrs.json` → `Versification`, `checking/alignments/<CODE>.json` →
   `Alignments(book)`, `checking/<tool>/<CODE>.json` → `Decisions(tool, book)`,
   `checking/<tool>/OBS.json` → `StoryDecisions(tool)`, `checking/resources.json` → `Resources`,
   `checking/settings.json` → `Settings`, `content/NN.md` → `Story(n)`. That file must be in
   `fold.derived_files()`; else `FileNotDerived` (the source bytes, and "not derived" as
   projected), even when `render()` of it works. Every derived file the source lacks →
   `report.added_files`.
3. **Bytes.** Each `ingredients/<CODE>.usfm` == `fold.render(&DerivedFile::Usfm(book))`
   (`UsfmBytes`, mismatch text = the first differing line with its number, both sides); `vrs.json`
   == `render(Versification)` (`VrsBytes`); each `content/NN.md` == `render(Story(n))`
   (`StoryBytes`). Carried: for each carried source file, a placement in `fold.carried_files()` at
   the same path with equal `md5`, `size` and `oid`, and `StateSeed::carried[oid]` == the bytes
   (`CarriedFile`).
4. **Frame text (OBS).** For each story and each part `decode_story` records (title as frame 0,
   frames 1..), `fold.check_texts(Collection::Stories).text(&CheckTarget::Frame { story, frame })`
   content == `decode_story`'s text (`FrameText`). This is the "field for field" of a story: what
   the screens read must be what tC4 reads. (Expected to fail for frame 0, fact 8.)
5. **Alignments** (per `checking/alignments/<CODE>.json`): parse both sides strictly. Document:
   `schemaVersion`, `book` equal, and no top-level member other than `schemaVersion`, `book` and
   `chapters` on either side (`DocumentField`). Records keyed by (chapter, label): missing → `RecordMissing`, extra →
   `RecordAdded`. Per record, the tC4 reading: `alignments` and `wordBank` equal as ordered arrays
   (word order is the translator's work), `targetVerseMd5`, `sourceVersion` equal; `done`
   (absent = false) equal; `source` equal (absent = absent); `invalid`: source reads
   `invalid || targetVerseMd5 != md5_now` (I-3), where `md5_now` is
   `fold.check_texts(Book(book)).text(&target).map(|t| t.md5())`; when that is `None`, compare the
   two flags as written; every other source field equal; the journal-only fields `markedInvalid`,
   `dismissed`, `anchor`, `spanEnd` compared only when the source has them (`RecordField`, naming
   the field). Round trip: `decode_alignments(book, projected)` == the seeded alignment records
   (`RoundTrip`).
6. **Decisions** (per decision file): document `schemaVersion`, `tool`, `book` equal; `resource`
   equal as objects (absent = absent) (`DocumentField`); source `summary` → declared
   `SummaryDropped`; any other member on either side → `DocumentField`. Records keyed by
   canonical `contextId` (missing/extra as above). Per record, the tC4 reading: `contextId`,
   `category`, `selections` (`false`, absent and `[]` all read `false`), `comments` (absent =
   false), `reminders`, `nothingToSelect`, `invalidated`, `userInvalid` (absent = false), the
   effective `status` (BURRITO-SPEC:288), every other source field; `modifiedTimestamp` as an
   instant when the source has one (same instant, other text → declared `TimestampRewritten`);
   the journal-only fields only when the source has them. `verseEdits` difference → declared
   `VerseEditsRecomputed`. Round trip: `decode_decisions(tool, book, projected)` == the seeded
   decision records.
7. **Pins.** `render(Resources)` parsed == the expected document (the source, or D8's migrated
   one), after removing from the projection an empty `resources` or `languageSets` member the
   expected document lacks (→ declared `EmptyMemberAdded`). Object key order is not compared:
   `serde_json::Map` with the workspace's `preserve_order` is an `IndexMap`, whose `==` ignores
   order; arrays stay ordered. Unequal → `DocumentField` naming the first differing path.
8. **Settings.** `render(Settings)` parsed == the source (schemaVersion included).
9. **Metadata.** `fold.metadata_source((FORMAT_MAJOR.into(), FORMAT_MINOR.into())).fields`: for
   every record path the seed made, the field is present and equal to the source value
   (`MetadataField`; a field missing there was refused by `check_field`, step 1 names why).
   Derived fields are not compared (STORAGE §3.4 rebuilds them; tC4's server owns them too, G3);
   the write-up lists that rule once.

---

## 7. Predicted outcome (verify it; do not assume it)

| Project | Changes | Outcome predicted |
|---|---|---|
| `sample_burrito` (all three scenarios) | 127 = project 17 (9 meta, vrs, 5 pin groups, 2 settings) + JON 54 (presence, header, 4 fronts, 48 verses incl. `9` span text and `10` covered) + TIT 56 (presence, header, 3 fronts, 46 verses, 1 alignment, 4 decisions) | passes; declared: `VerseEditsRecomputed` (tN gr8c TIT 1:4, true → false), `SummaryDropped` ×2 (tN and tW TIT); ignored `.gitignore` (and `.remotes.json` in `shared`) |
| `sample_burrito_obs` (all three) | 66 = project 16 (9 meta, 2 pin groups, 1 setting, 4 carried) + 50 stories × 1 `StoryBase` | refused: `FileNotDerived` `ingredients/checking/settings.json` (fact 6); `FrameText` frame 0 of all 50 stories (story 1: tC4 `La Creación`, fold `1. La Creación`; stories 2–50: tC4 empty, fold `N.`); declared `EmptyMemberAdded` resources.json `resources` |

If a count differs, find out why before you assert it. If the story bytes or frames 1.. mismatch,
or anything not listed appears, that is a finding: write it up (§11), do not bend a rule.
`en-pins` changes only pins and decision `resource`s: its decision files resolve to `primary`
English pins (pins without `books` cover every book in the fold, `Unrecorded::CoversEveryBook`, `pins.rs:389`), so the
same declared list is expected. `shared` must give exactly `sample`'s change ids.

Consequence to state in your final answer: B1b's `MemoryBackend` cannot load the OBS project of
any scenario until the storage team fixes facts 6 and 8, since a mismatch fails the scenario load
(§4.2). That is the row's purpose: the mismatch reaches storage now.

Two more storage findings, outside the seed check, to write up with a small demonstration each
(§11 part 5; a test asserting today's behaviour is welcome but optional):
- a title edit (`SaveFrame` frame 0, text `La Creación`, as tC4's title box sends it) projects
  `# La Creación`: the story number is lost and tC4's `TITLE_RE` can no longer read the file;
- an edit of an empty frame (`SaveFrame` frame 3 of story 1) projects
  `image\n\n\n\ntext\nimage`, where tC4's `writeFrame` (and `tc_obs::story::write_frame`) writes
  `image\n\ntext\n\nimage`.

---

## 8. Steps in order (each ends green and is committed)

**Step 0 — start.** `df -h /home` (under 30 GB free and no rustc in this worktree →
`python3 -I /home/bot/tc-rst/.audit/tools/prune-target.py /home/bot/tc-rst/.claude/worktrees/app-b1a/target`).
Every cargo/node command starts with `. ~/.tc-env && ` and uses `-j 6`.

**Step 1 — crate wiring** (commit with this plan file).
- `crates/tc-app/Cargo.toml`: `[features] fixture-backend = []` with a comment (the fixture
  backend and its seed, APP-DESIGN AD6/AD7/AD35; never in release builds); `[dependencies]` add
  `tc-journal`, `tc-usfm`, `tc-obs`, `tc-burrito` (path deps; B1b needs tc-journal anyway);
  `[dev-dependencies]` the self line becomes `features = ["test-hooks", "fixture-backend"]`.
- `crates/tc-app/src/lib.rs`: `pub mod backend;` (and a line in the crate doc).
- `crates/tc-app/src/backend/mod.rs`: a module doc (the storage seam of APP-DESIGN §4.2 comes
  with B1b; B1a holds only the seed) and `#[cfg(feature = "fixture-backend")] pub mod seed;`.
- `crates/tc-app/src/backend/seed.rs`: module doc and the types of §4.
- Check: `cargo tree -p tc4 -e features -i tc-app` shows no `fixture-backend` in a normal build.

**Step 2 — reader, strict JSON, kind, refusals.** `Tc4Folder`, `read_json_strict` (a
`serde::Deserialize` visitor that builds `serde_json::Value` and errors on a repeated key in any
map, ~60 lines), `kind_of`, step 1–2 refusals. Unit tests: `strict_json_refuses_duplicate_keys_at_any_depth`,
`strict_json_reads_what_serde_reads` (proptest). Integration: `reading_a_project_folder_equals_building_it_from_its_files`,
`a_project_with_a_journal_is_refused_for_the_tc4_importer`, `a_folder_of_the_new_format_is_refused`,
`backups_dot_files_and_git_history_are_listed_not_imported` (the last needs step 4; add it then).

**Step 3 — project and book records, pins with the migration, settings, carried files,
stories.** Unit tests: `meta_split_covers_every_non_derived_field_once`,
`utc_millis_round_trips_tc4_timestamps`, `resources_v1_migration_matches_burrito_spec_5_3`
(literal expected JSON in the test).

**Step 4 — decisions, alignments, batches, seal, fold.** The interim fold, D6, D3, D4. Tests:
`seed_is_deterministic`, `seed_does_not_depend_on_record_order`,
`seed_changes_are_the_importer_s_one_batch_per_book_plus_the_project`,
`every_seeded_record_is_the_value_the_fold_shows`,
`decision_hash_is_the_current_verse_and_tc4_flags_carry_over`,
`tc4_invalid_alignment_is_marked_invalid`, `obs_stories_are_kept_as_the_files_are` (unit test,
on the private builder, since the public seed refuses the OBS sample).

**Step 5 — the check.** §6 in full. Unit tests: `decision_reading_applies_tc4_defaults`,
`alignment_reading_applies_i3`. Integration: the refusal and mutation tests of §9.

**Step 6 — scenarios, write-up, design text.** `fixtures/parity-state/` overlay and README (and
a `## parity-state/` section in `fixtures/README.md`); produce the overlay with
`. ~/.tc-env && node parity/app/state.mjs --scenario en-pins --out target/b1a/state-en-pins`
(and `shared`), copy the six files, never by hand. Scenario tests of §9. The evidence file (§11).
APP-DESIGN changes (§12) in their own commit whose message says why each changed.

**Step 7 — measurements and gates.** The bench example (§10), the evidence numbers, the gates
(§14), the last commit.

---

## 9. Tests

`tests/app/state_seed.rs` (add `mod state_seed;` to the `mod app { … }` list in `tests/app.rs`). Helpers there:
`sample_files(project) -> Vec<(String, Vec<u8>)>` from `fixtures/tc4/...`, `scenario_files(scenario,
project)` = sample files + `fixtures/parity-state/<scenario>/<project>/` overlay, `with(files,
path, edit)` to change one file, `seed(files) -> Result<StateSeed, SeedRefusal>` with
`fixture_project_id`. sha256 with `ring::digest::SHA256` (no new dependency).

The row's "done when":
- `scenario_projects_are_the_recorded_state_bytes` — for each of `sample`, `en-pins`, `shared`
  and each project, the assembled files and their sha256 equal `state.sha256.json`'s entries
  under `repos/_local_/_local_/<project>/`, no file missing or extra.
- `bible_project_reproduces_every_file_in_every_scenario` — each scenario: `Ok`; TIT and JON
  bytes equal `fold.book_usfm`; `vrs.json` equals `render(Versification)`; resources and settings
  equal; `report.declared` equals exactly the expected list of §7; `report.ignored` exactly
  `.gitignore` (plus `.remotes.json` in `shared`); batch labels `[Project, Book(JON), Book(TIT)]`
  (canonical order: JON before TIT) with the counts you verified.
- `obs_project_outcome_is_exactly_the_written_up_mismatches` — each scenario: the exact
  `SeedRefusal::Mismatches` list you found and wrote up (predicted §7), compared as
  `(path, record, field, rule)` tuples. Rename to `obs_project_reproduces_every_file_in_every_scenario`
  and assert `Ok` if the check passes.
- `shared_scenario_seeds_exactly_like_sample` — the Bible project's change ids and `fold.frontier()`
  equal `sample`'s; `.remotes.json` is in `ignored` as `MockServerRemotes`.
- `en_pins_decision_files_resolve_to_the_english_pins` — the projected tN and tW `TIT.json`
  `resource` equal the source's (en_tn `e586762e…`, en_twl `7cbb3419…`, `languageSet: primary`),
  and no decision is invalidated except gr8c.

The seed:
- `seed_is_deterministic` — two seeds of the same folder: equal change ids in equal order, equal
  frontier.
- `seed_does_not_depend_on_record_order` — reverse the `decisions` arrays and the verse order in
  the alignments file: equal change ids.
- `seed_changes_are_the_importer_s_one_batch_per_book_plus_the_project` — every change's device is
  the D2 device, cause `Import(Burrito { file_name: "sample_burrito" })`, no parents, `seq` its
  index in its batch, `BatchRef.n` the batch's length, batch ids per D3.
- `every_seeded_record_is_the_value_the_fold_shows` — `projected(key) == record`, one head each.
- `decision_hash_is_the_current_verse_and_tc4_flags_carry_over` — gr8c: `verse_text_md5` ==
  `verse_text_md5_v1` of TIT 1:4's content, `imported_invalid`; t1g7 `status` Valid; a9p2
  `bookmarked`; swi9's comment text exact; each decision change's `at` == its
  `modifiedTimestamp`.
- `tc4_invalid_alignment_is_marked_invalid` — TIT 1:1 with `invalid: true`: `Ok`,
  `marked_invalid`, projected `invalid: true`. Revert D5's mapping → this fails.
- `an_unknown_field_in_a_record_is_kept` — `"x-note"` added to a decision and to the alignment:
  `Ok`, both projected files carry it, the records' `extra` hold it.
- `backups_dot_files_and_git_history_are_listed_not_imported` — `ingredients/TIT.usfm.bak`,
  `.remotes.json`, `.git/HEAD`, `README.md`: `Ok`, each in `ignored` with its reason, no record.
- `metadata_fields_survive_field_for_field` — add `localizedNames.TIT`, `agencies`,
  `identification.upstream` (valid values): `Ok`, `metadata_source().fields` hold them equal.
- `obs_stories_are_kept_as_the_files_are` (unit) — the OBS sample's built fold: 50 `StoryBase`
  records each equal to its file, no `Frame`/`StoryRef` record, `render(Story(n))` == file; the 4
  carried files' md5/size/oid and bytes.

Refusals (each asserts the variant and the named path; nothing is returned):
- `an_unknown_top_level_field_refuses` — `"foo": 1` in tN `TIT.json` → `Mismatches` with one
  `DocumentField` naming `foo`.
- `a_duplicate_json_key_refuses` — a repeated `"1"` verse key in the alignments file →
  `DuplicateKey { at: "chapters.1", key: "1" }`. Remove D9 → this fails.
- `two_decisions_on_one_check_refuse` — a second gr8c record with another `quoteString` →
  `DuplicateRecord`.
- `a_decision_against_another_pin_refuses_as_a_mismatch` — change only tN `TIT.json`'s
  `resource.sha` to another commit. Every decision then records a resource the pins do not resolve
  to, so the fold invalidates swi9, which tC4 showed as valid: `Mismatches` holds
  `DocumentField resource` (the projected resolution record is the pin's, not the file's),
  `RecordField invalidated` and `RecordField status` for swi9, and nothing for gr8c (already
  invalidated, same reading). Assert exactly what you find; the point is that a check the
  translator saw as valid never comes back invalidated without a refusal.
- `resources_schema_1_migrates_to_two_language_sets` — the sample without its decision files and
  with a v1 `resources.json` built from its fallback set: `Ok`; projected `resources.json` ==
  D8's document; `declared` holds `ResourcesMigrated { from: 1 }`; `primary == fallback`;
  `translationWordsLinks` is en_twl v91 `7cbb3419…`.
- `resources_schema_1_keeps_a_links_pin_it_has` — same with a `translationWordsLinks` in the v1
  file: kept unchanged.
- `an_unknown_resources_schema_refuses` — `schemaVersion` 3, and absent → `Schema`.
- `a_project_with_a_journal_is_refused_for_the_tc4_importer` — `HasJournal`.
- `a_folder_of_the_new_format_is_refused` — `ingredients/tc/project.json` → `NewFormat`.
- `unreadable_usfm_refuses_with_its_line` — `\v 3` twice in TIT → `Unreadable` naming
  `ingredients/TIT.usfm` and the line.
- `a_book_in_scope_without_its_file_refuses` — `"RUT": []` in `currentScope` →
  `ScopeWithoutFile`.
- `a_book_file_named_in_lower_case_refuses` — `ingredients/tit.usfm` → `UnexpectedFile`.
- `an_unknown_sidecar_refuses` — `ingredients/checking/notes/TIT.json` → `UnexpectedFile`.
- `an_invalid_metadata_value_refuses_as_a_mismatch` — `identification.name.en` with a trailing
  space → `Mismatches` with `MetadataField identification.name` (and the `FoldAnomaly` words).

Property:
- `random_texts_and_decisions_round_trip` (proptest, 64 cases) — random verse texts for TIT 1:1–5
  (non-empty; letters, accented letters, inner spaces, punctuation; no `\`, no line break, no
  leading or trailing space) and random decision
  `comments`/`reminders`/`nothingToSelect`/`status` (`valid`, `todo`, none) on the
  non-invalidated decisions: `Ok`; USFM bytes equal; each decision's tC4 reading equal.

Unit (`seed.rs`): `strict_json_refuses_duplicate_keys_at_any_depth`,
`strict_json_reads_what_serde_reads` (proptest over `serde_json::Value` with unique keys),
`utc_millis_round_trips_tc4_timestamps`, `meta_split_covers_every_non_derived_field_once`,
`decision_reading_applies_tc4_defaults` (pairs that must read equal: absent/false flags, absent
vs derived `status`, `[]` vs `false`; pairs that must not), `alignment_reading_applies_i3`,
`resources_v1_migration_matches_burrito_spec_5_3`, `obs_stories_are_kept_as_the_files_are`.

Verifiers will revert each core change and expect a test to fail: the `markedInvalid` mapping,
the md5 recomputation, the `userStatus` rule, the strict reader, the file-set rule, the
round-trip rule, the record-order sort. Make sure each has the test named above.

---

## 10. Measurements

`crates/tc-app/examples/state_seed_bench.rs` with `[[example]] name = "state_seed_bench"
required-features = ["fixture-backend"]` in `Cargo.toml`. Arguments `--state <build-state output
dir> [--runs 20]`; for each folder under `repos/_local_/_local_/`: one `Tc4Folder::read` (timed),
then `--runs` × `tc4_state_seed` (refused or not, timed to the result), and it prints JSON:
project, outcome, batches, changes, read ms, seed median and p95 ms. No `unwrap`/`expect` (clippy
runs on examples as non-test code): return `Result<(), Box<dyn std::error::Error>>`.

Run (release, 20 runs, PERFORMANCE rule 5):
```
. ~/.tc-env && node parity/app/state.mjs --scenario sample --out target/b1a/state-sample
. ~/.tc-env && cargo run --release -j 6 -p tc-app --features fixture-backend --example state_seed_bench -- --state target/b1a/state-sample --runs 20
```
Record in the evidence file: the numbers, the commit, the machine (`lscpu` model, and whether
the flags show `sha_ni` and `avx512f`), and the test count and time of `cargo test -p tc-app
--test app state_seed`. No budget applies (a fixture path); expect a few milliseconds per project;
explain anything above 100 ms, since B1b loads every scenario project at `tc4 serve --backend
memory` start.

---

## 11. The write-up (`docs/evidence/state-seed-check-<date of your run>.md`)

1. What ran: the commit, the scenarios and projects, the command.
2. A table: scenario × project → passed or refused, batches, changes.
3. The rules of the check in one paragraph each (the tC4 reading, what is declared, what is not
   compared and why: derived metadata fields, record order).
4. **Every mismatch**, grouped when one cause makes many (frame 0 of 50 stories is one entry
   listing the stories): the file and record; the bytes, source and projected, in fenced blocks;
   the rule involved (STORAGE section and line, the tc-journal code with file:line, tC4's code with
   file:line); who owns it (storage); the change that would fix it. For fact 6: STORAGE §3.2 lists
   no `settings.json` for OBS, `derived_files` adds it only for `Bible` (`project.rs:526-528`),
   tC4 always derives it (`journalingStore.ts:716-723`). For fact 8: STORAGE §7.7 ("the template's
   first `# ` line is frame 0") against tC4's `TITLE_RE` (`journal/story.mjs:15`) and tc-obs's
   `title_line_of` (`story.rs:54`); tc-journal `template_frame` `project.rs:2004`, `splice_story`
   `:2054`.
5. Findings outside the check (§7's two edit demonstrations), with their bytes.
6. Declared differences per project, and the declared differences from tC4 4.0 (§15).
7. Spec questions for the storage team: S-1 a §13.2 seed's `ProjectId`; S-2 its `ImportSource`
   variant; S-3 the `at` of non-decision seed changes; S-4 `VersificationValue.name` for a seed;
   S-5 `source` of a seeded alignment (§13.1 sets it from the pin whose label equals
   `sourceVersion`, §13.2 says nothing); S-6 whether `verseEdits: true` should survive (it cannot,
   with §13.2's "`verse_text_md5` from the current verse"); S-7 DeviceSeq of a non-Local change
   counts from 0 or 1; S-8 the empty `resources` member (fact 7); S-9 an OBS project's carried
   front/back/licence files at a seed (STORAGE §13 covers OBS nowhere); S-10 a pin without `sha`
   in a seeded `resources.json`.
8. Measurements (§10).

---

## 12. Design text to change (docs/APP-DESIGN.md), in one commit that says why

1. **§4.2 `MemoryBackend`, "Projects".** Replace "It encodes the batches with `encode_pending` and
   folds them with `FoldState::fold`" by: it seals the batches with `tc_journal::pack::seal` under
   a fixed Local sealer and reads the packs back with `read_pack`, then folds them (pending files
   are for Local devices only, `encode_pending` refuses the seed's `Importer` device). Replace
   "OBS story records … plus `tc_obs::decode_story`" by: each story's `StoryBase` is its file as it
   is, with no `Frame` or `StoryRef` record, as tC4 keeps a project with no journal; the check
   reads every frame back through the fold and compares it with `tc_obs::decode_story`'s reading;
   an OBS project's licence and front and back matter are `CarriedFile` records. Define "field for
   field" in one sentence (the tC4 reading of D7) and name the declared differences. Mark each
   *(B1a: …)*.
2. **§10 phase 1, the line before the table**: B1a was cut from the integration branch `app`
   after unit C merged.
3. **The B1a row**: "the two `.state` scenarios" → "the `.state` scenarios (`sample`, `en-pins`,
   `shared`)"; then the annotation *(B1a: built. D1 … D12 in one or two lines each; the outcome per
   scenario; the evidence file's path; the mismatches handed to the storage team; that B1b cannot
   load the OBS project until they are fixed.)*, in the style of the A9 row.
4. **AD7** (optional, one clause): the seed is checked as §4.2 says, and its mismatches are in the
   evidence file.

Do not edit STORAGE.md (the storage team's), even where §11's write-up shows it is ambiguous.

---

## 13. Traps

1. `encode_pending` refuses a non-Local device (fact 1).
2. `decode_alignments` ignores tC4's `invalid`; `decode_decisions` needs `verseTextMd5` and reads
   `importedInvalid`/`userStatus`, not tC4's fields (fact 2). Copy `seeded()`'s rule; do not
   reinvent it.
3. Compute each decision's hash with the fold's own `check_texts`, from an interim fold of the
   books; your own USFM parse would disagree on spans, stubs and poetry.
4. `modifiedTimestamp` comes from the newest head's `at`: set the decision change's `at` from it,
   or every decision mismatches.
5. An invalidated decision without `status` projects `status: "invalid"`; tC4 reads the same.
   Compare the effective status, not the bytes.
6. Alignment `invalid` is computed; compare with I-3, and `done: false` is not a difference.
7. `serde_json` keeps the last of two equal keys silently; read strictly (D9).
8. `serde_json::Map` equality ignores key order (intended); arrays are ordered (word order is the
   translator's work; never sort words).
9. `derived_files()` omits OBS `settings.json`; a comparison through `render()` alone would miss
   it. The file-set rule is what finds it.
10. Seeding `decode_story`'s records breaks every story (fact 8); seed the file as the base.
11. Two modules named `seed` in tc-app (A9's `crate::seed`, the new `crate::backend::seed`), and
    two `ProjectId`s (`tc_api::ids::ProjectId` on the wire, `tc_journal::ProjectId` here); two
    `StoryNo`/`FrameNo` (tc-obs's and tc-journal's).
12. `BookId::from_str` accepts any letter case; a book file's name must be the exact upper-case
    code, or the projection writes another path.
13. `ingredients/checking/**` and `ingredients/content/*.md` are derived patterns: a file there the
    seed does not understand cannot be carried; refuse it with words.
14. tC4's `.gitignore` here is `**/*.bak`, not the store's constant: ignore it (AppOwned).
15. `build-state.mjs`'s default tC4 path is the cloud machine's: set `TC4_ROOT` when you run it on
    the tC4 checkout.
16. No `FoldState::record_keys` (APP-DESIGN §4.3); walk the seeded records.
17. Clippy denies `unwrap`/`expect`/`panic` in the library and the example; tests are allowed
    (`clippy.toml`).
18. Keep `fixture-backend` out of normal builds; the dev-dependency self-line enables it for tests
    only.
19. `PinGroupValue::from_json` returns `None` for a non-integer number or a bad slot name: a
    `BadValue` refusal with the path, never an unwrap.
20. JON 2:9-10 is a span: `\v 9-10` must come back exactly; the verse records are `9` (text, last
    10) and `10` (covered).

---

## 14. Gates and finishing

Before the last commit, all with `. ~/.tc-env && ` in front:
```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -j 6 -- -D warnings
cargo test --workspace -j 6
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 --features gallery -- -D warnings
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 --features app -- -D warnings
cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
node --test parity/app/state.test.mjs
```
(tc-ui has no `harness` feature yet.) Commit each step as it goes green, with the attribution
lines your task gives. Final answer: what you built, the outcome per scenario and project, the
mismatches handed to the storage team (with the evidence file's path), the design text changed
and why, measured numbers, test counts, and where you departed from this plan and why.

---

## 15. Declared differences from tC4 4.0 ("Matching tC4", record them in the B1a annotation)

- A `schemaVersion: 1` `resources.json` is migrated as BURRITO-SPEC §5.3 describes (STORAGE §13.2);
  tC4 4.0's seed check refuses such a project. Reason: the translator gets their project with the
  English suite pinned, instead of a refusal they cannot act on.
- `verseEdits` is not carried: the format computes it from the hash §13.2 takes from the current
  verse, and no tC4 4.0 view reads it (`grep verseEdits src/views` is empty at `d310bf3`); the
  decision's invalidation is carried by `imported_invalid`.
- The check compares tC4's reading of each record, where tC4's compares whole documents; the
  rebuild's files write computed flags explicitly (`status`, `done`, `invalid`), with the same
  meaning for every tC4 reader.
- `metadata.json`'s free fields are seeded and checked (tC4 passes `meta: null` and lets the server
  own the file): more is kept, nothing is lost.

## 16. If something needs the owner

Nothing in this row waits for an owner answer. If you find a mismatch that changes what a
translator sees and neither STORAGE nor tC4 decides it, end your final answer with a section
OWNER QUESTION in plain words (what the person would see under each answer, and your
recommendation). The two OBS mismatches predicted here are storage bugs with a clear tC4 answer,
not owner questions.
