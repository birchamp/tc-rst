---
id: INT-4
title: Land resume-build-lane on main
track: A
lane: integration
milestone: M0
state: claimed
owner: checks lane (Claude Code 2198ee19)
branch: track-INT-4
worktree: .worktrees/track-INT-4
depends: [INT-2, INT-3]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d plus about 1.5 h of gates (basis: the full test run took about 899 s of binaries plus builds)
merged: none
updated: 2026-10-23
next: step 1
blocked_by: none
---

# INT-4: Land resume-build-lane on main

## Goal

Main holds every app unit built on `resume-build-lane`: B4b, B5b, B7, B8, B11, C2, C3, C4 and
C5 (38 of 48 app units). Main sits at a commit that passed the full gate list with the
journeys, the alignment budget and the Windows VM gate, and GitHub CI is green on it. The resume
lane's B11 records are finished first, so no minor is lost.

## Why it matters to a translator

Nine finished parts, among them the conflict review and marks and comments on checks, reach the
main line, and every later item starts from the newest app. Today they sit on a branch that has
had no full gate since its B8, C4 and B11 merges, and no Windows run since 2026-10-19.

## Before you start

- `python3 -I .audit/tools/board.py` shows INT-2 and INT-3 as `done`.
- `/home/bot/tc-rst/scripts/gates.sh --list` prints the gate list (INT-3 made it).
- `git -C /home/bot/tc-rst rev-list --count resume-build-lane..claude/focused-ride-5uzj7p`
  printed 0 on 2026-10-21, so a fast-forward was possible then. Once the roadmap commit, INT-2
  and INT-3 are on main it is above 0, and the landing is a merge commit (step 2 checks for
  conflicts).
- `df -h /home` shows at least 120 GB free (estimate: merged worktrees' own build folders were
  31 to 61 GB each, `evidence/2026-10-21/process-integration.md` §2).
- `pgrep -fa 'cargo (test|clippy|build)'` shows no more than one other heavy run: at most two
  share this PC, and the storage lane has the first claim (`ROADMAP.md` "Capacity").
- `~/win11-vm.sh status` reports the Windows VM; `~/win11-vm.sh start` starts it.
- Never edit `.worktrees/resume-build-lane` or `.worktrees/resume-build-lane/target/lanes/b11`:
  the "tC4 Rebuild" session (Enjoy, `d8a954f9…`) owns them.

## Funnel card

- **Invariants**:
  - INV-NO-UNGATED-MERGE: main moves only to a commit that `scripts/gates.sh --check-receipt`
    accepts and on which the journeys, the alignment budget and the Windows VM gate passed.
  - INV-NOTHING-DROPPED: every commit of `resume-build-lane` up to BOUNDARY is an ancestor of
    main afterwards, and every row of `.audit/b5b-minors-pending.tsv` is in `.audit/minors.tsv`.
  - INV-CI-IS-THE-GATE: GitHub CI is green on main's new tip.
- **Wrong states**:
  - Main at a commit CI rejects: af4eef3 fails at "clippy for Windows" (run 39426828571).
  - B11 and C4 merged with no recorded passing gate (checks-inventory C3).
  - The five B5b minors lost because the pending file was never folded in.
  - Main moved while the lane session is still writing B11's records.
- **Code**:
  - Types: none. This item merges finished work; it writes product code only to fix a gate.
  - One entry point: a merge commit in `.worktrees/track-INT-4`, then
    `git merge --ff-only` in the main checkout after `--check-receipt` passes.
  - Removed: `.audit/b5b-minors-pending.tsv` (the lane session folds it in).
  - Left to checks, and why: "this tree passed every gate" is not a type; INT-3's receipt and
    GitHub CI hold it.
- **Checks**:
  - `scripts/gates.sh --journeys` (INT-3); kind: the gate list plus every journey, failing on
    any `# fail` or `# skipped` above 0; runs in: this worktree, and in CI by step.
  - `cargo test -p tc-align --release --test suggestion_budget -- --ignored --nocapture`;
    kind: budget (`ci.yml` job `alignment-budget`).
  - `.audit/tools/winvm/gates.ps1` through `.audit/tools/winvm/run.sh`; kind: Windows gates.
  - `git merge-base --is-ancestor $BOUNDARY claude/focused-ride-5uzj7p`; kind: command.
  - Proof they can fail: CI on af4eef3 is red at the Windows clippy step; CI run 39426828571's
    `parity-baseline` job skipped 142 browser tests, which the skip rule fails; `gates.ps1`
    prints a `---- <gate>` block for each failed gate.
