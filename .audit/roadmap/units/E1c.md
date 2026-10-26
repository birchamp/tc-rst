---
id: E1c
title: One name for a project; open from Door43 asks none
track: A
lane: app
milestone: M3
state: blocked
owner: E1b session (Claude Code fd622395)
branch: track-E1c
worktree: .worktrees/track-E1c
depends: [E1b]
needs_owner: []
proves: [P1]
no_loss: false
size: 0.5 d (basis: E1b step 6 as planned, plus Home's second line)
merged: none
updated: 2026-10-26
next: Ready for a batch with E1a and E1b (track-E1c holds track-E1b2).
blocked_by: E1b not done; E1c is ready on track-E1c and merges with or after E1b
---

# E1c: One name for a project; open from Door43 asks none

## Goal

Ruling H14 (`rulings/2026-10-26-H14.md`, #837): a project has one name, its
`identification.name`. It matches Door43 after a sync and changes only in Project details, which
syncs to the team. Open from Door43 asks for no name; the folder on disk gets a unique name,
chosen without asking. When two Home cards share a name, each shows a second line saying where
it lives ("On Door43: Juan/es_tit_text" or "Only on this computer"). A computer holds one copy
of a Door43 project: an open of one already here makes no copy and offers "Open it".

## Why it matters to a translator

A team works on one project under one name. A name typed at the open lived only on one computer
and drifted from what the team calls the project.

## Funnel card

- **Invariants**:
  - INV-ONE-NAME-PER-PROJECT: an open from Door43 asks for no name and never writes one; the
    opened project's card shows its `identification.name`, and a card whose name another card
    holds says where it lives.
  - INV-ONE-COPY-PER-COMPUTER: an open of a repository a project here syncs with makes no copy
    and sends nothing; the dialog offers that project.
- **Wrong states**: a card under a name only this computer knows; an open that lands on an
  entry of the projects folder; a second copy of a Door43 project on one computer.

## Declared differences

- tC4 asks for a new name at the open. Here the open asks none: one name for the whole team,
  and nothing to drift (APP-DESIGN E1b row, E1c).
- tC4 opens a second copy of a project already on the computer. Here it is refused before
  anything is sent (`open-door43.already-here`), and the dialog's "Open it" opens the copy here.

## Steps

### Step 1: Drop the name from the open, choose the folder, say where twin cards live

- Do: `door43/open { request, repo }` (no `name`); `tc_api::door43::open_folder` picks the
  repository's name as a folder, or the first free `-n`, letter case aside, over every entry of
  the projects folder. The dialog loses its name field and its errors. Home: `shared_names` and
  `where_line` (`crates/tc-ui/src/app/home.rs`), drawn under the card's name as
  `card-where-<folder>`; new key `home.whereOnDoor43` (pending in nine languages). Journeys J14
  and J17 (`open-door43.test.mjs`) and `share-open.test.mjs` follow; the env's `join` takes the
  repository alone.
- Also: `door43/open` refuses `AlreadyHere { project }` when a project here syncs with the
  repository (host with port, owner and name, letter case aside); the dialog shows
  `open-door43-yours` with `open-door43-open-yours`. The contract opens on a second computer
  for the real port (`Opener`, `computer_b`).
- Checks: `an_open_asks_no_name_and_keeps_the_teams_name`,
  `opening_a_project_you_have_makes_no_copy_and_offers_yours` (tc-app port_contract);
  `a_project_already_here_is_offered_until_the_repository_changes` (tc-ui share);
  `the_open_passes_over_every_entry_of_the_projects_folder` (tc-app store);
  `an_open_takes_the_first_free_folder_of_the_repository_name` (tc-api ids);
  `two_cards_of_one_name_each_say_where_they_live` (tc-ui home); journeys `open-door43`,
  `share-open`.
- Docs: `docs/TC4-ISSUES.md` #837 and #677; `docs/APP-DESIGN.md` E1b row.

## Log

- 2026-10-26 13:11 EDT | E1b session (Claude Code fd622395) | created | none | E1b's step 6 split off at the audit session's request so E1b lands first; ruling H14 recorded | .audit/roadmap/rulings/2026-10-26-H14.md
- 2026-10-26 13:11 EDT | E1b session (Claude Code fd622395) | ruling | none | H14: "I agree with your naming recommendation." One name per project, the open asks none, twin cards say where they live | .audit/roadmap/rulings/2026-10-26-H14.md
- 2026-10-26 13:50 EDT | E1b session (Claude Code fd622395) | ruling | none | H14, second question: "1 agree". A computer holds one copy of a Door43 project; Open from Door43 offers the one here | .audit/roadmap/rulings/2026-10-26-H14.md
- 2026-10-26 15:40 EDT | E1b session (Claude Code fd622395) | step 1 | 42a58850 | door43/open asks no name and refuses a project already here (AlreadyHere, "Open it"); Home's second line for twin names; the contract and J14/J17 open on a second computer | git log --oneline origin/track-E1b2..42a58850
- 2026-10-26 15:40 EDT | E1b session (Claude Code fd622395) | gate | 42a58850 | scripts/gates.sh --touched: 17 crates, 13 journey files, app-e2e 102 pass 0 skipped (J14, J17, the open case of share-open, J11, J24 among them); cargo fmt --check passes | scripts/gates.sh --touched
- 2026-10-26 16:10 EDT | E1b session (Claude Code fd622395) | verify | 42a58850 | reviewer: general-purpose subagent, read-only. 1 Code: one entry point (door43/open's handler) carries the one-copy check before any request; the name is gone from the request type. 2 Checks: every named test exists; the touched gate ran them (not seen failing without the fix). 3 Reach: two opens at once are not reachable from the dialog. 4 Siblings: Copy the project keeps the Door43 link (older code, minor). 5 No loss: not a no-loss card. 6 Prose and Human: two comment slips fixed; H14 and its second question have a ruling file. No blockers, no majors; six minors in .audit/minors.tsv | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/E1c-verify.md
- 2026-10-26 16:10 EDT | E1b session (Claude Code fd622395) | block | none | Ready for a batch on track-E1c; it merges with or after E1b, which merges with or after E1a | python3 -I -B .audit/tools/board.py --check
- 2026-10-26 18:10 EDT | E1b session (Claude Code fd622395) | note | 9a7a70a0 | Merged track-E1a2 at 20c3faeb (main ad4b67bc, batch 13) into track-E1c, so this one branch carries E1a, E1b and E1c (audit session's request); F1's import_removed and E1b's notices both stay on HomeView. Recaptured open-door43, open-door43-internet-consent and open-door43-refused (store, --real-sync): the dialog has no name field by H14, so their DOM rows say so and the q3 masks hold both positions | node parity/app/capture-rust.mjs --unit E1b --only open-door43,open-door43-internet-consent,open-door43-refused --real-sync
- 2026-10-26 18:50 EDT | E1b session (Claude Code fd622395) | gate | f8190b95 | scripts/gates.sh --touched on the branch that carries E1a, E1b and E1c over main ad4b67bc: 17 crates, 14 journey files, app-e2e 109 pass 0 skipped; screen_record passes | scripts/gates.sh --touched
