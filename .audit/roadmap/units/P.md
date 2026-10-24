---
id: P
title: Scale: re-apply unit P on post-S2 main
track: A
lane: storage
milestone: M2
state: todo
owner: none
branch: none
worktree: none
depends: [S2]
needs_owner: []
later: ["S3 from step 3"]
proves: []
no_loss: true
size: 2-3 d (basis: the original 23 commits in about 5 h, on a store that has since changed by +16,598/-1,873 lines)
merged: none
updated: 2026-10-24
next: Claim this card after S2 merges, then do step 1.
blocked_by: none
---

# P: Scale: re-apply unit P on post-S2 main

## Goal

Every budget that tc-journal, tc-store, tc-check, tc-align and tc-resources own is measured end
to end on a whole Bible, in CI at three times the budget and on the reference laptop for the
verdict. Unit P's work on the `storage` branch is re-applied on post-S2 main, tooling first and
speed-ups after S3's core, one at a time. The `storage` branch is never merged: it is built on the
old S2 (19e7fd2), was never reviewed, and conflicts with the lane in 17 files, including a second
speed-up of aligner suggestions that C5 already did differently.

## Why it matters to a translator

A translator keeps a whole Bible in one project for years. This item shows that a save, a check
or an alignment stays as fast in year three as on day one, and finds the rows that miss, such as
the checkpoint (460 to 591 ms median against 500 ms, STORAGE §14).

## Before you start

- `python3 -I .audit/tools/board.py` lists S2 as `done`.
- `git rev-parse --short storage` prints `dd63328`.
- `git ls-tree -r --name-only storage -- crates/tc-corpus/examples/perfreport | wc -l` prints `13`.
- Read the old plan: `git show storage:.audit/plans/P.md` (§0.4 the budgets, Step 13, F4,
  "Invariants every change must keep", "Traps found while planning").
- `df -h /home`: the whole-Bible project alone is about 880 MiB (STORAGE §14 "Working tree"), and
  each round writes corpora under `target/perf/` (estimate: several GB; delete them after step 5).

## Funnel card

- **Invariants**:
  - INV-AN-ACTION-COSTS-WHAT-IT-TOUCHES: an interactive action makes the same operation counts
    on the Psalms corpus and on one ten times larger (PERFORMANCE.md rule 2).
  - INV-BUDGETS-AT-3X-IN-CI: every owned budget row's p95 in CI is at most three times its budget
    (PERFORMANCE.md rule 5); the laptop run gives the real verdict.
  - INV-ONE-SUGGESTION-ENGINE: the aligner has one suggestion engine, equal to tC4's link for link.
- **Wrong states**:
  - An action whose cost grows with the project or its history (a save that reads packs, a book
    read that opens every file).
  - A speed-up that loses a save: eb108fb and b01c963 change the seal, the open and the fold cache,
    which S2 redesigned.
  - Two suggestion engines: C5's d45be90 (on main after INT-4) and the `storage` branch's fbef0dc both rewrote
    `crates/tc-align/src/suggest/{boost,engine,wordmap}.rs`.
  - A `perf` job that fails main on its first push.
- **Code**:
  - Types: none new in the product; the tooling is `crates/tc-corpus/examples/perfreport/`.
  - One entry point: `perfreport ci DIR --tolerance 3` for CI; `perfreport run` for verdicts.
  - Removed: nothing. The `storage` branch stays as a reference until INT-6 decides about it.
  - Left to checks, and why: speed is not a type. Op counts hold it in `cargo test`; `perf` holds
    the budgets.
