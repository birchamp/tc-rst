---
id: INT-1
title: Back up the unpushed branches off this PC
track: A
lane: integration
milestone: M0
state: done
owner: roadmap session (Cursor)
branch: track-INT-1
worktree: /home/bot/tc-rst/.worktrees/track-INT-1
depends: []
needs_owner: []
proves: []
no_loss: false
size: under 1 h (basis: three git push commands and a check)
merged: ff648f95
updated: 2026-10-23
next: none
blocked_by: none
---

# INT-1: Back up the unpushed branches off this PC

## Goal

Three branches exist only on this PC: `resume-build-lane`, `s2-proof-executor` and `storage`.
When this item is done, each has a copy outside this PC whose tip equals the local tip at the
time of the copy. If the owner says yes to H1, the copy is a branch of the same name on `origin`
(`https://github.com/birchamp/tc-rst`, private). If the owner says no, the copy is one git bundle
per branch in a folder outside the repository, which the owner can carry off the PC. Nothing is
merged, and main does not move: main moves only through INT-4.

## Why it matters to a translator

It protects finished work that translators will use: nine app parts (B4b, B5b, B7, B8, B11, C2,
C3, C4, C5) and S2, the storage work that keeps their text safe. Today 182 commits exist on no
remote (ASSESSMENT.md §2), so one disk failure on this PC would lose them.

## Before you start

- `ls .audit/roadmap/rulings/ | grep -E -- '-H1(-.*)?\.md$'` prints one file. Read it. The
  pattern is the one `board.py` uses, so a ruling for H10 to H17 does not match.
- `git -C /home/bot/tc-rst remote get-url origin` prints `https://github.com/birchamp/tc-rst`.
- This prints a number above 0 for each branch (2026-10-21: 89, 69, 56). A branch that prints 0
  already has a copy on a remote: skip it in every step.
  ```bash
  cd /home/bot/tc-rst
  for b in resume-build-lane s2-proof-executor storage; do echo "$b $(git rev-list --count "$b" --not --remotes)"; done
  ```
- `git -C /home/bot/tc-rst merge-base --is-ancestor origin/storage storage && echo ff` prints
  `ff`: pushing `storage` is a fast-forward of `origin/storage` (0d7d4f7 on 2026-10-21).
- Two other sessions own `.worktrees/resume-build-lane` and `.worktrees/s2-proof-executor`.
  A push reads the repository's objects only; it never touches a worktree. Do not open, edit or
  build in those worktrees.

## Funnel card

- **Invariants**:
  - INV-OFF-PC-COPY: every branch that holds commits on no remote has a copy outside this PC
    whose tip equals the local tip at the time of the copy. (not in code: a one-time backup step, checked by the commands of its log)
  - INV-NO-SECRET-LEAVES: no pushed branch adds a token, a password, a private key or a path to
    a secret file that origin's main does not already hold. (not in code: a one-time backup step, checked by the commands of its log)
  - INV-MAIN-ONLY-THROUGH-INT-4: this item never moves or pushes `claude/focused-ride-5uzj7p`. (not in code: a one-time backup step, checked by the commands of its log)
- **Wrong states**:
  - A disk failure on this PC loses B4b to C5 and S2 (182 commits on no remote).
  - A forced push replaces `origin/storage` (0d7d4f7) and loses what only the remote holds.
  - Main is pushed without INT-4's gates.
  - A token or password reaches GitHub inside a pushed file.
- **Code**:
  - Types: none. This item is an operation on git, not code.
  - One entry point: the push loop of step 3. It pushes a recorded sha to
    `refs/heads/<the same name>`, never with force, never main.
  - Removed: nothing.
  - Left to checks, and why a type cannot carry it: GitHub CI cannot see this PC, so the check
    is a command run here, with its output quoted in the log (steps 2 and 4).
