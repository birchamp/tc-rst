# The parity harness, Rust side and gate (unit A6b, 2026-10-18)

Measured on the owner's Linux PC (docs/evidence/linux-machine-2026-10-18.md): 20 threads,
shared with up to three other agents building at the same time, so each timing gives the load
(`uptime`) beside it. Plan: `.audit/plans/A6b.md`.

## M1, M2: the tC4 baseline on this PC

tC4 `d310bf3`, built in a fresh clone with `npm ci` and the help pages fetched by tC4's own
`scripts/fetch-help.mjs`; Playwright 1.56.1, Chromium 141.0.7390.37 (build 1194) as
Playwright's Ubuntu 24.04 build; mock on port 29871.

The digest of a PNG set is the sha256 of the lines `"<name> <sha256>\n"` in manifest order; the
digest of the DOM set is the same over `domSha256`.

| Run | Code | Started (load) | Took | PNG digest | DOM digest | PNG bytes |
|---|---|---|---|---|---|---|
| committed (cloud machine) | `01befe2` | — | — | `befcbf2a…b5978` | `029f4b70…30cb2f` | 8,094,421 |
| control | `01befe2`, unmodified | 20:03:48 (1.27) | 2 min 22 s | `5229c9e2e4d33212f1de2083d04fdf0d9deb8e0293e24a6184e0c0c42894241f` | `029f4b7066582924f8f080dcd5490aed018d966aefc0994257437181f430cb2f` | 8,058,301 |
| recapture | A6b steps 1 and 2 (`settle` with `busy`, `domSnapshot({ ids })`, `machine`) | 20:08:04 (6.42) | 2 min 15 s | the same | the same | 8,058,301 |

- **M1.** The control run gave the planner's digests exactly (the planner's two runs on this PC
  gave them too), so this PC's fonts have not changed since planning.
- **M2.** The recapture with A6b's harness changes gave byte-identical PNGs to the control run
  (`diff -r` of the two `screens/` folders is empty) and the same `manifest.json` apart from the
  new `machine` and the new `rebuildDiffers` of `open-door43-internet-consent`. The 62 DOM
  files did not change (`git diff --stat parity/tc4-baseline/dom` is empty), which proves that
  `domSnapshot` without `ids` writes the lines it wrote before.
- The recapture's `machine`: `linux-x64`, Chromium `141.0.7390.37`, fonts 535 lines of
  `fc-list --format '%{file}\t%{index}\t%{fontversion}\n'`, sorted bytewise, sha256
  `a15fd0433b66112396faed20c7b88e11229781916faeaad035b9c8961a72eb33` (fontconfig 2.17.1).

Pixels that differ between the cloud machine's PNGs (`git show 01befe2:…`) and this PC's,
counted by pixelmatch 7.1.0 with threshold 0 and anti-aliased pixels counted:

| Pixels | Screens |
|---|---|
| 0 | `en-obs-translate`, `en-obs-understand`, `en-understand-questions`, `obs-translate`, `obs-understand` |
| 48 | `en-translate-section-editor`, `en-translate-tit1`, `en-understand-tit1`, `translate-tit1`, `understand-tit1` |
| 52 | `about`, `account-menu`, `app-language`, `home`, `open-door43`, `open-door43-internet-consent`, `share-internet-consent`, `share-signin`, `shared-home`, `sync-failed`, `sync-internet-consent`, `sync-ok`, `sync-ok-restored`, `sync-ok-teammates` |
| 96 | `en-translate-verse-editor`, `en-understand-tw-article` |
| 154 to 1,416 | `add-book` 154, `en-check-picker` 262, `share-dialog-check` 267, `project-settings` 291, `add-project` 296, `import` 430, `open-door43-refused` 464, `en-community-checking` 719, `share-dialog` 1,073, `new-obs` 1,136, `feedback` 1,281, `new-bible` 1,416 |
| 1,707 to 3,249 | `check-picker` 1,707, `en-check-ta-drawer` 1,897, `en-check-tn-session` 1,913, `check-tn-session` 1,999, `ar-align-workspace-620` 2,046, `ar-align-workspace-row` 2,204, `ar-check-tw-session-620` 2,507, `en-check-tw-session-620` 2,514, `ar-check-tn-session-620` 2,521, `en-check-tn-session-620` 2,521, `en-check-tw-article` 2,563, `en-check-tw-session` 2,799, `en-check-tn-session-row` 2,810, `en-obs-check-tn-session` 2,833, `ar-check-tw-session` 3,007, `ar-check-tn-session-row` 3,021, `align-workspace-620` 3,023, `obs-check-picker` 3,249 |
| more | `guided-fix` 3,945, `source-texts` 5,152, `align-workspace-row` 10,931, `align-original` 49,452, `align-workspace` 66,388, `help` 78,263 |

