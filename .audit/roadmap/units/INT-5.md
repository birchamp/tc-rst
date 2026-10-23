---
id: INT-5
title: Converge the trackers onto the roadmap
track: A
lane: integration
milestone: M0
state: verifying
owner: integration lane (Claude Code 3d07d5f8)
branch: track-INT-5
worktree: .worktrees/track-INT-5
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d
merged: none
updated: 2026-10-23
next: Step 7: one verify pass, then wait for a merge turn from the orchestrator; S2 is not merged, so merge parked with next pointing at step 8.
blocked_by: none
---

# INT-5: Converge the trackers onto the roadmap

## Goal

`.audit/roadmap/` is the only live tracker. The old trackers say on their first line that they
are history. `.audit/HANDOFF.md` describes only the machine, the Windows VM, the reference
laptop and the tools. Two branches that append to `.audit/minors.tsv` or `.audit/rebuild.tsv`
merge without a conflict. CI runs `board.py --check`, so a card that contradicts git fails the
build. The watcher no longer rewrites STATUS.md or pushes main.

## Why it matters to a translator

A translator never sees it. It protects their work from agents that follow a stale tracker:
today STATUS.md on the lane says B11 is "ready to start" although it is merged, so an agent
could build it a second time, or skip a step that keeps text safe.

## Before you start

