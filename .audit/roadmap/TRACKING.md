# Tracking: how work is recorded and handed off

The rebuild has been interrupted by usage limits every few hours and has had three sessions
working at once. So the state of the work must always be readable from the repository alone,
and two branches must never fight over one tracking file. This file says how.

## Where the truth lives

| Truth | Where | Who writes it |
|---|---|---|
| What is merged | git: the work's tip commit is an ancestor of main | nobody by hand; `board.py` reads it |
| What each item is, its state, owner, branch, next step | its card, `units/<ID>.md` (front matter) | only the item's owner |
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
depends: [S2, DOM-1, T]     # ids that must be done first
needs_owner: []             # open owner questions, e.g. [H5]; empty when none
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
| `parked` | Stopped on purpose, with the work committed; `next` says how to resume | owner |

## Checklist: claim an item

1. `git fetch` if you have a remote; check out or create the branch the card names, or
   `track-<id>` if it names none. Use your own worktree if another session is active on this PC.
2. Run `python3 -I .audit/tools/board.py`. Confirm the item is **ready** and nobody owns it.
3. In the card set `state: claimed`, `owner`, `branch`, `worktree`, `updated`, and `next`.
4. Add a log line (format below).
5. Commit only the card: `git commit -m "Claim <ID>: <title>"`.

## Checklist: after every step

1. Commit the step's work and, in the same commit, the card: append a log line, update `state`,
   `updated` and `next`.
2. Run `python3 -I .audit/tools/board.py --check`. It must print no errors.
3. If the step found minors, append them to `.audit/minors.tsv` in the same commit.

## Checklist: when you stop (planned or not)

Usage limits end sessions without warning, so do this at every step boundary, not only at the
end.

1. Commit everything on your branch, even unfinished work: `git commit -m "WIP <ID>: <what>"`.
2. Log line with event `interrupt` or `park`, and a `next` that a stranger can act on: the exact
   command or file to start with.
3. If you hold a heavy gate run, write where its log is.

## Checklist: pick up an item someone else started

1. Read the card: front matter, funnel card, then the log from the bottom up.
2. `git -C <worktree> status` and `git log -5 <branch>` to see the real state. If the card and
   git disagree, git wins: write a log line that says so.
3. Never edit another active session's worktree. If the owner session is gone, take ownership
   with a log line `takeover`, naming the old owner.
4. Continue at `next`.

## Checklist: merge an item

1. The full gate list passes on the exact tree you will merge (INT-3's `scripts/gates.sh`).
2. Merge into main in a separate worktree, with a merge commit titled `Merge <ID>: <what the
   translator gets>` and a trailer line `Unit: <ID>`.
3. In the same merge commit: card `state: done`, `merged: <the work's tip sha>`, a log line
   `merge`.
4. Push main only under the push rule in `ROADMAP.md`.
5. `board.py --check` on main passes.

## Log line format

One line per event, newest last, never edited afterwards:

```
- 2026-10-21 14:05 EDT | <who> | <event> | <commit or none> | <one sentence> | <evidence path or command>
```

Events: `created`, `claim`, `step <n>`, `verify`, `fix`, `gate`, `merge`, `interrupt`, `park`,
`resume`, `takeover`, `block`, `unblock`, `ruling`, `note`.

Evidence is a path in the repository (a committed log, a test name) or a command anyone can run.
A local file outside git (for example `.enjoy-logs/…`) may be named, but it is not proof for the
next agent; quote its key lines in the log.

## Rulings

When the owner answers a question, create `rulings/<YYYY-MM-DD>-<H-id or slug>.md`:

```markdown
# <H-id>: <the question in one line>

- Asked: <date>, by <who>, in <where>
- Answer: <the owner's words, quoted>
- Effect: <what changes for translators>
- Unblocks: <card ids>
```

Then remove the id from those cards' `needs_owner` and add a log line `ruling`. `board.py` warns
when a ruling exists for a question a card still lists.

## Minors

`.audit/minors.tsv` keeps its columns (`unit`, `finding`, `evidence`). Append only. Git merges
it with the `union` driver (`.gitattributes`), so two branches can both append without a
conflict.

## The board

```
python3 -I .audit/tools/board.py            # the board, as Markdown
python3 -I .audit/tools/board.py --check    # exit 1 on any contradiction; prints each one
python3 -I .audit/tools/board.py --json     # for scripts
python3 -I .audit/tools/board.py --ref main # check `merged` commits against another ref
```

It reads every card, checks the front matter, works out which items are ready, checks that every
`done` item's `merged` commit is an ancestor of the ref (HEAD by default), finds dependency
cycles, and lists every contradiction it can see. Its own test is
`python3 -I .audit/tools/board_test.py`.

## Adding a work item

1. Copy `units/_TEMPLATE.md` to `units/<ID>.md`. Use a new prefix only if no existing one fits.
2. Fill in the front matter and every section; the funnel card can stay a template until claimed.
3. Add it to `ROADMAP.md`'s milestone table.
4. `board.py --check`.