- **Checks**:
  - The secret scan of step 2; kind: command; runs in: this card, before any push.
  - `git rev-list --count <sha> --not --remotes` prints 0 after `git fetch origin`, and
    `git ls-remote origin refs/heads/<branch>` prints the pushed sha; kind: command; runs in:
    step 4.
  - Proof it can fail: before the push the count prints 89, 69 and 56 (2026-10-21). The scan's
    pattern fires: without the "already on origin main" filter it prints the HANDOFF.md line
    that names the VM key path for `s2-proof-executor`; the filter drops it because origin's main
    already holds that line.
- **Prose**: none.
- **Human**: H1 in `ROADMAP.md` "What needs the owner". The answer is a file
  `rulings/<date>-H1.md`.

## Steps

### Step 1: Claim the card and record the owner's answer

- Do: claim the card as `TRACKING.md` "Checklist: claim an item" says, in a worktree of its
  own (only the card changes there):
  `git -C /home/bot/tc-rst worktree add -b track-INT-1 .worktrees/track-INT-1 claude/focused-ride-5uzj7p`.
  Read `rulings/<date>-H1.md` and remove `H1` from `needs_owner`. If the answer is yes, do
  steps 2, 3, 4 and 6. If it is no, do steps 2, 5 and 6. If the ruling allows only some
  branches, push those and bundle the rest.
- Files: `.audit/roadmap/units/INT-1.md`.
- Done when: `python3 -I .audit/tools/board.py --check` prints no error for INT-1, and the card
  shows `state: claimed` and `needs_owner: []`.
- Record: a log line `ruling` naming the ruling file, then a log line `claim`; update `next`.

### Step 2: Scan each branch for secrets

- Do: run this from the main checkout. It prints `== <branch>` for each branch, then one
  `NEW:` line for each added line that looks like a secret and is not already on origin's main.
  ```bash
  cd /home/bot/tc-rst
  for b in resume-build-lane s2-proof-executor storage; do
    echo "== $b"
    git diff origin/claude/focused-ride-5uzj7p..."$b" 2>/dev/null | grep -E '^\+[^+]' | cut -c2- \
      | grep -E 'BEGIN [A-Z ]*PRIVATE KEY|gh[pousr]_[A-Za-z0-9]{30,}|github_pat_|xox[bp]-|AKIA[0-9A-Z]{16}|\.ssh/|[Pp]assword *[:=] *["'"'"'][^"'"'"']+|[Tt]oken *[:=] *["'"'"'][A-Za-z0-9_-]{20,}' \
      | sort -u | while IFS= read -r line; do
          git grep -qF -e "$line" origin/claude/focused-ride-5uzj7p -- . || echo "NEW: ${line:0:140}"
        done
  done
  ```
- Then list added files whose names look like key or secret files. It must print nothing:
  ```bash
  cd /home/bot/tc-rst
  for b in resume-build-lane s2-proof-executor storage; do
    git diff --name-only origin/claude/focused-ride-5uzj7p..."$b" 2>/dev/null \
      | grep -iE '\.(pem|key|p12|pfx)$|(^|/)\.env$|id_rsa|id_ed25519|credentials' | sed "s|^|$b: |"
  done
  ```
  On 2026-10-21 it printed nothing. A plain case-insensitive grep for "token" or "password"
  prints about 600 lines per app branch (the sign-in code), so use the two scans above instead.
- For each `NEW:` line, find its file with `git log -S'<a short part of the line>' --oneline <branch>`.
  A test value in `fixtures/`, in a `tests/` folder or in `crates/tc-door43-fake` is not a
  secret: name it in the log and go on. A real token, password, private key or path to a secret
  file: do not push that branch. Bundle it in step 5 instead, and tell the owner the file and
  line. Never copy the value itself into the log, a commit or a message.
- Files: none.
- Done when: the scan prints only the three `== ` lines, or every `NEW:` line is judged in the
  log. On 2026-10-21 it printed only the three `== ` lines.
- Record: a log line `step 2` with the number of `NEW:` lines and, for each, its file and how
  you judged it. Never paste a `NEW:` line's text into the log: it may hold the secret.

### Step 3: Push each branch under its own name (H1 is yes)

