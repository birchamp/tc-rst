---
id: PUSH-1
title: A large push goes to Door43 as a chain of commits, so a poor link can finish it
track: A
lane: storage
milestone: M2
state: claimed
owner: PUSH-1 lane (Claude Code e939c6d5)
branch: track-PUSH-1
worktree: .worktrees/track-PUSH-1
depends: [LOSS-1]
needs_owner: []
proves: [P1]
no_loss: true
size: 2-3 d (basis: X's push and confirm, steps 4-5, about a day, plus the chain's tree builds)
merged: none
updated: 2026-10-26
next: Step 1, the chain in the store.
blocked_by: none
---

# PUSH-1: A large push goes to Door43 as a chain of commits, so a poor link can finish it

## Goal

`docs/STORAGE.md` §9.5 "Large pushes are a chain" is built: when the new blobs of a merge or
checkpoint commit exceed 16 MiB, the commit is a chain of valid commits, each pushed and
confirmed on its own, so a dropped connection loses only the commit in flight, and the next sync
goes on from the last confirmed one. After three `sync.unreachable` failures of one chain the
report offers "Save an exchange file instead" (§10). §15 row L96 names its test,
`sync_large_push_chain_resumes`.

## Why it matters to a translator

Today one sync sends everything new in one push, and a push cannot resume. A translator can reach
this now: the first share or sync of a project that already holds a lot of work (a whole Bible:
packs about 70 MiB compressed, §14), a resource upgrade over a checked Bible (one batch of about
150 MB, §9.5), or a USB-received project synced for the first time. On a slow or unstable link the
push breaks part way every time; each sync starts it again from nothing and ends with "Not sent"
(`syncResult.error.notSent`, or `cloneFailed` before intake). The work stays safe on the computer,
but it never reaches Door43 or the team, and nothing offers the exchange file instead.

## Before you start

- `git merge-base --is-ancestor <LOSS-1's merge> HEAD`: LOSS-1's sync tests are on your branch.
- `grep -n 'L96' docs/STORAGE.md` shows `sync_large_push_chain_resumes` `(pending: PUSH-1)`.
- `grep -rn 'git::push' crates/tc-sync/src/sync.rs` shows the one push of S8 this changes.

## Funnel card

- **Invariants**:
  - INV-VALID-BURRITO (§9.1 S7): every commit of a chain is a valid burrito whose
    derived files equal the fold of its own packs.
  - INV-PUSH-IS-NEVER-FORCED: each link is a fast-forward of the last confirmed one.
- **Wrong states**:
  - A dropped connection in a large push makes the next sync send it all again.
  - A link of the chain holds a derived file its packs do not make (a reader or §11.2's intake
    check would take it for a hand edit).
  - A chain is pushed past a link that was not confirmed.
- **Code**:
  - Types: the chain as a list of commits with their blob sizes, built by the store (S7 or C5).
  - One entry point: S8's push in `tc-sync/src/sync.rs` pushes the links one at a time and
    confirms each.
  - Removed: nothing.
  - Left to checks, and why: the 16 MiB bound is a number, checked by the test with a lower
    bound set through the store's test hooks.
- **Checks**:
  - `sync_large_push_chain_resumes` (L96) in `crates/tc-sync/tests/sync/door43.rs`; kind:
    integration against `tc-door43-fake` (`Fault::BreakOff` on the second link's
    `git-receive-pack`); runs in: CI `cargo test --workspace`.
  - A store test that every link of a chain validates as a burrito and its derived files equal
    the fold of its packs.
  - Proof it can fail: push the whole chain in one push; the test's resume assertion fails.
- **Prose**: §9.5 if the build differs from it; §15 L96's test cell.
- **Human**: none.

## Steps

### Step 1: The chain in the store

- Do: S7 and C5 build the chain when the new blobs exceed the bound (§9.5).
- Files: `crates/tc-store/src/checkpoint.rs`, its tests.
- Done when: the store test passes, and fails with the chain collapsed into one commit.
- Record: a log line `step 1`.

### Step 2: Push and confirm link by link; the exchange file after three failures

- Do: S8 pushes each link and confirms it; a failure reports the last confirmed link; after
  three `sync.unreachable` failures of one chain the report offers the exchange file.
- Files: `crates/tc-sync/src/sync.rs`, `crates/tc-sync/src/machine.rs`,
  `crates/tc-sync/tests/sync/door43.rs`, `docs/STORAGE.md` §15 L96.
- Done when: `sync_large_push_chain_resumes` passes, and fails with one push of the whole chain.
- Record: a log line `step 2`; `state: verifying`.

## Gates before merge

`scripts/gates.sh`, plus:

- The review gate of `FUNNEL.md` §8: three `verify` log lines (no-loss item, one per lens; see
  `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that
  harms a translator's data or view.

## Stop and ask when

- A link of the chain cannot be a valid burrito without changing what a commit holds (§9.4).
- The offer of the exchange file needs a screen or words tC4 does not have.

## References

- `docs/STORAGE.md` §9.1 S7, §9.5 "Large pushes are a chain", §14, §15 L96; `.audit/roadmap/units/LOSS-1.md`.

## Log

- 2026-10-26 12:50 EDT | LOSS-1 lane (Claude Code d63a7912) | created | none | at the orchestrator's word: LOSS-1 found §9.5's push chain is not in the code (one `git::push` of HEAD in S8), so L96 has no protection to test | crates/tc-sync/src/sync.rs
- 2026-10-26 13:18 EDT | PUSH-1 lane (Claude Code e939c6d5) | claim | none | claimed at the orchestrator's word, on a branch from track-LOSS-1 | .worktrees/track-PUSH-1
