---
id: REL-1
title: First release candidate (4.1.0): the cut scope green, the owner's reviews recorded
track: A
lane: integration
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: [C7a, C7b, CHK-1, CHK-2, CHK-3, CHK-6, D1, D2, DOM-1, DOM-2, E1a, E1b, E2, F1, FIX-J, FIX-R, G1, G3, G4, G5, I, INT-6, REF1, S2, SWEEP, T, UI-1, UI-2, UI-3, X, REF1b, REF1c, HIST-1, LOSS-1, E1c, E1d, ALIGN-LEAVE, FIX-KEPT-EVENT]
needs_owner: []
proves: []
no_loss: false
size: 1 d
merged: none
updated: 2026-10-26
next: Claim this card when its dependencies merge.
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
