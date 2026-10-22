---
id: INT-6
title: Rescue stray work and free the disk
track: A
lane: integration
milestone: M0
state: todo
owner: none
branch: none
worktree: none
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d
merged: none
updated: 2026-10-21
next: Claim this card after INT-4 merges, then do step 1.
blocked_by: none
---

# INT-6: Rescue stray work and free the disk

## Goal

Every uncommitted change in an old worktree is saved to a git ref, without touching that
worktree. lane-a8's sign-in cancel fix reaches main, with its gates. Old worktrees whose work is
on main, and their build folders, are removed after the owner says yes. The watcher can no
longer fill the root drive. The free space before and after is in the log.

## Why it matters to a translator

A translator gets tC4's sign-in behavior back: a cancel once the token calls started signs out
instead of leaving a dead sign-in that Door43 later refuses. The rest protects the work: the
root drive is 97% full, and a full disk fails builds and tests in the middle of a gate.

## Before you start

- `python3 -I .audit/tools/board.py` shows INT-4 as `done`.
- `df -h / /home` prints the root drive near full (2026-10-21: `/` 905G, 27G free, 97%;
  `/home` 916G, 213G free).
- `du -sh /var/tmp/tc-rst-target` (2026-10-21: 815G). It takes a few minutes.
- `git -C /home/bot/tc-rst worktree list | wc -l` (2026-10-21: 46 lines: the main checkout and
  45 worktrees).
