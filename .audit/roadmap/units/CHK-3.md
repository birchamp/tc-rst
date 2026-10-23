---
id: CHK-3
title: Claimed tests must exist
track: A
lane: checks
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: [INT-4]
needs_owner: []
proves: [P1, P3]
no_loss: false
size: 1 d
merged: none
updated: 2026-10-23
next: Claim this card after INT-4 merges, then do step 1.
blocked_by: none
---

# CHK-3: Claimed tests must exist

## Goal

Every test a document names exists, or the row says which card will write it. Three documents
are checked in CI: `docs/TC4-ISSUES.md` (covered and done rows), `docs/STORAGE.md` §15 (the
silent-loss cases) and a new `docs/P-LEDGER.md` (which CI job and tests show each of P1 to P10).
A new `parity/app/journeys/coverage.json` maps each live tC4 journey and each tC4 e2e spec to the
Rust journey tests, or to the card that will write them.

## Why it matters to a translator

The owner and the next agents trust these documents to say what is protected. A row that names
a test that does not exist hides a gap: a tC4 bug can come back, or a save can be lost, while
the document says it is covered.

## Before you start

- `python3 -I .audit/tools/board.py | grep '^| INT-4 '` shows `done`.
- `git ls-files --error-unmatch .audit/tools/board.py .audit/roadmap/units/CHK-3.md` succeeds (the
  pending check in CI reads the board; if the roadmap is not committed yet, stop: INT-5 commits it).
- `scripts/gates.sh --list | grep -w node` shows the node step; it runs `.audit/tools/*.test.mjs`
  and `parity/app/journeys/*.test.mjs`, so new test files there run in CI with no workflow edit.
- `python3 -I .audit/tools/board.py | grep -E '^\| (S2|FIX-R) '`. Step 5 needs S2 `done` (S2
  rewrites `docs/STORAGE.md` §15, from 102 rows to 143). FIX-R step 5 also edits
  `docs/TC4-ISSUES.md`: if FIX-R is claimed and not done, change only the rows step 2 below
  names, and tell FIX-R's owner in a `note`.
- `parity/tc4-baseline/manifest.json` `tc4.commit` is `d310bf304cfee784f4ca45dee474a3faf0d5616e`;
  `docs/PLAN.md:40` names `d310bf3` as the reference.
- The tC4 clone used to list journeys and specs (read only):
  `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/ext/tc4-tip`,
  or any clone of `unfoldingWord/translationCore4` that has `d310bf3`.

## Funnel card

- **Invariants**:
  - INV-CLAIMED-TESTS-EXIST: every backticked name in the test column of a checked row resolves
    to a test that exists, unless the cell says `(pending: <ID>)` and card `<ID>` is not `done`.
    A TC4-ISSUES row whose Status is `unit todo` fails once every card it names is `done` (or it
    names none): the test must exist by then or the row must change. A `done` card's funnel
    card names only tests that exist.
  - INV-INVARIANT-NAMES-ARE-USED: every `INV-` name in a `done` card's Invariants list appears
    in at least one file under `crates/*/tests`, `crates/*/src`, `parity/` or `.audit/tools`
    (as a message or a test name).
  - INV-P1-COVERED: every live tC4 journey (24 at d310bf3: J1-J8, J9a, J9c, J9d, J11, J12, J13,
    J16, J17, J18, J20-J26) and every tC4 e2e spec (37 at d310bf3) maps to journey tests that
    exist, or to a card that is not `done`.
