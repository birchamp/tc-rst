---
id: XXX-0
title: One line a translator or a developer understands
track: A
lane: app
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: false
size: 1 d (basis: a comparable finished item and how long it took)
merged: none
updated: 2026-10-22
next: Claim this card, then do step 1.
blocked_by: none
---

# XXX-0: Title

## Goal

Two or three sentences: what is true when this is done.

## Why it matters to a translator

One or two sentences. If a translator never notices it, say what it protects.

## Before you start

Facts you can check, each with the command that checks it:

- `git merge-base --is-ancestor <sha> HEAD` shows <dependency> is on your branch.
- ...

## Funnel card

- **Invariants**:
  - INV-<NAME>: ...
- **Wrong states**:
  - ...
- **Code**:
  - Types:
  - One entry point:
  - Removed:
  - Left to checks, and why:
- **Checks**:
  - `<test_name>` in `<file>`; kind: ...; runs in: ...
  - Proof it can fail:
- **Prose**:
- **Human**:

## Steps

### Step 1: <verb phrase>

- Do: ...
- Files: ...
- Done when: `<command>` prints ... / `<test>` passes, and fails with the fix reverted.
- Record: a log line `step 1`.

## Gates before merge

The full gate list (`scripts/gates.sh` once INT-3 lands; until then every command in `AGENTS.md`
"Tests" plus the Windows clippy step from `.github/workflows/ci.yml`), plus:

- ...
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- ...

## References

- ...

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/ASSESSMENT.md