- **Checks**:
  - Op counts from a694344 in `crates/tc-journal/tests/operation_counts.rs`:
    `opening_a_book_costs_that_book`, `moving_to_a_chapter_costs_that_chapter`,
    `the_conflict_counts_after_a_save_cost_nothing_per_conflict`,
    `a_verse_save_costs_nothing_per_year_of_history`; from c5a82c1 in
    `crates/tc-store/tests/store/opcount.rs`: `a_decision_save_reads_no_pack_and_writes_one_pending_file`,
    `an_alignment_save_reads_no_pack_and_writes_one_pending_file`,
    `a_resolution_reads_no_pack_and_writes_one_pending_file`, `reading_a_book_reads_only_its_values`
    (all to port); kind: count, on Psalms and ten times Psalms
    (`the_larger_corpus_is_ten_times_larger`); runs in: `rust` job.
  - With the speed-ups (step 3 and 4): `a_close_writes_the_sections_it_made_and_not_the_cache`
    (eb108fb), `a_tools_decisions_read_only_its_own_records` (d28489a),
    `decision_resources_are_read_without_decoding_decisions` (eacae8c), and the
    `tests/operation_counts.rs` files of tc-check, tc-align (fbef0dc) and tc-resources (bad2671).
  - `perf` job (to create in `.github/workflows/ci.yml`): `perfreport ci "$RUNNER_TEMP/perf" --tolerance 3`; kind: budget.
  - One engine: `memory_only_predictions_equal_tc4s_link_for_link` and
    `boosted_predictions_equal_tc4s_under_the_same_seed` (`crates/tc-align/tests/tc4_suggest.rs`,
    the vectors in `fixtures/align/suggest-vectors.json`), and
    `suggestions_meet_the_full_training_corpus_budget` (`tests/suggestion_budget.rs:80`, the
    `alignment-budget` job).
  - No loss: `cargo test -p tc-store --test store` (model and crash rows) and the four long model
    seeds (S3.md "SP-3") after each store speed-up.
  - Proof it can fail: `perfreport ci <DIR> --tolerance 0` exits 1; each ported count test fails
    when the speed-up it came with is reverted, or (c5a82c1's) when the read it counts is made to
    open every pack. Quote one failure per test in the log.
- **Prose**: `docs/evidence/storage-scale-<date>.md`; STORAGE §14 rows; PERFORMANCE.md "Rows of
  the project store that are open" and rule 5 (name the `perf` job); APP-DESIGN §9.2 R9 and Q-5,
  one line each pointing to the evidence.
- **Human**: the checkpoint budget is the owner's. Ask only with the laptop's numbers (step 7).
  The R9 numbers (saves during a checkpoint) inform Q-5. Never change a budget yourself. H17
  (are the budgets part of done) does not block steps 1 to 6; step 7 gives H17 its numbers.

## Steps

### Step 1: Bring the report tooling onto a branch from post-S2 main

- Do:
  1. Create `track-P` from main, the branch `claude/focused-ride-5uzj7p` (TRACKING.md "Claim an item").
  2. Take whole files from `storage`:
     `git checkout storage -- crates/tc-corpus/examples/perfreport spikes/perfreport-windows crates/tc-corpus/src/timeline.rs .audit/tools/laptop-ratios.py .audit/tools/laptop-ratios.sh docs/evidence/storage-scale-2026-10-19`.
  3. Port by hand the hunks these commits made to files main also changed; read each with
     `git show <sha> -- <path>`: 08a79c7 (`crates/tc-corpus/Cargo.toml` dependencies;
     `crates/tc-store/src/lib.rs`: `pub const TEST_HOOKS` and the `REPACK_BYTES`,
     `REPACK_OBJECTS` re-export; `store.rs`: `Store::start`), c5a82c1
     (`ProjectHandle::close_counted` in `handle.rs`), 3b74198 and fff9ac5
     (`crates/tc-corpus/src/{lib,history}.rs`, `tests/corpus.rs`).
  4. Make perfreport compile against post-S2 APIs. Change no product behavior.
- Files: as listed.
- Done when: `cargo build --release -p tc-corpus --example perfreport` succeeds;
  `target/release/examples/perfreport ci <DIR> --tolerance 3` runs to its table (log its exit code
  and every failing row); `cargo clippy -p tc-corpus --all-targets -- -D warnings` passes.
- Record: log line `step 1` with the table's failing rows; `next: do step 2`.

### Step 2: Port the operation counts and add the perf job

- Do:
  1. Port the eight count tests listed in the funnel card (from a694344 and c5a82c1). A test that
     fails on main is a finding: keep it unchanged, mark it `#[ignore = "P step 3: <why>"]`, and
     log it.
  2. Run `perfreport ci <DIR> --tolerance 3` three times. Add a `perf` job to
     `.github/workflows/ci.yml` as storage's 08a79c7 did (`needs: rust`, `timeout-minutes: 90`,
     `cargo build --release -p tc-corpus --example perfreport`, then
     `target/release/examples/perfreport ci "$RUNNER_TEMP/perf" --tolerance 3`). If all three runs
     exit 0, run it on every push; else run it only on `schedule` and `workflow_dispatch` (as
     `store-model-nightly` does) and log the failing rows. If INT-3 has landed, register it the
     way INT-3's card says.
  3. Name the job in PERFORMANCE.md rule 5. Merge steps 1 and 2 into main.
