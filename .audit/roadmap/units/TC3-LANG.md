---
id: TC3-LANG
title: A tC3 import keeps the project's language name and direction
track: A
lane: app
milestone: M5
state: done
owner: tc4-issues session (Claude Code dc817a96)
branch: track-TC3-LANG
worktree: .worktrees/track-TC3-LANG
depends: []
needs_owner: []
proves: []
no_loss: true
size: 0.5 d (basis: FIX-R step 1's Settings reader)
merged: 5ebfa874
updated: 2026-10-27
next: none
blocked_by: none
---

# TC3-LANG: A tC3 import keeps the project's language name and direction

## Goal

A tC3 project imports with the language name and text direction its tC3 manifest holds (`target_language.name`, `.direction`), so metadata.json, Settings and Home show them.

## Why it matters to a translator

Today the name is replaced by the code ("cfm" for "Chin, Falam") in metadata.json itself, and a right-to-left project imports as left to right. Reproduced on main 58e8e10b: `docs/evidence/tc4-issues-2026-10-27/repro/832-tc3-rtl-badge.test.mjs`.

## Before you start

- `create_details` in `crates/tc-app/src/import.rs` sets `language_name: String::new()` and `direction: Direction::Ltr`.
- The fixture `cfm_fbt_tit_book.zip` (find it with `git ls-files | grep cfm_fbt_tit`) holds `Chin, Falam`.

## Funnel card

- **Invariants**: none new. The gate test holds two existing ones for an import: INV-TEXT-DIRECTION-HAS-ONE-SOURCE (metadata.json's scriptDirection and what Settings reads agree) and INV-VALID-BURRITO.
- **Code**: tc-app's tC3 import reads the manifest's language name and direction; when the manifest has none, today's behaviour stays.
- **Checks**: tc-app test: importing the cfm fixture writes metadata.json with name "Chin, Falam" and Settings reads it; an rtl tC3 manifest imports rtl.
  - Proof it can fail: the test fails on main 003aa087.
- **Prose**: no new words unless the step says so.
- **Human**: none.

## Steps

### Step 1: A gate test that fails on main
### Step 2: The fix
### Step 3: Review lens(es) the card needs, then `scripts/gates.sh --touched` with main merged in

## Gates before merge

`scripts/gates.sh --touched` with main merged in; the `verify` lines FUNNEL §8 asks for.

## Stop and ask when

- The fix would change what a translator sees beyond the issue (a new word, a new flow).

## References

- tC4 #832; `docs/evidence/tc4-issues-2026-10-27/open.md`.

## Log

- 2026-10-27 09:10 EDT | tc4-issues session (Claude Code dc817a96) | created | none | from the orchestrator: fix the 4.1.0 blockers the tC4 issue sweep found | docs/evidence/tc4-issues-2026-10-27/open.md
- 2026-10-27 09:08 EDT | tc4-issues session (Claude Code dc817a96) | claim | none | claimed on branch track-TC3-LANG in .worktrees/track-TC3-LANG | git log -1
- 2026-10-27 09:39 EDT | tc4-issues session (Claude Code dc817a96) | step 1 | none | gate test fails on main 003aa087: `Tita ltr: metadata.json's language` left name.en "cfm", right "Chin, Falam" | cargo test -p tc-app --test app a_tc3_import_keeps_the_manifest_language_name_and_direction
- 2026-10-27 09:39 EDT | tc4-issues session (Claude Code dc817a96) | step 2 | none | tc-import reads target_language.name and .direction (ltr/rtl into an enum, else none) and the import's create uses them; a manifest without them imports as before; tc-import 90 and tc-app 800+ tests pass | cargo test -p tc-import -p tc-app
- 2026-10-27 09:46 EDT | tc4-issues session (Claude Code dc817a96) | verify | 1a038a3f | loss lens (independent subagent): no blocker, no major; 3 minors. §8: 1 Code: typed Direction enum, one entry point imported_details; 2 Checks: gate test covers ltr, rtl, bare and failed on main; 3 Reach: all reachable, two need hand-edited manifests; 4 Siblings: no other hard-coded empty name or ltr in tc-app/src; 5 No loss: create path unchanged and atomic through staging; 6 Prose: comments give reasons, no owner question | subagent ab78cbafe987b5efb report; .audit/minors.tsv
- 2026-10-27 09:46 EDT | tc4-issues session (Claude Code dc817a96) | fix | none | loss lens minors fixed: a code changed on the review step no longer takes the manifest's name and direction, and the manifest's direction is read case-insensitively; the multi-zip first-wins minor is logged | cargo test -p tc-app --test app import::
- 2026-10-27 09:56 EDT | tc4-issues session (Claude Code dc817a96) | verify | b890cab5 | races and protocol lens (independent subagent): no blocker, no major; 2 minors (a code differing only in case dropped the name and direction: fixed; the test never saves through Settings: logged). §8: 1 Code: typed enum, one entry point; 2 Checks: gate test names both invariants, failed on main, runs in the tc-app suite; 3 Reach: all reachable; 4 Siblings: as the loss lens; 5 No loss: Review and Run parse the same uploads, Run is pure in its inputs, the create stays staged and atomic, settings/save's compare-and-swap holds rtl; 6 Prose: reasons only, no owner question | subagent a287f0b1f629918d7 report
- 2026-10-27 09:56 EDT | tc4-issues session (Claude Code dc817a96) | verify | b890cab5 | browser and parity lens (independent subagent): no blocker; 1 major (prose: the declared difference was missing from docs/APP-DESIGN.md F1: fixed), 1 minor (letter case: fixed). Journey 832-tc3-rtl-badge on b890cab5 in Chromium passed: Settings language name "Chin, Falam", metadata name.en "Chin, Falam", card LTR before (the fixture says ltr) and RTL after a reload. tC4 d310bf3 src/data/import/tc3.ts:41,261,299-300 and shell.ts:182,186-192 read only target_language.id. §8: 1 Code: good; 2 Checks: four cases, failed on main; 3 Reach: all reachable; 4 Siblings: none new; 5 No loss: create unchanged, Burrito validated; 6 Prose: declared difference added, no owner question | subagent af51b9b7d16d3447c report; node --test docs/evidence/tc4-issues-2026-10-27/repro/832-tc3-rtl-badge.test.mjs
- 2026-10-27 09:56 EDT | tc4-issues session (Claude Code dc817a96) | fix | none | the lenses' fixes: the review-step code is compared without regard to letter case (gate test case "CFM"), and docs/APP-DESIGN.md F1 declares the difference from tC4 4.0 with its reason; the save-through-Settings minor is logged | cargo test -p tc-app --test app import::
- 2026-10-27 09:56 EDT | tc4-issues session (Claude Code dc817a96) | gate | none | started scripts/gates.sh --touched in .worktrees/track-TC3-LANG, main 17c9fb18 merged in | scripts/gates.sh --touched
- 2026-10-27 10:34 EDT | tc4-issues session (Claude Code dc817a96) | gate | 77826672 | scripts/gates.sh --touched with main b17d258a merged in: claims, coverage and touched checks 10/10, tc-api endpoints and golden 13/13, tc-app, tc-import and dependents 1049 passed (9 slow, 1 skipped), doc tests ok, 0 journey files picked; its first line board.py --check fails only on REL-1 (read from branch track-REL-1: depends on VRS-GUESS, which has no card, and its ROADMAP row lags its card), so the rest ran by hand with that line skipped | scripts/gates.sh --touched claude/focused-ride-5uzj7p
- 2026-10-27 10:52 EDT | integration lane (Claude Code 3d07d5f8) | merge | 5ebfa874 | merged into main in batch 43 with the H13 addendum, gated by scripts/gates.sh --wait-ci batch-43 | scripts/gates.sh --wait-ci batch-43