- **Prose**: none.
- **Human**: the owner says yes before main moves and is pushed (step 4; the push rule is
  `README.md` "Rules that never bend"), because the lane's
  own instruction says "Do not push without the owner" (`.audit/plans/HANDOFF-resume-lane.md`
  line 7).

## Steps

### Step 1: Find the resume lane's boundary commit

- Do: make your worktree, and claim the card there (`TRACKING.md` "claim an item"):
  `git -C /home/bot/tc-rst worktree add -b track-INT-4 .worktrees/track-INT-4 claude/focused-ride-5uzj7p`.
  Every card commit of this item goes on `track-INT-4`. Wait for the "tC4 Rebuild"
  session to finish B11's records: it commits its two uncommitted B11 rows of
  `.audit/minors.tsv`, appends the five rows of `.audit/b5b-minors-pending.tsv` to it, deletes
  the pending file, and claims C7a in a commit whose subject starts `Claim C7a`. That claim
  commit is BOUNDARY. Check it, read-only, from the main checkout:
  ```bash
  cd /home/bot/tc-rst
  BOUNDARY=$(git log --format='%H %s' resume-build-lane | grep -m1 ' Claim C7a' | cut -d' ' -f1)
  echo "BOUNDARY=$BOUNDARY"
  git cat-file -e "$BOUNDARY:.audit/b5b-minors-pending.tsv" 2>/dev/null && echo "pending file still there"
  git show "$BOUNDARY:.audit/minors.tsv" | grep -c '^B11'
  git show f9067bc:.audit/b5b-minors-pending.tsv | while IFS= read -r row; do
    git show "$BOUNDARY:.audit/minors.tsv" | grep -qxF -- "$row" || echo "MISSING: ${row:0:80}"
  done
  ```
- The session has ended: the owner confirmed it on 2026-10-22 12:53, and the roadmap session
  committed the two B11 rows as WIP (`43496296` on `resume-build-lane`). So take the takeover
  path without asking: log `takeover` naming `d8a954f9`; in `.worktrees/resume-build-lane`
  append the five rows of `.audit/b5b-minors-pending.tsv` to `.audit/minors.tsv`, delete the
  pending file, commit with the subject `Claim C7a boundary: fold B5b minors` (no C7a claim is
  made: C7a is released to todo and starts from main after this card), and use that commit as
  BOUNDARY. The grep for ` Claim C7a` then finds it.
- Files: `.audit/roadmap/units/INT-4.md`.
- Done when: BOUNDARY is a sha; "pending file still there" is not printed; the `^B11` count is
  at least 2 (0 at f9067bc); no `MISSING:` line. Set `BOUNDARY=<sha>` again in every new shell.
- Record: a log line `step 1` with BOUNDARY.

### Step 2: Make the merge commit in a new worktree