- Do: fetch, then push each branch's recorded sha. Never add `--force`, `-f`,
  `--force-with-lease` or a leading `+`. Never push `claude/focused-ride-5uzj7p`.
  ```bash
  cd /home/bot/tc-rst
  git fetch origin
  for b in resume-build-lane s2-proof-executor storage; do
    sha=$(git rev-parse "$b")
    echo "PUSH $b $sha"
    git push origin "$sha:refs/heads/$b"
  done
  ```
- If git asks for a user name, a password or a token: stop at once and type nothing. Tell the
  owner, in one sentence, that the push needs their GitHub sign-in on this PC.
- If the `storage` push is rejected as not a fast-forward, someone pushed to it from another
  machine. Do not force. Push the same sha as `refs/heads/storage-pc-<date>` instead, and say so
  in the log.
- Pushing starts GitHub CI on each branch (`.github/workflows/ci.yml` runs `on: push`). Its
  results are information only: fix nothing here.
- Files: none.
- Done when: each push prints `* [new branch]` (for `resume-build-lane` and
  `s2-proof-executor`) or `<old>..<new>` (for `storage`), and the three `PUSH` lines are copied
  into the log.
- Record: a log line `step 3` with the three branch names and shas.

### Step 4: Confirm each remote tip equals the pushed sha

- Do:
  ```bash
  cd /home/bot/tc-rst
  git fetch origin
  for b in resume-build-lane s2-proof-executor storage; do
    printf '%s %s\n' "$b" "$(git ls-remote origin "refs/heads/$b" | cut -f1)"
  done
  ```
  Then, for each sha from step 3: `git rev-list --count <sha> --not --remotes`.
  Then list the CI runs, for the log only: `gh run list --branch <branch> --limit 1`.
- Files: none.
- Done when: each printed sha equals the sha step 3 pushed, and each count prints `0`.
- Record: a log line `step 4` with the shas and each CI run's URL and result.

### Step 5: Make one bundle per branch (H1 is no, or a branch step 2 kept back)

- Do:
  ```bash
  dir=/home/bot/tc-rst-backups/$(date +%F)
  mkdir -p "$dir"
  cd /home/bot/tc-rst
  for b in resume-build-lane s2-proof-executor storage; do
    git bundle create "$dir/$b.bundle" "$b"
    git bundle verify "$dir/$b.bundle"
    sha256sum "$dir/$b.bundle"
  done
  ```
  Bundle only the branches this step is for. The folder is outside the repository on `/home`. Each bundle is at most about 220 MiB
  (estimate: `git count-objects -vH` showed 82.65 MiB loose and 139.42 MiB packed on
  2026-10-21); `/home` had 213 GB free.
- Then tell the owner, in two plain sentences, the folder's path, and that copying the folder
  to a USB drive or a cloud folder makes the backup safe if this PC's disk fails.
- Files: none in the repository.
- Done when: `git bundle verify` prints `is okay` for each bundle, and the log holds each
  bundle's path and sha256.
- Record: a log line `step 5` with the paths and sha256 sums.

### Step 6: Close the card

