# S2 oracle-independence probes — prepared, not executed

These two probes supplement the 37 standard mutation recipes. They do not change the
standard requirement that each mutant fail every required exact regression. The generic
model is allowed to pass under a supplemental mutant: that result may demonstrate the
oracle gap being investigated and must be retained rather than rejected as a bad run.
No clone, Cargo invocation, source mutation, stage or gate has run for these probes.

The owner must supply GO for an exact clean revision containing adopted process tools.
Use the single isolated executor sequentially, with its existing isolated source/target
and the owning repository's heavy lock. Do not start another clone, pipeline or executor.
Never modify the builder checkout. Retain temporary oracle patches separately from product
mutations; apply the same temporary generic/control tests to clean, mutant and restored
trials. Record original, temporary-baseline, mutant and restored source hashes and diffs.
Restore exact source bytes in `finally`; never clean/reset away unexpected changes.

For each probe, use three separate exact Cargo invocations in each phase: its generic
witness test, its independent control test, and the existing 200-case
`model::random_histories_lose_no_work`. Keep named histories enabled. Each invocation
must run exactly one libtest test and receive its own fresh gate receipt and retained
timed log. Source `~/.tc-env` before launching the outer executor, use offline Cargo and
the isolated target, and remove `TC_MODEL_NO_NAMED` from the test environment.

All clean-before and restored commands must pass. The mutant independent control must
fail the exact named test with Cargo exit 101, a runtime assertion/panic, its unique
assertion marker, and a one-test failed summary. A compiler error, omission, timeout,
preflight denial or interrupted command is incomplete evidence. For the mutant generic
witness, accept either an exact one-test pass or a valid exact runtime failure and label
the result as described below. The original random test is a separate observation; a
failure in an unrelated permanent history cannot establish detection of this witness.

## FA — false adoption of outside verse text

In `classify.rs::perform`, change only the `Action::Record(base)` arm for
`ingredients/TIT.usfm`: skip `found_outside`, write an `Event::Adopted` for the outside
bytes, persist the manifest, and return `Next::Converged`. Leave other paths unchanged.
This asks whether the model accepts a ledger claim as proof that outside text is safe.

The generic witness uses only `Run::run` over unseen outside verse 1 edit, open, save of
verse 2, and leave. The independent control runs the same steps and requires outside
verse 1 text `zq1x` in readable History, an exact current ingredient/projection value,
or a complete readable preservation copy. Its marker is
`ORACLE_FALSE_ADOPTION_LOST_OUTSIDE_TEXT`. The control must derive survival from public
snapshots/files and must not trust placement-ledger ownership claims.

## ON — omission of outside-edit notices

In `writer.rs::Writer::notice`, suppress only the eight notice variants the model's
`outside_notice` helper recognizes: `OutsideEditRecorded`, `OutsideEditHeld`,
`OutsideEditUnreadable`, `OutsideRemovalKept`, `StorageDamage`, `MetadataEditedOutside`,
`DerivedFieldsEdited`, and `FixedFileRestored`. Preserve all I/O and other notices.

The generic witness leaves, advances time, crashes, opens, damages USFM, crashes, opens,
then chooses to keep the held file. The independent control runs that prefix before
Choose, requires a matching `StorageDamage` or `OutsideEditHeld` notice with a copy path,
and proves the copy contains the complete damaged bytes. Its marker is
`ORACLE_OMITTED_NOTICE_HIDES_HELD_BYTES`. The generic witness must not contain this
independent assertion: a control failure must never be reported as generic detection.

## Required classification

| Mutant generic witness | Mutant independent control | Classification |
|---|---|---|
| Exact pass | Exact marked assertion failure | `proof_gap_observed` — generic green permits real loss/hidden work; integration blocked pending oracle repair |
| Exact runtime assertion failure | Exact marked assertion failure | `generic_witness_detected` — the generic oracle rejects this witness; preserve the evidence and identify the assertion |
| Any other outcome | Any outcome | `incomplete` |
| Any outcome | Pass, unmarked failure, compiler/timeout/omission/interruption | `incomplete` |

Classifications require all clean-before and restored passes. Retain the random-test
outcome alongside the classification without conflating it with the witness/control.
An observed gap must be fixed in the model before S2 certification; old evidence must
not be relabeled as proof of the corrected oracle. Repeat against the corrected exact
revision to demonstrate the generic witness now detects the seeded defect.

The independent coverage reviewer's concrete draft is retained as `reviewer-draft.md`;
its four append-ready temporary tests are `probes.rs`. `recipes.py` supplies in-memory
test appending, exact FA/ON source transformations and supplemental classification only;
it launches no processes and mutates no files. The authorized single executor can import
these helpers while retaining all source patches and gate evidence. They remain
hypotheses until execution establishes baseline, mutant and restoration outcomes.

Commands after sourcing the environment (substitute one exact test name per invocation):

```text
cargo test -p tc-store --offline --test store model::oracle_independence_false_adoption -- --exact --nocapture
cargo test -p tc-store --offline --test store model::oracle_independence_false_adoption_control -- --exact --nocapture
cargo test -p tc-store --offline --test store model::oracle_independence_omitted_notice -- --exact --nocapture
cargo test -p tc-store --offline --test store model::oracle_independence_omitted_notice_control -- --exact --nocapture
cargo test -p tc-store --offline --test store model::random_histories_lose_no_work -- --exact --nocapture
```