- `python3 -I .audit/tools/board.py` shows INT-4 as `done`.
- `git -C /home/bot/tc-rst merge-base --is-ancestor f9067bc claude/focused-ride-5uzj7p && echo in`
  prints `in` (the lane's trackers are on main).
- In your worktree, `ls .audit/STATUS.md .audit/PROGRESS.md .audit/plans/RESUME-2026-10-20.md
  .audit/plans/CONTINUE-REBUILD-2026-10-20.md .audit/plans/HANDOFF-resume-lane.md` lists all
  five, and `sed -n 4p .gitattributes` prints `* -text` (three comment lines come first).
- `scripts/gates.sh --list` prints INT-3's gate list; its `node` step runs
  `.audit/tools/*.test.mjs`, which does not include Python tests.
- INT-6 step 5 also edits `.audit/tools/watch-runs.py` and `.audit/HANDOFF.md`. Both cards are
  on the integration lane, so one owner does them in turn. Merge main into your branch before
  you merge, and keep the other card's change.
- `/tmp` was full on 2026-10-21. For local test runs set a temp folder on `/home`:
  `mkdir -p /home/bot/.cache/tc-tmp && export TMPDIR=/home/bot/.cache/tc-tmp`.

## Funnel card

- **Invariants**:
  - INV-TRACKER-TRUE: every card's front matter is valid and agrees with git and with the other
    cards; `board.py --check` holds it in CI.
  - INV-LOGS-NEVER-CONFLICT: two branches that each append rows to `.audit/minors.tsv` or
    `.audit/rebuild.tsv` merge without a conflict, and the result keeps every row.
  - INV-ONE-LIVE-TRACKER: every older tracker's first line says it is history.
- **Wrong states**:
  - An agent trusts STATUS.md ("37 of 48", B11 "ready to start") and redoes merged work.
  - Two branches conflict on the minors list (the RL × S2 merge has a 94-line add/add conflict
    in `.audit/minors.tsv`, process-integration §1.1).
  - The watcher's `status_tick` commits STATUS.md and pushes it straight to main with
    `git push origin HEAD:claude/focused-ride-5uzj7p`, outside every gate and the push rule.
  - HANDOFF.md says both "push main (standing authorization)" and, in the lane's handoff, "Do
    not push without the owner".
- **Code**:
  - Types: none; this is tooling and text.
  - One entry point: `python3 -I .audit/tools/board.py` for the picture of all work.
  - Removed: `status_tick` and `STATUS_SEEN` in `.audit/tools/watch-runs.py` (with its push to
    main); the "Runs and how to resume them" section of `.audit/HANDOFF.md`.
  - Left to checks, and why: card consistency and the merge driver are facts about files and
    git, not types; `board.py --check` and `union_test.py` hold them.
- **Checks**:
  - `.audit/tools/board_test.py` (exists); kind: unit; runs in: CI (step 5).
  - `test_two_branches_appending_to_an_audit_log_merge_without_conflict` in
    `.audit/tools/union_test.py` (to create); kind: unit over a scratch git repository; runs in:
    CI (step 5).
  - `python3 -I .audit/tools/board.py --check`; kind: consistency; runs in: CI (step 5), with
    full history (`fetch-depth: 0`), because a shallow clone turns "merged is not an ancestor"
    from an error into a warning.
  - Proof it can fail: delete the two `merge=union` lines and `union_test.py` fails with a
    message starting `INV-LOGS-NEVER-CONFLICT:`; set one card to `state: building` with
    `owner: none` and `board.py --check` exits 1 with `state is building but owner is none`.
- **Prose**: one line in `AGENTS.md` (step 4).
- **Human**: none.

## Steps

### Step 1: Make the append-only logs merge without conflicts

- Do: claim the card (`TRACKING.md`) in a worktree of its own:
  `git -C /home/bot/tc-rst worktree add -b track-INT-5 .worktrees/track-INT-5 claude/focused-ride-5uzj7p`.
  Work there. Write the test first:
  `.audit/tools/union_test.py` (to create), a `unittest` module in the style of
  `board_test.py`. For each of `.audit/minors.tsv` and `.audit/rebuild.tsv` (one `subTest`
  each) it: makes a temporary git repository (`tempfile.TemporaryDirectory`), copies the
  repository's `.gitattributes` into it, commits the log with a header row and one row on a
  branch `base`, makes branch `a` that appends row `A` and branch `b` that appends row `B`,
  then runs `git merge a` on `b`. It asserts the merge exits 0 and the file holds both rows,
  with the message `INV-LOGS-NEVER-CONFLICT: <path> conflicted; .gitattributes needs
  '<path> merge=union'`. Run git as `git -c user.name=t -c user.email=t@example.invalid ...`.
  Run it: it must fail. Then add these two lines to `.gitattributes`, after `* -text`:
  ```
  .audit/minors.tsv merge=union
  .audit/rebuild.tsv merge=union
  ```
- Files: `.gitattributes`, `.audit/tools/union_test.py` (to create).
- Done when: `python3 -I .audit/tools/union_test.py` prints `OK`; with the two lines removed it
  fails with `INV-LOGS-NEVER-CONFLICT:`; `head -4 .gitattributes | tail -1` prints `* -text`.
- Record: a log line `step 1` quoting the failing and the passing output.

### Step 2: Mark the old trackers as history

- Do: insert this as the new first line of each file below, followed by one blank line:
  `History: superseded by .audit/roadmap/ on 2026-10-21; do not update.`
  - `.audit/STATUS.md`
  - `.audit/PROGRESS.md`
  - `.audit/plans/RESUME-2026-10-20.md`
  - `.audit/plans/CONTINUE-REBUILD-2026-10-20.md`
  - `.audit/plans/HANDOFF-resume-lane.md`
  Delete nothing.
- Files: the five files above.
- Done when: this prints the line five times:
  `head -qn1 .audit/STATUS.md .audit/PROGRESS.md .audit/plans/RESUME-2026-10-20.md .audit/plans/CONTINUE-REBUILD-2026-10-20.md .audit/plans/HANDOFF-resume-lane.md`
- Record: a log line `step 2`.

### Step 3: Cut HANDOFF.md down to the machine and its tools

- Do, in `.audit/HANDOFF.md`:
  1. Replace the intro (the lines from "Written for the next session" to the end of the
     numbered reading list) with: "The machine, the Windows VM, the reference laptop and the
     tools. The plan and the live state of all work: `.audit/roadmap/README.md`. The push rule:
     `.audit/roadmap/ROADMAP.md`, section 'The push rule'."
  2. Delete "How the owner wants the work done". Before you delete it, check each bullet
     against `.audit/roadmap/README.md` "Rules that never bend". A bullet not found there
     becomes one line in that list. Expected: "Speed is judged against tC4 doing the same work
     on the reference laptop, not Word or Edge." The bullet "push main (standing
     authorization). Push only main and `storage`" is dropped: ROADMAP's push rule replaces it.
  3. Keep "The machine", "The Windows VM (builds and tests)" and "The reference laptop
     (measurements only)" as they are.
  4. Replace "Runs and how to resume them" with one line: "Work in flight: run
     `python3 -I .audit/tools/board.py` and read `.audit/roadmap/README.md`."
  5. Replace "What needs the owner" with: "Open owner questions: `.audit/roadmap/ROADMAP.md`,
     section 'What needs the owner'."
  6. Replace "Owner rulings (pointers)" with: "Rulings up to 2026-10-21: the `ruling` lines of
     `.audit/rebuild.tsv`. Rulings after it: `.audit/roadmap/rulings/`."
- Files: `.audit/HANDOFF.md`, `.audit/roadmap/README.md`.
- Done when: `grep -nE 'Runs and how to resume|STATUS\.md|PROGRESS\.md|standing authorization|Push only main' .audit/HANDOFF.md`
  prints nothing; `grep -c 'roadmap/README.md' .audit/HANDOFF.md` prints at least 1;
  `wc -l < .audit/HANDOFF.md` prints 80 or less.
- Record: a log line `step 3` naming any bullet moved to README.

### Step 4: Point AGENTS.md at the roadmap

- Do: in `AGENTS.md`, in the paragraph that says "Read `docs/PLAN.md` first.", add right after
  that sentence: "Plan and live state: `.audit/roadmap/README.md`; record your work as
  `.audit/roadmap/TRACKING.md` says." Rewrap the paragraph to the file's width (about 92
  characters).
