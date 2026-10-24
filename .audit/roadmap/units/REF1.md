---
id: REF1
title: Move the tC4 reference and port its changes
track: A
lane: app
milestone: M3
state: todo
owner: none
branch: none
worktree: none
depends: [SWEEP]
needs_owner: []
proves: [P1, P7]
no_loss: false
size: 3-5 sessions (estimate)
merged: none
updated: 2026-10-24
next: Claim this card now: H7 is answered, match tC4 4.0 rc.2 (rulings/2026-10-24-H3-H4-H7-fast-release.md).
blocked_by: none
---

# REF1: Move the tC4 reference and port its changes

## Goal

The rebuild matches the tC4 version the owner names in H7 (recommended: `v4.0.0-rc.2`, commit
a5ce0307df90b558f5dc584d6591b3dc57f62fce, whose tree equals tC4 `main` b1d1f377). PLAN, the
baseline's manifest, the vendored catalogs, CSS and help pages all name that one commit, and a
test fails if they disagree. The 62 baseline screens and the gallery are captured again there.
Every "reference change" row of `docs/TC4-ISSUES.md` is built with its test or moved to its
owner card.

## Why it matters to a translator

tC4 4.0's newest screens and words are what translators will meet in tC4. The rebuild should
look and behave like them: folding rail groups and "By figure" in Check (#769), the current help
pages, and the newest interface words in all ten languages.

## Before you start

- `ls .audit/roadmap/rulings/ | grep H7` prints the ruling file. Without it, stop.
- `python3 -I .audit/tools/board.py | grep -E '^\| SWEEP '` shows SWEEP `done`.
- `sed -n 38,43p docs/PLAN.md` names `d310bf3` as the reference.
- `python3 -I -c "import json;print(json.load(open('parity/tc4-baseline/manifest.json'))['tc4'])"`
  prints commit `d310bf3…` and version `4.0.0-rc.1`.
- `sed -n 3p crates/tc-ui/vendored-from-tc4.txt` prints `commit d310bf3…`;
  `sed -n 4p crates/tc-ui/help-pages.txt` prints `commit cdfdf9b…`.
- `git -C ~/unfoldingword/tc4-sweep rev-parse 'v4.0.0-rc.2^{commit}'` prints `a5ce0307…` (SWEEP
  made that clone). `git -C ~/unfoldingword/tc4-sweep show
  'v4.0.0-rc.2:scripts/fetch-help.mjs' | grep TC_WEBSITE_REV` prints `dbb7cd03…`.
- `git -C ~/unfoldingword/tc4-sweep diff --stat d310bf3 v4.0.0-rc.2 -- src/i18n src/ds src/ui.css`
  shows 10 catalog files and no CSS change. In `src/i18n/en.json` 11 lines are added and 1
  removed (`git -C ~/unfoldingword/tc4-sweep diff d310bf3 v4.0.0-rc.2 -- src/i18n/en.json`).
- `grep -n '| reference change |' docs/TC4-ISSUES.md` lists the rows this card owns (9 on
  resume-build-lane at f9067bc: #782, #768, #773, #769, #763 twice, #766, #791, D104; SWEEP
  may add #810).
- Never check out or fetch in `~/unfoldingword/translationcore4` (it stays at d310bf3) or
  `~/unfoldingword/tc4-ref`.

## Funnel card

- **Invariants**:
  - INV-THE-REFERENCE-IS-ONE-COMMIT: `docs/PLAN.md` "The reference version",
    `parity/tc4-baseline/manifest.json` `tc4.commit`, the `commit` line of
    `crates/tc-ui/vendored-from-tc4.txt`, and the help pin (`crates/tc-ui/help-pages.txt`
    equal to tC4's `TC_WEBSITE_REV` at that commit, recorded in the baseline README) agree.
  - INV-NO-ROW-WAITS-FOR-A-PAST-MOVE: after the move, no row of `docs/TC4-ISSUES.md` says
    "reference change" for a change the new reference holds.
- **Wrong states**:
  - PLAN names one tC4 commit while the screens, catalogs or help pages come from another.
  - Rows wait for "the reference move" that has happened (#769, #763 and others).
  - rc.2 shows poem lines as tabs (#810); copying that would go against the ruling the owner
    gives in H7 (recommended: keep showing `\q` markers).
- **Code**:
  - Types: none new.
  - One entry point: one test reads the four records above and compares them.
  - Removed: "reference change" rows that are built; PLAN bullets that the new reference
    makes plain parity (the Q15 bullet at about line 60, once #798 is in the reference).
  - Left to checks, and why: the vendored bytes themselves are already checked
    (`crates/tc-ui/tests/vendored_assets.rs`, `crates/tc-ui/tests/help_pages.rs`); only the
    agreement between the records is new.
- **Checks**:
  - `the_reference_is_one_commit_everywhere` (to create) in
    `parity/tc4-baseline/baseline.test.mjs`; kind: ledger; runs in CI's `parity-baseline` job.
    If CHK-6 already added a PLAN-and-manifest test, extend that one instead.
  - The #769 tests named in the row (step 4) in tc-check, tc-helps and
    `parity/app/journeys/check-picker.test.mjs` or `check-keyboard.test.mjs`; CI for the Rust
    ones; journeys local until G5.
  - `help_home_shows_no_rc1_build_text_offline` (to create) in
    `parity/app/journeys/help.test.mjs`; journey.
  - `help_pages_name_the_real_projects_folder` (to create) in `crates/tc-ui/tests/help_pages.rs`;
    kind: unit; CI.
  - `parity/app/journeys/bundled-upgrade.test.mjs` (to create; the port of tC4's
    `e2e/bundled-upgrade.spec.ts`); journey.
  - app-language case k (#650) in `parity/app/journeys/app-language.test.mjs`; journey.
  - Proof it can fail: change one of the four records by one character, run the test, paste
    the failure in the log, restore it.
- **Prose**: `docs/PLAN.md` "The reference version" and "Where the rebuild differs from tC4
  4.0 on purpose"; `parity/tc4-baseline/README.md`.
- **Human**: H7 (`ROADMAP.md`). Recommendation: move to rc.2 now, keep showing `\q` markers
  for poem lines, and move again at v4.0.0.

## Steps

### Step 1: Record the new reference and prepare a tC4 clone at it

- Do:
  1. Write `the_reference_is_one_commit_everywhere` first; it passes on d310bf3.
  2. Rewrite `docs/PLAN.md` "The reference version" (about 38-43): the reference is the
     commit H7 names, with its tag, date and tree; the next move is at `v4.0.0`.
  3. Make a working clone outside the repository: `git clone --no-checkout
     ~/unfoldingword/tc4-sweep ~/unfoldingword/tc4-rc2` (to create), then
     `git -C ~/unfoldingword/tc4-rc2 checkout --detach a5ce0307df90b558f5dc584d6591b3dc57f62fce`,
     then in it `npm ci` and `node scripts/fetch-help.mjs`.
- Files: `docs/PLAN.md`, `parity/tc4-baseline/baseline.test.mjs`.
- Done when: `git -C ~/unfoldingword/tc4-rc2 rev-parse HEAD` prints the commit; the new test
  fails now (PLAN moved, the rest did not) with a message naming the record that differs.
- Record: log line `step 1` with the failing message; `next` to step 2.

### Step 2: Vendor the catalogs, CSS and help pages from the new reference

- Do:
  1. `crates/tc-ui/vendor-tc4.sh ~/unfoldingword/tc4-rc2 a5ce0307df90b558f5dc584d6591b3dc57f62fce`.
  2. `crates/tc-ui/vendor-help.sh dbb7cd03c657637ec97ee4c0f00ed7bae44698d9` (it needs the
     network; tests never run it).
  3. Build. If a key the rebuild uses was removed or renamed, the `keys::` constant fails to
     compile; move each call site to the new key.
  4. Apply Q31 (b) (`docs/TC4-DELTA-2026-10-19.md` Q31): Help's banner never names a tC4
     build; Help and About name one version. The vendored help files stay byte for byte
     (`crates/tc-ui/tests/help_pages.rs`); choose where the rebuild hides or replaces the line
     and record the choice in the log. Write `help_home_shows_no_rc1_build_text_offline`.
  5. Correct the two help pages that describe tC4, not this app, as a declared difference
     (PLAN "differs on purpose"; H16 in `ROADMAP.md` asks for it): `st-where-projects-live.html`
     (:129-131) names `~/pankosmia/tc4-projects` and this app keeps projects in
     `~/translationCore4/projects` (`crates/tc-app/src/config.rs:17-19`); `gs-install.html`
     (:129) points at translationCore4's releases and `start-tc4.sh`. Replace the folder table
     and the install paragraph with this app's (the download page is
     https://translationcore.com/tc4, `rulings/2026-10-22-H16.md` part 3; no placeholder is
     needed; or, for anything the ruling leaves open, a
     placeholder the ruling fills in later), as an overlay the vendor script re-applies so the
     pinned vendored copies stay byte for byte. Write `help_pages_name_the_real_projects_folder`
     in `crates/tc-ui/tests/help_pages.rs`: the served pages hold `translationCore4/projects`
     and not `pankosmia/tc4-projects` (also in `search-index.json`).
  6. Port tC4's `e2e/bundled-upgrade.spec.ts` (the bundle is re-vendored in this step) as
     `parity/app/journeys/bundled-upgrade.test.mjs`: an app whose bundled resources are newer
     than a project's pins offers the upgrade on open. CHK-3's coverage map names this file.
- Files: `crates/tc-ui/assets/`, `crates/tc-ui/vendored-from-tc4.txt`, `crates/tc-ui/help/`,
  `crates/tc-ui/help-pages.txt`, call sites of changed keys, `parity/app/journeys/help.test.mjs`.
- Done when: `cargo test -p tc-ui --test vendored_assets --test help_pages --test i18n_keys
  --test i18n_catalogs` passes; `sed -n 3p crates/tc-ui/vendored-from-tc4.txt` prints the new
  commit; `grep -rn 'pankosmia/tc4-projects' crates/tc-ui/help` prints nothing once the overlay
  is applied (or only the pinned vendored copy, if the overlay lives beside it: then the test
  reads the served page); `node --test parity/app/journeys/bundled-upgrade.test.mjs` passes.
- Record: log line `step 2` with the keys added and removed; `next` to step 3.

### Step 3: Capture the baseline and the gallery again

- Do: follow `parity/tc4-baseline/README.md` "Capture another tC4 commit" with
  `TC4_ROOT=~/unfoldingword/tc4-rc2` on each command, run from `parity/tc4-baseline`:
  `npm run capture`, then `node ../app/state.mjs --record`, then `npm run check`. If the check says tC4's fixtures changed, copy the folders
  `REPO_COPIES` lists in `parity/app/state.mjs` into `fixtures/tc4/` and record their commit in
  `fixtures/README.md`. Capture the view gallery as `parity/view-gallery/README.md` says.
  Update the baseline README: commit, totals, and a section "Changes since d310bf3" listing each
  screen that changed (keep the old images under `history/d310bf3/`). Then
  `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record` and review `git diff
  parity/app/report.json` row by row.
- Files: `parity/tc4-baseline/`, `parity/app/state.sha256.json`, `parity/view-gallery/`,
  `fixtures/tc4/` (only if tC4's fixtures changed), `parity/app/report.json`.
- Done when: in `parity/tc4-baseline`, `npm run check` passes; `the_reference_is_one_commit_everywhere`
  passes; `cargo test --workspace` passes.
- Record: log line `step 3` with the count of changed screens; `next` to step 4.

### Step 4: Build each reference-change row

- Do: for each row from `grep -n '| reference change |' docs/TC4-ISSUES.md`:
  1. #769 (rail groups that fold, and "By figure"): build it with the row's five tests,
     `rail_groups_by_figure_in_first_note_order`, `group_counts_ignore_the_filter`,
     `fold_survives_a_restart`, `next_into_a_folded_group_unfolds_it` and
     `fold_all_keeps_the_active_group_open`, red first. Read `docs/TC4-DELTA-2026-10-19.md`
     §3.12.1 and tC4's `src/views/Check.jsx` and `src/state.jsx` `setRailFolded`. Update
     `docs/APP-DESIGN.md` §3.8 and §4.9.
  2. #763 (help pages; two rows, about 268 and 352): done in step 2; set both to covered
     with `help_home_shows_no_rc1_build_text_offline`.
  3. #782, #768, #773: their tests exist; set the rows to covered.
  4. #766: `open_writes_creation_pins_only_when_none_exist` exists (C1a's D11); set covered.
  5. #791 (D108 per-verse rows) and `5fefba8 (D104)` (8 MiB journal pieces): change their
     status to "unit todo" with the unit E1a and I, and add a `note` log line to each of those
     cards if it is unclaimed.
  6. #810 (poem lines as tabs): follow H7's ruling. With the recommendation, keep `\q`
     markers and add a declared difference in step 5. If the owner chose tabs, stop: that is a
     new card.
  7. Port app-language case k (#650; tC4 `e2e/app-language.spec.ts` about 890) and the
     in-project halves of help a and app-language h and j (`docs/APP-DESIGN.md` about 3071).
- Files: tc-check, tc-helps, `crates/tc-ui/src/views/check/rail.rs`, `crates/tc-app/src/check/`,
  the journeys above, `docs/APP-DESIGN.md`.
- Done when: each new Rust test passes and fails with its fix stashed; the journeys pass
  locally (`TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test
  parity/app/journeys/<file>.test.mjs`); `grep -c '| reference change |' docs/TC4-ISSUES.md`
  prints 0.
- Record: log line `step 4`; `next` to step 5.

### Step 5: Update the declared differences and tell the owner

- Do:
  1. In `docs/PLAN.md` "Where the rebuild differs from tC4 4.0 on purpose": add the rc.2
     differences the rebuild keeps (poem lines as markers, #810; no review mark when a link goes
     back to the bank, against tC4 D106; the export keeps a verse's bytes and names it,
     against D105). Remove bullets the new reference makes plain parity (Q15 at about 60).
  2. Run `node --test parity/tc4-baseline/baseline.test.mjs`: the README's screen rulings must
     still be recorded in PLAN.
  3. Tell the owner in two or three plain sentences what changed for translators.
- Files: `docs/PLAN.md`, `parity/tc4-baseline/README.md`, `docs/TC4-ISSUES.md`.
- Done when: the baseline test passes and the owner was told.
- Record: log line `step 5`; set `state: verifying`.

## Gates before merge

The full gate list: `scripts/gates.sh` once INT-3 lands. Until then, each must pass:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and
  again with `--features gallery`, `--features app` and `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`
- `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`

Plus: in `parity/tc4-baseline`, `npm run check`; the help, app-language and Check journeys run
locally with their summary lines in the log. Captures are taken on the owner's Linux PC (the
machine fingerprint in `manifest.json`).

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- H7 has no ruling file.
- The owner chose tabs for poem lines (#810): that is a new card, not a step here.
- A new tC4 screen in rc.2 needs a translator-visible choice no H question covers.
- tC4 tags `v4.0.0` while you work: finish at rc.2, then say so in the log.

## References

- `.audit/roadmap/evidence/2026-10-21/tc4-coverage.md` F5, F13, §3 C
- `.audit/roadmap/evidence/2026-10-21/done-definition.md` DOD-8
- `.audit/roadmap/evidence/2026-10-21/tc4-bugs.md` T9
- `parity/tc4-baseline/README.md` "Capture another tC4 commit"; `crates/tc-ui/vendor-tc4.sh`;
  `crates/tc-ui/vendor-help.sh`
- `docs/TC4-DELTA-2026-10-19.md` §3.12.1, Q31

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/tc4-coverage.md
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
