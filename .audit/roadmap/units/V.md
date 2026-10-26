---
id: V
title: Old projects keep working; freeze format 1.0
track: A
lane: storage
milestone: M2
state: parked
owner: none
branch: none
worktree: none
depends: [X, I, T, S3]
needs_owner: []
later: ["H9 from step 8"]
proves: [P10]
no_loss: true
size: 1.5-3 d (estimate; nothing comparable has been built)
merged: none
updated: 2026-10-24
next: Parked until after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md).
blocked_by: none
---

# V: Old projects keep working; freeze format 1.0

## Goal

PLAN P10 is shown in CI. A frozen format version leaves sample projects ("capsules") that nobody
may change. CI opens every capsule with the current build and finds every verse, alignment, check
decision, comment, conflict and history entry unchanged, valid, and editable, savable and
exchangeable. CI also has the last frozen build open folders the current build wrote: it edits
them, opens them read-only with a clear message, or refuses a newer major before writing, and
never calls them damaged. A pinned-byte change fails until it has a new version and new capsules.
Before the freeze, V settles the format items that `docs/STORAGE-DELTA-2026-10-21.md` §2.15
lists: it pins `ImportSource::Markdown`, the import cause's `unknown_base` mark and the new
`.tc/materialized.json` (`kept`, and `held` without `damage`); it removes
`Cause::External { LocalFile }` if no capsule holds it; and after H18 it keeps or removes
`Door43Edit` and `ExchangeEdit`. Then the owner freezes format 1.0 (H9).

## Why it matters to a translator

