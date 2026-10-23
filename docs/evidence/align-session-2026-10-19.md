# The aligner's session: opening, clicking and the rail (unit B6, 2026-10-19)

Machine: the owner's Linux PC (`docs/evidence/linux-machine-2026-10-18.md`), shared with other
builds. Load average 8.6 during the click runs, 10 to 13 during the open runs. Release builds:
`tc4` with `--release --features test-hooks,fixture-backend`, the UI with
`crates/tc-ui/build-app.sh --release`. Commit: the B6 branch (`lane-b6`) at the commit that adds
this file. A budget met here is not shown met on the reference laptop (the CPU has SHA
instructions and 20 threads, the laptop has 4 cores and Defender).

## A click, painted (PERF:85, budget 30 ms)

`parity/app/journeys/align.test.mjs`, test `align_click_round_trip_p95`: 60 clicks on the first
word of the bank of Titus 1:1 (a request to `align/edit`, the reply of about 20 KB, the redraw of
the bank and the 16 cards). Each is timed from the click to the paint of the frame that holds the
reply, measured as G0 measured it (a message posted from the frame's animation frame, handled
after style, layout and paint).

| Run | p50 | p95 |
|---|---:|---:|
| release 1 | 19.9 ms | 20.6 ms |
| release 2 | 18.2 ms | 19.3 ms |
| release 3 | 18.2 ms | 19.8 ms |

The median is one display frame at 60 Hz: the reply is drawn within the frame that follows it.
The test asserts the 30 ms with `TC4_PERF=1`; without it, it prints the figures. G0 measured
the same path with a stand-in reply at 22.4 to 24.5 ms at the p95 on the reference laptop, and
Q-13 does not arise (APP-DESIGN, "G0 results").

The same test counts the requests: 60 clicks are 60 `align/edit` requests, none refused or lost.
`fast_clicks_in_the_page_are_queued_not_refused` sends five clicks with no wait between them and
sees five requests and five chosen words.

## A placement on a rail as long as the Psalms' (budget 30 ms)

Test `align_placement_paints_within_30_ms_on_a_psalms_sized_rail`: the rail's reply is made 2,461
rows long in the route, then a word is placed on a card and sent back, 25 times each, timed as
above from the click to the paint. The first build of the rail drew every row again on each edit:
560 to 860 ms in the dev build. The rows are now keyed, each reads only its own verse's state, and
an unchanged open verse is not announced to them (every row reads it).

| Run (release) | p50 | p95 |
|---|---:|---:|
| 1 | 16.4 ms | 24.3 ms |
| 2 | 12.4 ms | 23.9 ms |
| 3 | 14.9 ms | 23.0 ms |

## Opening a verse (PERF:83, budget 30 ms) and the rail

`crates/tc-app/tests/op_counts/align.rs`, run with `cargo test --release -p tc-app --test
op_counts op_counts::align -- --nocapture` over Psalms and the corpus ten times larger, on the memory backend and on the store (the corpus pins the Hebrew Bible v3.0.0, which the test installs).

| Action | Memory, Psalms | Memory, ten times larger | Store, Psalms | Store, ten times larger |
|---|---:|---:|---:|---:|
| First open of a verse (the book's resources read, the Hebrew Bible cut into verses) | 39.3 ms | 36.7 ms | 40.0 ms | 38.0 ms |
| The next verse | 0.34 ms | 0.48 ms | 0.39 ms | 0.44 ms |
| The rail: one request for 2,461 verses, 449 KB of JSON (nearly every verse of the corpus is aligned, so each one is checked) | 122 ms | 124 ms | 148 ms | 124 ms |

Counts, which the tests hold: opening a verse reads at most 24 records and loads the book's
resources once, whatever the corpus (`align_open_touches_one_verse`); a click reads none
(`align_edit_touches_one_verse`); the rail reads two records for each verse of the book and no
more, the same on both corpora (`align_index_costs_the_book`).

The first open of a verse in a session also reads the book's resources and cuts the Hebrew Bible
into verses (once for each book of a session); the verse itself is the 0.3 to 0.5 ms of the next
open. The first open is above 30 ms and under the 90 ms at which a budget fails (PERF:30-34).
The rail costs what the book holds, once when the tool opens and again when a teammate's change
reaches the book; it runs beside the verse's own open.

## What was not measured

- A faster first draw of a Psalms-sized rail. The page draws every verse, as tC4 does, and keeps
  them while the tool is open: the 2,461 rows were in the page 316, 345 and 638 ms after the
  first verse (release; the rail's own request of about 120 ms is in that), and the verse can be
  worked on meanwhile. B5a's virtual list would serve it if that is too slow.
- Training: the sample's one aligned verse learns at once. A Psalms-sized project's training runs
  in a background job and was not timed; C5 owns its budget (PERF:84).
- The reference laptop. The owner's laptop session only measures what needs its hardware
  (`.audit/HANDOFF.md`); the click's figure above is the one to repeat there.
