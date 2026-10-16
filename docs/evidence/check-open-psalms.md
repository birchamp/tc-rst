# Opening the Psalms check lists

Budget (PERFORMANCE.md, Checking): open the Check screen on Psalms tN and tW in 250 ms
the first time.

## What is measured

`crates/tc-check/tests/performance.rs`, test `opening_the_psalms_check_lists_meets_the_budget`.
One open reads the Psalms tN v91 table (`fixtures/scale/psa/en_tn/tn_PSA.tsv`, 8,212
rows, 3,332,327 bytes) and TWL v91 table (`fixtures/scale/psa/en_twl/twl_PSA.tsv`, 7,683
rows, 552,985 bytes) with tc-helps, and derives both check lists for a whole-book `eng`
project with tc-check: 7,969 tN checks (110 on superscriptions, `N:front`) and 7,683 tW
checks (155 on superscriptions), the tW rows gated by the en_tw v91 articles. The files
are read into memory before the clock starts. Placing saved decisions and computing status
are not part of this measurement.

The first open of a process is timed alone; then 21 opens give the median and p95.

## Results

Commit `99b2926` (branch `fix-helps-front`), release build, rustc 1.97.0, Linux 6.18,
4 vCPU Intel Xeon @ 2.80 GHz, 15 GiB RAM, shared with other builds (load average 2.6 to
3.0 during the runs). Five separate processes, 2026-10-16:

| Process | First open | Median of 21 | p95 of 21 |
|---|---|---|---|
| 1 | 84 ms | 70 ms | 100 ms |
| 2 | 101 ms | 69 ms | 82 ms |
| 3 | 83 ms | 68 ms | 95 ms |
| 4 | 88 ms | 68 ms | 75 ms |
| 5 | 88 ms | 66 ms | 80 ms |

Every run is under the 250 ms budget. The p95 spread follows the load from the other
builds on the machine. The test fails in a release build only above 750 ms, three times
the budget (PERFORMANCE.md rule 5).

To reproduce:

```sh
cargo test --release -p tc-check --test performance -- --nocapture
```
