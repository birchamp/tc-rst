# Opening and editing the longest Psalms verse in the aligner

Budgets (PERFORMANCE.md, Aligning): open the aligner on the longest verse in Psalms in
30 ms; reflow an alignment after a verse edit in 2 ms.

## What is measured

`crates/tc-align/tests/psalms.rs`, test
`opening_editing_and_reflowing_the_longest_psalms_verse_meet_the_budgets`. The verse is
PSA 42:4 of the ULT in `fixtures/scale/psa/en_ult/19-PSA.usfm`, the Psalms verse with the
most words (42 words on 17 cards, aligned to UHB 3.0.0). It is read as tC4 reads aligned
USFM and stored with its bank and every card's words reversed, so that opening it sorts
every card into verse order (tC4 #734 and #754, the owner's Q15 ruling).

- Open: `Aligner::open` on the stored value, which checks it, shows it in verse order
  and finds that nothing is to be saved.
- Edits: 44 edits on the open verse, timed together: each of the 42 placed words sent
  back to the bank (each sorts the bank and every card), all 42 placed on one card in one
  drop (`place_many`), and the verse cleared.
- Reflow: the saved alignment carried over a one-word edit of the verse text.

The fixture files are read and the verse is prepared before the clock starts. Each figure
is the median and p95 of 21 runs after one untimed run.

## Results

Commit `0471d8e` (branch `delta-align`), release build, rustc 1.97.0, Linux 6.18,
4 vCPU Intel Xeon @ 2.80 GHz, 15 GiB RAM, shared with other builds (load average 6 to 7
during the runs). Five separate processes, 2026-10-16:

| Process | Open, median | Open, p95 | 44 edits, median | 44 edits, p95 | Reflow, median | Reflow, p95 |
|---|---|---|---|---|---|---|
| 1 | 34 µs | 39 µs | 104 µs | 151 µs | 26 µs | 32 µs |
| 2 | 34 µs | 39 µs | 105 µs | 130 µs | 26 µs | 29 µs |
| 3 | 34 µs | 40 µs | 105 µs | 123 µs | 26 µs | 29 µs |
| 4 | 34 µs | 39 µs | 119 µs | 185 µs | 25 µs | 31 µs |
| 5 | 35 µs | 75 µs | 105 µs | 129 µs | 25 µs | 30 µs |

Every run is far under its budget: opening takes about a thousandth of 30 ms, and a
reflow about a seventieth of 2 ms. An edit costs about 2 to 4 µs. The p95 spread follows
the load from the other builds on the machine. The test fails in a release build only
above three times a budget (PERFORMANCE.md rule 5): 90 ms to open, 6 ms to reflow.

The aligner takes one verse and nothing of the project, so these costs cannot grow with
the project or its history. Each word's place in the verse is indexed once when the verse
is read, so the verse-order sort after an edit costs O(n log n) in the words it sorts.

To reproduce:

```sh
cargo test --release -p tc-align --test psalms -- --nocapture
```
