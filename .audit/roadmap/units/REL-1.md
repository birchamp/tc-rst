---
id: REL-1
title: First release candidate (4.1.0): the cut scope green, the owner's reviews recorded
track: A
lane: integration
milestone: M5
state: building
owner: integration lane (Claude Code 3d07d5f8)
branch: track-REL-1
worktree: .worktrees/track-REL-1
depends: [C7a, C7b, CHK-1, CHK-2, CHK-3, CHK-6, D1, D2, DOM-1, DOM-2, E1a, E1b, E2, F1, FIX-J, FIX-R, G1, G3, G4, G5, I, INT-6, REF1, S2, SWEEP, T, UI-1, UI-2, UI-3, X, REF1b, REF1c, HIST-1, LOSS-1, E1c, E1d, ALIGN-LEAVE, FIX-KEPT-EVENT, LOSS-2, BURRITO-HOLD, SAVE-WAIT]
needs_owner: []
proves: []
no_loss: false
size: 1 d
merged: none
updated: 2026-10-27
next: step 3: the release run on this branch (unsigned); signing waits for the owner (H4, A4)
blocked_by: none
---

# REL-1: First release candidate (4.1.0)

## Goal

The first public release, 4.1.0, is built by G3's pipeline for Windows, macOS and Linux, from a
main whose gate is green. It holds the scope the owner set on 2026-10-24
(`rulings/2026-10-24-H3-H4-H7-fast-release.md`): everything except the parked cards (AQ1, S3,
DOM-3, CHK-5, V, P, G2) and the cut import kinds. The owner has reviewed the installed app and
said yes.

## Why it matters to a translator

It is the first build translators can install.

## Funnel card

- **Invariants**:
  - INV-NO-UNTRANSLATED-STRING-SHIPS: a release tag builds nothing while an interface key is
    still English (`crates/tc-api/tests/refusals.rs`, release.yml `translations`; H10).
  - INV-A-RELEASE-HAS-NO-TEST-HOOKS: the shipped `tc4` and page carry no test flag, route or
    text (`crates/tc4/tests/release.rs`, release.yml `build`).
- **Wrong states**: a release built from a commit with no gate receipt; a tag pushed without the
  owner's yes; a Windows installer published unsigned.
- **Checks**: release.yml's translations, build and smoke jobs on each H3 platform; the P table of
  step 2.
- **Human**: the owner's yes before a tag (step 4), H4's signing, H10's translations.

## Steps

1. Check that every card in `depends` is done on main and the board shows 0 errors.
2. Run DONE's P1 to P10 checks that do not need a parked card. List the ones skipped and the
   parked card each one waits for.
3. Build the installers with G3's pipeline, signed as H4 rules, and install each one on its
   platform.
4. Send the owner the installers and the list from step 2. Record the owner's yes.

## Done when

The owner's yes is recorded, and the release tag points at a commit
`scripts/gates.sh --check-receipt` accepts.

## Log

- 2026-10-24 | roadmap | created | none | the owner cut scope to finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
- 2026-10-26 11:30 EDT | integration lane (Claude Code 3d07d5f8) | ruling | none | the owner: "Yes, REL-1 waits for LOSS-1." (2026-10-26): depends gains LOSS-1, the 26 §15 loss tests CHK-3 found | message from the orchestrator, 2026-10-26
- 2026-10-26 19:05 EDT | integration lane (Claude Code 3d07d5f8) | ruling | none | depends gains E1c (one project name, owner ruling H14) and E1d (E1a's two unwritten tests), on the orchestrator's word | messages from the orchestrator, 2026-10-26
- 2026-10-26 19:58 EDT | E1d lane (Claude Code 07ffe14c) | ruling | none | the owner, on E1d's follow-up: Home asks Stay or Leave without it once an aligner click is lost for sure; REL-1 waits for ALIGN-LEAVE (orchestrator's message) | rulings/2026-10-26-ALIGN-LEAVE.md
- 2026-10-26 | kept-event session (Claude Code 2e51a251) | ruling | none | depends gains FIX-KEPT-EVENT, on the orchestrator's word: a copy kept just after an open was not told until a reload | message from the orchestrator, 2026-10-26
- 2026-10-26 23:31 EDT | storage lane (Claude Code LOSS-2) | note | none | depends gains LOSS-2 (two store losses the model test excused), on the orchestrator's word: "REL-1 waits for it" | message from the orchestrator, 2026-10-26
- 2026-10-27 05:47 EDT | integration lane (Claude Code 3d07d5f8) | claim | none | Claimed on branch track-REL-1 from main b9d42bb4 (batch 36b), the orchestrator's go | python3 -I .audit/tools/board.py
- 2026-10-27 05:47 EDT | integration lane (Claude Code 3d07d5f8) | step 1 | b9d42bb4 | All 41 cards in depends are done on main b9d42bb4, except UI-3, which is parked because it was split into UI-3a and UI-3b, both done. The board reads 75 cards, 0 errors, 4 warnings (stale updated dates on T and X, and two more). Green on b9d42bb4: ci 40480182362 (test, test-app, parity-baseline, alignment-budget, lint-linux, lint-windows, lint-wasm), app-e2e 40480182382 (journeys) and shell 40480182361 (unit, builds, e2e-linux, e2e-windows, e2e-macos) | scripts/gates.sh --check-receipt b9d42bb4
- 2026-10-27 05:47 EDT | integration lane (Claude Code 3d07d5f8) | note | none | release.yml has never passed: 30 failed runs and 1 cancelled. The late G3 runs and gate/batch-7 failed only at translations, which is expected off a tag while 264 keys are English (H10). Since run 40349886927, common also fails: notices.test.mjs runs cargo offline, and adler2 (a test-only crate the screen record added in 8412f07f) is never downloaded by the job's builds. Fixed: common runs cargo fetch --locked first. Locally the test passes after a fetch (3 of 3) | node --test scripts/release/notices.test.mjs
- 2026-10-27 05:49 EDT | integration lane (Claude Code 3d07d5f8) | step 2 | b9d42bb4 | P table written from the ledger, every row with its job and green run on b9d42bb4: P1, P2, P3, P4, P5 and P8 green, plus a security row. P7 is green for the screens it records, P9 for the kept scope, and P10's pins are green. Skipped: P6 waits for AQ1; the full P7 review waits for G2; the P10 freeze waits for V (H9); P9's cut import kinds by the owner's ruling. The owner list is in the evidence | docs/evidence/rel-1-2026-10-27.md
