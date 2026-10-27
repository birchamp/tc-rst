---
id: SRC-VER
title: Get source texts names the versions that are installed
track: A
lane: app
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: FIX-R step 5)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1 (a gate test that fails on main).
blocked_by: none
---

# SRC-VER: Get source texts names the versions that are installed

## Goal

The gateway package cards on Get source texts name the notes, words and academy versions installed on this computer (or none when not installed), never a fixed number.

## Why it matters to a translator

Today the English card says v89 while v91 is installed and pinned, so a translator thinks their helps are old. Reproduced on main 58e8e10b: `docs/evidence/tc4-issues-2026-10-27/repro/833-5-source-texts-versions.test.mjs`.

## Before you start

- `GATEWAYS` in `crates/tc-resources/src/gateways.rs` holds `("v89", "v89", "v89")` as `Seen`.
- Find where the card's line is built: `graft ask "Get source texts package card notes words academy version" --source`.

## Funnel card

- **Invariants**: none new.
- **Code**: The card's line reads the installed versions; the fixed `Seen` numbers go if nothing else needs them.
- **Checks**: A test on the en-pins scenario: the English card names v91 for notes, words and academy.
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

- tC4 #833 item 5; `docs/evidence/tc4-issues-2026-10-27/open.md`.

## Log

- 2026-10-27 09:10 EDT | tc4-issues session (Claude Code dc817a96) | created | none | from the orchestrator: fix the 4.1.0 blockers the tC4 issue sweep found | docs/evidence/tc4-issues-2026-10-27/open.md
