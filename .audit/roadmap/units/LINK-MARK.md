---
id: LINK-MARK
title: A link a save sends back to the word bank stays marked for review across a crash
track: A
lane: storage
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: [LINK-DROP]
needs_owner: []
proves: [P2]
no_loss: true
size: 1-2 d (basis: LINK-DROP plus a stored record field and its fold rules)
merged: none
updated: 2026-10-27
next: After 4.1.0: claim this card, then do step 1.
blocked_by: none
---

# LINK-MARK: A link a save sends back to the word bank stays marked for review across a crash

## Goal

When a verse or section save sends an alignment link back to the word bank, the store keeps a
"links to review" mark on that verse's alignment, and Align shows it until the person reviews the
verse, so the notice survives a crash, a reload and another computer.

## Why it matters to a translator

LINK-DROP names the links on Translate, but only in the page. A crash between a save's write and
its reply loses the notice; the link stays in History with nothing pointing to it. The owner
accepted that for the 4.1.0 preview (orchestrator, 2026-10-27, under
`rulings/2026-10-22-acceptable-loss.md`) and asked for this card after it.

## Before you start

- LINK-DROP is merged: `git log --oneline --grep LINK-DROP claude/focused-ride-5uzj7p`.
- Read `docs/STORAGE.md` on the alignment record and its fold before you change either.

## Funnel card

- **Invariants**:
  - INV-NO-SILENT-LINK-DROP (LINK-DROP's) holds across a crash.
- **Code**: a field on the alignment record (or its own record) the reflow sets; Align's rail and
  open read it; a review clears it. Format change: a capsule note for V.
- **Checks**: a crash-injection test between the write and the reply; Align shows the mark after
  the restart.
- **Human**: none.

## Steps

### Step 1: Design the stored mark against STORAGE.md; get it reviewed before code

## Log

- 2026-10-27 12:30 EDT | tc4-issues session (Claude Code dc817a96) | created | none | the owner, through the orchestrator: the crash gap in LINK-DROP is a declared limit for the preview, and this card follows after 4.1.0 | docs/PLAN.md (declared differences)