- Files: `crates/tc-journal/tests/operation_counts.rs`, `crates/tc-store/tests/store/opcount.rs`,
  `.github/workflows/ci.yml`, `docs/PERFORMANCE.md`.
- Done when: `cargo test -p tc-journal --test operation_counts` and
  `cargo test -p tc-store --test store opcount::` pass; `perfreport ci <DIR> --tolerance 0` exits 1.
- Record: log line `step 2`, then `merge`; `next: wait for S3's "core merged" note, then step 3`.

### Step 3: Re-port the store speed-ups, one at a time

- Do: start only when `grep -n "core merged" .audit/roadmap/units/S3.md` prints a line. Port
  eb108fb (fold cache as base and delta), then b01c963 (a large batch and the month's consolidation
  written off the actor), each in its own commit, onto the simplified store. For each: run the
  rows it changes with `perfreport run <DIR> --only <rows> --rounds 3` before and after; run
  `cargo test -p tc-store --test store`, the four long model seeds, the mutation executor (CHK-5)
  and storebench (S3.md SP-3, SP-4, SP-6). Drop a port that does not improve its rows.
- Files: from `git show --stat eb108fb b01c963` (tc-store `open.rs`, `seal.rs`, `actor.rs`,
  `project.rs`; new `stage.rs`, `cons.rs`, `tests/store/stage.rs`).
- Done when: each kept port has before and after numbers in the log, and all the runs above pass.
- Record: log line `step 3` per port, then `merge`; `next: do step 4`.

### Step 4: Re-port the domain speed-ups main lacks, keeping one suggestion engine

