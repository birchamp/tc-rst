---
id: TOAST-ORDER
title: The sync toast names its teammates in the same order on every run
track: A
lane: app
milestone: M5
state: gating
owner: tester findings lane (Claude Code 76125a0f)
branch: track-REL1-tester
worktree: .worktrees/track-REL1-tester
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.1 d (basis: one sort)
merged: none
updated: 2026-10-27
next: scripts/gates.sh --touched with main merged in, then push and hand to the integration lane.
blocked_by: none
---

# TOAST-ORDER: The sync toast names its teammates in the same order on every run

## Goal

FIX-791's "Changes from: teammate-1 · teammate-2" listed senders by device id, which is random,
so the order changed between runs (the integration lane's recapture of `sync-ok-teammates`).
`tc_app::sync::sort_senders` orders them by name (Door43 or given), then unnamed teammates by
number.

## Funnel card

- **Invariants**:
  - INV-TOAST-SENDERS-STABLE: the toast's "Changes from" order depends on the names alone, never
    on device ids.
- **Checks**: `senders_are_named_in_name_order_whatever_their_devices` (tc-app lib, sync.rs);
  without the sort the names stay in the order given and it fails.
- **Prose**: none. **Human**: none.

## Log

- 2026-10-27 13:17 EDT | tester findings lane (Claude Code 76125a0f) | created | none | Asked by the audit session after the integration lane saw the order change between captures | parity/app/captures.json sync-ok-teammates
- 2026-10-27 13:17 EDT | tester findings lane (Claude Code 76125a0f) | claim | none | Claimed with the fix on track-REL1-tester | none
- 2026-10-27 13:17 EDT | tester findings lane (Claude Code 76125a0f) | verify | none | Self-review only: a pure sort with its unit test; the toast's other fields are untouched | crates/tc-app/src/sync.rs