- Do:
  ```bash
  cd /home/bot/tc-rst/.worktrees/track-INT-4
  git merge --no-edit claude/focused-ride-5uzj7p     # picks up main if it moved since step 1
  git merge-tree --write-tree --name-only HEAD "$BOUNDARY"; echo "exit $?"
  ```
  `exit 0`: no conflicts. `exit 1` lists the conflicted files. Expected:
  `.github/workflows/ci.yml` (the lane added 8 lines; INT-2 and INT-3 rewrote the file): keep
  INT-3's `scripts/gates.sh --step` form and every job of both sides, so
  `node --test .audit/tools/*.test.mjs` (INT-3's drift tests) passes. Also possible:
  `.audit/roadmap/units/C7a.md` and `S2.md`, if the lane merged main and claimed its card: take
  the lane's copy of `C7a.md` and main's copy of every other card. Then:
  ```bash
  git merge --no-ff --no-commit "$BOUNDARY"
  git commit -m "Merge resume-build-lane: B4b, B5b, B7, B8, B11, C2, C3, C4, C5" \
    -m "The app units built on the resume lane reach main after the full gate, the journeys and the Windows VM gate." \
    --trailer "Unit: INT-4" --trailer "Unit: B4b" --trailer "Unit: B5b" --trailer "Unit: B7" \
    --trailer "Unit: B8" --trailer "Unit: B11" --trailer "Unit: C2" --trailer "Unit: C3" \
    --trailer "Unit: C4" --trailer "Unit: C5" \
    --trailer "<the attribution line your session was given>"
  git log -1 --format='%(trailers:key=Unit,valueonly)'
  ```
  If you resolved a conflict by hand, run one verification pass over those files
  (`FUNNEL.md` §8) before step 3.
- Files: what the merge brings.
- Done when: the last command prints the ten unit ids; `git merge-base --is-ancestor
  "$BOUNDARY" HEAD && echo in` prints `in`; `git status --porcelain` prints nothing.
- Record: a log line `step 2` with the merge sha and the conflict list (or "none"), in a
  card-only commit on `track-INT-4`.

### Step 3: Run every gate, and fix only what the fix rule allows

- Do, in `/home/bot/tc-rst/.worktrees/track-INT-4`, in a shell that first runs `. ~/.tc-env`
  (`.audit/HANDOFF.md` "The machine": it caps the test threads and points at Playwright):
  1. `scripts/gates.sh --journeys`. If your `gates.sh` has no `--journeys`, run
     `scripts/gates.sh`, then the journeys as `TRACK-B.md` §5 step 4 shows.
  2. `cargo test -p tc-align --release --test suggestion_budget -- --ignored --nocapture`.
  3. From the main checkout, the Windows VM gate, then free the VM's 80 GB disk:
     ```bash
     cd /home/bot/tc-rst
     WINVM_DIR=int-4 .audit/tools/winvm/run.sh "$(git rev-parse track-INT-4)" \
       'powershell -NoProfile -ExecutionPolicy Bypass -File .audit\tools\winvm\gates.ps1'
     ssh -i ~/.ssh/tc-vm -p 2222 builder@127.0.0.1 \
       'powershell -NoProfile -Command "Remove-Item -Recurse -Force C:\src\int-4, C:\src\int-4.bundle"'
     ```
  Fix every blocker, and a major only when it harms a translator's data or what they see.
  Every other finding is a row in `.audit/minors.tsv`. Each fix is its own commit with a test
  that fails without it (`FUNNEL.md` §2); then run 1 to 3 again on the new tip. Keep fixes small:
  the lane session is changing the app for C7a.
- Files: only what a fix needs; `.audit/minors.tsv`.
- Done when: `gates.sh` prints `gates: all <n> steps passed` with the journeys run; the budget
  test passes; `gates.ps1` prints `exit 0` for `fmt`, `clippy`, `test` and `wasm` and
  `tests: <n> passed, 0 failed`. Copy each summary line for step 5.
- Record: summaries kept for step 5.

### Step 4: Ask the owner before main moves

- Do: write to the owner: "The newest app work (nine finished parts, among them the conflict
  review and marks and comments on checks) passed every check on this PC and on Windows. May I
  put it on the main line and send it to GitHub? Yes: everyone starts from the newest app, and
  GitHub's checks run on it. No: it stays on this PC for now. I recommend yes." Save the answer
  as `.audit/roadmap/rulings/<date>-int-4-main.md` (`TRACKING.md` "Rulings").
- On no: commit the ruling and the card on `track-INT-4` with `state: parked` and `next:` "The
  owner said not yet on <date>; ask again at the next integration boundary." Stop here.
- Files: the ruling file.
- Done when: the ruling file exists.
- Record: in step 5's commit.

### Step 5: Record, gate the final commit, move main and push

- Do: on `track-INT-4`, commit the ruling and the card: `state: done`, `merged: <BOUNDARY>`,
  log lines `gate` (one per summary line of step 3), `ruling` and `merge`.
  This commit changes only `.audit/roadmap/`, but INT-3's receipt is per tree, so run
  `scripts/gates.sh` once more on it (the build is cached; the tests take about 15 minutes,
  estimate from 899 s). Then:
  ```bash
  cd /home/bot/tc-rst/.worktrees/track-INT-4 && scripts/gates.sh --check-receipt
  cd /home/bot/tc-rst
  git status --porcelain            # nothing listed that the move would overwrite
  git merge --ff-only track-INT-4
  git push origin claude/focused-ride-5uzj7p
  gh run list --branch claude/focused-ride-5uzj7p --limit 1 --json databaseId,headSha,conclusion,url
  gh run watch <databaseId> --exit-status
  ```
  Never use `--force`. If `--ff-only` refuses, main moved: merge main into `track-INT-4` and
  repeat steps 3 and 5. If CI turns red on main, every merge stops: set `state: fixing` on a new
  branch, fix it with the same gates, and push.