- Files: `AGENTS.md`.
- Done when: `grep -c 'roadmap/TRACKING.md' AGENTS.md` prints 1, and the sentence follows
  "Read `docs/PLAN.md` first." in the same paragraph.
- Record: a log line `step 4`.

### Step 5: Run the board's checks in CI

- Do: add one step to the `STEPS` list of `scripts/gates.sh` (INT-3), after `node`:
  ```
  "board|python3 -I .audit/tools/board_test.py && python3 -I .audit/tools/union_test.py && python3 -I .audit/tools/board.py --check"
  ```
  In `.github/workflows/ci.yml`, job `parity-baseline`, add `- run: scripts/gates.sh --step board`
  after its `--step node` line, and give its checkout full history, because `board.py` reads
  ancestry:
  ```yaml
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
  ```
  INT-3's drift test (`.audit/tools/gates-sync.test.mjs`) requires every step to run in CI by
  name, so both edits go in the same commit.
- Files: `scripts/gates.sh`, `.github/workflows/ci.yml`.
- Done when: `scripts/gates.sh --step board` exits 0; `node --test .audit/tools/*.test.mjs`
  passes; with one card set to `state: building` and `owner: none` (not committed),
  `scripts/gates.sh --step board` exits 1 and prints `state is building but owner is none`.
  Undo that edit. After the merge and push, the CI log on main shows the `board` step.
- Record: a log line `step 5` with the failing output of the scratch check.

### Step 6: Retire the watcher's STATUS regeneration

- Do: in `.audit/tools/watch-runs.py`, delete `STATUS_SEEN`, the function `status_tick` and its
  call `status_tick()` in the main loop (lines 122-144 and 189 at f9067bc). Keep
  `windows_gate_tick` and the disk checks. Leave `.audit/tools/status.py` as it is (history).
  In `.audit/tools/README.md`: add rows for `board.py [--check] [--json] [--ref REF]`
  ("prints the roadmap board from the cards and git; `--check` fails on contradictions"),
  `board_test.py` and `union_test.py`; change the `status.py` row to "History: superseded by
  `board.py` on 2026-10-21; do not run it (it rewrites STATUS.md)"; in the `watch-runs.py` row
  delete the clause about regenerating and pushing STATUS.md.
