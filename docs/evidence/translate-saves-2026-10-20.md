# Production Translate saves, 2026-10-20

C2 promotes the existing real-store verse and section saves into the production editor
service and adds layout saves, drafts and durable current-draft Cancel. The code snapshot
is the `Build C2:` commit on `resume-build-lane`, based on
`07445751bd90a4bcc07a172bc3fbcf9e050f34bd`; the checkpoint records its full hash.
`.audit/plans/C2.md` records implementation, no-loss proof and final gates.

## Release save measurement

Reuse the existing `store_adapter_bench` M4 path, with `--m4-only` to skip unrelated open,
watcher and in-memory timings. It alternates 200 direct store saves and 200 production
`editor/saveVerse` calls over Psalms in the generated whole-Bible project. App timing runs
from `App::call` to the durable Ack and includes dispatch, request caching, store fsync,
fold update, token projection and Resume recording. Request serialization and untimed
open/close are outside the sample; browser transport, paint and the desktop shell are not
timed. Runtime hooks are disabled. Every Ack must report
`Written` without a disk warning; both generated project folders validate as Burritos
after close.

Corpus setup generated 617,086 changes from the existing STORAGE §14 generator. It reported
zero Burrito problems. The unchanged strict operation-count suite separately measures
verse saves, layout saves and drafts on Psalms and ten times it, including history and
device growth, with all store counters compared.

Commands (offline, release):

```sh
cargo build --release -p tc-store --example storebench
cargo build --release -p tc-app --features test-hooks --example store_adapter_bench
target/release/examples/storebench setup target/store-adapter-bench
taskset -c 8 target/release/examples/store_adapter_bench target/store-adapter-bench --m4-only
```

Machine: Intel Core i7-12700, 20 logical CPUs, Linux 7.0.0-38-generic x86_64, Rust 1.99.1
(`58cf8f9c3`), pinned to CPU 8 (P-core 4). SHA instructions are present; AVX-512 is absent.
Generated files and raw logs stay under ignored `target/` and `.enjoy-logs/`.

Two paired runs (200 samples per path in each):

| Run | Path | Median ms | p95 ms |
|---|---|---:|---:|
| Normal folders | direct store control | 16.578 | 26.520 |
| Normal folders | production editor API | 12.194 | 17.904 |
| Swapped folders | direct store control | 16.320 | 26.666 |
| Swapped folders | production editor API | 11.285 | 18.331 |

Raw logs: `.enjoy-logs/resume-c2-perf.log` and `resume-c2-perf-swapped.log`. The second
run uses the existing `STORE_ADAPTER_BENCH_SWAP=1` control and ran after all other checks
finished. Both API runs are below the local 25 ms target. The direct control exceeds 25 ms
in both runs; every row is below PERFORMANCE's 75 ms CI tolerance. No budget changed, and
these API timings do not certify every store scenario or the full input-to-paint budget.

This Linux CPU and disk do not establish
the 25 ms target on the specified five-year-old Windows laptop without SHA and with Defender;
that reference measurement remains P/G2 work. PERFORMANCE's budget is unchanged.
