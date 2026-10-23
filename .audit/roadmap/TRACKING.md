# Tracking: how work is recorded and handed off

The rebuild has been interrupted by usage limits every few hours and has had three sessions
working at once. So the state of the work must always be readable from the repository alone,
and two branches must never fight over one tracking file. This file says how.

## Where the truth lives

| Truth | Where | Who writes it |
|---|---|---|
| What is merged | git: the work's tip commit is an ancestor of main | nobody by hand; `board.py` reads it |
| What each item is, its state, owner, branch, next step | its card, `units/<ID>.md` (front matter) | only the item's owner |
| The live state of a claimed card | the copy of the card on the card's `branch`; the copy on main is as of the claim | its owner |
| What happened, step by step | the card's `## Log` (append only) | only the item's owner |
| An owner ruling | one file in `rulings/` | the agent that received the answer |
| Minor findings | `.audit/minors.tsv` (append only) | whoever found them |
| The overall picture | `python3 -I .audit/tools/board.py` (printed, never committed) | generated |

Why this shape: each card has one owner at a time, so two branches never edit the same card;
the log is append-only; rulings are one file each; and the board is generated, so it cannot go
stale. `.audit/STATUS.md`, `.audit/PROGRESS.md`, `.audit/plans/RESUME-*.md` and the old
HANDOFF "runs in flight" sections are history from 2026-10-21 on. Keep them; do not update them.

## The card's front matter

Every card starts with this block. `board.py --check` refuses anything else.

```yaml
---
id: X                       # must equal the file name
title: Door43 sync and USB exchange
track: A                    # A (main line) or B (second developer)
lane: storage               # integration | storage | app | checks | domain | shell
milestone: M2               # M0 to M5
state: todo                 # see "States" below
owner: none                 # the session or person who holds it; none when free
branch: none                # the branch the work is on
worktree: none              # the worktree path, if any
depends: [S2, DOM-1, T]     # ids that must be done before this item merges
needs_owner: []             # open owner questions, e.g. [H5]; empty when none
later: ["S3 from step 4"]   # optional: what a later step waits for, "<card or H-id> from step <n>"
proves: [P2, P3, P4, P5]    # PLAN items this item helps prove
no_loss: true               # true for items that touch a translator's saved work
size: 5-8 days (basis in the card)
merged: none                # the tip commit of the work once it is merged
updated: 2026-10-21         # the date of the last edit
next: Claim this card, then do step 1.
blocked_by: none            # why it cannot move, when state is blocked
---
```

### States

| State | Meaning | Set by |
|---|---|---|
| `todo` | Nobody holds it. `board.py` shows it as **ready** when its `depends` are done and `needs_owner` is empty | the roadmap |
| `claimed` | Someone holds it and has not changed code yet | the owner, in the claim commit |
| `building` | Code is being written | owner |
| `verifying` | An independent review is running | owner |
| `fixing` | Fixing the review's blockers and data or view majors | owner |
| `gating` | Running the full gate list on the final tree | owner |
| `merging` | Merging into main | owner |
| `done` | Merged into main; `merged` holds the work's tip commit | owner, in the merge commit |
| `blocked` | Cannot move; `blocked_by` says why | owner |
| `parked` | Stopped on purpose, with the work committed; `next` says how to resume. A card the owner decides not to do is `parked` with `next: not doing: <reason>` and a log line naming the owner's decision; DONE counts it as closed | owner |

Rules the states rest on:

- `depends` lists the ids that must be `done` before this item **merges**. The board shows
  `ready` only when all are done. Starting earlier is allowed only when the card's "Before you
  start" says which steps may; log a `note` naming the unmet ids at the claim. The board then
  warns `active with <id> not done: the merge waits`, which is true and expected.
- A step that waits for another card's step or for an owner ruling is written in `later`
  (`"S3 from step 4"`, `"H12 from step 4"`). The board prints `ready to step 3; then S3, H12`
  and `--check` errors when the log holds a `step <n>` line at or past a gate that is not met.
