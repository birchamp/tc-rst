# Raw output of the 2026-10-16 local head-to-head

Everything the record in `../journal-head-to-head-2026-10-16.md` was derived from. Committed
as it ran: the scripts hold the absolute paths of the machine that ran them, and are not
cleaned up, so what is here is exactly what produced the numbers.

## `results/`

One file per run. Each begins and ends with a line of context:

```
side=rebuild phase=A run=1 start=14:07:58Z load1=1.37 speed_limit=100
... the harness's own output ...
end=14:08:06Z load1=1.46 speed_limit=100
```

`load1` is the one-minute load average; `speed_limit` is `CPU_Speed_Limit` from
`pmset -g therm`. **It is 100 in every header and footer of all 15 runs**, which is how the
record claims the chassis never thermally throttled. A run with a lower value would not be
comparable and should be discarded.

| File | Side | Corpus |
|---|---|---|
| `a-tc4-{1..5}.log` | tC4 at `d310bf3` | checked whole Bible, 278,041 events |
| `a-rs-{1..5}.log` | rebuild at `6e8adac` | `bible-share 0.42`, 277,880 changes |
| `b-rs-bible-{1..5}.log` | rebuild at `6e8adac` | whole Bible, 617,143 changes |

The `a-` runs are phase A and were interleaved tC4, rebuild, tC4, rebuild, as the record's
timestamps show. The `b-` runs are phase B and have no tC4 counterpart, because tC4's whole
Bible *is* the 278,041-event corpus.

`save-cost-curve.txt` is the per-save fold cost against history (the 23.81 µs an event in §F3
of `../../LOCAL-BENCH-FINDINGS-2026-10-16.md`): twelve corpus sizes, three repetitions each,
tab-separated, with the individual run times in the `runs` column and a `clean` column
confirming every corpus folded without forks.

## `bench/`

- `run-h2h.zsh` — the phase A and B runner. `run-h2h.zsh A 5` interleaves five runs a side on
  the matched corpus; `run-h2h.zsh B 5` does five runs of the whole-Bible corpus. It writes the
  `load1` and `speed_limit` lines around every run. Generate the corpora first, with the
  commands in the record's "To reproduce".
- `save-cost-curve.mjs` — the per-save sweep. It imports tC4's **real** `journal/fold.mjs`,
  `skeleton.mjs` and `hlc.mjs` by absolute path and replicates `conformance/bench-fold.mjs`'s
  corpus construction from the same fixtures, then folds cumulative prefixes at book
  boundaries, so every point is a valid complete corpus. Point the `TC4` constant at a tC4
  checkout with `npm ci` already run.

## Not here

No CPU-time or RSS figures: `foldbench` reports both as zero on macOS (§F4). The zeros in the
`b-` and `a-` logs are that, not measurements.
