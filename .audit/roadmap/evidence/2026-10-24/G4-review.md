# G4 review (independent, read-only), 2026-10-24

Scope: `git diff track-G1...track-G4` (shell/graphite.cjs, shell/test/graphite.test.cjs,
shell/e2e/graphite.test.cjs, parity/app/journeys/rtl-project.test.mjs, shell.yml), card G4.md.
Ran: `node --test 'shell/test/*.test.cjs'` -> 34 pass, 0 fail, 0 skipped. Did not run e2e or cargo,
so the Linux/macOS numbers in the log are taken from the log, not re-observed.

## Verdict

No blocker. No major that harms a translator's data or view in the code G4 adds. One product
defect G4 found (Hebrew cards tagged `lang="el"`) is real and correctly routed as a REQUEST.
Card bookkeeping is incomplete (steps 3, 4, frontmatter `next`).

## FUNNEL §8

1. **Code.** Met. `graphiteVerdict` is pure, limits are named constants with their source in a
   comment (graphite.cjs:10-18). `measureProbe(page, fontUrl)` is the one entry point used by both
   the shell and plain-Chromium tests (e2e/graphite.test.cjs:120, :140). It takes `fontUrl` beyond
   the card's `measureProbe(page)`; that is justified (the two runs load the face from different
   origins).
2. **Checks.** Unit test can fail (log shows the 700 px red run; it also covers NaN and 480).
   `plain_chromium_fails_the_graphite_probe` is a real can-fail proof: it asserts the face loaded,
   then that the verdict is not ok on boxWidth. Both e2e tests run in shell.yml via the
   `shell/e2e/*.test.cjs` glob with `no-skips.mjs`; the Playwright install step was added to
   e2e-linux only (plain Chromium is Linux-only by design). The journey runs in app-e2e through
   `scripts/gates.sh --step journeys` (glob `parity/app/journeys/*.test.mjs` + no-skips).
3. **Reach.** All scenarios are translator-reachable (Create screen, USFM import, Draft, Check,
   Align). Nothing to drop.
4. **Siblings.** Partly. The same `lang="el"` hard-code was found at session.rs:288 and :305. The
   check tool's target pane direction in an RTL project was not asserted (see minor 4).
5. **No loss (not a ◆ item).** Verse bytes: compared byte-for-byte after the leave's checkpoint
   (rtl-project.test.mjs:90-92), and the log records the red run. Decision: exactly one record,
   status valid, selection text `يسوع` (:111-114). Alignment: a group on disk joins the clicked
   card's Greek word and `بولس` (:138-140). Waits use the same `settings/viewFromHome` barrier as
   align/check-writes journeys, so the stale `saved` indicator does not race. The claim holds.
6. **Prose and Human.** Comments give reasons. No owner question needed. Card prose is stale
   (minor 1).

## Probe: can it pass without Graphite?

- Font fails to load: `fontLoaded` false fails the verdict (graphite.cjs:23, :37-43). Good.
- Fallback face (e.g. a Naskh face joins and is narrow, under 480): the ascent band 55..70 is what
  catches it (Naskh at 32 px has a much lower ascent). Good, but see minor 2: the descent band does
  not discriminate at all.
- Wrong element: width is the span's DOM box, ascent/descent from a canvas with the same family.
  Both measure the probe text in AwamiProbe. Good.
- Limits: width 480 sits between 302.9 (shaped) and 633-640 (unshaped); wide margin both ways.

## Hebrew step (REQUEST) justified?

Yes, with one correction. Evidence checked:
- build-state.mjs:207 lists `hbo_uhb v3.0.0` as ABSENT from the served resources; screens.mjs:17
  counts it as a missing version. So the en-pins scenario cannot show Hebrew in Align today.
- session.rs:288 and :305 hard-code `lang="el"` on the original-word spans, while the container's
  `dir` follows `orig_dir` (:1136). A Hebrew card would be tagged Greek.
- Track B may not edit `fixtures/` or `crates/**`, so a REQUEST is the right route.
- Correction: the log says "fixtures/tc4/test-fixtures/resources/ holds no Hebrew", which is true,
  but `fixtures/tc4/test-fixtures/hbo_uhb/JON.usfm` already exists in the repo. The REQUEST should
  name it as the source, so the main team only has to serve it, not find one.
- Consequence: INV-RTL-PROJECTS-EDIT-CORRECTLY's clause "the original-language cards read right
  to left" has no test (the Arabic project's originals are Greek, LTR). The card must say the
  invariant is only partly held until the REQUEST lands.

## Findings

### Blocker
None.

### Major
None in G4's code. (The `lang="el"` on Hebrew cards, session.rs:288/:305, is a translator-visible
product defect for screen readers and per-language font choice, but it predates G4 and is correctly
handed over as a REQUEST; the main team should triage it as a major when it lands.)

### Minor
1. **Card bookkeeping incomplete.** G4.md log has no `step 3` line (Windows CI numbers missing;
   Linux numbers are local only, macOS is filed under step 1), no `step 4` note that G3's smoke
   jobs must call the probe, and frontmatter `next` still says "then step 5" though step 5 is
   logged. Fix: add a step 3 line with the e2e-linux/e2e-windows/e2e-macos run id and numbers; add
   the step 4 note; update `next`.
2. **Descent band does not discriminate.** graphite.cjs:17-18: DESCENT 8..20 admits plain
   Chromium's 18 (log 14:24). Measured shaped values are 12.6-13.2, so 8..17 keeps the card's
   4 px spare and would reject the unshaped 18. Fix: `DESCENT_MAX = 17`, and update the unit test.
3. **Hebrew test written but not kept anywhere.** Log 14:38 says "The test is written; it goes in
   once both land", but it is in no commit. Fix: paste it into the PR description with the REQUEST
   (no-skips forbids committing it skipped). Also name `fixtures/tc4/test-fixtures/hbo_uhb/JON.usfm`
   in the REQUEST.
4. **INV-RTL partly proven; check pane direction unasserted.** rtl-project.test.mjs:101-108
   records a decision in an RTL project but never asserts `check-target` reads right to left.
   Fix: `assert.equal(await direction(target), 'rtl')` after :104, and note in the card that the
   cards clause waits for the Hebrew REQUEST.
5. **Short deadline.** rtl-project.test.mjs:136 gives the save 8000 ms while every other wait uses
   60 s; the following `leave` already barriers on the checkpoint. Fix: drop the 8000.
6. **Opaque failure if verse 1 missing.** rtl-project.test.mjs:91-92: if no `\v 1 ` line exists,
   `saved.slice` throws a TypeError instead of the named assertion. Fix:
   `assert.ok(saved, 'verse 1 is on disk')` first.
7. **Journey couples to the `el` hard-code.** rtl-project.test.mjs:133 selects `span[lang="el"]`.
   When the REQUEST fix makes lang follow the original (likely `grc`), this and align.test.mjs
   break. Fix: select the card's word span by testid, not lang, or note it in the REQUEST.
8. **Debug UI, not release.** Step 3 asked for `build-app.sh --release`; the shell test runs the
   debug UI (log 14:24 says the font file is identical). Acceptable; step 4 on the release build
   closes it.
9. **Font URL found by DOM scraping.** e2e/graphite.test.cjs:116-119 derives the build path from
   the first `script[src*="/ui/"]`. If that changes, the test fails loudly (fontLoaded false), not
   falsely, so low risk. No change needed.

Not checked: the CI runs themselves (only the log's run ids), Windows numbers, cargo gates.
