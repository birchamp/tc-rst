# Roadmap: start here

This folder holds the plan that takes the tC4 rebuild from where it stood on 2026-10-21 to
done. It is written so that any agent, including a less capable model, can pick up the next
piece of work and finish it without this conversation. A person can use it the same way.

Done means `docs/PLAN.md`'s P1 to P10, each shown by a check in CI that can fail, and a release
that installs on the platforms the owner chooses.

## Read in this order

1. This file.
2. `ASSESSMENT.md`: where the project stands, with evidence.
3. `ROADMAP.md`: the milestones, the order, the critical path, and what needs the owner.
4. `FUNNEL.md`: how every bug, function and feature is built. Read it before you write code.
5. `TRACKING.md`: how you record what you did, so the next agent can continue.
6. The card of the work item you are doing: `units/<ID>.md`.
7. `GLOSSARY.md` whenever a word is unfamiliar.

A second developer working on the hand-off package reads `TRACK-B.md` instead of step 3.

The older files still hold detail you will need: `AGENTS.md` (the rules), `docs/PLAN.md` (what
done means), `docs/APP-DESIGN.md` (the app layer), `docs/STORAGE.md` (storage, on the S2
branch), `docs/TC4-ISSUES.md` (tC4's issues per unit) and `.audit/HANDOFF.md` (the machine, the
Windows VM and the reference laptop). `.audit/STATUS.md` and `.audit/PROGRESS.md` are the old
trackers. They are history now; `board.py` replaces them (see `TRACKING.md`).

## How to pick your next task

1. Run `python3 -I .audit/tools/board.py` from the repository root. It prints every work item,
   its state, and which ones are ready.
2. Take the first `ready` item on your track that nobody owns. Track A is the main line on the
   owner's Linux PC. Track B is the second developer's package.
3. Open `units/<ID>.md`. Claim it as `TRACKING.md` says, in its own commit.
4. Follow the card's steps in order. Each step says what to do, which files to touch and how to
   know it is done.
5. Record each finished step in the card's log, in the same commit as the work.

If the board shows nothing ready on your track, read the blocked items' `blocked_by` field. An
owner question goes to the owner (see below). Otherwise pick an item on another lane that does
not touch the same files, and say so in its log.

## Rules that never bend

These come from `AGENTS.md` and the owner's rulings. A card never overrides them.

- Never lose, hide or silently rewrite a translator's text, alignment, check decision or
  comment. If two versions disagree, keep both and show the conflict.
- Count only scenarios a translator can reach: the app's own screens and API, the app or computer
  stopping at any moment, another program editing or holding a project file the app wrote, a
  teammate's work arriving by sync or USB, a full disk, the network dropping. Drop everything
  else. Do not test it and do not log it.
- Fix every blocker. Fix a major only when it harms a translator's data or what they see. Log
  every other finding as a minor.
- Logic that decides about a translator's work is a typed state machine: a state enum, an event
  enum, one pure `next()`, and an executor for the I/O, proven by a model-based property test.
- Write project files only through the atomic helpers in `tc-store`.
- No network outside `tc-door43` and `tc-resources`, and only behind the internet switch. Tests
  never touch the network.
- Before a commit: `cargo fmt --all`, then the full gate list (`INT-3` makes it one script).
  A red CI on main stops every merge until it is green again.
- Never touch another session's worktree. Never send a message to an agent that a workflow is
  still running; pass notes through the next step instead.
- Push only what the owner allows (see the push rule in `ROADMAP.md`). Never record a token or
  password anywhere.
- Default models for every agent.

## When to stop and ask

Stop and write to the owner, in short plain sentences, only when:

- a translator would notice the answer (a screen, a word, a behavior, a data choice);
- a card's step would break a rule above;
- you would have to touch files that another owner's work item has claimed;
- a required gate fails twice in a row and you cannot tell why.

Each question gives the options, what each one means for the person using the app, and your
recommendation. Decide technical questions yourself and record the decision in the card's log.

## What is in this folder

| Path | What |
|---|---|
| `README.md` | This file |
| `ASSESSMENT.md` | Where the project stands on 2026-10-21, with evidence |
| `ROADMAP.md` | Milestones M0 to M5, the order, the critical path, the owner's decisions |
| `FUNNEL.md` | The enforcement funnel and the funnel card every work item fills in |
| `TRACKING.md` | How to claim, record, hand off and pick up work |
| `TRACK-B.md` | The package for a second developer, and how it is merged back |
| `GLOSSARY.md` | Terms used in this repository |
| `units/` | One card per work item, with its front matter (state, owner, branch, next step) |
| `rulings/` | One file per owner ruling made after 2026-10-21 |
| `evidence/2026-10-21/` | The eleven evaluation reports this plan is based on |
| `../tools/board.py` | Prints the board from the cards and git; `--check` fails on contradictions |