- Files: `.audit/tools/watch-runs.py`, `.audit/tools/README.md`.
- Done when: `grep -nE 'status_tick|STATUS' .audit/tools/watch-runs.py` prints nothing;
  `python3 -I -c "import ast; ast.parse(open('.audit/tools/watch-runs.py').read())"` exits 0;
  `grep -c 'board.py' .audit/tools/README.md` prints at least 1.
- Record: a log line `step 6`.

### Step 7: Merge steps 1 to 6

- Do: merge `track-INT-5` into main as `TRACKING.md` "Checklist: merge an item" says, with the
  title `Merge INT-5: one tracker, and logs that never conflict` and the trailer `Unit: INT-5`.
  If S2 is already merged into main (`python3 -I .audit/tools/board.py` shows S2 `done`), do
  step 8 first and merge once, with `state: done`. If not, set `state: parked`, `next:` "When
  S2 is merged into main, merge main into track-INT-5 and do step 8.", and merge with that.
- Files: `.audit/roadmap/units/INT-5.md`.
- Done when: the merge is on main, CI is green on it, and `board.py --check` on main prints no
  error.
- Record: a log line `merge` (or `park`).

### Step 8: Fold S2's tracking files in, after S2 merges

- Do: after S2's merge, on main's state:
  1. `.audit/PROCESS.md`: add the same history first line as step 2.
  2. `.audit/HANDOFF.md`: if S2's merge kept its section "S2: where it stands", replace it with
     one line: "S2's history: `units/S2.md` and `.audit/plans/S2-PROOF-*.md`." Before that, add
     a log line `note` to `units/S2.md` naming the commit that still holds the old section
     (`git log -1 --format=%h -- .audit/HANDOFF.md`). S2 is done, so nobody else owns its card.
  3. `.audit/minors.tsv`: check that S2's rows are there once and the header row once:
     `grep -c $'^unit\tfinding\tevidence$' .audit/minors.tsv` prints 1, and each row of
     `git show s2-proof-executor:.audit/minors.tsv` is found by `grep -qxF`. Append any missing
     row; never delete one except a duplicate header.
  4. `.audit/tools/README.md`: add the rows S2 brought for `lane-state.mjs` and
     `run-gates.mjs` if the merge dropped them.
  Then merge as step 7 says, with `state: done`.
- Files: `.audit/PROCESS.md`, `.audit/HANDOFF.md`, `.audit/minors.tsv`,
  `.audit/tools/README.md`, `.audit/roadmap/units/S2.md` (log line only), this card.
- Done when: `head -n1 .audit/PROCESS.md` prints the history line; `grep -c 'S2: where it stands' .audit/HANDOFF.md`
  prints 0; the header count prints 1; no S2 row is missing.
- Record: a log line `step 8`, then `merge`.

## Gates before merge

`scripts/gates.sh` (INT-3), which from step 5 on includes the `board` step, and
`scripts/gates.sh --check-receipt` on the exact commit main moves to.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A history file is still being updated by a running session after step 2 (its new commits
  touch the file): log `block` and leave the file to that session's next boundary.
- A bullet of HANDOFF's owner section contradicts a rule in `.audit/roadmap/README.md`: keep the
  README's rule and ask the owner only if a translator would notice the difference.

## References

