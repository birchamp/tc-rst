# CHK-8 measurements, 2026-10-26

This PC, 20 cores, every run at 4 test threads as on CI's 4-core runners, under `tc-heavy`
(22 GB cap) while other lanes held the second slot. Times are test time, not build time.

## The hang proof

`hang-proof/` holds one test that waits on a channel nobody sends to. Run from that folder with
the repository's config:

    cargo nextest run --config-file ../../../../../.config/nextest.toml

`hang-proof.log`: SLOW at 60, 120, 180 and 240 s, TERMINATING then TIMEOUT at 300.007 s, the
test named each time; exit 100.

## Found on main while measuring

`tc-sync::sync door43::a_push_that_loses_a_race_fetches_again_and_keeps_both` hung in 2 of 5
runs alone under libtest and under nextest (fixed in D1, batch 13). nextest named it TIMEOUT at
300 s. tc-heavy stopped two libtest runs on it after 5 idle minutes and could name only a thread,
`door43::a_push_`.

## Before CHK-7c (scale tests at 10x)

| Suite | libtest | nextest |
|---|---|---|
| workspace without tc-app | 972 s for the binaries it reached, then hung on the tc-sync race | 1,290 s, 2,939 pass (race skipped); 19.4 GB summed peak with four corpus tests at once |
| tc-app without op_counts | (with op_counts) 1,116 s, 14.6 GB peak | 244 s, 854 pass, 1.2 GB |
| tc-app op_counts | 576 to 869 s, 14.2 GB | killed by the OOM killer at 22 GB: each test built the 7.1 GB corpora |

## After CHK-7c (scale tests at 2; 10x at night on libtest)

| Suite | nextest |
|---|---|
| workspace without tc-app | 978 s, 3,014 pass, 4.4 GB summed peak |
| tc-app with op_counts | 588 s; 6.3 GB summed peak; op_counts failed only on its fixed resource folder, which each process cleared under the others (now `build_once_per_run`) |

Slowest tests (s): tc-api roundtrip 516 and 434 (210 to 300+ in other runs), tc-app
store_scenario crash 190 to 277, tc-store crash_every_command 118, tc-usfm fixtures 79,
op_counts check_open_derives_one_unit 74. CI's libtest times (run 40356855830): test job 31 min,
test-app 16 min, tc-api roundtrip binary 502 s.
