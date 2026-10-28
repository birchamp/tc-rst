# C5 alignment saves and suggestions evidence — 2026-10-20

Scope: C5 alignment persistence, confirmed-save learning, page queue/close behavior and
prediction performance, based on B7 commit `b230dad3b1114d442d1afafb705e0e3e7d9f652a`.
The user-approved full gate
`resume-c5-approved` (host PID 3829250) started successfully, then stopped on a new random seed
in the `tc-api` B4b placement property: `text = "1-4 0\n2"`, placement keys `["1-2", "3", "4"]`,
no drops; the serialized parse lost pin 1. The serializer fix passes all 47 section tests and
scoped Clippy; its real tC4 fixture difference is declared in PLAN and APP-DESIGN.
The complete final gate passes in `.enjoy-logs/resume-c5-complete-full.log`: all required
formatting/workspace/release/wasm Clippy checks, **3,230 Rust tests passed, 0 failed, 7 ignored**,
and fixture/app UI builds. An unrelated journal salvage property oracle was also corrected
to recognize cancelling bit flips; production journal code is unchanged. The two transport
browser reruns pass in `resume-c5-transport-final.log`, completing all 13 alignment journeys
across the focused runs. `tracker:plans/C5.md` records the final proof and separate fixes.
The 50 ms reference target remains open for P/G2.

## Completed focused proofs

- Real-store tests exercise quit before 2 s/reopen, the timer, changing verse, Mark valid,
  crash offers with both displayed tokens, newer edits during a save, and failed final quit.
  Persisted links and Scripture Burrito validity are checked. All 13 store draft tests passed;
  restoring the old R26 unconditional removal makes the crash-before-Ack regression fail.
  The temporary mutation was removed, and the enabled app R26 contract passed.
- The 12 focused aligner library tests and the focused app aligner rerun passed. Coverage
  includes save durability before Ack, no-loss mutations, confirmed-save snapshots, re-save
  counting, training generations/cancellation and testament separation.
- All 15 pure UI queue tests passed, including failure/retry conservation and the planted old
  drop. All 13 alignment browser journeys pass across the focused runs. The last two transport
  cases now require exactly one expected aborted-fetch console error and preserve every
  unrelated error check; both reruns pass with exact persisted links and Burrito checks.
- All five strict alignment count rows passed on memory/store and Psalms/10×, including an
  edited save while the training job remains held after capturing its snapshot. The save
  reaches the actual store without releasing training. No counter limit was relaxed.

## Prediction profiling and release measurement

Profiling: kernel sampling was denied (`perf_event_paranoid=4`); no settings or permissions changed. Temporary opt-in deterministic stage timers measured the existing full NT + GEN + PSA public `Model` fixture. All profiling/RawLink print instrumentation has been removed.

The initial NT probe had 61,380 candidates and 230 trees. Tree scoring took 160–165 ms of the instrumented prediction; wordMAP took 37–39 ms. After batching/bounds, the updated measured phases were about 68–70 ms scoring, 45 ms wordMAP (candidate keys 10 ms, plain frequencies 13 ms, lemma frequencies 12 ms, per-candidate features 10 ms), and 7 ms selection. Instrumentation costs and machine load mean these are hotspot measurements, not the final timing gate.

Final change: fit-time compilation into contiguous preorder tree nodes (implicit left child, indexed right child), bounded 64-row scoring, batch-constant/range split elimination, and contiguous leaf additions. Each candidate receives one leaf addition per tree in the original tree order. Sentence-only n-gram metrics are derived once, and zero alignment frequencies no longer cause redundant source/target lookups or sum entries. No prediction cache or budget change.

| Probe | OT median / p95 | NT median / p95 |
|---|---:|---:|
| Original full-corpus release run | 43.065 / 43.570 ms | 186.249 / 191.637 ms |
| Final pinned P-core 8 release run | 26.087 / 26.580 ms | 94.155 / 95.949 ms |

Original measurements were not pinned, so this table is not a controlled speedup ratio. Both runs use the same seeded public fixtures: OT 3,994 aligned verses, PSA 78:55 with 30 source / 30 target words; NT 7,958 verses, REV 20:4 with 61 source / 70 target words. Final training: OT 6.180 s, NT 10.822 s; 20 measured predictions after one warm prediction, median middle pair and p95 the 19th sorted sample (index 18).

Final machine: Intel Core i7-12700, Linux 7.0.0-38 x86_64, Rust 1.99.1, CPU 8 verified P-core
(affinity inherited by the test thread). SHA instructions present; AVX-512 absent. Load before
the final probe: 4.80, 6.52, 4.45; this conversation's other Cargo/browser/server jobs had ended,
external activity unknown. See `linux-machine-2026-10-18.md` for the machine and its difference
from the Windows reference laptop. Reproduce from the repository with:

```sh
taskset -c 8 cargo test -p tc-align --release --test suggestion_budget -- --ignored --nocapture
```

Proof: both final warm `RawLink` vectors exactly equal the original captured vectors, including all positions and round-trip decimal confidence values (byte-for-byte). The test `batching_keeps_every_confidence_bit_across_splits_and_chunk_boundaries` compares the flat batch scorer with the unchanged original training-tree evaluator through numeric/categorical branches, NaN/inf/signed zero, unseen values, omitted rows, contiguous rows, and chunk boundaries. All 4 library tests and 19 public Model/tC4 fixture tests passed (one existing opt-in growth test ignored). Scoped `cargo clippy --offline -p tc-align --all-targets -- -D warnings`, formatting and whitespace checks passed.

The prescribed 150 ms CI gate passes. The 50 ms reference target remains unmet on this Linux run and unproven on the Windows reference laptop. Keep that performance work explicit in P/G2; this evidence does not relax the budget or claim it met.

Logs in the integration worktree: `.enjoy-logs/resume-c5-budget.log` (original),
`resume-c5-predict-profile.log` and `resume-c5-predict-bounds-profile.log` (temporary stage
measurements), `resume-c5-predict-flat-equality.log` (final equality fixtures),
`resume-c5-predict-flat-budget.log` (final pinned timing), `resume-c5-predict-clippy.log` (lint).
