---
id: TC3-LANG
title: A tC3 import keeps the project's language name and direction
track: A
lane: app
milestone: M5
state: claimed
owner: tc4-issues session (Claude Code dc817a96)
branch: track-TC3-LANG
worktree: .worktrees/track-TC3-LANG
depends: []
needs_owner: []
proves: []
no_loss: true
size: 0.5 d (basis: FIX-R step 1's Settings reader)
merged: none
updated: 2026-10-27
next: Step 1: a tc-app import gate test that fails on main.
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

- **Invariants**: none new.
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