- A step that stops on an owner question puts its H-id into `needs_owner` while it waits, even on
  an active card; the board prints `waits for owner: H<n>` and warns when the ruling file appears.
- When a card reaches a step that waits for another card's note (for example S3's "core merged"),
  and the note does not exist, set `state: blocked` and `blocked_by: <card> note "<text>"` in the
  same commit; unblock with a log line `unblock` when the note exists. The board shows the reason.

## Checklist: claim an item

1. Worktree and branch: `git -C "$(git rev-parse --show-toplevel)" worktree add -b track-<ID>
   .worktrees/track-<ID> claude/focused-ride-5uzj7p` (main, `GLOSSARY.md`), or check out the branch
   the card names if it has one. Track B branches are `ext/<card>-<short>` in the developer's own
   clone (`TRACK-B.md` §7). Build in that worktree's own `target` on /home; never link `target` to
   /var/tmp or /tmp (the root drive filled to 97% on 2026-10-21 and `/tmp` writes fail on a
   quota; INT-6 step 5 moves the old links). After the merge, `git worktree remove` (never
   `--force`) and keep the branch.
2. Run `python3 -I .audit/tools/board.py`. Confirm the item is **ready** and nobody owns it.
3. In the card set `state: claimed`, `owner`, `branch`, `worktree`, `updated`, and `next`.
4. Add a log line (format below).
5. Commit only the card: `git commit -m "Claim <ID>: <title>"`.

## Checklist: after every step

1. Commit the step's work and, in the same commit, the card: append a log line, update `state`,
   `updated` and `next`.
2. Run `python3 -I .audit/tools/board.py --check`. It must print no errors.
3. If the step found minors, append them to `.audit/minors.tsv` in the same commit.

## Heavy runs

The PC runs at most two heavy runs at once (a gate list, a model or mutation run), storage first.
There is no orchestrator process; the owner of the integration lane's current card is the
orchestrator, and when no integration card is active, decide yourself and log the decision.

1. Before a gate or model run: `pgrep -fa 'cargo (test|clippy|build)|node --test' | grep -v $$`.
   Two other runs alive: wait; a storage run goes first.
2. Commit a log line `gate | none | started <command> in <worktree>` before you start, and the
   result line (`gate | <sha> | passed/failed …`) after. The log is how another session sees it.

## Checklist: when you stop (planned or not)

Usage limits end sessions without warning, so do this at every step boundary, not only at the
end.

1. Commit everything on your branch, even unfinished work: `git commit -m "WIP <ID>: <what>"`.
2. Log line with event `interrupt` or `park`, and a `next` that a stranger can act on: the exact
   command or file to start with.
3. If you hold a heavy gate run, write where its log is.
4. A `WIP` commit is allowed on a work branch, never on main.

## Checklist: pick up an item someone else started

1. Read the card: front matter, funnel card, then the log from the bottom up.
2. `git -C <worktree> status` and `git log -5 <branch>` to see the real state. If the card and
   git disagree, git wins: write a log line that says so.
3. Never edit another active session's worktree while it may still run. A session counts as
   **gone** when `git log -1 --format=%ci <branch>` and `git --no-optional-locks -C <worktree>
   status --porcelain` show no commit and no changed file for 24 hours (and, for a session with
   a log folder such as `.enjoy-logs/`, no new file there either). Then ask the owner in one
   sentence whether it still runs; with a no, or no answer by the next day, take ownership with a
   log line `takeover`, naming the old owner. The board warns `claimed by <owner> but <branch>
   last moved <n> days ago` from 24 hours on.
4. First save what you found, in that worktree: `git add -A && git commit -m "WIP <ID>:
   uncommitted work found at takeover by <who>"`. Never stash, reset or checkout over it.
5. If `git status` says a merge is in progress: finish it only if you will run the full gate
   list on the result; otherwise `git merge --abort` and log the merged commit's sha.
6. Continue at `next`; if `next` names a step whose commit is missing, redo that step.