- Files: `.audit/roadmap/units/INT-4.md`, the ruling file.
- Done when: `--check-receipt` exits 0; `git rev-parse claude/focused-ride-5uzj7p` equals
  `git rev-parse track-INT-4`; `git ls-remote origin refs/heads/claude/focused-ride-5uzj7p`
  prints the same sha; `gh run watch` exits 0.
- Record: the card commit above.

### Step 6: Hand over and clean up

- Do: lane-a8's sign-in cancel fix (58aa8a9, 599e516) is INT-6 step 2; nothing to do here. Do
  not message the lane session: it reads the board and merges main at its next boundary. Once
  CI is green, `git -C /home/bot/tc-rst worktree remove .worktrees/track-INT-4` (never
  `--force`); keep the branch. Put the CI run's URL in the next integration card's first log
  line, so main gets no ungated commit.
- Files: none.
- Done when: `python3 -I .audit/tools/board.py --check` on main prints no error for INT-4, and
  `git worktree list` no longer shows `.worktrees/track-INT-4`.
- Record: the next card's log line.

## Gates before merge

`scripts/gates.sh --journeys` (INT-3), plus:

- the alignment budget and the Windows VM `gates.ps1` (step 3);
- `scripts/gates.sh --check-receipt` on the exact commit main moves to (step 5);
- GitHub CI green on main's new tip.
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- The lane shows no boundary for 24 hours and you cannot tell whether its session runs.
- The merge conflicts in a file other than `.github/workflows/ci.yml`, `C7a.md` or `S2.md`: log
  `block` with the file list.
- `~/win11-vm.sh start` fails twice: under H1 (or the standing authorization for main), push the
  exact sha as a branch and run CHK-5's `nightly-windows` job on it by `workflow_dispatch`
  (INT-2 step 4 adds the job early for this reason); a green run on that sha counts as the VM
  gate; log the run id. If the job does not exist yet, log `block` naming it.
- A gate fails twice in a row and you cannot tell why.
- A fix would touch files the lane's C7a card has claimed.
- Before main moves and is pushed (step 4).

## References

- `evidence/2026-10-21/process-integration.md` §1.2 (M0, M1) and finding F1.
- `evidence/2026-10-21/app-remaining.md` "B11 close-out" and finding F1.
- `evidence/2026-10-21/checks-inventory.md` findings C1, C2, C3 and C4.
- `units/INT-3.md` (`scripts/gates.sh`, `--journeys`, `--check-receipt`).
- `TRACK-B.md` §5 step 4; `.audit/HANDOFF.md` "The Windows VM"; `.audit/tools/watch-runs.py`
  `windows_gate_tick` (how `gates.ps1` was started before).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/process-integration.md
- 2026-10-22 | roadmap session (Cursor) | note | 43496296 | the owner confirmed the resume lane session has ended; B11's loose rows are WIP 43496296; step 1 takes the takeover path | .audit/roadmap/units/C7a.md
- 2026-10-22 | roadmap session (Cursor) | note | ff648f95 | INT-1 ran its yes path (three branches pushed; log on branch track-INT-1, pushed too); step 2's merge takes track-INT-1 along (its only change is its own card: set INT-1 state done, merged ff648f95, log line merge) | .audit/roadmap/units/INT-1.md
- 2026-10-23 07:31 EDT | checks lane (Claude Code 2198ee19) | claim | none | Claimed on branch track-INT-4 in .worktrees/track-INT-4 (the checks lane takes the integration lane's card: nobody holds that lane, and INT-4 needs INT-2 and INT-3). INT-3's merge 5f2b69af is on main; its CI run 39946614391 is in progress and gates this card's merge, not its local steps. Main's previous tip 04a686fd was green on run 39942717141 | gh run view 39946614391
