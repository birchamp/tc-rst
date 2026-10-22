---
id: SWEEP
title: Sweep tC4 again, then every week
track: A
lane: integration
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d, then about 1 h a week
merged: none
updated: 2026-10-22
next: Claim this card after INT-4 merges, then do step 1.
blocked_by: none
---

# SWEEP: Sweep tC4 again, then every week

## Goal

`docs/TC4-ISSUES.md` has a row for every tC4 change and issue up to tC4 `main` b86270f and
`release/4.0` 9f06f99 (or the newer tips on the day you run it). Each row names its status, the
card that owns it and its regression test. A new `docs/TC4-DELTA-<date>.md` lists what changed.
After that, the sweep runs once a week and leaves one log line here.

## Why it matters to a translator

tC4's fixes are lessons about what goes wrong for translators. A tC4 bug that no row tracks can
come back in the rebuild unnoticed. Today tC4 has moved 98 commits past the last sweep, and the
rebuild tracks none of the 16 new or changed issues except #792.

## Before you start

- `git merge-base --is-ancestor f9067bc HEAD && echo landed` prints `landed` (INT-4 is in).
- `git -C ~/unfoldingword/translationcore4 rev-parse --short HEAD` prints `d310bf3`. That clone
  is the read-only reference. Never fetch, pull or check out anything in it.
- `git -C ~/unfoldingword/tc4-ref rev-parse --short HEAD` prints `d310bf3`: another clone at the
  reference. Do not touch it either.
- `sed -n 1,16p docs/TC4-ISSUES.md` says the last sweep stopped at tC4 `main` 9ad4d5c on
  2026-10-19. `docs/TC4-DELTA-2026-10-19.md` is that sweep's delta.
- `grep -n "2026-10-19\|2026-10-01\|d310bf3\.\.\|'storage'" .audit/workflows/tc4-issue-sweep.js`
  prints the fixed dates, output file name, start commit and old storage branch that step 2
  replaces. `claude/focused-ride-5uzj7p` in it is main's real name; keep it.
- `gh auth status` succeeds. You use `gh` only to read tC4's issues. Never comment, label,
  close or react.

## Funnel card

- **Invariants**:
  - INV-EVERY-TC4-CHANGE-HAS-A-ROW: every first-parent merge on tC4 `main`, every commit on
    `release/4.0`, and every tC4 issue opened or updated since the last sweep has a row in
    `docs/TC4-ISSUES.md` (or a line in the delta's not-applicable list) with its status, owner
    card and test.
  - INV-THE-SWEEP-NAMES-ITS-RANGE: the top of `docs/TC4-ISSUES.md` holds one line that names the
    swept `main` commit, the swept `release/4.0` commit and the issue date, and the next sweep
    starts from that line.
- **Wrong states**:
  - New tC4 changes (#807, #812, #813, #820, #821, #822, #825, #830 to #837, the 646-line
    `e2e/sync-refusal.spec.ts`, D105, D108) have no row, so no card builds their tests.
  - `docs/PLAN.md` (about 91-100) says tC4 4.0 moves a link "unannounced" and shows "the stored
    cards as they are"; rc.2 (#815, #816) no longer does either.
  - Rerunning the workflow as it is writes over `TC4-DELTA-2026-10-19.md`, because the file
    name is fixed in the script.
  - A fetch into the reference clone silently moves the rebuild's reference.
- **Code**:
  - Types: none; this item changes documents and one workflow script.
  - One entry point: the range line at the top of `docs/TC4-ISSUES.md` (to create). The
    workflow and the by-hand stages both read their start commits and date from it.
  - Removed: the fixed dates, the fixed output file name, the fixed start commit `d310bf3`
    and the old `storage` branch in `.audit/workflows/tc4-issue-sweep.js`.
  - Left to checks, and why: whether tC4 moved since the last sweep needs the network, so CI
    cannot check it. The weekly log line holds that rule.
- **Checks**:
  - CHK-3's TC4-ISSUES ledger test (every test a covered row names exists; no "rebuild bug" row
    for a fixed bug); kind: ledger; runs in CI once CHK-3 lands.
  - `tc4_issues_names_its_swept_range` (to create) beside CHK-3's TC4-ISSUES test: the range
    line exists and holds two 40-hex commits and a date. Kind: ledger; CI. If CHK-3 has not
    landed, write it as a `node --test` case in `parity/tc4-baseline/baseline.test.mjs`,
    which CI's `parity-baseline` job runs.
  - Proof it can fail: delete the range line, run the test, paste the failure in the log,
    restore the line.