## Checklist: merge an item

1. The card's log holds a `verify` line per `FUNNEL.md` §8 (who reviewed, the six questions
   answered in order, the evidence path) and a `fix` line for every blocker and every major that
   harms a translator's data or view. A no-loss item has three `verify` lines, one per lens
   (`GLOSSARY.md` "Verification lenses").
2. The full gate list passes on the exact tree you will merge (INT-3's `scripts/gates.sh`).
3. Merge into main in a separate worktree, with a merge commit titled `Merge <ID>: <what the
   translator gets>` and a trailer line `Unit: <ID>`.
4. In the same merge commit: card `state: done`, `merged: <the work's tip sha>`, a log line
   `merge`.
5. Push main only under the push rule (`README.md` "Rules that never bend").
6. `board.py --check` on main passes.

## Log line format

One line per event, newest last, never edited afterwards:

```
- 2026-10-21 14:05 EDT | <who> | <event> | <commit or none> | <one sentence> | <evidence path or command>
```

Events: `created`, `claim`, `step <n>`, `verify`, `fix`, `gate`, `merge`, `interrupt`, `park`,
`resume`, `takeover`, `block`, `unblock`, `ruling`, `note`.

`<who>` is the card's `owner` string: `<session name> (<harness> <first 8 characters of the
session id>)`, or a person's name. Date, time and zone as in the example. `board.py --check`
errors when `updated` is not `YYYY-MM-DD` or a card has no log line, and warns when `updated` is
older than the last log line's date.

Evidence is a path in the repository (a committed log, a test name) or a command anyone can run.
A local file outside git (for example `.enjoy-logs/…`) may be named, but it is not proof for the
next agent; quote its key lines in the log.

## Rulings

When the owner answers a question, create `rulings/<YYYY-MM-DD>-<H-id or slug>.md`:

```markdown
# <H-id>: <the question in one line>

- Asked: <date>, by <who>, in <where>
- Options: <the options as put to the owner, the recommendation marked>
- Answer: <the owner's words, quoted>

## The rule

## Effect for translators

## Unblocks and changes
```

Rulings before 2026-10-21 are the `ruling` lines of `.audit/rebuild.tsv`. The board lists the
files in `rulings/`; `ROADMAP.md`'s answered rows hold only `Answered: rulings/<file>`.

Then remove the id from those cards' `needs_owner` and add a log line `ruling`. `board.py` warns
when a ruling exists for a question a card still lists.

## Minors

`.audit/minors.tsv` keeps its columns (`unit`, `finding`, `evidence`). Append only. From INT-5
step 1 on, git merges it with the `union` driver (`.gitattributes`), so two branches can both
append without a conflict. Until then, resolve a conflict in it by keeping every row of both
sides and the header once.

## The board

```
python3 -I .audit/tools/board.py            # the board, as Markdown
python3 -I .audit/tools/board.py --check    # exit 1 on any contradiction; prints each one
python3 -I .audit/tools/board.py --json     # for scripts
python3 -I .audit/tools/board.py --ref main # check `merged` commits against another ref
```

It reads every card, checks the front matter, works out which items are ready, checks that every
`done` item's `merged` commit is an ancestor of the ref (HEAD by default), finds dependency
cycles, compares each `ROADMAP.md` table row with its card, and lists every contradiction it can
see. For an active card whose `branch` exists here it reads the copy on that branch
(`git show <branch>:.audit/roadmap/units/<ID>.md`) and marks the row `(from <branch>)`. It prints
the worktrees in use, the rulings on file, and `--invariants` lists every `INV-` name with the
cards that use it (the registry of invariant names). Its own test is
`python3 -I .audit/tools/board_test.py`.

## Adding a work item

1. Copy `units/_TEMPLATE.md` to `units/<ID>.md`. Use a new prefix only if no existing one fits.
2. Fill in the front matter and every section; the funnel card can stay a template until claimed.
3. Add it to `ROADMAP.md`'s milestone table.
4. `board.py --check`.