- `evidence/2026-10-21/process-integration.md` §3, §5 and findings F8, F9, F10, F11.
- `evidence/2026-10-21/checks-inventory.md` (CI jobs; finding C2 on gate drift).
- `TRACKING.md` "Where the truth lives", "Minors" and "The board".
- `FUNNEL.md` §5, row "The board's cards are consistent with git".
- `.audit/tools/watch-runs.py` `status_tick` (lines 124-144 at f9067bc).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/process-integration.md
- 2026-10-22 | roadmap session (Cursor, review of 2026-10-22) | note | none | Done early, on main, so a new session finds the roadmap (review T1, T7): step 2 for `.audit/STATUS.md` and `.audit/PROGRESS.md` (the three `.audit/plans/*` files are on `resume-build-lane` only; mark them when INT-4 lands them); step 3 items 1, 4, 5 and 6 and the worktree rows of HANDOFF.md (item 2 stays); step 4; the `board.py` and `board_test.py` rows of `.audit/tools/README.md` (step 6's row change). Steps 1, 5, 6's watcher work and 7 stay | .audit/roadmap/evidence/2026-10-21/review-2026-10-22.md
- 2026-10-23 16:43 EDT | integration lane (Claude Code 3d07d5f8) | claim | none | claimed on track-INT-5 from main eaf809d1 while INT-7 waits for its merge turn (the orchestrator's queue); steps 2, 3 (items 1, 4-6) and 4 were done early on main, per the 2026-10-22 note | python3 -I -B .audit/tools/board.py
- 2026-10-23 16:45 EDT | integration lane (Claude Code 3d07d5f8) | step 1 | none | union_test.py written; without the lines: `AssertionError: False is not true : INV-LOGS-NEVER-CONFLICT: .audit/minors.tsv conflicted; .gitattributes needs '.audit/minors.tsv merge=union'`; with the two merge=union lines after `* -text`: `OK`; line 4 is still `* -text` | python3 -I .audit/tools/union_test.py
- 2026-10-23 16:47 EDT | integration lane (Claude Code 3d07d5f8) | step 2 | none | history line added to the three .audit/plans files (STATUS.md and PROGRESS.md had it from 2026-10-22); head -qn1 prints it five times | head -qn1 .audit/STATUS.md .audit/PROGRESS.md .audit/plans/RESUME-2026-10-20.md .audit/plans/CONTINUE-REBUILD-2026-10-20.md .audit/plans/HANDOFF-resume-lane.md
- 2026-10-23 16:49 EDT | integration lane (Claude Code 3d07d5f8) | step 3 | none | item 2 done (items 1, 3-6 were done on 2026-10-22): HANDOFF's owner section deleted. Checked against README's rules: the fix rule, typed state machines, default models and the workflow-message rule were there; short plain questions are README "When to stop and ask". Moved to README as rules: "Speed is judged against tC4 … on the reference laptop" and "Hours, not days: one verification pass, the simplest change, reuse tested work, deterministic tests over reading code". The push bullet is gone; 49 lines | grep -nE 'Runs and how to resume|STATUS\.md|PROGRESS\.md|standing authorization|Push only main' .audit/HANDOFF.md
- 2026-10-23 16:49 EDT | integration lane (Claude Code 3d07d5f8) | step 4 | none | already on main since 2026-10-22: AGENTS.md's PLAN paragraph holds the roadmap sentence | grep -c 'roadmap/TRACKING.md' AGENTS.md
- 2026-10-23 16:53 EDT | integration lane (Claude Code 3d07d5f8) | step 5 | none | `board` step added after `node` in scripts/gates.sh, with `-B` on each python3 (without it board_test.py leaves .audit/tools/__pycache__, an untracked folder, and the gate then writes no receipt); ci.yml parity-baseline runs `--step board` with fetch-depth 0. `scripts/gates.sh --step board` exits 0; node --test .audit/tools/*.test.mjs: 7 pass, 0 fail (the drift test). With S3 set to building and owner none (not committed): exit 1, `[('error', 'S3', 'INV-TRACKER-TRUE: state is building but owner is none')]`; undone | scripts/gates.sh --step board
- 2026-10-23 16:56 EDT | integration lane (Claude Code 3d07d5f8) | step 6 | none | status_tick, STATUS_SEEN and the call deleted from watch-runs.py (windows_gate_tick and the disk checks kept; it parses); tools README: union_test.py row added, status.py row says do not run it, watch-runs.py row no longer regenerates or pushes STATUS.md | grep -nE 'status_tick|STATUS' .audit/tools/watch-runs.py