- **Wrong states**:
  - A covered row names a renamed or never-written test (`#507` names
    `j25_a_story_opens_with_its_rail_its_units_and_a_comment_box_that_takes_no_typing`; the test
    is `j25_a_story_opens_with_its_rail_its_units_and_writable_comment_boxes`,
    `parity/app/journeys/story.test.mjs:26`).
  - A §15 row names a test that does not exist (49 of 219 names on S2 at 3b075d7, by the
    evidence's script; L54 names `loss_salvage_damaged_pack_lines`).
  - A card is done but the journey or test it promised is still missing.
  - A `unit todo` row (89 rows on 2026-10-22) whose card finishes without writing the test: the
    ledger never looks at it because it only reads `covered` and `done` rows.
  - A done card's funnel card names `push_is_never_forced`, and no test has that name.
- **Code**:
  - Types: none in product code. One pure module, `.audit/tools/claims.mjs` (to create):
    `testNames(repo)`, `resolve(name, names)`, `checkTable(path, rows, rule)`.
  - One entry point: `.audit/tools/claims.test.mjs` checks every document; adding a document is
    one more entry in its list.
  - Removed: nothing.
  - Left to checks, and why a type cannot carry it: the claims are prose in Markdown.
- **Checks** (all to create; kind: ledger; runs in: the CI job that runs the gate step `node`):
  - In `.audit/tools/claims.test.mjs`: `INV-CLAIMED-TESTS-EXIST: TC4-ISSUES covered rows`,
    `INV-CLAIMED-TESTS-EXIST: STORAGE section 15`, `INV-CLAIMED-TESTS-EXIST: P-LEDGER`, and
    `the resolver finds fns, journey titles, template titles and parity ids` (planted names).
  - In `parity/app/journeys/coverage.test.mjs`: `INV-P1-COVERED: every live journey is mapped`,
    `INV-P1-COVERED: every tC4 spec is mapped`, `INV-P1-COVERED: the reference is the baseline's`.
  - Proof it can fail: each step plants one wrong name and records the message.
- **Prose**: `docs/P-LEDGER.md` (to create) says in two lines how to read it.
- **Human**: none.

## Steps

### Step 1: Write the resolver and the TC4-ISSUES check, red first

- Do: in `.audit/tools/claims.mjs` (Node built-ins only), collect the names a claim may resolve
  to: every `fn <name>` in `crates/**/*.rs`; every title of `browserTest(` and `test(` in
  `parity/**/*.mjs`, where a template title such as
  `` `item_navigation_${tool}_${locale}_marking_row_boundaries_tab_and_wrapping` ``
  (`check-detail.test.mjs:62`) matches with each `${...}` as a wildcard; every test-binary stem
  `crates/*/tests/<stem>.rs`; every `rows[].name` of `parity/app/report.json`; every
  `name: '<id>'` in `parity/ds-gallery/scenarios.mjs`. A claim is each backticked token in the
  test column; `{a,b}` in a claim expands to one claim per choice; a `mod::fn` path resolves by its
  last segment. In `.audit/tools/claims.test.mjs`, for `docs/TC4-ISSUES.md`: read every table whose
  header has `Status` and `Regression test`; for each row whose Status is `covered` or `done`,
  fail on a claim that does not resolve, and on a test cell that says "to add" (a covered row
  cannot wait for its test). `(pending: <ID>)` passes only when
  `python3 -I .audit/tools/board.py --json` lists `<ID>` with a state other than `done`. Messages:
  `INV-CLAIMED-TESTS-EXIST: docs/TC4-ISSUES.md:<line> (#<issue>) is covered by <name>, but no
  such test exists` and `... is pending on <ID>, but <ID> is done: write the test or fix the row`.
  Third rule, for rows whose Status is `unit todo`: the row fails when every card id its unit
  cell names is `done` on `board.py --json`, or when it names no card at all, with
  `INV-CLAIMED-TESTS-EXIST: docs/TC4-ISSUES.md:<line> (#<issue>) waits on <IDs>, but they are
  done: write the test or mark the row`. (A unit cell may name an old unit id such as `D2`; the
  map from old ids to cards is `ROADMAP.md` "How the work is organised".)
- Files: `.audit/tools/claims.mjs`, `.audit/tools/claims.test.mjs` (to create).
- Done when: `node --test .audit/tools/claims.test.mjs` runs and the TC4-ISSUES test fails,
  listing each unresolved claim; the planted failure for the third rule (mark one `unit todo`
  row's card `done` in a copy of the board's JSON) prints the `waits on` message. A scan of resume-build-lane at f9067bc found these candidates
  (estimate; your list is the truth): #507, #529 `imported_obsolete_checking_language_is_kept_and_ignored`,
  #737 and #42 `WordNav` (a component, not a test). The resolver test passes.
- Record: a log line `step 1` with the failing list.

### Step 2: Make the TC4-ISSUES rows true

- Do: for each failure of step 1: a renamed test: write the current name (#507 above); a test
  the row's unit has not written: `(pending: <card>)` (#529's import half: `(pending: I)`); a
  non-test word in backticks (`WordNav`): drop the backticks. Rows FIX-R step 5 owns (#576, #731,
  #599, #795, #612, #507, #796): if FIX-R is not done, mark them `(pending: FIX-R)` and change
  nothing else in them.
- Files: `docs/TC4-ISSUES.md`.
- Done when: the TC4-ISSUES test passes. Change one covered row's test name by one letter: it
  fails naming that line and issue. Revert.
- Record: a log line `step 2` with the number of rows changed and the planted failure.

### Step 3: Map every tC4 journey and spec

- Do: create `parity/app/journeys/coverage.json`: `tc4.commit` (the full sha of d310bf3);
  `tc4.liveJourneys` (the J ids of rows in tC4's `docs/JOURNEYS.md` index whose Status is not
  "retired" or "Phase 2": `git -C <tc4 clone> show d310bf3:docs/JOURNEYS.md | sed -n 26,58p`);
  `tc4.specs` (`git -C <tc4 clone> ls-tree --name-only d310bf3 e2e/ | grep 'spec.ts$'`, 37
  files); `journeys` and `specs`, each id or spec mapped to `{ "file": "<journey file>", "tests":
  [<exact titles>] }`, to `{ "rust": "<crates/... test file>", "tests": [...] }`, to
  `{ "pending": "<card id>" }`, or to both a file and a pending card for a partial one. Start from
  `evidence/2026-10-21/done-definition.md` §2: J7 and J23 pending D2; J9a, J9c, J9d pending F1;
  J24 pending E1b; J26 pending E1a; partial J3, J12, J13 pending FIX-J; partial J8 pending D1,
  J11 and J17 pending E1b, J18 pending C7a and E1a. The three specs with no test: `j10-rtl`
  maps to G4; `bundled-upgrade` to REF1 (its step 2 ports it as
  `parity/app/journeys/bundled-upgrade.test.mjs`); `j14-join-isolation` to E1b (its step 4 ports
  it into `open-door43.test.mjs`). Add no card here; every spec has one. Write `coverage.test.mjs`: the keys of `journeys` equal
  `tc4.liveJourneys`; the keys of `specs` equal `tc4.specs`; each file and each title exists; each
  pending card is not `done` (`board.py --json`); `tc4.commit` equals
  `parity/tc4-baseline/manifest.json` `tc4.commit`. Message example:
  `INV-P1-COVERED: J26 is pending on E1a, but E1a is done; add its journey`.
- Files: `parity/app/journeys/coverage.json`, `parity/app/journeys/coverage.test.mjs` (to create).
- Done when: `node --test parity/app/journeys/coverage.test.mjs` passes; deleting the `J5` entry
  fails it naming J5; changing a title by one letter fails it naming the file and title. Revert.
- Record: a log line `step 3` with the counts (mapped, partial, pending).

### Step 3b: Check the cards' own claims

- Do: add a fourth table to `.audit/tools/claims.test.mjs`, `INV-CLAIMED-TESTS-EXIST: units`:
  for every card in `.audit/roadmap/units/` whose front matter has `state: done`, every
  backticked snake_case name in the `- **Checks**` bullets of `## Funnel card` resolves through
  `resolve(name, names)` (fn names, journey titles; `(to create)` is allowed only while the card
  is not done). Message: `INV-CLAIMED-TESTS-EXIST: units/X.md names push_is_never_forced but no
  test has that name`. Second assertion, `INV-INVARIANT-NAMES-ARE-USED`: every `INV-[A-Z0-9-]+`
  in a done card's `- **Invariants**` list appears in at least one file under `crates/*/tests`,
  `crates/*/src`, `parity/` or `.audit/tools`; message `INV-INVARIANT-NAMES-ARE-USED: units/X.md
  names INV-PUSH-NEVER-FORCED but nothing prints or tests it`.
- Files: `.audit/tools/claims.test.mjs`.
- Done when: the test passes on today's done cards (fix any card it catches by renaming to the
  real test, in that card's log as a `note`); misspelling one name in a done card fails it naming
  the card and the name. Revert.
- Record: a log line `step 3b` with the counts.

### Step 4: Write the P-item ledger

- Do: create `docs/P-LEDGER.md`: one table `| P | What it promises | CI job | Tests |`, one row
  per P1 to P10 of `docs/PLAN.md`, with the CI job ids of `.github/workflows/*.yml` and the
  tests in backticks, or `(pending: <ID>)`. Start from `done-definition.md` §1, for example P3:
  job `test` (INT-2 creates it; `rust` before INT-2), `fold_converges_under_any_union`, `conservation`,
  `concurrent_differences_are_visible`, `merge_units_sound`, `resolution_always_possible`
  (`crates/tc-journal/tests/properties.rs`); P7: `the_committed_record_passes_the_gate`
  (`crates/tc-ui/tests/screen_record.rs:40`); P10: `every_kind_has_the_pinned_wire_form`,
  `the_encoding_is_pinned`, `tokens_have_the_pinned_byte_form_and_key`, then `(pending: V)`;
  P2 and P9 pending (X, E1a, E1b, E2, C7b; I, F1), one `(pending: <ID>)` per card. Add the
  ledger to `claims.test.mjs`: every
  claim resolves, every CI job named is a job key in a workflow file, every P from P1 to P10 has
  a row.
- Files: `docs/P-LEDGER.md` (to create), `.audit/tools/claims.test.mjs`.
- Done when: the P-LEDGER test passes; naming job `nightlyy` fails it. Revert.
- Record: a log line `step 4`.

### Step 5: Check STORAGE §15

- Do: only when S2 is `done`. Add `docs/STORAGE.md` §15 (from `## 15.` to `## 16.`): every row
  `| L<n> |`, test column = the last cell; every claim must resolve or carry `(pending: <ID>)`.
  Fix the failures: a renamed test, after you read it and it asserts what the row says (L54:
  `a_truncated_pack_salvages_every_whole_line`, `crates/tc-journal/tests/packs.rs:302`; if it
  does not cover a damaged pack from a teammate, treat L54 as a missing test: see "Stop and ask
  when");
  sync, Door43, USB rows: `(pending: X)`; import rows: `(pending: I)`; any other missing test:
  the storage card that owns it, or a minor and a new card.
- Files: `.audit/tools/claims.test.mjs`, `docs/STORAGE.md` (§15 test cells only).
- Done when: `node --test .audit/tools/claims.test.mjs` passes; renaming one §15 claim by a
  letter fails naming `docs/STORAGE.md:<line> (L<n>)`. Revert.
- Record: a log line `step 5` with the counts (resolved, renamed, pending per card).

### Step 6: Run the gate list and hand on

- Do: `scripts/gates.sh`. Write a `note` for X, I and FIX-R: "your rows are `(pending: <ID>)`;
  when you write a test, replace the marker with its name in the same commit".
- Files: none.
- Done when: `scripts/gates.sh` ends with its pass line.
- Record: a log line `gate`; set `state: verifying` and `next`.

## Gates before merge

Run `scripts/gates.sh` (INT-3 made it; INT-4 needs INT-3) on the exact tree you merge. It must print
`gates: all <n> steps passed; receipt ...`. If the script is missing, run these instead:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
3. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, then
   the same with `--features gallery`, `--features app` and `--features test-hooks`
4. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
5. `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
6. `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
   (needs `gcc-mingw-w64-x86-64` and `nasm`, as `.github/workflows/ci.yml` installs them)

Plus: `node --test .audit/tools/claims.test.mjs parity/app/journeys/coverage.test.mjs` passes,
and each planted failure is in the log.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A covered row's or a §15 row's test is missing and no card owns it: that is a finding. Write
  it in the log, open a card for it (TRACKING.md "Adding a work item"), and mark the row
  `(pending: <that card>)`; a TC4-ISSUES row also goes back from `covered` to `unit todo`. Never
  mark a row pending on a card that does not own the test.
- A step would edit `docs/STORAGE.md` while S2 is not `done`, or `docs/TC4-ISSUES.md` rows FIX-R
  owns while FIX-R is claimed.
- A gate fails twice in a row and you cannot tell why.

## References

- `evidence/2026-10-21/done-definition.md` §2, DOD-2, DOD-11, "Highest-leverage additions" 3
- `evidence/2026-10-21/checks-inventory.md` ranks 5 and 15, finding C14 (its #568 example
  resolves today: `align_by_keyboard_only` is a journey title, `align.test.mjs:202`)
- `evidence/2026-10-21/health-storage.md` §11, F15; `evidence/2026-10-21/tc4-bugs.md` T1, T11, §6
- tC4 `docs/JOURNEYS.md` index at d310bf3 (lines 26-58); `units/FIX-R.md` step 5
- Size basis: checks-inventory ranks 5 and 15, half a day each.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/done-definition.md
- 2026-10-23 23:50 EDT | C7a session (Claude Code 3058c665), for FIX-J | note | none | FIX-J's journeys exist on track-FIX-J: J3 `j3_a_download_from_the_seeded_fake_installs_the_pinned_version` (parity/app/journeys/source-texts.test.mjs), J12 `j12_an_upgrade_through_the_screens_moves_the_pin_on_disk` and J13 `j13_change_writes_the_primary_set_to_disk` (parity/app/journeys/upgrades.test.mjs); step 3's coverage map points J3, J12 and J13 at them, not at a pending card, once FIX-J merges | .audit/roadmap/units/FIX-J.md