A translator who updates the app, or opens a teammate's project made by another version, keeps
every piece of work, and an older app never calls their project damaged (tC4 RISKS #10, #46).

## Before you start

- `python3 -I .audit/tools/board.py` lists X, I, T and S3 as `done`. S3 was added to `depends`
  on 2026-10-21: S3 step 9 now applies the rulings and changes `.tc/materialized.json` (`kept`,
  `held` without `damage`), and V must pin that manifest after it lands (delta §2.15).
- `ls fixtures/capsules fixtures/format 2>&1` says both are missing; `git tag --list 'format-v*'`
  prints nothing.
- `grep -n "TC_JOURNAL_BLESS" crates/tc-journal/tests/*.rs` prints `canonical.rs:261` (a doc
  comment), `canonical.rs:266` and `versioning.rs:281` (lines from `s2-proof-executor`): two
  switches that rewrite pins with no version check.
- `grep -n "fn the_encoding_is_pinned\|fn mixed_minor_versions_converge\|fn the_gate_refuses_newer_majors_and_reads_newer_minors" crates/tc-journal/tests/*.rs`
  prints three lines (`index.rs`, `properties.rs`, `versioning.rs:293`): the pins V builds on.
- V no longer waits for a carried-file format change: the 2026-10-21 carried-files ruling needs
  no byte change (delta §2.14). `grep -rn "refs/tc/carried" crates/*/src` shows no code that
  writes that ref (S3 step 9 (c) removed it).
- `ls .audit/roadmap/rulings/ | grep -E 'H18|H19|H20|H22'`: note which rulings exist. Step 1
  (the `carried` capsule), step 3 and step 8 wait for some of them.
- Read STORAGE §12 (format versioning, rules 1 to 7), PLAN P10, the two rulings
  `.audit/roadmap/rulings/2026-10-21-*.md`, and delta §1, §2.3, §2.14, §2.15 and §6.

## Funnel card

- **Invariants**:
  - INV-OLD-PROJECTS-OPEN-UNCHANGED: every capsule of every frozen version opens with the current
    build with all its work unchanged, validates, and can be edited, saved and exchanged.
  - INV-PINNED-BYTES-NEED-A-VERSION: a change to any pinned byte form (change lines,
    `format.json`, pack and pending headers, `.tc/materialized.json`, the fold cache, tokens)
    fails CI unless the format version and its `FORMAT_PINS` row and capsules are new.
  - INV-CAPSULES-ARE-IMMUTABLE: a frozen capsule's files equal its lock, and equal the files at
    its tag.
  - INV-FORMAT-SETTLED-BEFORE-FREEZE: every row of delta §2.15 has a decision, recorded in the
    log, before 1.0 freezes. A value the app no longer writes leaves the format unless a capsule
    holds it or an open ruling keeps it.
- **Wrong states**:
  - `TC_JOURNAL_BLESS=1` rewrites `fixtures/journal/v0-every-kind.ndjson` and
    `format-1.0.json` silently, so a format change can ship with no new version.
  - Packs, the store's manifest `.tc/materialized.json` (`layout::MANIFEST`) and its states are
    not pinned (done-definition DOD-15).
  - An older build moves a newer `installation.json` aside as `.damaged-<stamp>`
    (`crates/tc-store/src/appdata.rs:100-108`) when it cannot parse it.
  - Someone edits a capsule to make a test pass.
  - 1.0 freezes holding values nobody writes any more (`Cause::External { LocalFile }`, and
    after an "unsupported" H18 `Door43Edit` and `ExchangeEdit`), or without
    `ImportSource::Markdown` and the import's `unknown_base` mark that "Update a book" writes.
  - A no-parents import change ranks as provisional, so a frozen capsule shows an old file's
    text over a teammate's newer verse (delta review R6).
  - A manifest from a development build whose `held` says `cause: damage` fails to open, or is
    called damaged, instead of opening as a held change that is asked about (delta §2.15).
- **Code**:
  - Types and files (all to create): `fixtures/capsules/<version>/<capsule>/` (a project folder)
    with `EXPECT.json` (what each must hold); `fixtures/capsules/<version>/LOCK.sha256`;
    `fixtures/capsules/<version>/FROZEN` once frozen; the generator
    `crates/tc-corpus/examples/capsule.rs` with `write`, `check` and `edit`; a `FORMAT_PINS` table
    keyed by version in `crates/tc-corpus/tests/capsules.rs`.
  - One entry point: `capsule write` makes every capsule and refuses an existing folder;
    `capsule check` is what CI's older build runs.
  - Removed: the unchecked bless writes (they refuse once a version is `FROZEN`); and, by the
    step 3 decisions, `Cause::External { LocalFile }` and, if H18 says unsupported,
    `ExternalSource::Door43`, `ExchangeFile` and device kinds `Door43Edit` = 4,
    `ExchangeEdit` = 5 (`crates/tc-journal/src/change.rs`).
  - Left to checks, and why: whether an old build reads new bytes cannot be typed; only running
    the old build shows it.
- **Checks**:
  - `capsules_match_their_lock` (to create, `crates/tc-corpus/tests/capsules.rs`); kind:
    contract; runs in: `rust` job. Proof: change one byte of a capsule; it fails.
  - `every_capsule_opens_unchanged_and_validates`, `every_capsule_edits_saves_and_exchanges`,
    `a_capsule_with_newer_minor_changes_opens_and_keeps_them` (to create, same file); kind:
    contract; runs in: `rust` job. Proof: drop one record from a capsule's `EXPECT.json` copy.
  - `pinned_bytes_need_a_new_version` (to create, same file); kind: contract; runs in: `rust` job.
    Proof: change one byte the encoder writes in a scratch commit; it fails, naming the pin.
  - `format_1_0_holds_the_settled_items` (to create, same file); kind: contract; runs in: `rust`
    job. Proof: drop `ImportSource::Markdown` or the `unknown_base` mark in a scratch commit; it
    fails.
  - `a_development_manifest_with_held_damage_opens_as_held` (to create,
    `crates/tc-store/tests/store/tc4.rs`); kind: unit; runs in: `rust` job. Proof: make the
    manifest parser refuse `damage`; it fails.
  - `a_newer_major_is_refused_before_anything_is_written`,
    `a_project_that_needs_a_newer_reader_opens_read_only_with_the_message`,
    `older_build_starts_on_newer_app_data` (to create, `crates/tc-store/tests/store/tc4.rs`);
    kind: unit; runs in: `rust` job.
  - `format-older-reader` (to create in `.github/workflows/ci.yml`); kind: contract across
    builds; runs on every push. Proof: run it with a build whose `capsule check` prints `damaged`.
- **Prose**: STORAGE §12.3 "Freezing a format version" (to create): the release procedure.
- **Human**: H9 (ROADMAP.md): freeze format 1.0. Asked in step 8 with the evidence. Before
  that, the rulings that decide what goes into 1.0: H18 (Door43 web edits: keep or drop
  `Door43Edit`), H19 (carried files now or later: the `carried` capsule), H20 (Update a book
  with no known base: the no-base part of the `updated-book` capsule), H22 (delta Q6: keep or
  drop `ExchangeEdit`).

## Steps

### Step 1: Write the capsule generator and the lock test

- Do:
  1. Red first: write `capsules_match_their_lock` in `crates/tc-corpus/tests/capsules.rs`: for
     each `fixtures/capsules/<version>/`, every file's sha-256 equals its line in `LOCK.sha256`,
     and no file is missing or extra.
  2. Write `crates/tc-corpus/examples/capsule.rs`: `capsule write <version> <dir>` builds every
     capsule through the public APIs only (tc-store's `ProjectHandle`; X's
     `tc_sync::exchange::{export, receive}`; I's `tc_import::import`; both cards name them as
     their one entry point) and writes `EXPECT.json` and `LOCK.sha256`; it exits non-zero if
     `<dir>/<version>` exists. Add tc-store, tc-sync and tc-import to `[dev-dependencies]` of
     `crates/tc-corpus/Cargo.toml` (an example may use them; the `storage` branch adds tc-store
     there for perfreport), not to `[dependencies]`: tc-store already dev-depends on tc-corpus.
  3. Capsules: `bible-titus` (text, alignments, tN and TWL decisions, comments, one open
     conflict, three checkpoints of history); `stories` (an OBS project); `exchanged` (a USB
     exchange between two devices); `imported-tc4` (a tC4 4.0 project, from the fixture I's
     tests import); `imported-usfm` (from `fixtures/tc4/conformance-fixtures/import/usfm`);
     `updated-book` (`bible-titus` after "Update a book from USFM" of a kept copy with a known
     base, and an OBS story after "Update a story from Markdown": causes `Import(Usfm)` and
     `Import(Markdown)`, delta §2.16); `held-outside` (`bible-titus` with a book file changed
     outside and kept, waiting for the answer, and an earlier copy left by *Not now*: the
     manifest's `held` state and two `kept` entries, delta §2.3); `mixed-minor`
     (`bible-titus` plus the changes of `fixtures/journal/newer-minor.ndjson`). No capsule
     holds `refs/tc/carried` or a `held` with `cause: damage`.
  4. `carried` (`rulings/2026-10-22-H19.md` keeps S2's carried code without its git backups):
     a project with an audio file whose record names the version it replaced and its Door43
     user, and a *Keep both* pair named `<stem>-<Door43 user>.<ext>`
     (`rulings/2026-10-22-H22.md` part 2, for example `jon_01-Juan.wav`). Build it through the
     store's public API. The capsule holds no `refs/tc/carried`.
  5. `updated-book`, both parts: the known-base part, and the no-base part (a file chosen from
     anywhere) under `rulings/2026-10-22-H20.md`: one verse empty in the project and drafted in
     the file (imported, no conflict), one drafted in the project and empty in the file (left,
     no conflict), and two that differ with text on both sides (one grouped row marked
     `unknown_base`). Wait for I step 7b's merge before writing the no-base part.
- Files: `crates/tc-corpus/examples/capsule.rs`, `crates/tc-corpus/tests/capsules.rs`,
  `crates/tc-corpus/Cargo.toml`.
- Done when: `cargo run -p tc-corpus --example capsule -- write 1.0-draft fixtures/capsules`
  writes the set; running it again exits non-zero and changes nothing (`git status --short
  fixtures/capsules` is unchanged); `cargo test -p tc-corpus --test capsules capsules_match_their_lock`
  passes, and fails after one byte of any capsule file is changed. Before the freeze a draft set
  may be deleted and written again (for a ruling that came later); a frozen set never.
- Record: log line `step 1` with the set's size (`du -sh fixtures/capsules`) and the capsules
  still waiting for H19 or H20; `next: do step 2`.

### Step 2: Read, edit, save and exchange every capsule

- Do: write `every_capsule_opens_unchanged_and_validates` (copy each capsule to a temp folder,
  never open it in place; open with the current store; compare every verse, alignment, decision,
  comment, conflict and history entry with `EXPECT.json`; `tc_burrito::validate_dir` returns
  no problem),
  `every_capsule_edits_saves_and_exchanges` (save a verse, a decision and an alignment; checkpoint;
  export and import by USB; reopen; nothing lost), and
  `a_capsule_with_newer_minor_changes_opens_and_keeps_them` (`mixed-minor`: the unknown changes
  are kept and counted, STORAGE §12.2 rule 2). For `held-outside`, opening keeps both kept
  copies with their notices and asks the question; nothing from the copies enters the record
  until *Import* (ruling OE 1, OE 3). Give `capsule check <dir>` and `capsule edit <dir>`
  the same logic; `check` prints one line: `opened`, `read-only: <message>`, `refused: <message>`
  or `damaged: <what>`.
- Files: `crates/tc-corpus/tests/capsules.rs`, `crates/tc-corpus/examples/capsule.rs`.
- Done when: the three tests pass; each fails when one record is removed from a temp copy of a
  capsule.
- Record: log line `step 2`; `next: do step 3`.

### Step 3: Settle the format items of delta §2.15

- Do:
  1. Check each row of delta §2.15 on main: `ImportSource::Markdown { file_name }` exists; the
     import cause has an `unknown_base` mark and `CauseClass::rank` (`fold/meta.rs`) gives it 0;
     no code writes `Cause::External { source: LocalFile }` (`grep -rn "LocalFile" crates/*/src`);
     `RepoPath::parse_source` refuses `metadata.json` as a `reconciles` path; the manifest has
     `kept`, and `held` has no `damage` cause. These belong to S3 and I. If one is missing, ask
     that card's owner; do not write it here.
  2. `Cause::External { LocalFile }`: if `grep -rl LocalFile fixtures/capsules` finds nothing,
     remove it from the format (delta §2.15: "V decides"), refresh
     `fixtures/journal/v0-every-kind.ndjson` with `TC_JOURNAL_BLESS=1` (nothing is `FROZEN` yet)
     and write the draft capsules again.
  3. `rulings/2026-10-22-H18.md` says Door43 tool edits are unsupported (B), and
     `rulings/2026-10-22-H22.md` part 3 treats a hand-edited book inside an exchange file the
     same way, so §11.3's walk is not built. Remove `ExternalSource::Door43`, `ExchangeFile`,
     device kinds 4 and 5, and then `Cause::External` entirely, as in item 2. No capsule holds
     a `Door43Edit` cause.
  4. Write `format_1_0_holds_the_settled_items` in `capsules.rs`: an `Import(Markdown)` change and
     an import change with `unknown_base` encode and read back; a no-parents import change is
     never provisional over a teammate's newer verse. Write
     `a_development_manifest_with_held_damage_opens_as_held` in `crates/tc-store/tests/store/tc4.rs`:
     on a temp copy of `held-outside` whose `held` says `cause: damage`, the project opens, the
     path is held and asked about, and no file or kept copy is lost. First grep for the delta §5.3
     tests S3 and I wrote; reuse one that already covers a case.
- Files: `crates/tc-corpus/tests/capsules.rs`, `crates/tc-store/tests/store/tc4.rs`, and for the
  removals `crates/tc-journal/src/change.rs`, `fixtures/journal/`, `fixtures/capsules/`.
- Done when: both tests pass and fail as their Proof says; every §2.15 row has a decision.
- Record: log line `step 3` naming each §2.15 row's decision, and any row waiting for H18 or H22;
  `next: do step 4`.

### Step 4: Pin every byte form by version

- Do: in `capsules.rs` add `FORMAT_PINS: &[(&str, &[(&str, &str)])]`, one row per format
  version (`"1.0"` first), each a list of (pin name, sha-256): `fixtures/journal/v0-every-kind.ndjson`,
  `fixtures/journal/format-1.0.json`, a pack header and a pending header the current encoder
  writes, `bible-titus`'s and `held-outside`'s `.tc/materialized.json` (the manifest after S3
  step 9: `kept` entries, `held` without `damage`), and the version's `LOCK.sha256`. Write
  `pinned_bytes_need_a_new_version`: the current build's bytes equal the row of
  `FormatDoc::current()`'s version; on a mismatch it prints
  `INV-PINNED-BYTES-NEED-A-VERSION: <pin> changed; raise the format minor, add a row and capsules`.
  Make both `TC_JOURNAL_BLESS` blocks refuse (fail the test) when
  `fixtures/capsules/<major.minor>/FROZEN` exists. Keep `the_encoding_is_pinned` as it is: it
  already pins the fold cache by `INDEX_VERSION`.
- Files: `crates/tc-corpus/tests/capsules.rs`, `crates/tc-journal/tests/{versioning,canonical}.rs`.
- Done when: the test passes; it fails after a one-byte change to the pack header encoder in a
  scratch commit; with a `FROZEN` file present, `TC_JOURNAL_BLESS=1 cargo test -p tc-journal --test canonical`
  fails and leaves the fixture unchanged.
- Record: log line `step 4`; `next: do step 5`.

### Step 5: Show what a newer project does to this build

- Do: in `crates/tc-store/tests/store/tc4.rs` write, on temp copies of `bible-titus` whose
  `ingredients/tc/format.json` is raised: `a_newer_major_is_refused_before_anything_is_written`
  (major 2: `OpenError::NewerMajor`, every file's bytes unchanged),
  `a_project_that_needs_a_newer_reader_opens_read_only_with_the_message` (`minReaderMinor` above
  ours: `ReadOnlyReason::NewerMinor`, the message of STORAGE §12.2 rule 3, no write). Write
  `older_build_starts_on_newer_app_data`: `installation.json` and `ledger.json` with members a
  newer build adds; `tc_store::appdata::AppData::open` keeps both, writes no `.damaged-` file and
  keeps the secret. First `grep -rn "NewerMajor\|NewerMinor" crates/*/tests`: reuse a test that
  already covers a case.
- Files: `crates/tc-store/tests/store/tc4.rs` (and `crates/tc-store/src/appdata.rs` if the app
  data test fails).
- Done when: the three tests pass; the app data test fails if the extra members make
  `parse_installation` return `None`.
- Record: log line `step 5`; tell G1's owner in a `note` that tc-app's own settings need the same
  test (TC4-ISSUES #46 lists V and G1). `next: do step 6`.

### Step 6: Run the last frozen build against today's folders in CI

- Do: add the job `format-older-reader` to `.github/workflows/ci.yml`: `actions/checkout@v4` with
  `fetch-depth: 0`; pick the newest `format-v*` tag (`git tag --list 'format-v*' --sort=-v:refname | head -1`),
  or `HEAD` when none exists; `git worktree add ../old <ref>`; build its example
  (`cargo build --release -p tc-corpus --example capsule --manifest-path ../old/Cargo.toml`); with
  the current build, `capsule write current "$RUNNER_TEMP/new"`; run the old `capsule check` and
  `capsule edit` on each folder; fail on any `damaged:` line. With a tag, also run
  `git diff --exit-code <tag> -- fixtures/capsules/<its version>`.
- Files: `.github/workflows/ci.yml`.
- Done when: the job passes on a branch push (with `HEAD` as the old build); a scratch commit whose
  `check` prints `damaged:` makes it fail.
- Record: log line `step 6`; `next: do step 7`.

### Step 7: Write the release procedure

- Do: add STORAGE §12.3 "Freezing a format version": T's derived-byte fixes, X, I, S3 (with the
  2026-10-21 rulings) and any approved format change have merged; every row of delta §2.15 has
  a decision; `capsule write <v>-draft` is current and every V check
  passes; the owner says yes (H9); rename `<v>-draft` to `<v>`, add `FROZEN` and the
  `FORMAT_PINS` row, regenerate `LOCK.sha256`; tag `format-v<v>`; push the tag under the push
  rule (ROADMAP.md). After that, a byte change needs a new minor (§12.2 rule 7).
- Files: `docs/STORAGE.md`.
- Done when: the section exists, and every command it names exists in the repository.
- Record: log line `step 7`; `next: do step 8`.

### Step 8: Ask the owner to freeze format 1.0, and freeze on yes

- Do: the rulings H18, H19, H20 and H22 (`rulings/2026-10-22-H18.md` to `-H22.md`) decided what
  goes into 1.0 (steps 1 and 3); check that the capsules and step 3 items follow them, then merge
  steps 1 to 7 into main. Then ask H9 in plain words: what freezing means for a
  translator (projects saved from now on open in every later version; a later change that older
  apps cannot read makes them read-only, never damaged), the evidence (the passing checks, the
  capsule list), and the recommendation (ROADMAP.md H9). On yes: write `rulings/<date>-H9.md`,
  follow §12.3, and merge.
- Files: `fixtures/capsules/`, `crates/tc-corpus/tests/capsules.rs`, `rulings/`.
- Done when: `git tag --list 'format-v1.0'` prints `format-v1.0`, the `format-older-reader` job
  uses it, and `fixtures/capsules/1.0/FROZEN` exists. Until the answers: `state: parked`,
  `next: wait for H18, H19, H20, H22` (those still open), then `wait for H9`.
- Record: log line `ruling`, then `merge`; `state: done`.

## Gates before merge

Until INT-3 lands, every command below; after INT-3, `scripts/gates.sh` (to create in INT-3):

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and the
  same with `--features gallery`, `--features app`, `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` (the
  Windows step of `.github/workflows/ci.yml`; needs `gcc-mingw-w64-x86-64` and `nasm`)

Plus the `format-older-reader` job, and the capsule tests on the Windows VM gate (`gates.ps1`).

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A capsule would need editing to pass: never edit it. Find the bug, or make a new version.
- The capsules would add more than a few MB to the repository (`du -sh fixtures/capsules`):
  shrink them (Titus, a few chapters) before you commit.
- X, I or S3 has no public entry point a capsule needs: ask that card's owner; do not reach
  inside.
- A delta §2.15 item is missing on main, or a ruling (H18 to H22) adds or removes a value in the
  format: settle it in step 3 before step 8. Never freeze with a row undecided.
- A capsule would hold `Cause::External { LocalFile }`: say so in the log before step 3
  removes it; the value then stays in 1.0.

## References

- `.audit/roadmap/evidence/2026-10-21/storage-remaining.md` §6, ST-6, ST-9;
  `done-definition.md` P10 row, DOD-15; `tc4-bugs.md` §2 (V row).
- `docs/STORAGE.md` §12; `docs/PLAN.md` P10; `docs/TC4-ISSUES.md` #46; tC4 `docs/RISKS.md` row 10.
- `docs/STORAGE-DELTA-2026-10-21.md` §2.3, §2.14, §2.15, §2.16, §6 (V row), §7, Review R6;
  `.audit/roadmap/rulings/2026-10-21-outside-edits.md`,
  `.audit/roadmap/rulings/2026-10-21-carried-files.md`; ROADMAP.md H9, H18 to H22.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/storage-remaining.md
- 2026-10-21 | roadmap | note | none | updated to the owner's rulings of 2026-10-21 and docs/STORAGE-DELTA-2026-10-21.md | .audit/roadmap/rulings/
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
- 2026-10-26 | shell lane (Claude Code 6f39d119), for CHK-3 | note | none | your rows of docs/STORAGE.md §15 are `(pending: V)`; when you write a test, replace the marker with its name in the same commit (claims.test.mjs checks §15 now) | docs/STORAGE.md