The design-system gallery's screens (`parity/ds-gallery`) were captured on the cloud machine
too, both sides. Its gate reads committed reports and recomputes nothing, so it still passes;
whoever recaptures the gallery here must recapture both of its sides.

## The masks, measured on tC4

`parity/app/measure-masks.mjs` on this PC, from the recaptured baseline's screens; each
rectangle was checked by eye against its crop of the baseline PNG (it covers its element and
nothing else of note).

| Mask | Screens | Rectangle (x, y, width, height) |
|---|---|---|
| `about-build` | `about` | 482, 314, 124, 17 (the `<strong>` "4.0.0-rc.1 (d310bf3)") |
| `feedback-attachment` | `feedback` | 364, 585, 552, 59 (the `<pre>` of the attachment) |
| `q2-obs-card` | `shared-home`, `sync-internet-consent`, `sync-failed`, `sync-ok`, `sync-ok-teammates`, `sync-ok-restored` | 866, 236, 138, 31 on each (the OBS card's "Upload changes") |
| `q3-where` | `open-door43`, `open-door43-internet-consent` | 417, 242, 400, 38 (the note "The project opens from git.door43.org. Only public repositories can be opened.") |
| `q3-where` | `open-door43-refused` | 417, 185, 400, 39 |
| `q3-refused` | `open-door43-refused` | 417, 509, 446, 37 (the refusal's sentence) |

tC4's note is a `display: contents` element with no box of its own, so `measureTargets`
measures such an element by the range of what it holds.

## M3: the gate's time

`cargo test -p tc-ui --test screen_record` after a build, debug profile, 26 tests:

| | Load | `a_planted_pixel_fails_the_gate` alone | The whole file (three runs) | `the_committed_record_passes_the_gate` |
|---|---|---|---|---|
| png crates unoptimized | 3.5–4.4 | 3.09 s | 3.81, 3.84, 4.02 s | — |
| png, fdeflate, flate2, miniz_oxide, crc32fast and simd-adler32 at `opt-level = 3` (workspace `Cargo.toml`) | 4.9 | 0.96 s | 1.34, 1.26, 1.26 s | 0.09 s |

The targets were under 2 s for the committed record and under 15 s for the file. The committed
record decodes no PNG (no capture yet); it hashes the 62 baseline PNGs and DOM files.

## M4: the Rust pixel count equals pixelmatch

pixelmatch 7.1.0 (`parity/ds-gallery`, threshold 0, `includeAA: true`) and the gate's
`pixels::compare_files`, on the cloud machine's PNG against this PC's:

| Pair | pixelmatch | Rust |
|---|---|---|
| `home` | 52 | 52 |
| `feedback` | 1,281 | 1,281 |
| `help` | 78,263 | 78,263 |
| `about` against itself with the red byte of (5, 5) flipped | 1 | 1 |
| all 62 screens | the counts of the table above | the same 62 counts (`diff` of the two lists is empty) |

## M5 and the browser tests of the Rust environment

`TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/env-rust.test.mjs`
(tc4 built with `--features test-hooks`; UI build `45a904a0b4623a99`, debug wasm): 10 of 10
pass, 26.3 s, at 20:41 (load 3.39); an earlier run at 20:14 (load 13.6) passed 9 of 10, the
tenth being the first form of E5 (below).

- **M5.** E1: from `env.load` returning (the page's load event) to `settle` returning, with
  `quietMs` 800 and `busy` (`test/quiet` and `__tcIdle`): 954 ms at load 3.4, 1,062 ms at load
  13.6, so about 150 to 260 ms of boot before the quiet starts. When settle returned, no tracked
  request was in flight, and a second tracker that counts only `/api/v1/events` held exactly
  that request, still open.
- E2: with the event stream counted, `settle` waits out a 2 s deadline naming
  `GET …/api/v1/events`.
- E4: with a `test/fieldStore` waiting at `test/hold { at: "save" }`, a shot with `quietMs` 200
  and a 3 s deadline fails with "the app did not go quiet for 200 ms within 3 s; in flight:
  nothing now; the server answered busy (test/quiet)" and still writes its PNG; `home()`'s
  `test/reset` releases the hold, the store answers `written`, and the next shot settles.
- E5: the page's CSP (`connect-src 'self'`) stops a page `fetch` to another origin before the
  network; a navigation to `https://example.org/`, which the CSP does not cover, meets the
  context's route, fails with `ERR_BLOCKED_BY_CLIENT` and is listed in `refused`. The plan's
  first form of E5 expected the fetch itself in `refused`.
- E6: across a language switch through the stand-in's picker, the picker and the sample line
  keep their node numbers, the focus stays on the picker, and the sample's text changes language.

A smoke run of `capture-rust.mjs --unit A6b --only home,about --layers dom` into a scratch
folder under `target/` wrote both PNGs, both DOM files and a `captures.json` with this PC's
machine, and failed both screens after 30 s, waiting for Home's project cards, which the Rust
app does not have until B2: no baseline screen can be captured on the Rust side yet, so all 62
rows of `parity/app/report.json` are `unported`.

## The gate's planted tests

`crates/tc-ui/tests/screen_record/planted.rs` runs the gate on the real baseline with Rust
captures planted in a temporary `parity/app`. These normalizations and rules were broken on
purpose, one at a time, and the planted tests failed each time: no renumbering of generated ids
(at least 8 tests failed), no folder renaming (at least 8), no asset paths (at least 8), no
pixel count (4), failed rows not reported as `CaptureFailed` (2), home cards never compared (2).

Correction, after the verification of A6b: commit `8412f07` and the build's report said that
breaking any normalization or rule failed these tests. That was false for four rules, each of
which the verifier broke on its own with all 26 tests still passing: ids renumbered before the
masks are taken out (the reverse of §7.3's order), behaviour node numbers not renumbered per
side, no masks passed to the behaviour compare, and `syncToast` never compared. Each now has a
planted test, and with each rule broken again exactly that test failed (32 of 33 passed):

| Rule broken | The test that fails |
|---|---|
| ids renumbered before the masks are taken out | `ids_after_a_mask_are_numbered_without_the_ids_inside_it` (`open-door43`: the rebuild's Q3 note holds a `tc-f` id tC4's does not; the two fields after it) |
| no per-side renumbering of behaviour node numbers | `a_scoped_sequence_numbers_its_nodes_whatever_precedes_the_scope` (`about` DOM-first, one more element before the dialog on the Rust page) |
| no masks passed to the behaviour compare | `a_masked_element_takes_no_part_in_a_sequence` (`about`, the version text differs at every step) |
| `syncToast` never compared | `the_sync_toast_is_compared_field_by_field` (`sync-ok-teammates`: `received` 3 for 2, then no toast) |

The verification also found two gate rules missing, now added with their tests (APP-DESIGN
§10, A6b's D12): a residual must name exactly the layers its row differs in, `behaviour` among
them (not checking it fails `a_residual_names_exactly_the_layers_that_differ` and
`a_row_that_differs_only_in_behaviour_is_explained_as_behaviour`), and a Rust screenshot of
another size than tC4's is a failed capture, not a difference a residual can excuse (counting it
as a difference fails `a_screenshot_of_another_size_fails_even_with_a_residual`).
