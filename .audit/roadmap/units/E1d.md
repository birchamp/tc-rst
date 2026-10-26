---
id: E1d
title: E1a's two unwritten tests
track: A
lane: app
milestone: M3
state: verifying
owner: E1d lane (Claude Code 07ffe14c)
branch: track-E1d
worktree: .worktrees/track-E1d
depends: [E1a]
needs_owner: []
proves: [P2]
no_loss: true
size: 0.5 d (basis: two tests over existing code)
merged: none
updated: 2026-10-26
next: Both tests written and proved; three lenses, then the touched gate and the hand-over to the integration lane.
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
  - `failed_open_keeps_the_sync_toast_links`, journey in `parity/app/journeys/sync-button.test.mjs`
    (the toast's actions are the browser's, not the UI's pure side): a
    sync toast with Review and Download; the open each starts is refused (the project locked or
    gone); the toast is still shown with both links and its counts, and the next press opens
    again (#724). The J18 journey "A refused open keeps the sync toast and its Review" already
    covers Review; this test adds Download, which today has no refused-open test
    (the toast's Download action in `crates/tc-ui/src/app/web/app_ctx.rs`).
  - `a_refused_align_save_behind_a_span_save_lets_home_leave`, journey in
    `parity/app/journeys/align.test.mjs` (the aligner's gate hook is the browser's): an alignment save refused as stale after a sync while a
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
- 2026-10-26 18:55 EDT | E1d lane (Claude Code 07ffe14c) | claim | none | Claimed at the audit session's request; worktree from track-E1c | .audit/roadmap/units/E1d.md
- 2026-10-26 19:10 EDT | E1d lane (Claude Code 07ffe14c) | step 1 | e23d8f7c | Both tests are journeys. The Download half of #724 already held; planting `sync_result.try_set(None)` before Download's open fails it with `INV-THE-SYNC-TOAST-OUTLIVES-A-REFUSED-OPEN: the toast stays when its open is refused`. #807 was a real bug in another form: the aligner's gate hook held Home for any refused save of the links, so a refusal no retry can fix trapped the translator in the aligner (`INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR: Home is reached in one click after a refused alignment save` before the fix). Fixed in `start_align_hook`: such a refusal passes, as a field's does (#801); a full disk still holds (queued_links_hold_the_close_guard_through_save_failure_and_retry). The rebuild has no span-save queue and the aligner no text field, so the span half is not reachable; the #807 row says so | node --test parity/app/journeys/sync-button.test.mjs parity/app/journeys/align.test.mjs (21 pass)
- 2026-10-26 19:20 EDT | loss lens (Claude Code subagent of 07ffe14c) | verify | e23d8f7c | No blocker. M1: the gate could pass with a click still queued on the page after its send failed, and drop it. M2: the journey's reopen check was vacuous (the faked reply let the store save the links). Minors: nothing proves Disk still blocks the gate (it does: the close-guard journey); with a second writer both installations share `.tc/drafts.json`, older than this diff. Six questions: code yes after M1; checks red without the fix; reach M1 and M2 reachable; siblings (restore, tab switch, close guard) fine; no loss after M1; prose fine | code reading
- 2026-10-26 19:28 EDT | E1d lane (Claude Code 07ffe14c) | fix | see branch | M1: the hook passes only with no click pending (`!edits_pending()`). M2: the reopen half is gone; the draft's keeping is the server's (`AlignCache::finish`, drafts tests). The journey still fails with the hook's old answer | node --test parity/app/journeys/align.test.mjs (16 pass)
- 2026-10-26 19:30 EDT | races and protocol lens (Claude Code subagent of 07ffe14c) | verify | 12074edd | No blocker. Major (pre-existing, not this diff): a page edit the server refuses stays queued as `Refused`, so the gate holds; those clicks live only in the page, so holding keeps them, and it is a follow-up for the owner's lane, not this card. Minors: the journey did not count hits on the faked flush (fixed); the toast check ran right as the error appeared (now after 500 ms); `refused_for_good` reads the map entry the flush just wrote, so it is current on the path that matters. Six questions: code fine; checks red without the fix; reach yes; siblings: the refused edit above; no loss; prose fine | code reading
- 2026-10-26 19:33 EDT | browser and parity lens (Claude Code subagent of 07ffe14c) | verify | 12074edd | Both journeys pass in Chromium. Major: after a refused alignment save Home says nothing, where tC4 keeps Home blocked with the refusal shown; PLAN's #801 difference named a field's text only. Minors: the name promises a span save the test cannot make (kept: the card names it, the comment says why); the faked reply never shows a lasting refusal (the server's draft tests now named); no proof the faked flush ran (fixed). Six questions: code fine; checks deterministic and in CI; reach yes (BadToken, StaleScope); siblings: the aligner is the only gate hook; no loss on disk is the server's tests; prose needed the declared difference | scratchpad probe, node --test sync-button align
- 2026-10-26 19:35 EDT | E1d lane (Claude Code 07ffe14c) | fix | see branch | PLAN's declared difference now covers the aligner's links (#801, #807): kept for the next open's offer, Home not blocked, a full disk still holds; the #807 row names the server's draft tests. The journey counts the faked flush; the toast check waits 500 ms. Owner to be told: Home after a refused alignment save says nothing, as after a refused field | node --test --test-name-pattern="failed_open_keeps|a_refused_align_save" (2 pass)
