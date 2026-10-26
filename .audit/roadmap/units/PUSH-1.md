---
id: PUSH-1
title: A large push goes to Door43 as a chain of commits, so a poor link can finish it
track: A
lane: storage
milestone: M2
state: verifying
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
next: The three lenses, loss first; then gates --touched.
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

- GitHub issue: https://github.com/birchamp/tc-rst/issues/2.
- Owner ruling, 2026-10-26: "REL-1 doesn't wait for PUSH-1; it lands in the next release."
- `docs/STORAGE.md` §9.1 S7, §9.5 "Large pushes are a chain", §14, §15 L96; `.audit/roadmap/units/LOSS-1.md`.

## Log

- 2026-10-26 12:50 EDT | LOSS-1 lane (Claude Code d63a7912) | created | none | at the orchestrator's word: LOSS-1 found §9.5's push chain is not in the code (one `git::push` of HEAD in S8), so L96 has no protection to test | crates/tc-sync/src/sync.rs
- 2026-10-26 13:18 EDT | PUSH-1 lane (Claude Code e939c6d5) | claim | none | claimed at the orchestrator's word, on a branch from track-LOSS-1 | .worktrees/track-PUSH-1
- 2026-10-26 13:32 EDT | PUSH-1 lane (Claude Code e939c6d5) | ruling | none | owner, 2026-10-26, through the orchestrator: "REL-1 doesn't wait for PUSH-1; it lands in the next release."; the card links issue #2 at the owner's ask | https://github.com/birchamp/tc-rst/issues/2
- 2026-10-26 15:12 EDT | PUSH-1 lane (Claude Code e939c6d5) | step 1 | none | C5 and S7 write a chain when a commit's new blobs pass 16 MiB: each link is its base's tree with more new packs and their metadata.json, taken only while FoldState::completes_batch says no batch becomes whole, so it keeps its base's derived files; a_large_commit_is_a_chain_of_valid_commits passes and fails with the chain off (bound u64::MAX) | cargo test -p tc-store --test store a_large_commit_is_a_chain
- 2026-10-26 15:13 EDT | PUSH-1 lane (Claude Code e939c6d5) | step 2 | none | S8 pushes the commits between Door43's commit and HEAD as links of up to 16 MiB, each from refs/tc/push-link and read back before the next; a failure names the last confirmed link; three sync.unreachable failures of one chain (counted in .tc/sync.json chainFailures) set offer_exchange; sync_large_push_chain_resumes and a_large_push_that_keeps_breaking_offers_the_exchange_file pass and both fail with links_of off; STORAGE §9.5 and L96 updated | cargo test -p tc-sync --test sync -- sync_large_push_chain_resumes a_large_push_that_keeps_breaking
- 2026-10-26 16:34 EDT | PUSH-1 lane (Claude Code e939c6d5) | verify | 7bca94e9 | loss lens (an independent subagent): merge, no blocker or major. 1 Code: store builds the chain, one push entry point. 2 Checks: the three tests pass and fail with the chain off; a merge-path test passed. 3 Reach: large first share, long offline history, split batch received. 4 Siblings: checkpoint and merge, sync.json writers. 5 No loss: none under crashes, full disk, races, consolidated, held or released batches, partial chain on Door43. 6 Prose: S7 and three comments stale | crates/tc-store/tests/store/intake.rs; crates/tc-sync/tests/sync/door43.rs
- 2026-10-26 16:34 EDT | PUSH-1 lane (Claude Code e939c6d5) | fix | none | loss-lens minors: the failure count no longer resets when saves make a new tip between tries; S7 prose and three comments name the last link as a chained merge's second parent; a_large_merge_is_a_chain_grown_from_door43s_commit kept; the offer's app wiring and non-app commits in an adopted history went to minors.tsv | cargo test -p tc-store --test store -- a_large_; cargo test -p tc-sync --test sync -- a_large_push sync_large
- 2026-10-26 17:24 EDT | PUSH-1 lane (Claude Code e939c6d5) | verify | 7d2e6362 | races and protocol lens (an independent subagent): fix_first, one major. 1 Code: push and merge chains sound under races; recreated lost when a recreating chain breaks. 2 Checks: L96 and the offer test; probes showed a teammate push between links and a chained first share work. 3 Reach: big first share, upgrade, USB-received project, deleted branch on a poor link. 4 Siblings: share goes through sync(); join and connect updated. 5 No loss: none in any race run; copies converge. 6 Prose: §9.5 should say the H5 notice rule | crates/tc-sync/tests/sync/door43.rs
- 2026-10-26 17:24 EDT | PUSH-1 lane (Claude Code e939c6d5) | verify | 7d2e6362 | browser and parity lens (an independent subagent): merge, no blocker or major. 1 Code: S8's one push becomes push_chain; tc-app, tc-ui, tc-api untouched. 2 Checks: the chain tests pin the outcome; no journey affected. 3 Reach: chain reachable; the offer is not yet (minor). 4 Siblings: toast paths unchanged. 5 No loss: a link keeps its base's derived files. 6 Prose: tC4 (d310bf3, syncOperation.ts) pushes once and never resumes; declare the chain in APP-DESIGN; the offer needs words tC4 lacks, owner to be asked under Q-9 by the card that wires it | docs/APP-DESIGN.md; .audit/minors.tsv
- 2026-10-26 17:24 EDT | PUSH-1 lane (Claude Code e939c6d5) | fix | none | races major: a deleted branch put back by the first links of a chain that then broke was never said; .tc/sync.json putBackUnsaid keeps the H5 notice for the next sync that gets through; a_branch_put_back_by_a_broken_chain_is_said_by_the_next_sync passes and fails without the fix. Also: the last push of a resumed chain counts toward the offer; APP-DESIGN declares the chain; four minors to minors.tsv | cargo test -p tc-sync --test sync -- a_branch_put_back sync_large_push a_large_push_that_keeps