- Do: for each of fbef0dc, d28489a, bad2671, 0123eab, eacae8c, dd63328: read it
  (`git show <sha>`), compare with main (`git log --oneline claude/focused-ride-5uzj7p -- <its files>`), and port only
  what main lacks, with its count tests. For `crates/tc-align/src/suggest/*` keep main's engine
  (C5's d45be90); take a part of fbef0dc only if `suggestion_budget` fails without it and the two
  tC4 oracle tests pass with it. Before you start, check the board for an active domain card on
  tc-check, tc-align, tc-resources or tc-helps.
- Files: per commit (`git show --stat <sha>`).
- Done when: `cargo test -p tc-align --test tc4_suggest`,
  `cargo test -p tc-align --release --test suggestion_budget -- --ignored --nocapture`, and the
  count tests of each port pass; before and after numbers per port are in the log.
- Record: log line `step 4` per port, then `merge`; `next: do step 5`.

### Step 5: Measure three rounds and write the evidence

- Do: on the final commit, `perfreport setup <DIR>`; then at least three full rounds, at
  different times of day, of `taskset -c 0-7 env RAYON_NUM_THREADS=8 target/release/examples/perfreport run <DIR> --ratios docs/evidence/storage-scale-2026-10-19/laptop-ratios.json`,
  and of `r9 <DIR>` and `memory <DIR>`; `growth <DIR>` once; `years <DIR> --open-every 1` once.
  Record `uptime` before and after each. Write `docs/evidence/storage-scale-<date>.md` with the
  sections of the old plan's 13.2; update STORAGE §14 (13.3) and PERFORMANCE.md (13.4), and
  add "Checkpoint: review for the owner" with the options. Change no budget. Delete `target/perf/`.
- Files: `docs/evidence/storage-scale-<date>.md`, `docs/STORAGE.md`, `docs/PERFORMANCE.md`,
  `docs/APP-DESIGN.md`.
- Done when: the evidence doc gives a verdict for every owned row, and
  `grep -n "Checkpoint: review for the owner" docs/PERFORMANCE.md` prints a line.
- Record: log line `step 5`, then `merge`; `next: do step 6`.

### Step 6: Have the laptop session run the report

- Do: copy "Task 8" from `git show storage:.audit/tasks/windows-track.md` into
  `.audit/tasks/windows-track.md`, changed to start from main. Push main under the push rule
  (ROADMAP.md). Send the laptop session the task with SendMessage to the session named
  "Part 3 complete" (it only measures). Never pass an owner approval through it. A send is not
  confirmed read: watch origin for its `windows-perfreport` branch. Add its table to the evidence
  doc and redo the verdicts. If no `windows-perfreport` branch appears within 3 days, ask the
  owner to run the one command in `.audit/tasks/windows-track.md` on the laptop by hand and paste
  the table; a pasted table is evidence when the log quotes it with the commit and the machine
  line. (If H17 is answered "no", the rows are still measured and recorded here; DONE does not
  wait for them, `ROADMAP.md` H17.)
- Files: `.audit/tasks/windows-track.md`, `docs/evidence/storage-scale-<date>.md`.
- Done when: the evidence doc has the laptop table, and the log names the laptop branch's commit
  (or quotes the pasted table with its commit and machine line).
- Record: log line `step 6`; `next: do step 7`.

### Step 7: Put the checkpoint question to the owner

- Do: if H17 has no ruling, put these numbers into H17's question; otherwise add the question as
  the next free H id in ROADMAP.md "What needs the owner". Give the checkpoint's laptop figure
  against 500 ms, what a translator feels, the options (keep 500 ms and build the one-pack
  checkpoint, old plan F4; restate it as "off the actor, saves continue, N s"; relax it), and a
  recommendation. When the owner answers, write `rulings/<date>-<H-id>.md` and change the row as
  ruled.
- Files: `.audit/roadmap/ROADMAP.md` (one row), `rulings/`, `docs/PERFORMANCE.md` after the ruling.
- Done when: the ruling file exists, or the card is `parked` with `next: wait for <H-id>`.
- Record: log line `step 7` (`ruling` when answered); `state: done` after the last merge.

## Gates before merge

Until INT-3 lands, every command below; after INT-3, `scripts/gates.sh` (to create in INT-3):

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and the
  same with `--features gallery`, `--features app`, `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` (the
  Windows step of `.github/workflows/ci.yml`; needs `gcc-mingw-w64-x86-64` and `nasm`)

Plus: `perfreport ci <DIR> --tolerance 3` as the `perf` job runs it; the `alignment-budget` job's
command; for steps 3 and 4, the store's model, crash and long-seed runs and storebench.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A ported speed-up fails the model or a crash row and the cause is not clear after one fix.
- A domain card owns the crate a step 4 port touches.
- A budget row would change: that is the owner's (step 7), never yours.

## References

- `.audit/roadmap/evidence/2026-10-21/storage-remaining.md` §2, ST-3, ST-14; `process-integration.md` M3, F3.
- `git show storage:.audit/plans/P.md`; `docs/PERFORMANCE.md`; STORAGE §14.
- `.audit/tasks/windows-track.md`; `.audit/HANDOFF.md:60` (the laptop session "Part 3 complete", reachable with SendMessage, only measures).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/storage-remaining.md
- 2026-10-24 01:44 EDT | checks lane (Claude Code 2198ee19) | note | none | From the checks lane: in CI the alignment budget only reports. When this card measures the budgets on the reference laptop, run `TC_ALIGN_BUDGET_ENFORCE=1 cargo test --locked -p tc-align --release --test suggestion_budget -- --ignored --nocapture`, which fails over three times the 50 ms budget | crates/tc-align/tests/suggestion_budget.rs
