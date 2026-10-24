History: superseded by .audit/roadmap/ on 2026-10-21; do not update.

# Building the rebuild

This is the operating policy for the next completed-stage boundary. It changes how work is
scheduled and evidence is reused; PLAN P1–P10 and AGENTS.md remain the acceptance rules.

## Finish the critical path

Give S2 one named owner, a dedicated worktree and first claim on the heavy-test slot. Its
placement ledger and crash recovery unblock P, T, X, I and V, plus the app's checkpoints and
exports. Keep already running independent app work; schedule new work by what it unblocks.
Record S2's next failing test or missing proof and its next concrete action at each handoff.
The process tools do not finish S2 or assign an owner automatically.
While that storage owner is active, set the app lane's `maxActiveUnits: 1`; after S2 finishes,
its default of two app pipelines can use both slots. Count the storage pipeline in the budget.

Before adding risky save, merge, recovery or import logic, write the typed states and events,
the exhaustive pure transition function and its independent model. Seed known loss bugs into
tests first. Exercise crashes and restarts through the existing fault hooks, and validate every
written project as a Burrito. Settle shared public types before dependent implementations.

## One review, reproducible evidence

Keep the independent review lenses required for no-loss work. Reviewers read source and
commit-bound evidence; they never edit, revert, reset or clean the builder's worktree. One
executor runs requested tests and shares the exact command, commit, exit status and log with
all reviewers. A new finding needs a reachable scenario and reproducible evidence.

One mutation executor checks that regressions detect the bug, in its own disposable checkout
and target directory. It records the mutation and the failing test, restores the source, then
proves the clean test passes. Concurrent reviewers never mutate a shared tree. Recheck a fix
and its affected behavior; reopen the wider review only when new evidence warrants it.

Fix every blocker and majors that harm a translator's data or what they see. Log internal
majors and minors in `.audit/minors.tsv`. A failed required gate remains a failed gate; this
finding policy cannot waive a safety test, release check or acceptance requirement.

## Preserve the gates, resume the work

Use `tools/lane-state.mjs` for durable unit/stage reports, and `tools/run-gates.mjs` for command
receipts and logs. Their `--help` gives the current CLI. Completed reports bind to a clean
current HEAD; a moved HEAD or changed checkout needs fresh evidence. Historical reports are
handoff information, never proof for a different revision.

Run focused checks while implementing, then the complete integration gate prescribed by
AGENTS.md and CI. Keep workspace tests, crash/no-loss/model properties, Burrito validation,
browser journeys and parity for the changed feature, wasm feature variants, and the release
paths without test hooks. Run the unit's supplemental commands through the runner too; the
default Cargo commands alone do not prove browser, platform or performance acceptance.
Existing before-commit requirements still apply.
Gather a coherent change with focused checks while editing, then run every required
before-commit check before committing it. The later integration gate remains mandatory.

Reuse a successful command only when the gate runner accepts its receipt for the same clean
revision, command and recorded environment. Changed code, toolchain, features, fixture inputs
or command configuration invalidate the relevant evidence. A failed or interrupted command
is rerun; an interruption does not discard earlier valid receipts. No missing receipt counts
as a pass. Dirty development runs always execute and do not produce reusable passing receipts.
Separate per-worktree targets remain mandatory.

The gate runner checks prerequisites before expensive work and serializes heavy runs through
a lock shared by worktrees. Report a busy slot and preserve its owner; never kill another run
or remove its live lock to make progress. Commands launched outside the runner do not acquire
its lock, so inspect existing work before adopting it. Windows and release-pipeline checks
remain required where the design or CI calls for them.

Record unit, stage, revision, start/end time, commands, result, logs and next action. Compare
build, review, fix, gate and queue durations separately. Wall elapsed between commits includes
interruptions and waiting; it is not a measure of active agent work.
Call `lane-state begin` before each attempt and `save` after completion. Fixer reports keep the
prior review evidence and the next targeted validation/check stage; a verified checkpoint
keeps the full review-and-fix chain. Old reviews never become proof for a changed SHA.

## Adopt at the next boundary

1. Let the current stage finish. Running sessions have already loaded their scripts; editing
   these files neither updates those sessions nor restarts their agents.
2. In the actual builder worktree, inspect HEAD, local changes, processes and the last completed
   report. Preserve unfinished work and logs. Do not take a stale STATUS file as current truth.
3. Include the process changes and ignore rules in that branch, then use the state and gate
   tools for the next stage. Reuse only receipts the tools accept;
   record missing evidence as pending. Point reviewers to the shared evidence and give any
   mutation work to the isolated executor.
4. Name the S2 owner and next proof, then continue independent app work within the resource
   budget. Keep the full integration and platform checks before a verified merge.
5. At the verified milestone, update HANDOFF/PROGRESS and regenerate STATUS from the correct
   ref. Record that adoption happened, its revision and the next stage. Until then the new
   policy is prepared, not active in the other running conversation.