- **Prose**: `.audit/workflows/README.md`: the sweep's arguments and the weekly rhythm.
- **Human**: none new. Rows that need the owner point at H7 (#810), H13 (#822) and H14
  (#837) in `ROADMAP.md`.

## Steps

### Step 1: Make a sweep clone outside the reference clone

- Do:
  1. `git clone --no-checkout https://github.com/unfoldingWord/translationcore4
     ~/unfoldingword/tc4-sweep` (to create).
  2. `git -C ~/unfoldingword/tc4-sweep fetch --tags origin`.
  3. Never check out a branch there; read with `git show <ref>:<path>`, `git log` and
     `git diff`.
- Files: none in the repository.
- Done when: `git -C ~/unfoldingword/tc4-sweep rev-parse 'v4.0.0-rc.2^{commit}'` prints
  `a5ce0307df90b558f5dc584d6591b3dc57f62fce`; `git -C ~/unfoldingword/tc4-sweep log
  --first-parent --oneline 9ad4d5c..origin/main | wc -l` prints 18 or more; and
  `git -C ~/unfoldingword/translationcore4 rev-parse --short HEAD` still prints `d310bf3`.
- Record: a log line `step 1` with the two tips (`origin/main`, `origin/release/4.0`); set
  `next` to step 2.

### Step 2: Make the workflow take its range as arguments

- Do:
  1. In `.audit/workflows/tc4-issue-sweep.js`, add arguments `base` (the last swept `main`
     commit), `releaseBase` (the last swept `release/4.0` commit, or `base`), `since` (the
     issue date) and `date` (today, for file names). Use them for the delta's range (in place
     of `d310bf3..${TIP}`), the issue search (`updated:>=${since}`) and the output name
     (`TC4-DELTA-${date}.md`).
  2. In its context text, keep `claude/focused-ride-5uzj7p` (that is main). Replace the
     `storage` branch with: `docs/STORAGE.md` on main once S2 has merged, else on
     `s2-proof-executor`. Replace the unit list with `python3 -I .audit/tools/board.py`.
  3. Keep the rule that the reference clone is never checked out.
  4. Add a line for the arguments to the `tc4-issue-sweep.js` row of
     `.audit/workflows/README.md`.
- Files: `.audit/workflows/tc4-issue-sweep.js`, `.audit/workflows/README.md`.
- Done when: `grep -n "2026-10-19\|2026-10-01\|d310bf3\.\.\|'storage'" .audit/workflows/tc4-issue-sweep.js`
  prints nothing, and `grep -c 'claude/focused-ride-5uzj7p' .audit/workflows/tc4-issue-sweep.js`
  prints at least 1.
- Record: log line `step 2`; `next` to step 3.

### Step 3: Sweep 9ad4d5c to the tips

- Do: run the workflow with `root` the repository, `tc4` `~/unfoldingword/tc4-sweep`, `out`
  a folder under your scratch directory, `tip` `origin/main`, `base` `9ad4d5c`,
  `releaseBase` `9ad4d5c`, `since` `2026-10-19` and `date` today. Without the Workflow tool,
  do its stages by hand:
  1. `git -C ~/unfoldingword/tc4-sweep log --first-parent --format='%h %s' 9ad4d5c..origin/main`
     (18 merges at b86270f, from #785 to #827).
  2. `git -C ~/unfoldingword/tc4-sweep log --format='%h %s' 9ad4d5c..origin/release/4.0`
     (36 commits at 9f06f99). The release branch gets fixes by cherry-pick: match each to its
     `main` PR by the PR number in the message.
  3. `gh issue list -R unfoldingWord/translationcore4 --state all --search "updated:>=2026-10-19"
     --limit 200 --json number,title,state,updatedAt`, then `gh issue view <n> -R
     unfoldingWord/translationcore4 --comments` for each.
  4. Classify each with the six statuses of the "Status key" in `docs/TC4-ISSUES.md`. For
     "covered", name the test and check it exists. For "rebuild bug", name a reachable scenario.
  5. `git -C ~/unfoldingword/tc4-sweep diff 9ad4d5c origin/main -- src/i18n/en.json` lists the
     new interface strings (6 in each of the 10 catalogs at b86270f). They reach the rebuild
     through REF1.
- Files: your scratch folder only.
- Done when: every merge, release commit and issue from the three lists has one classified line
  in your scratch notes (count them: the three `wc -l` totals match).
- Record: log line `step 3` with the counts by status; `next` to step 4.

### Step 4: Write the rows, the delta and the PLAN corrections

- Do:
  1. Write `docs/TC4-DELTA-<date>.md` in the shape of `docs/TC4-DELTA-2026-10-19.md`.
  2. Add the rows to `docs/TC4-ISSUES.md`, each in its area table. At least these (from the
     2026-10-21 reading; your step 3 may add more):
     #807 (span save behind a refused alignment save: likely not applicable, else E1a);
     #813 and D110 with #821 (I: a burrito with no `vrs.json`); #822 (I, owner H13);
     #812 (T: a corpus test that every bundled ULT and UST verse body holds every
     `target_words` word, with 1SA 2:2, 1SA 4:1, REV 7:12 and MAT 27:9 saved whole);
     #820 (T: the "Dios Dios" to "Dios Pablo" vector and the `follow` proptest);
     `e2e/sync-refusal.spec.ts` (E1a port list); D108 per-verse rows from #806 (E1a note);
     D105 export skip wording from #809, #811, #814 (D2); #830, #832, #836 (FIX-R);
     #825 (T and FIX-R); #831 (FIX-R step 5, since C2 is built: test
     `resume_card_names_the_verse_of_its_text_and_never_shows_the_stub`); #833 (.1 FIX-R,
     .2 G1 and D2, .3 F1, .4 G1, .5 FIX-R step 5, since B9a is built); #835 (F1); #837 (E1b, owner H14);
     #810 (REF1, owner H7); #818, #823, #827, #828, #829, #834 (not applicable).
  3. Add the range line at the top: `Swept: tC4 main <40-hex>, release/4.0 <40-hex>, issues
     updated to <date>.` Update "Latest tC4 state" and recount the Counts table.
  4. Rewrite the `docs/PLAN.md` declared differences that now describe tC4 wrongly (about
     91-100): after #815 (D106) tC4 follows a link by position and marks the verse for
     review; after #816 (D107) tC4 repairs source cards from the pinned verse; #806 (D108)
     ships per-verse rows. Keep the rebuild's own behavior as it is.
  5. Add each new test to its card: for a card in `todo` with `owner: none`, add the test to its
     funnel card's Checks with "(to create)" and a `note` log line; change nothing in its front
     matter. For a claimed card, do not
     edit it: its row in `docs/TC4-ISSUES.md` names the card, and your log line lists it.
  6. FIX-R edits the same two documents. Whichever merges second rebases and recounts.