- Never touch these; running sessions own them: `.worktrees/resume-build-lane`,
  `.worktrees/resume-build-lane/target/lanes/b11`, `.worktrees/s2-proof-executor`,
  `.worktrees/s2-fix-git`, `.worktrees/s2-fix-meta`, `.worktrees/s2-fix-proof`,
  `.worktrees/s2-review-exec`, `.worktrees/s2-commit-check`, `.worktrees/s2-proof-handoff`,
  the folder `.worktrees/s2-review-exec-evidence`, and `.claude/worktrees/s2-verify` (the lane's
  HANDOFF still names it as S2's home). Also keep `.worktrees/rebuild-process-improvements`
  (2fe47e2, one commit on no other branch; S2's tools replace it after M2). Also never touch a worktree that a card names in its
  `worktree:` field: `grep -h '^worktree:' .audit/roadmap/units/*.md | grep -v none`.
- Read only with `git --no-optional-locks -C <worktree> status`, so the check does not even
  refresh another worktree's index.

## Funnel card

- **Invariants**:
  - INV-NO-STRAY-WORK-LOST: before any worktree is removed, every uncommitted change, untracked
    files included, in a worktree no session owns is reachable from a ref under `refs/wip/`.
  - INV-REMOVE-ONLY-MERGED-AND-CLEAN: a removed worktree's HEAD is an ancestor of main and its
    `git status --porcelain` was empty; a removed build folder belonged to a removed worktree or
    to none.
  - INV-BUILDS-STAY-OFF-ROOT: no tool moves build folders onto the root drive.
- **Wrong states**:
  - quick-timing's 54 changed files and its new, untracked crate `crates/tc-timing/` are lost
    with its worktree; no tracking file mentions them (process-integration F5).
  - A fix a translator would see (58aa8a9) stays on a side branch forever (F4).
  - `watch-runs.py` `relocate_idle` checks only `/home` and moves build folders to `/var/tmp`
    on the root drive, which filled it (F7).
  - A worktree inside a `target/` folder is deleted by a `cargo clean` there (the lane's b11
    worktree lives in `.worktrees/resume-build-lane/target/lanes/b11`).
- **Code**:
  - Types: none for the clean-up. The fix itself is lane-a8's code. It fixes the A8 rows of
    `.audit/minors.tsv` that `.audit/plans/A8.md` (added in 58aa8a9) calls minors 6 and 8. No
    review record of these two commits was found, so step 2's test, reverted to fail, and the
    journeys are their proof.
  - One entry point: the deletion list of step 3; nothing is removed that is not on it.
  - Removed: `relocate_idle` and its call in `.audit/tools/watch-runs.py`; the worktrees and
    build folders the owner approves.
  - Left to checks, and why: a worktree's state is a fact on this disk, not a type; the
    commands of steps 1 and 3 hold it, and `git worktree remove` without `--force` refuses a
    dirty worktree.
- **Checks**:
  - `a_cancel_once_the_token_calls_started_forgets_the_token_an_earlier_session_kept` in
    `crates/tc-app/tests/app/account.rs` (comes with 58aa8a9); kind: unit through tc-app's
    API; runs in: CI, `cargo test --workspace`.
  - The journeys `account-menu.test.mjs`, `j11-sign-in.test.mjs` and `share-open.test.mjs`;
    kind: journey; runs in: this card's gate (and G5's CI job once it exists).
  - The ref check of step 1 and the list of step 3; kind: command; runs in: this card.
  - Proof they can fail: with `crates/tc-app/src/account.rs` of 58aa8a9 reverted
    (`git show <new sha> -- crates/tc-app/src/account.rs | git apply -R`), the account test
    fails; in a probe worktree with one changed tracked file, `git worktree remove` refuses
    (step 4).
- **Prose**: one row of `.audit/HANDOFF.md` "The machine" (step 5).
- **Human**: the owner says yes to the deletion list (step 3), because removing worktrees and
  build folders is hard to undo.

## Steps

### Step 1: Save every stray uncommitted change to a ref

- Do: claim the card (`TRACKING.md`) in a worktree of its own:
  `git -C /home/bot/tc-rst worktree add -b track-INT-6 .worktrees/track-INT-6 claude/focused-ride-5uzj7p`.
  Write `df -h / /home` into the log. List the worktrees that no session owns (see "Before you
  start") and that have changes: `git --no-optional-locks -C <wt> status --porcelain | wc -l`.
  On 2026-10-21 these were `.claude/worktrees/quick-timing` (55 lines: 54 changed files and the untracked
  `crates/tc-timing/`), `.claude/worktrees/lane-b7` (3) and `.claude/worktrees/lane-merge` (2,
  detached at a896279). For each one, with `<name>` its folder name and `<date>` today:
  ```bash
  cd /home/bot/tc-rst/.claude/worktrees/<name>
  idx="$(git rev-parse --git-dir)/int6-wip-index"
  cp "$(git rev-parse --git-path index)" "$idx"
  GIT_INDEX_FILE="$idx" git add -A
  tree=$(GIT_INDEX_FILE="$idx" git write-tree)
  rm -f "$idx"
  sha=$(git commit-tree "$tree" -p HEAD -m "WIP <name>: uncommitted files saved by INT-6 on <date>")
  git update-ref "refs/wip/<name>/<date>" "$sha"
  echo "$sha"
  ```
  This uses a copy of the index, so the worktree, its index and the stash list stay as they
  were. Do not use `git stash create`: it leaves out untracked files, and quick-timing's new
  crate is untracked.
- Append one row per saved ref to `.audit/minors.tsv`: unit `INT-6`, finding "<name> holds
  uncommitted work nobody has judged (<n> files)", evidence `refs/wip/<name>/<date>`.
- Files: `.audit/minors.tsv`, this card.
- Done when: `git for-each-ref refs/wip/` lists one ref per worktree; for each, with `wt` its
  path, this prints nothing (the ref holds exactly the changed and untracked files):
  ```bash
  diff <(git -C "$wt" diff --name-only HEAD "refs/wip/<name>/<date>" | sort) \
       <( (git --no-optional-locks -C "$wt" diff --name-only HEAD;
           git --no-optional-locks -C "$wt" ls-files --others --exclude-standard) | sort)
  ```
  and `git --no-optional-locks -C "$wt" status --porcelain | wc -l` prints the same count as
  before (55, 3 and 2 on 2026-10-21).
- Record: a log line `step 1` listing each ref and its sha, plus the `df -h` lines.

### Step 2: Bring lane-a8's sign-in cancel fix to main

- Do: in `.worktrees/track-INT-6`:
  ```bash
  git cherry-pick -x 58aa8a9 599e516
  ```
  58aa8a9: "A cancel once the sign-in's token calls started signs out, as tC4's does" (it also
  updates APP-DESIGN §5.6 item 6's declared difference). 599e516: "Delete the flow gate's
  sign-in flow, which nothing entered". A read-only trial on 2026-10-21 showed both apply to
  `resume-build-lane` with no conflict. Do not pick lane-a8's other two commits: e5d901f only
  changes how parity captures name the fake Door43's address, and it is the commit that
  conflicts in the parity files (`parity/app/captures.json`, `report.json`,
  `capture-rust.mjs`, `dom/net-failed.json`, `dom/share-signin.json`, `dom/signin-error.json`);
  abc315d deletes rows from `.audit/minors.tsv`, which is append-only. Instead append two rows
  to `.audit/minors.tsv`: unit `A8`, "fixed: a cancel after Door43 deleted the old token" and
  "fixed: FlowRequest::SignIn had no product caller", each with the new commit's sha. The A8 row
  about the fake Door43's random port stays open.
  Then run the account test and the sign-in journeys (build `tc4`, the fake and the UI first, as
  `TRACK-B.md` §5 step 4 shows):
  ```bash
  cargo test -p tc-app --test app a_cancel_once_the_token_calls_started_forgets_the_token_an_earlier_session_kept
  TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
    TC4_DOOR43_FAKE=target/debug/tc-door43-fake node --test \
    parity/app/journeys/account-menu.test.mjs parity/app/journeys/j11-sign-in.test.mjs \
    parity/app/journeys/share-open.test.mjs
  ```
  Then run the parity gate: `cargo test -p tc-ui --test screen_record`. The two picks change no
  file under `parity/`, so it must pass unchanged. Never set `UPDATE_REPORT` (INT-3 refuses it).
  If it fails, abort as the next bullet says and log `block` with the failing capture names.
- If a pick conflicts or a gate fails twice and you cannot tell why: abort with
  `git cherry-pick --abort`, and ask the owner: "tC4 signs a translator out when they cancel a
  Door43 sign-in halfway. The rebuild keeps a sign-in that no longer works. Shall I keep
  working on matching tC4 (recommended), or leave it as it is?" Record the answer as a ruling.
- Files: what the two commits change (`crates/tc-app/src/account.rs`,
  `crates/tc-app/tests/app/account.rs`, `crates/tc-app/src/flows.rs`,
  `crates/tc-app/tests/app/flows.rs`, `crates/tc-api/src/error.rs`, `docs/APP-DESIGN.md`,
  `.audit/plans/A8.md`), `.audit/minors.tsv`.
- Done when: the account test passes; `cargo test -p tc-ui --test screen_record` passes; the
  account test fails after
  `git show <new sha of 58aa8a9> -- crates/tc-app/src/account.rs | git apply -R` (undo with
  `git checkout -- crates/tc-app/src/account.rs`); the three journeys report `# fail 0` and
  `# skipped 0`.
- Record: a log line `step 2` with the two new shas and the failing output of the reverted run.

### Step 3: Build the deletion list and ask the owner

- Do: in the main checkout:
  ```bash
  cd /home/bot/tc-rst
  main=claude/focused-ride-5uzj7p
  active=" $(grep -h '^worktree:' .audit/roadmap/units/*.md | awk '{print $2}' | grep -v '^none$' | tr '\n' ' ') "
  git worktree list --porcelain | awk '/^worktree /{print $2}' | grep '/\.claude/worktrees/' | while read -r wt; do
    case "$active" in *" $wt "*) echo "KEEP-CARD $wt"; continue;; esac
    [ "$wt" = /home/bot/tc-rst/.claude/worktrees/s2-verify ] && { echo "KEEP-SESSION $wt"; continue; }
    head=$(git -C "$wt" rev-parse HEAD)
    git merge-base --is-ancestor "$head" "$main" || { echo "KEEP-UNMERGED $wt"; continue; }
    [ -z "$(git --no-optional-locks -C "$wt" status --porcelain)" ] || { echo "KEEP-DIRTY $wt"; continue; }
    echo "REMOVE $wt"
  done
  ```
  Only `.claude/worktrees/` is listed: every worktree under `.worktrees/` belongs to a running
  session or holds unique work (`rebuild-process-improvements`). The ancestry check keeps
  `lane-a8`, `lane-c1a` and `storage` (commits not on main); the status check keeps
  `quick-timing`, `lane-b7` and `lane-merge` (their changes are saved in step 1, but
  `git worktree remove` without `--force` refuses them, so their build folders stay). For each `REMOVE` line, the build folder is `<wt>/target`; when it is a link, its
  target is `/var/tmp/tc-rst-target/<name>`. Measure both: `du -sh <wt>` and
  `du -sh /var/tmp/tc-rst-target/<name>`. Add the orphan build folders: folders in
  `/var/tmp/tc-rst-target` that hold a `debug/` folder and have no worktree
  `.claude/worktrees/<same name>` (2026-10-21: `lane-fproxy-gnu`, `merge-main-windows-gnu`).
  Never list a plain file there, or a folder without `debug/` (`lane-b7-shots` and
  `lane-b7-state-en` hold captures and state, not builds).
- Then write to the owner: "Old copies of finished work fill this PC's main disk (97% full).
  May I delete <n> of them and their build files? It frees about <x> GB. Nothing a translator
  wrote is in them: their branches stay in git, and every copy with unsaved changes is kept.
  Yes: I delete them now. No: the disk stays nearly full and builds may fail. I recommend
  yes." Attach the list. Save the answer as `rulings/<date>-disk-cleanup.md`.
- Files: `.audit/roadmap/rulings/<date>-disk-cleanup.md`, this card.
- Done when: the list (REMOVE and KEEP lines, the orphans, the sizes and the total) is in the
  log, and the owner's answer is in the ruling file.
- Record: a log line `step 3` with the totals, then `ruling`.

### Step 4: Remove what the owner approved

- Do: first prove the safety check works on a probe:
  ```bash
  cd /home/bot/tc-rst
  git worktree add --detach /home/bot/int6-probe HEAD
  echo probe >> /home/bot/int6-probe/AGENTS.md
  git worktree remove /home/bot/int6-probe; echo "exit $?"     # must refuse: exit 128
  git -C /home/bot/int6-probe checkout -- AGENTS.md
  git worktree remove /home/bot/int6-probe
  ```
  Then, for each approved `REMOVE` worktree and each approved build folder:
  ```bash
  git worktree remove "<wt>"                                       # never --force
  name="<folder name>"; [ -n "$name" ] && rm -rf -- "/var/tmp/tc-rst-target/$name"
  ```
  If `git worktree remove` refuses, leave that worktree, and log it.
- Files: none in the repository.
- Done when: the probe refusal printed a non-zero exit; `git worktree list` no longer shows any
  removed worktree; each removed `/var/tmp/tc-rst-target/<name>` is gone; branches are still
  there (`git branch --list <branch>` prints each removed worktree's branch).
- Record: a log line `step 4` with `df -h / /home`.

### Step 5: Keep build folders off the root drive

- Do: in `.audit/tools/watch-runs.py`, delete `relocate_idle` and the `if free < 100:` block that
  calls it. Make the low-space check read both drives: a helper `free_gb(path)` returning
  `statvfs(path).f_bavail * f_frsize // 2**30`, and `free = min(free_gb("/home"), free_gb("/"))`,
  with the printed message naming the drive. Keep the pruning (`prune-target.py`). In
  `.audit/HANDOFF.md` "The machine", replace the "Build folders" row with: "Each worktree owns
  its `target/` inside the worktree, on `/home`. Never link a build folder to `/var/tmp`: that
  filled the root drive to 97% (2026-10-21). Never create a worktree inside a `target/` folder:
  a `cargo clean` there deletes it."
- Files: `.audit/tools/watch-runs.py`, `.audit/HANDOFF.md`.
- Done when: `grep -nE 'relocate_idle|/var/tmp' .audit/tools/watch-runs.py` prints nothing;
  `grep -c 'free_gb("/")' .audit/tools/watch-runs.py` prints at least 1;
  `python3 -I -c "import ast; ast.parse(open('.audit/tools/watch-runs.py').read())"` exits 0.
- Record: a log line `step 5`.

### Step 6: Merge and close

- Do: merge `track-INT-6` into main as `TRACKING.md` "Checklist: merge an item" says, with the
  title `Merge INT-6: a cancelled Door43 sign-in signs out, as in tC4` and the trailer
  `Unit: INT-6`. In the merge commit: `state: done`, `merged:` the branch tip, a log line
  `merge` with `df -h / /home` after the clean-up.
- Files: this card.
- Done when: CI is green on main and `python3 -I .audit/tools/board.py --check` prints no error
  for INT-6.
- Record: the merge log line.

## Gates before merge

`scripts/gates.sh --journeys` (INT-3: the gate list, then every journey, failing on any fail
or skip), and `scripts/gates.sh --check-receipt` on the exact commit main moves to. The parity
gate (`crates/tc-ui/tests/screen_record.rs`, in the `test` step) must pass with any recaptured
files.

## Stop and ask when

- Step 2's picks conflict or its gates fail twice: ask the owner the question in step 2.
- Before any deletion (step 3).
- A worktree's owner is unclear (a session may still use it): keep it and log it.

## References

- `evidence/2026-10-21/process-integration.md` §1.2 (M4), §2 and findings F4, F5, F7, F15.
- `.audit/tools/watch-runs.py` `relocate_idle` (lines 57-80 at f9067bc) and its call.
- `git show 58aa8a9`, `git show 599e516`; `git cherry -v resume-build-lane lane-a8`.
- `.audit/minors.tsv` rows `A8` (lines 6-8 at f9067bc).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/process-integration.md
