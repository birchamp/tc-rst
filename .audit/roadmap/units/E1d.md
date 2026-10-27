---
id: E1d
title: E1a's two unwritten tests
track: A
lane: app
milestone: M3
state: todo
owner: none
branch: none
worktree: none
depends: [E1a]
needs_owner: []
proves: [P2]
no_loss: true
size: 0.5 d (basis: two tests over existing code)
merged: none
updated: 2026-10-26
next: Claim it and write both tests (the shell lane, at the audit session's request).
blocked_by: none
---

# E1d: E1a's two unwritten tests

## Goal

E1a's card named two tests that were not written when E1a landed. This card writes them, so a
refused save or a refused open after a sync never traps the translator or loses the way to the
work the sync brought (tC4 #724, #801, #807).

## Why it matters to a translator

After a sync, the toast's Review and Download are how a translator reaches what teammates sent,
and Home is how they leave. A refusal must not take either away.

## Funnel card

- **Invariants**:
  - INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR (E1a's): a save refused after a sync keeps
    its text and never holds Home.
  - INV-THE-SYNC-TOAST-OUTLIVES-A-REFUSED-OPEN: when the open that Review or Download starts is
    refused, the toast stays with both links, and the next press tries again.
- **Checks**:
  - `failed_open_keeps_the_sync_toast_links` in `crates/tc-ui/tests/app/sync.rs` (or a journey
    in `parity/app/journeys/conflict-review.test.mjs` if the UI's pure side cannot hold it): a
    sync toast with Review and Download; the open each starts is refused (the project locked or
    gone); the toast is still shown with both links and its counts, and the next press opens
    again (#724). The J18 journey "A refused open keeps the sync toast and its Review" already
    covers Review; this test adds Download, which today has no refused-open test
    (the toast's Download action in `crates/tc-ui/src/app/web/app_ctx.rs`).
  - `a_refused_align_save_behind_a_span_save_lets_home_leave` in
    `crates/tc-ui/tests/app/save.rs`: an alignment save refused as stale after a sync while a
    verse span save of the same chapter waits; Home leaves in one click, both texts and the
    alignment stay as drafts (#807, #801). `docs/TC4-ISSUES.md` #807 says the rebuild has no
    alignment-save queue for a span save to wait behind; if that holds, the test proves it
    (the span save does not wait on the alignment save) rather than being skipped, and the #807
    row names it.
  - Proof it can fail: for each, the mutation that breaks it (clearing the toast on a refused
    open; a span save that waits on the alignment save), with the failing line in the log.
- **Human**: none.

## Steps

### Step 1: Write both tests

- Do: as the Checks say; update `docs/TC4-ISSUES.md` #724 and #807, and E1a's card so its
  Checks name the tests without `(pending: E1d)`.
- Done when: both pass, each fails under its mutation, and
  `node --test .audit/tools/claims.test.mjs` passes with this card done.

## Log

- 2026-10-26 20:30 EDT | E1b session (Claude Code fd622395) | created | none | Split off E1a's two unwritten tests so E1a, E1b and E1c land (audit session's request); the shell lane writes them | .audit/roadmap/units/E1a.md
