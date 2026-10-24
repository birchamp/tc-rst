# The journal's files as one state machine: assessment, 2026-10-21

Asked by the owner on 2026-10-20 ("a state machine could be helpful for states of the
journal"). Read-only workflow `wf_a8ba05c0-f1e`: four readers mapped the code at the S2
working tree, one designer proposed a machine, two critics reviewed it. The full output is
kept in `.enjoy-lane/journal-machine-assessment-wf_a8ba05c0.json` (ignored, local).

## What exists

- Derived files (book USFM, `metadata.json`) already run through one typed machine: the
  placement ledger (`ledger.rs`: `State`, `Event`, `Facts`, pure `next()`, a row table, an
  exhaustive matrix, the model property test).
- The journal's own files (pending files, their temps, packs and pack temps, quarantine
  copies, salvage images, not-saved copies) have no single machine. About 17 call sites in
  `seal.rs`, `open.rs`, `temps.rs`, `packs.rs` and `writer.rs` delete or overwrite them,
  and a file's state lives in five sets and two booleans in `project.rs` (`Journal`,
  `PendingInfo`). Smaller typed pieces exist: `SealPhase`, `Found`, `recovery::next`,
  `FileSource` slots.
- Most of the journal bugs found on 2026-10-20 (salvage alias L128, same-path salvage,
  quarantine reuse, the sharing-wait backup loss, changed copies, dismissal identity,
  copy-name exhaustion, flush order) sit in that scattered code.

## Recommendation

Build it, in slices, as the next storage unit after S2 merges; not on the S2 branch, whose
validation and 43 mutation recipes are bound to the current lines.

1. Extend the independent model first so it reaches what it cannot today (a save held past
   its retries, many consolidations in one session, reused names, changed or deleted
   copies, foreign and newer files, read denial during open or seal). Each failure becomes
   a named BEFORE witness. No product change.
2. Turn the inferred defects below into public BEFORE tests; drop any that do not fail.
3. Add `journal_file::next()` with its row table and a matrix split per file kind (one
   combined fact space is about 13,800 combinations per kind pair, too many for one
   matrix), with no callers; then run it in shadow beside the current code in test builds.
4. Switch over in slices: copies; then pending-file retirement with salvage, quarantine and
   the source slots; then packs; then temps. The first two slices cover every journal bug
   of 2026-10-20.

The critics judged the design sound with gaps and the designer's time estimate optimistic:
about one to one and a half weeks for the first two slices with their proof, two to three
weeks for all four (estimates from the ledger's history, not measurements). Corrections they
require before code: keep the prior state on an unreadable read (as the ledger does), keep
the `replaces` and holder guards on pack retirement, never let dismissal remove a not-saved
attempt or a set-aside line, and give a missing pack's restore an "expect absent" check.

## Inferred defects to test (none reproduced yet)

- H1: a pack write that fails after its rename leaves a pack the session does not know; the
  next checkpoint takes it for a second writer and the project turns read-only
  (`seal.rs:154-181`, `seal.rs:653-665`, `actor.rs:406-477`). User harm.
- (e): a reopened quarantine copy whose bytes changed raises a notice whose identity is the
  changed bytes, so dismissing it deletes bytes the app never preserved
  (`open.rs:1491-1502`, `project.rs:337-341`). Data harm if it reproduces.
- An expected pack that a sync client restores mid-session is taken for a second writer
  (`seal.rs:656-664`, `actor.rs:261`, `packs.rs:126-136`).
- `write_back` replaces a damaged pack it cannot read without a quarantine copy
  (`packs.rs:192-201`).
- O2b's collision test ignores unreadable pending files (`temps.rs:202-209`); a
  not-saved copy's move can replace an occupied last name (`temps.rs:274-280`); newer-format
  temps are removed (`temps.rs:74-75`, `temps.rs:86`).

What the machine alone would not have caught: the twelve-consolidation chain and the
duplicate-span bug (routing and fact producers; the extended model finds those), fold
semantics in `tc-journal`, and platform behavior `FaultFs` cannot express.