- Files: `docs/TC4-DELTA-<date>.md` (to create), `docs/TC4-ISSUES.md`, `docs/PLAN.md`, the
  unclaimed cards in `.audit/roadmap/units/`.
- Done when: `for n in 807 810 812 813 820 821 822 825 830 831 832 833 835 836 837; do grep -q
  "| #$n " docs/TC4-ISSUES.md || echo "missing #$n"; done` prints nothing;
  `tc4_issues_names_its_swept_range` passes and fails without the line; `node --test
  parity/tc4-baseline/baseline.test.mjs` passes (it reads PLAN's rulings).
- Record: log line `step 4`; set `state: gating`, `next` to the merge.

### Step 5: Merge, then sweep every week

- Do:
  1. Merge as `TRACKING.md` says. Set `state: done` and `merged`. REF1 becomes ready then.
  2. Every week after, on the same weekday: fetch the sweep clone, read the range line, run
     steps 3 and 4 from it, and merge. If nothing moved, change no document. Also check whether
     tC4's `main` holds the moved-notice support the rebuild asked for
     (`docs/TC4-REQUEST-MOVED-NOTICE.md`): `git -C <sweep clone> grep -n "tc-moved\|moved.json" main`;
     the first week it prints, note it on F1's and X's cards (their straggler steps change), and
     on `ROADMAP.md` H16. The integration lane owns this weekly sweep; the board warns
     `SWEEP: last weekly line is older than 8 days` until the line is appended.
  3. Each week append one log line to this card, even on a done card:
     `- <date> | <who> | note | <commit or none> | weekly sweep: main <sha>, release/4.0 <sha>,
     <n> rows added | docs/TC4-DELTA-<date>.md or "no change"`, and set `updated`.
  4. If tC4 tags `v4.0.0`, say so in that line and in a note to REF1's owner: REF1 moves the
     reference again.
- Files: as in step 4.
- Done when: the first weekly line is in this log.
- Record: the weekly line itself.

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

Plus: `node --test parity/tc4-baseline/baseline.test.mjs` (PLAN's declared differences feed
it), and CHK-3's TC4-ISSUES test if CHK-3 has landed.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A tC4 change makes the rebuild choose between tC4's behavior and what a translator needs,
  and no H question in `ROADMAP.md` covers it. Write the question with options, their effect
  and a recommendation; do not decide it.
- `gh` or the clone needs a credential you do not have. Never record a token.
- tC4 tags `v4.0.0`: tell the owner in one sentence; REF1 will move the reference.

## References

- `.audit/roadmap/evidence/2026-10-21/tc4-coverage.md` F6, §3 C
- `.audit/roadmap/evidence/2026-10-21/tc4-bugs.md` §4, T8, T9
- `.audit/roadmap/evidence/2026-10-21/app-remaining.md` F13
- `docs/TC4-DELTA-2026-10-19.md`, `docs/TC4-ISSUES.md`, `.audit/workflows/tc4-issue-sweep.js`

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/tc4-coverage.md