- Do: set `state: done`, `merged:` the sha of `track-INT-1`'s last commit before the merge,
  and add a log line `merge`. Add a log line `note`: commits made on these branches after this
  backup are not covered; their owners push them at their own stage boundaries under the H1
  ruling. Merge `track-INT-1` into main as `TRACKING.md` "Checklist: merge an item" says. This
  branch changes only its own card, so you may merge it together with the next integration
  merge (for example INT-4's), so the full gate list runs once.
- Files: `.audit/roadmap/units/INT-1.md`.
- Done when: `python3 -I .audit/tools/board.py --check` on main prints no error for INT-1.
- Record: the log lines above, in the merge commit.

## Gates before merge

The full gate list (`scripts/gates.sh` once INT-3 lands; until then every command in `AGENTS.md`
"Tests" plus the Windows clippy step from `.github/workflows/ci.yml`), plus:

- `python3 -I .audit/tools/board.py --check` prints no error for INT-1.
- This item changes no file outside its own card.
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- Git asks for a user name, a password or a token (step 3).
- Step 2 finds a real secret: tell the owner which file and line holds it, never the value.
- The H1 ruling asks for something this card does not say: follow the ruling and log it.

## References

- `evidence/2026-10-21/process-integration.md` §0 and finding F6.
- `ASSESSMENT.md` §2 (the branch table and the count of commits on no remote).
- `ROADMAP.md` "The push rule until the owner answers H1" and H1.
- `TRACKING.md` (claim, log line format, merge checklist).
- `.github/workflows/ci.yml` lines 4-6 (`on: push`, `pull_request`).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/process-integration.md
- 2026-10-22 | roadmap session (Cursor) | ruling | none | H1 answered yes; the two sessions holding resume-build-lane and s2-proof-executor have ended, so nothing writes to those branches during the push | .audit/roadmap/rulings/2026-10-22-H1.md
- 2026-10-22 | roadmap session (Cursor) | claim | none | H1 is yes: steps 2, 3, 4, 6; the three branches are resume-build-lane (43496296), s2-proof-executor (3b075d7), storage | .audit/roadmap/rulings/2026-10-22-H1.md
- 2026-10-22 | roadmap session (Cursor) | step 2 | none | the two scans printed only the three `== ` lines: 0 NEW lines, no key or secret file names on any branch | none
- 2026-10-22 | roadmap session (Cursor) | step 3 | none | PUSH resume-build-lane 43496296a5cf5296631a05332c0b57f14923e252 (new branch); PUSH s2-proof-executor 3b075d7f7b8f9f4df6cb9b4d9d2dee429d4989ab (new branch); PUSH storage dd63328b40c49bd24f4dbefafdd34609c2ae1ad6 (0d7d4f76..dd63328b, fast-forward); no prompt for credentials | none
- 2026-10-22 | roadmap session (Cursor) | step 4 | none | ls-remote tips equal the pushed shas; `rev-list --count <sha> --not --remotes` prints 0 for all three; CI runs started: https://github.com/birchamp/tc-rst/actions/runs/39818790386 (resume-build-lane), https://github.com/birchamp/tc-rst/actions/runs/39818797278 (s2-proof-executor), https://github.com/birchamp/tc-rst/actions/runs/39818815348 (storage); results are information only | none
- 2026-10-22 | roadmap session (Cursor) | verify | none | reviewer: the same session (no second session exists today; INT-4's merge rereads this card). 1 What could a translator lose: nothing, a push reads objects only. 2 Does the code make the wrong state unrepresentable: not code; the push loop pushes recorded shas, never main, never forced. 3 Can each check fail: counts printed 90/69/56 before and 0 after. 4 Prose: none needed. 5 Human: H1 answered. 6 Evidence: the step 3 and 4 lines above | .audit/roadmap/rulings/2026-10-22-H1.md
- 2026-10-22 | roadmap session (Cursor) | note | none | commits made on these branches after this backup are not covered; their owners push them at their own stage boundaries under the H1 ruling | none
- 2026-10-22 | roadmap session (Cursor) | gate | none | step 6: this branch changes only its own card; its merge rides with INT-4's integration merge so the full gate list runs once (step 6 allows it); INT-4 sets state done and merged | .audit/roadmap/units/INT-4.md
- 2026-10-23 13:32 EDT | checks lane (Claude Code 2198ee19) | merge | ff648f95 | track-INT-1 reached main inside INT-4's integration merge (97da2945's parent chain, main d293375a); this card edit, which INT-4's merge should have carried, lands with its own gate receipt | git merge-base --is-ancestor ff648f95 claude/focused-ride-5uzj7p
- 2026-10-23 23:48 EDT | checks lane (Claude Code 2198ee19) | note | none | From CHK-3: INV-OFF-PC-COPY, INV-NO-SECRET-LEAVES and INV-MAIN-ONLY-THROUGH-INT-4 are marked "(not in code: a one-time backup step ...)": no code prints them; the commands of this log checked them | node --test .audit/tools/claims.test.mjs
