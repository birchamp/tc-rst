# Plan for design unit A6b: the parity harness, Rust side, and the gate in `cargo test`

Planned 2026-10-18 on the owner's Linux PC. Worktree `/home/bot/tc-rst/.claude/worktrees/p0-a6b`,
branch `p0-a6b`, created from `phase0` at `01befe2` (A4a merged; A1, A2, A3, A6a, A9, A10, A11
and G0 merged before it; A4b is in progress on `p0-a4b` and has no commit yet). Commit this file
with your first commit.

The unit's row (docs/APP-DESIGN.md §10, phase 0):

> A6b Parity harness: Rust side and gate *(A-17)* | `env-rust.mjs` (`tc4 serve`, token header,
> `test/reset`, `test/quiet`, event stream left out of `trackRequests`); the four-layer compare
> tool; `masks.json`, `report.json`, `residuals.json`; `screen_record.rs` | A6a, A2, A4a | — |
> the gate fails on a planted pixel, DOM and meaning difference; a screen that reaches the
> settle deadline fails | the gate runs in `cargo test`, and the planted differences fail it

What the planner read, so you need not: AGENTS.md; CLAUDE.md; docs/PLAN.md (P7 in full);
docs/ARCHITECTURE.md; docs/PERFORMANCE.md; docs/APP-DESIGN.md "Owner's answers", "G0 results",
§0, §1, §2 (AD1–AD39), §3.9, §3.10, §5.1–§5.3, §5.8, §6.5 (the Q-25 residual), §7.1–§7.9 in
full, §8.1–§8.4, §10 (introduction, every row, the order paragraph), §11, the red team table,
RT-17, RT-34 and "How each point was answered"; docs/evidence/linux-machine-2026-10-18.md.
Code: `parity/app/*` (all), `parity/tc4-baseline/{capture.mjs,baseline.test.mjs,README.md,
manifest.json,package.json}`, `parity/ds-gallery/{diff.mjs,dom-diff.mjs,capture-rust.mjs,
protocol.mjs,scenarios.mjs,README.md,package.json}`, `crates/tc-ui/{Cargo.toml,build-app.sh,
app/index.html,app/boot.js,app/smoke.mjs,tests/gallery_record.rs,tests/app.rs}`,
`crates/tc-ui/src/app/web/{mod.rs,api.rs,js.rs,root.rs}`, `crates/tc-api/src/{test.rs,ids.rs,
net.rs,prefs.rs}`, `crates/tc-api/tests/golden.rs` (the `UPDATE_GOLDEN` pattern) and its golden
files for `test/hold` and `test/fieldStore`, `crates/tc-app/src/hooks.rs` (`Quiet`, `Reset`,
`Holds`), `crates/tc-app/src/prefs.rs` (`reset`), `crates/tc-app/src/handlers.rs` (`hello`),
`crates/tc4/src/args.rs`, `crates/tc4/tests/hooks.rs`, `crates/tc-door43/src/names.rs`
(`ServerUrl`), the workspace `Cargo.toml`, `.github/workflows/ci.yml`. tC4 at `d310bf3`:
`src/ds/components/primitives/{Field,FieldGroup}.jsx`, `src/ds/components/surfaces/Modal.jsx`,
`src/ds/components/feedback/ConfirmDialog.jsx`, `src/ds/components/forms/{Radio,Select,
SuggestField}.jsx` (the generated ids), `src/i18n/en.json` (the Q3 sentences).

---

## 0. What the planner measured (read this first)

The planner built tC4 at `d310bf3` in a scratch clone and ran the baseline capture of
`parity/tc4-baseline` twice on this PC, from an unmodified copy of `parity/` (the code at
`01befe2`), with the help pages fetched from GitHub as tC4's prebuild does.

| Fact | Result |
|---|---|
| PNGs equal to the committed baseline | **5 of 62.** 57 differ. |
| DOM snapshots equal to the committed baseline | **62 of 62**, byte for byte. |
| `homeCards`, `syncToast`, `status`, `absentReads` in the manifest | all equal |
| Two consecutive full captures here | byte-identical PNGs, DOM files and `manifest.json` (load average 12.8 during the second) |
| One full capture | 2 min 24 s and 2 min 31 s |
| Digest of the PNG set (`sha256` of the lines `"<name> <sha256>\n"` in manifest order) | committed `befcbf2a…b5978`; this PC `5229c9e2e4d33212f1de2083d04fdf0d9deb8e0293e24a6184e0c0c42894241f` |
| Digest of the DOM set (same, with `domSha256`) | `029f4b7066582924f8f080dcd5490aed018d966aefc0994257437181f430cb2f` on both |
| New total of PNG bytes | 8,058,301 (committed: 8,094,421) |
| Font list here | `fc-list --format '%{file}\t%{index}\t%{fontversion}\n' \| LC_ALL=C sort \| sha256sum` = `a15fd043…72eb33`, 534 faces, fontconfig 2.17.1 |

Pixels that differ between the committed PNG and this PC's (every channel byte compared, which
is what `pixelmatch` with threshold 0 and anti-aliasing counted measures on these opaque
screenshots): `obs-understand`, `obs-translate`, `en-obs-understand`, `en-obs-translate`,
`en-understand-questions` 0; most Home-based screens 48 or 52 (one glyph: Home's "stories →"
arrow draws thin here and bold on the cloud machine, at x 295–304, y 366–375); dialogs 154 to
1,416; Check sessions about 1,900 to 3,000; `guided-fix` 3,945; `source-texts` 5,152;
`align-workspace-row` 10,931; `align-original` 49,452; `align-workspace` 66,388; `help` 78,263.

What this means, and why it shapes the whole unit:

- **The baseline does not reproduce here.** docs/evidence/linux-machine-2026-10-18.md says "the
  parity harness must show that the baseline reproduces here before a pixel difference is
  trusted". It does not, so a pixel layer that compared a Rust capture taken here with the
  committed PNGs would blame the port for this PC's fonts. A6b recaptures the tC4 baseline here
  (step 2) and makes both captures carry a machine fingerprint, so pixels are only ever compared
  between captures of one machine (APP-DESIGN risk 8: "Both captures run on one image").
- **The DOM layer is machine-independent.** A5's DOM-only parity (its "done when") can run on any
  machine.
- **Captures here are deterministic,** so a recapture after A6b's harness changes must give the
  digest above exactly; that is the proof the changes altered no image.

A scratch copy with tC4 built (and its `node_modules`) may still be at
`/tmp/claude-1000/-home-bot-tc-rst/8969a572-b4b3-440d-b68e-470f3f1198e1/scratchpad/a6b-measure/`
(`tc4/` is the clone, `manifest-run1.json` the local manifest). It is in tmpfs and is not yours:
read from it if it is there, never write or delete in it, and rebuild your own (§2) if it is gone.

---

## 1. What A6b is, and what it is not

A6b builds the Rust half of the parity harness and the gate that runs in `cargo test`:

1. `parity/app/env-rust.mjs`: the driver's environment over `tc4 serve` (the token header,
   `test/reset`, `test/quiet`, the event stream left out of `trackRequests`).
2. `parity/app/capture-rust.mjs`: captures Rust screens with the same driver, `captureScreen`,
   and rules as the tC4 capture, into committed files.
3. The four-layer compare: pixels, DOM, meaning and behaviour. **It is written in Rust, inside
   the gate** (decision D1 below): `crates/tc-ui/tests/screen_record.rs` recomputes every layer
   from the committed captures on every `cargo test`, compares the result with the committed
   `parity/app/report.json`, and applies the gate's rules. `UPDATE_REPORT=1` rewrites the report.
4. `parity/app/masks.json`, `parity/app/residuals.json`, `parity/app/report.json`, and the small
   inputs the rules need (`units.json`, `scopes.json`, `cases.json`, `captures.json`).
5. The tC4 baseline recaptured on this PC with a machine fingerprint (§0).

What A6b is **not**:

- It ports no screen. In phase 0 the Rust page is A4a's stand-in root
  (`crates/tc-ui/src/app/web/root.rs`): there is no Home, no project card, no backend with
  projects (`--backend memory --scenario` is B1b's). So **no baseline screen can be captured on
  the Rust side yet**, and at the end of A6b all 62 rows of `report.json` are `unported`, each
  naming the unit that must capture it. The planted tests prove the gate on real baseline data
  instead (§5, step 5).
- It writes no behaviour sequence of its own. A5 adds the first ones ("account-menu keyboard",
  "M16 preview and restore", "help a–i"); A6b provides the recorder, the file format and the
  comparison, and proves them on synthetic records and on the stand-in page.
- It does not build the tC4 view-state gallery (A7) or the fake Door43 in the Rust environment
  (B9b). It defines where A7's cases are listed (`parity/app/cases.json`, empty).
- It does not touch tc-store, tc-journal or the storage worktree.

No translator data is written by anything in this unit: `tc4 serve` runs with an empty projects
folder in `target/`, and the harness never opens a project. The no-loss rule still shapes the
gate: a capture with a page error (a wasm panic shows as one), a view error, a boot error, a
refused external request or a settle timeout is `failed`, and no residual can excuse it, so a
crash or a lost screen can never pass as "a known difference".

---

## 2. Before you start

1. `git -C /home/bot/tc-rst/.claude/worktrees/p0-a6b log --oneline -1` shows `01befe2` (or your
   own commits on top). If `phase0` moved (A4b may merge), `git merge phase0` and say so in the
   message; then add each newly landed unit to `parity/app/units.json` (step 5).
2. Every shell command that runs cargo, node, npx or Playwright starts with `. ~/.tc-env && `.
   Use `-j 6` for cargo; `CARGO_JOBS=6` for `crates/tc-ui/build-app.sh`. Use only this
   worktree's `target/`. `df -h /home` showed 253 GB free at planning.
3. Playwright is installed nowhere in this repository on this PC. Install the baseline's own
   pinned copy (network to the npm registry works):

   ```sh
   . ~/.tc-env && cd /home/bot/tc-rst/.claude/worktrees/p0-a6b/parity/tc4-baseline && npm ci --no-audit --no-fund
   ```

   That gives `playwright@1.56.1` and `fflate`; Chromium build 1194 is in
   `~/.cache/ms-playwright`, and `~/.tc-env` sets `PLAYWRIGHT_HOST_PLATFORM_OVERRIDE`. For the
   pixelmatch cross-check (M4), also `npm ci` in `parity/ds-gallery` (pixelmatch 7.1.0, pngjs).
   Both `node_modules` folders are git-ignored.
4. A tC4 build for the tC4 side (steps 2 and 4). The tC4 checkout
   `/home/bot/unfoldingword/translationcore4` is read-only and has **no `node_modules` and no
   `public/help`**, and `DEFAULT_TC4` in `build-state.mjs` is the cloud path
   `/home/user/unfoldingword/translationcore4`. So make your own clone in your scratchpad:

   ```sh
   S=<your scratchpad>/tc4-build && mkdir -p $S && cd $S
   git clone -q --no-checkout --no-hardlinks /home/bot/unfoldingword/translationcore4 tc4
   git -C tc4 checkout -q d310bf3
   . ~/.tc-env && (cd tc4 && npm ci --no-audit --no-fund)     # about 6 s with a warm cache
   export TC4_ROOT=$S/tc4
   ```

   `capture.mjs` runs tC4's `scripts/fetch-help.mjs` in `TC4_ROOT`, which fetches the pinned help
   pages from GitHub (reachable here) into the clone's `public/help`. **Never commit a capture
   whose `manifest.json` `client` says `WITHOUT the help pages`.** Use a port of your own for the
   mock (`--port 29871`); other agents may hold 19998.
5. Rust side builds, for the Playwright tests of step 3:

   ```sh
   . ~/.tc-env && cd /home/bot/tc-rst/.claude/worktrees/p0-a6b
   cargo build -j 6 -p tc4 --features test-hooks          # target/debug/tc4
   CARGO_JOBS=6 crates/tc-ui/build-app.sh                  # target/tc-ui-app
   ```

   `tc4 serve --test-hooks` only exists in a build with the feature; without the flag every
   `test/*` path is 404 (`crates/tc4/tests/hooks.rs:129`).

---

## 3. Decisions, and the design text that changes with them

The design is right in its intent and wrong or silent in the places below. Fix the text of
docs/APP-DESIGN.md in the commit that builds each part, say why in that commit's message, and
list them in the A6b row's built note (step 6) and in your final answer.

| Id | Decision | Why | Design text to change |
|---|---|---|---|
| D1 | The compare is Rust code in `crates/tc-ui/tests/screen_record/`; Node only captures. `screen_record.rs` recomputes all four layers from committed files and fails when the committed `report.json` differs; `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record` rewrites it (as `UPDATE_GOLDEN=1` does in `crates/tc-api/tests/golden.rs:553`). | The row asks that the planted differences fail the gate *in `cargo test`*. With a Node compare (the ds-gallery's way) the cargo test could only trust numbers in a report; here a planted pixel in a capture fails `cargo test` itself, a report cannot be stale or hand-edited, and there is one implementation of each layer. CI's rust job then runs the whole compare with no browser. | §7.5: "`report.json` is what `screen_record.rs` computes from the committed captures; it fails when the committed file differs". §7.1's table gains `capture-rust.mjs`. |
| D2 | A pixel counts when any of its channel bytes differ (after RGB → RGBA). Decode only when the two files' bytes differ; equal bytes are 0 at no cost. | This is exactly what `pixelmatch` with threshold 0 and `includeAA` counts on opaque images: equal pixels are skipped, and for unequal ones the YIQ delta is a nonzero quadratic form of an invertible transform of the difference, so it exceeds a threshold of 0. M4 checks the counts against pixelmatch on real pairs. | §7.3 layer 1: name the rule; keep pixelmatch as the reference it equals. |
| D3 | Both captures record a **machine fingerprint**: `{ platform, chromium, fonts: { count, sha256 } }`, where `fonts.sha256` is the sha256 of `fc-list --format '%{file}\t%{index}\t%{fontversion}\n'` sorted bytewise. tC4's goes into `manifest.json` (`machine`), Rust's into `captures.json`. The gate compares pixels only between captures whose fingerprints are equal. A6b recaptures the baseline on this PC (step 2). | §0: 57 of 62 PNGs differ here from fallback glyphs and system fonts; every DOM is identical. Without a fingerprint, the next machine move (a CI image, another PC) would again compare across machines silently. A font added, removed or upgraded changes the list, so it is caught; a fontconfig rule change that alters no listed face is not (stated as a residual of the rule). | §7.3 layer 1 and §7.9: the baseline is recaptured on the capture machine and both captures carry `machine`; risk 8 points to it; §7.9's pinned image uses the fonts of the baseline's machine. |
| D4 | Generated ids are renumbered per prefix by first appearance with `tc-(f\|g\|dlg\|cfm\|rg)(\d+)`, not `tc-[fg]\d+`. | tC4 mints `tc-f` (Field.jsx:14), `tc-g` (FieldGroup.jsx:35), `tc-dlg` (Modal.jsx:24), `tc-cfm` (ConfirmDialog.jsx:19) and `tc-rg` (Radio.jsx:34), each from its own module counter. The baseline holds `tc-dlg` on 40 lines and `tc-f` on 32; the design's pattern misses `tc-dlg`, so every dialog screen would differ on ids alone. tc-ui mints the same prefixes (`crate::ds::next_id`). | §7.3 layer 2. |
| D5 | Asset normalization applies only to same-origin paths (values starting with `/`) of `src`, `href` and `poster`, and to `url(/…)` in style text: the path is reduced to its file name with a Vite hash `-[A-Za-z0-9_-]{8}` before the extension removed. Other URLs compare as they are. | tC4's are `/clients/uw-tc4/assets/translationcore-logo.png` and `/clients/uw-tc4/help/index.html`; the rebuild's are `/ui/<build>/images/…` and `/help/…`. An external link (`https://git.door43.org/…`) reduced to its last segment would hide a wrong link. | §7.3 layer 2. |
| D6 | Status is `same`, `declared`, `differs`, `unported` (no layer captured yet) or `failed` (a capture error, a page error, a view or boot error, a refused external request, or the settle deadline). A row lists the layers it still lacks with the unit that brings each, from §7.6 (`missing`). `parity/app/units.json` lists the landed units. The gate fails when a missing layer's unit has landed, when a captured layer was captured by a unit that is not listed as landed, and when a `memory+scripted` row's `pendingUntil` unit has landed. A `failed` row fails whatever `residuals.json` says. | §7.5 requires a row for every baseline screen, and none can be captured in phase 0; without a "not yet, and who" status the gate could not exist until B2, and nothing would notice a unit that landed without capturing its screens. CI checks out one commit (`actions/checkout` depth 1), so "landed" must be a file, not git history. | §7.5. |
| D7 | `pendingUntil` of a `M+S` screen is its Recapture unit. | §7.6 writes `pending_until` in the notes of only 2 of the 5 scripted screens (`open-door43-refused` E1b, `sync-failed` E1a); both equal their Recapture unit, and `sync-ok`, `sync-ok-teammates`, `sync-ok-restored` have Recapture E1a and no note. | §7.5 and §7.6's lead-in. |
| D8 | A DOM-only capture (A5's and A8's screens before their pixels unit) compares only a subtree, named in `parity/app/scopes.json` (the dialog, or the account menu panel); a capture that claims pixels compares the whole `#root`. | §7.6 gives eight screens "A5 DOM; B2 pixels" or "A8 DOM; B9b pixels": the Home behind those dialogs is not ported until B2, so a whole-page DOM could never match before then. `open-door43-internet-consent` holds two dialogs; the consent is the last. | §7.3 layer 2 and §7.6's lead-in. |
| D9 | The Rust environment's `home()` is: `test/reset`, then `prefs/setLocale` when the screen's language is not English, then `localStorage` and `sessionStorage` cleared, then a load of `/#t=<token>` **through `about:blank`**; `settle` also waits for `test/quiet` and for the page's `window.__tcIdle`. | `test/reset` carries only a request id (`crates/tc-api/src/test.rs:211`) and resets prefs to their defaults; there is no scenario seed before B1b. A `goto` to the URL the page already shows, with a fragment, is a fragment navigation and reloads nothing. `__tcIdle` (`crates/tc-ui/src/app/web/js.rs:55`) also covers the catalog fetches under `/ui/`, which the tracker does not count. | §7.1's seams table. |
| D10 | The token header is `extraHTTPHeaders` on the context, as §5.2 and §7.1 say, and the context aborts every request to another origin before it is sent. | The UI also sets the header, and tc-server accepts a repeated header whose every value is the token (§5.2). `extraHTTPHeaders` would add the token to any origin; the abort means it never leaves the machine. (A4a's smoke used a route outside `/api/` on purpose, to catch a client that forgets the header; the parity capture is not that test.) | none (§7.1 already says it); note the abort. |

The masks of §7.3 (the build line in `about`, the build and platform lines in `feedback`, and the
Q2 and Q3 text on the eight `rebuildDiffers` screens) are built as the design says, with their
rectangles measured on tC4 by a script (step 4). They are ten screens in all.

---

## 4. File formats

All files are JSON with two-space indentation and a final newline, written in a stable order, so
a rerun gives the same bytes. Rust reads them with `#[serde(deny_unknown_fields)]`, except
`manifest.json`, which is A6a's and carries fields the gate does not read.

### 4.1 `parity/tc4-baseline/manifest.json` (A6a's; one field added)

A top-level `"machine": { "platform": "linux-x64", "chromium": "141.0.7390.37", "fonts": {
"count": 534, "sha256": "<64 hex>" } }`, written by `capture.mjs` from `machineFingerprint`
(step 2). Nothing else changes.

### 4.2 `parity/app/units.json`

```json
{
  "about": "The units of docs/APP-DESIGN.md §10 that have landed on the integration branch. Each unit adds its own id in the branch that builds it. screen_record.rs reads it to know which rows must be captured by now (APP-DESIGN §7.5).",
  "landed": ["A1", "A2", "A3", "A4a", "A6a", "A6b", "A9", "A10", "A11", "G0"]
}
```

Ids must be §10 unit ids (parsed from the doc, §5 step 5). Sorted as written above is not
required; the gate requires no duplicates. Add `A4b` if it merges before you do.

### 4.3 `parity/app/cases.json`

```json
{
  "about": "The cases of the tC4 view-state gallery (APP-DESIGN §7.4), which unit A7 builds. Each case needs a row in report.json. Empty until A7.",
  "cases": []
}
```

A case entry (A7 fills them; define the shape now so the Rust types exist): `{ "name":
"<case>", "png": "<path under parity/>", "dom": "<path under parity/>", "until": "A7" }`, where
`png` and `dom` are tC4's side of the case.

### 4.4 `parity/app/scopes.json`

```json
{
  "about": "The part of the DOM a DOM-only capture compares (APP-DESIGN §7.3 layer 2, §7.6): until a screen's pixels unit captures it whole, the Home behind its dialog is not the port's yet.",
  "scopes": {
    "account-menu": { "attr": "data-testid", "value": "account-menu-panel", "which": "first" },
    "app-language": { "attr": "role", "value": "dialog", "which": "first" },
    "help": { "attr": "role", "value": "dialog", "which": "first" },
    "about": { "attr": "role", "value": "dialog", "which": "first" },
    "feedback": { "attr": "role", "value": "dialog", "which": "first" },
    "share-internet-consent": { "attr": "role", "value": "dialog", "which": "last" },
    "share-signin": { "attr": "role", "value": "dialog", "which": "last" },
    "open-door43-internet-consent": { "attr": "role", "value": "dialog", "which": "last" }
  }
}
```

Checked against the baseline DOM at planning: `account-menu` line 20 has
`data-testid="account-menu-panel"`; `app-language`, `about` line 22, `feedback` line 128 and
`help` line 22 (`data-testid="help-panel"`) hold one `role="dialog"`; `share-internet-consent`
(line 128) and `share-signin` (line 129) hold one; `open-door43-internet-consent` holds two
(lines 128 and 164), and the consent is the second. The gate refuses a scope that matches no line
on either side, and a scope for a screen that is not DOM-first in §7.6.

### 4.5 `parity/app/masks.json`

```json
{
  "about": "Declared differences (APP-DESIGN §7.3): each mask names the element that may differ, the screens it applies to, the rectangle it covers in tC4's screenshot of each, and the ruling it cites. Rectangles are measured on tC4 by measure-masks.mjs on the baseline's machine.",
  "masks": [
    {
      "id": "about-build",
      "ruling": "build",
      "why": "The line names the program that runs: tC4 4.0.0-rc.1 (d310bf3) there, the rebuild's own version and build here.",
      "target": { "testid": "about-version" },
      "rust": null,
      "rects": { "about": { "x": 0, "y": 0, "width": 0, "height": 0 } },
      "meaning": null
    }
  ]
}
```

The entries to write (rects filled by step 4's script; `target` is tC4's element; `rust` is the
rebuild's element when it differs from `target`, `null` meaning "same as `target`"):

| id | screens | ruling | target | meaning |
|---|---|---|---|---|
| `about-build` | `about` | `build` | `{ "testid": "about-version" }` (`<strong>`, line 33 of `dom/about.json`, text `4.0.0-rc.1 (d310bf3)`) | — |
| `feedback-attachment` | `feedback` | `build` | `{ "testid": "feedback-attachment" }` (`<pre>`, line 169, "translationCore 4.0.0-rc.1 (d310bf3) · Linux x86_64 / no Report") | — |
| `q2-obs-card` | `shared-home`, `sync-internet-consent`, `sync-failed`, `sync-ok`, `sync-ok-teammates`, `sync-ok-restored` | `Q2` | `{ "testid": "share-card-<OBS>" }` (the card div with `data-action` and its button, `dom/shared-home.json` lines 53–54) | `{ "homeCards": { "<OBS>": ["upload", "sync"] } }` |
| `q3-where` | `open-door43`, `open-door43-refused` | `Q3` | `{ "text": "The project opens from" }` (the note's `<div>`, line 142 / 143; it has no test id) | — |
| `q3-refused` | `open-door43-refused` | `Q3` | `{ "text": "tC4 could not get the project from Door43." }` (the `<span>` inside `data-testid="open-door43-refused"`, line 164) | — |

`<OBS>` and `<BIBLE>` stand for the two projects: tC4's repo path
(`_local_/_local_/sample_burrito_obs`, `_local_/_local_/sample_burrito`) on the tC4 side, the
folder from `captures.json` `projects` on the Rust side. `ruling` is `build` or an owner ruling
`Q<n>` that docs/PLAN.md lists in "Where the rebuild differs from tC4 4.0 on purpose" as `(Q<n>)`.
A `text` target matches the element whose first text run starts with the text; on the Rust side
the text is the thing that differs, so B9b and B10 must set `rust` for the Q3 masks (for example
a test id the port adds inside the masked element) when they capture those screens; the gate
fails a captured row whose mask target is not found on the Rust side.

### 4.6 `parity/app/residuals.json`

```json
{
  "about": "Differences that are listed and explained rather than fixed (PLAN P7: a human reviews the differences, and the differences are listed). `rows` explains a report row whose status is `differs`; `general` lists differences no single row shows.",
  "general": [
    {
      "id": "engine",
      "why": "P7 compares screenshots taken in Chromium 141 (Playwright 1.56.1, build 1194). The product ships in Electronite v37, whose Chromium is 138, so the comparison engine is not the shipped one. G2 adds one capture run of the Rust app inside Electronite, compared with the Chromium 141 run of the same build (Q-28).",
      "cites": "RT-34, Q-28",
      "until": "G2"
    },
    {
      "id": "fontconfig-rules",
      "why": "The machine fingerprint lists every font face and its version, so a font added, removed or upgraded stops pixel comparisons until both sides are recaptured. A fontconfig rule that changes which face draws a fallback glyph without changing the list is not caught.",
      "cites": "APP-DESIGN §7.3 layer 1",
      "until": null
    }
  ],
  "rows": {}
}
```

A row entry: `"<row name>": { "layers": ["pixels"], "why": "<what differs and why it stays>",
"until": "<unit or null>" }`. The gate requires an entry for every `differs` row and refuses an
entry for any other row. B5b will add the general residual of Q-25 (§6.5).

### 4.7 `parity/app/captures.json` (written by `capture-rust.mjs`)

```json
{
  "about": "The Rust app's captures: what capture-rust.mjs saw at each shot. The PNG and DOM files are beside it, in screens/ and dom/.",
  "machine": null,
  "build": null,
  "projects": {},
  "screens": []
}
```

At the end of A6b it is exactly this (no capture). With captures:
`"machine"` as in 4.1; `"build": { "ui": "<16 hex of ui/<build>/>", "commit": "<hello.build.commit>",
"api": 1 }`; `"projects": { "sample": { "_local_/_local_/sample_burrito": "<folder>",
"_local_/_local_/sample_burrito_obs": "<folder>" }, "en-pins": {…}, "shared": {…} }`; and one
entry per captured screen, in manifest order:

```json
{
  "name": "about", "kind": "screen", "scenario": "sample", "locale": "en",
  "by": "A5", "backend": "memory", "layers": ["dom"],
  "error": null, "pageErrors": [], "refused": [],
  "homeCards": {}, "syncToast": null,
  "maskRects": { "about-build": { "x": 1, "y": 2, "width": 3, "height": 4 } },
  "png": "screens/about.png", "sha256": "<64 hex>",
  "dom": "dom/about.json", "domSha256": "<64 hex>"
}
```

`layers` is what the capture claims (`pixels`, `dom`, `meaning`); `by` is the unit that captured
it; `backend` is `memory`, `memory+scripted` or `store`. A capture whose `machine` differs from the
file's is refused by `capture-rust.mjs` unless it recaptures every screen in the file.

### 4.8 `parity/app/report.json` (written by `UPDATE_REPORT=1`)

```json
{
  "about": "…",
  "baseline": { "tc4Commit": "d310bf3…", "machine": { … } },
  "rust": null,
  "totals": { "rows": 62, "same": 0, "declared": 0, "differs": 0, "unported": 62, "failed": 0 },
  "rows": [
    { "name": "home", "kind": "screen", "scenario": "sample", "locale": "en", "status": "unported",
      "missing": { "pixels": "B2", "dom": "B2", "meaning": "B2" } },
    { "name": "account-menu", "kind": "screen", "scenario": "sample", "locale": "en", "status": "unported",
      "missing": { "pixels": "B2", "dom": "A5", "meaning": "B2" } }
  ]
}
```

A captured row adds: `by`, `backend`, `pendingUntil` (a unit or null), `error` (null unless
`failed`), and per compared layer:

- `pixels`: `{ "tc4Sha256", "rustSha256", "mismatched", "outsideMasks", "masks": [ids used],
  "diff": "diffs/<name>.png" | null }`;
- `dom`: `{ "tc4Sha256", "rustSha256", "scope": {…} | null, "same": bool, "declared": [mask
  ids whose subtree differs], "firstDiffs": [up to 5 strings] }`;
- `meaning`: `{ "same": bool, "declared": [mask ids], "diffs": [strings] }`;
- `behaviour`: `[ { "sequence", "steps", "same", "firstDiff": string | null } ]`.

Rows are in manifest order, then `cases.json` order. Line texts in `firstDiffs` are cut to 300
characters.

### 4.9 Behaviour recordings

`parity/tc4-baseline/behaviour/<sequence>.json` (tC4's) and `parity/app/behaviour/<sequence>.json`
(Rust's), each `{ "sequence": "<name>", "screen": "<baseline screen it starts from>", "steps": [
{ "label": "start" | "<step as text>", "dom": <domSnapshot with node ids>, "focus": "<pageState
focus line>", "overflow": "<body overflow>" } ] }`. A6b creates neither folder (A5 adds the first
recordings); the gate handles their absence.

---

## 5. Steps, in order

Each step ends in a commit that builds and passes its tests. Run `cargo fmt --all` before each
Rust commit.

### Step 1. The shared capture code: busy, machine, targets, node ids

Files: `parity/app/observe.mjs`, `parity/app/observe.test.mjs`.

1. `settle(page, inFlight, { quietMs = 800, deadlineMs = 30_000, busy = null } = {})`. When no
   tracked request has been in flight for `quietMs` and `busy` is given, `await busy()`; if it
   answers true, the quiet starts again from now and the loop goes on; otherwise settle ends as
   today. A deadline reached while the last look said busy throws
   `the app did not go quiet for ${quietMs} ms within ${deadlineMs / 1000} s; in flight: …;
   the server answered busy (test/quiet)` (keep today's first part word for word: the existing
   test matches it). Without `busy` the behaviour and the messages are exactly today's, which the
   tC4 recapture of step 2 proves.
2. `captureScreen({ page, app, screen, inFlight, file, busy = null, failureTestIds =
   ['view-error'] })`: passes `busy` to `settle`, and reports the first of `failureTestIds` on the
   page as `error` (`view error: …` for `view-error`, `boot error: …` for `boot-error`).
3. `export function machineFingerprint(chromiumVersion, { run = defaultRun } = {})` returning
   `{ platform: \`${process.platform}-${process.arch}\`, chromium, fonts: { count, sha256 } }`,
   where `defaultRun` calls `execFileSync('fc-list', ['--format',
   '%{file}\\t%{index}\\t%{fontversion}\\n'], { encoding: 'utf8' })`; the lines are split, empty
   ones dropped, sorted with a plain `<` comparison (bytewise for these ASCII paths, the same as
   `LC_ALL=C sort`), joined with `\n` plus a final `\n`, and hashed. When `fc-list` cannot run,
   `fonts` is `null` (a Windows or macOS run); the gate treats a `null` fingerprint as "not
   comparable" (step 5). The `run` parameter is for the unit test only. Use `node:child_process`
   and `node:crypto` only: the file says "No npm import".
4. `export async function measureTargets(page, targets)`: in the page, for each `{ id, target }`
   (`{ testid }` or `{ text }`), finds the element (for `text`: the element whose first non-empty
   text child starts with the text, via a TreeWalker; the first match in document order) and
   returns `{ [id]: { x: floor(left), y: floor(top), width: ceil(right) - floor(left), height:
   ceil(bottom) - floor(top) } | null }`, clipped to the viewport.
5. `domSnapshot({ ids = false } = {})`: with `ids`, each element line also carries ` #<n>` or
   ` #new` as its last property, from `tagNodes()` (port `parity/ds-gallery/protocol.mjs:222-229`,
   rooted at `#root`, numbering the elements that have no `__sid` yet). With `ids` false the
   output must be byte-identical to today's (the committed DOM files prove it in step 2). Callers
   pass the option through `page.evaluate(domSnapshot, { ids: true })`; today's
   `page.evaluate(domSnapshot)` passes `undefined`, so keep the default parameter.

Tests in `observe.test.mjs` (no browser; CI's `parity-baseline` job runs this file):

- `settle waits while the server answers busy, and counts the quiet from when it stops`: no
  request in flight; `busy` answers true until the fake clock reaches 300 ms, then false;
  settle with `quietMs: 40` returns no earlier than 300 + 40 ms and before 300 + 40 + 200.
- `a screen whose server stays busy fails at the deadline and says so`: `busy` always true;
  rejects with `/did not go quiet for 40 ms within 0\.15 s; in flight: nothing now; the server
  answered busy \(test\/quiet\)/`.
- `busy is asked only once the requests are quiet`: a request in flight for 100 ms; `busy` counts
  its calls; no call before 100 + quietMs.
- `the machine fingerprint hashes the sorted font list`: with an injected `run` returning two
  lines in reverse order, `fonts.count` is 2 and `fonts.sha256` equals the sha256 of the sorted
  text; with a `run` that throws, `fonts` is `null`.

Commit: "Let the capture wait for a busy server, record its machine, and measure elements".

### Step 2. Recapture the tC4 baseline on this PC with its machine

Files: `parity/tc4-baseline/capture.mjs`, `manifest.json`, `screens/*.png`, `README.md`,
`baseline.test.mjs`; `docs/evidence/linux-machine-2026-10-18.md` (one line); APP-DESIGN §7.3
layer 1, §7.9, risk 8 (D3).

1. `capture.mjs` writes `machine: machineFingerprint(browser.version())` into the manifest, after
   `browser` and before `viewport` (take the version before `browser.close()`; it already does).
   Nothing else in `capture.mjs` changes.
2. Recapture: `. ~/.tc-env && cd parity/tc4-baseline && TC4_ROOT=$S/tc4 node capture.mjs --port
   29871`. Then:
   - the PNG digest of §0 must be `5229c9e2e4d33212f1de2083d04fdf0d9deb8e0293e24a6184e0c0c42894241f`
     and the DOM digest `029f4b7066582924f8f080dcd5490aed018d966aefc0994257437181f430cb2f`
     (`node -e "const m=require('./manifest.json');const c=require('crypto');const d=(f)=>c.createHash('sha256').update(m.screens.map(s=>s.name+' '+s[f]+'\n').join('')).digest('hex');console.log(d('sha256'),d('domSha256'))"`).
     If the PNG digest differs, the PC's fonts changed since planning: run the capture once more,
     require the two runs to agree byte for byte, and record the new digest and font list in the
     evidence. The DOM digest must not change in any case: if it does, a step 1 change altered
     the snapshot; fix it.
   - `git diff --stat parity/tc4-baseline/dom` must be empty.
   - `manifest.client` must say "with the help pages".
3. README (`parity/tc4-baseline/README.md`): the "Output" bullet gets the new total (8,058,301
   bytes if the digest matched; `baseline.test.mjs` checks the number); the "Browser" bullet says
   where Chromium 1194 is on this PC (Playwright's cache, with
   `PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64`, docs/evidence/linux-machine-2026-10-18.md);
   a new bullet "Machine" says the screenshots were captured on the owner's Linux PC on <date>,
   that `manifest.json` `machine` records Chromium and the digest of the font list, and that the
   Rust capture compares pixels only on a machine with the same fingerprint; a new subsection
   under "Rendering determinism", "Recaptured on the Linux PC", with §0's table (57 of 62 PNGs
   changed, counts by group, the "stories →" example, 62 of 62 DOM files and every `homeCards`,
   `syncToast`, `status` and `absentReads` unchanged, two runs byte-identical) and where the cloud
   images are (git, before this commit). "Capture another tC4 commit" says to `npm ci` in the
   clone when the checkout has no `node_modules` (this PC). "Using the baseline for the Rust
   rebuild": replace "The two renderers differ, so a comparison needs a tolerance." with a pointer
   to `parity/app` (exact counts, declared masks, listed residuals).
4. `baseline.test.mjs`: a test `the manifest records the machine of its screenshots` — `machine`
   has a `platform`, a `chromium` equal to the version in `manifest.browser`, and `fonts` with a
   positive `count` and a 64-hex `sha256`; and the README contains the first 12 hex digits of
   `fonts.sha256`.
5. `docs/evidence/linux-machine-2026-10-18.md`: after its last bullet, one line: the baseline did
   not reproduce here; A6b recaptured it (cite the evidence file of step 6).

Run `node --test parity/tc4-baseline/baseline.test.mjs parity/tc4-baseline/mock-server/server.test.mjs
parity/app/state.test.mjs parity/app/observe.test.mjs` (CI's parity job): green.

Commit: "Recapture the tC4 baseline on the Linux PC, where 57 of 62 PNGs differ by fonts and no
DOM differs, and record each capture's machine".

### Step 3. The Rust environment, its capture and the behaviour recorder

Files: `parity/app/env-rust.mjs`, `parity/app/capture-rust.mjs`, `parity/app/behaviour.mjs`,
`parity/app/env-rust.test.mjs`, `parity/app/.gitignore` (nothing new: run folders go to
`target/`), `.github/workflows/ci.yml` (add `parity/app/env-rust.test.mjs` to the parity job's
`node --test` line; its browser tests skip there). APP-DESIGN §7.1 (D9, D10).

#### 3.1 `env-rust.mjs`

Built-ins only at the top level (`node:child_process`, `node:http`, `node:crypto`, `node:fs`,
`node:path`, `node:readline`), so CI can import it for the pure tests.

```js
/** The folders the Rust app gives the two projects of each scenario. B1b's scenario loader
 * must create them under these names, or change this map. */
export const PROJECTS = {
  sample: { [BIBLE]: 'sample_burrito', [OBS]: 'sample_burrito_obs' },
  'en-pins': { [BIBLE]: 'sample_burrito', [OBS]: 'sample_burrito_obs' },
  shared: { [BIBLE]: 'sample_burrito', [OBS]: 'sample_burrito_obs' },
};

/** The requests a shot waits for: the API and pictures, never the event stream, which stays
 * open for the life of the page (APP-DESIGN §7.1). */
export function rustTracks(url) { const p = new URL(url).pathname; return (p.startsWith('/api/v1/') && p !== '/api/v1/events') || p.startsWith('/media/'); }

export async function startRustEnv({ scenario, tc4, ui, runDir }) → env
```

`startRustEnv`:

1. Makes `runDir/<scenario>/{appdata,projects}` (runDir defaults to
   `<repo>/target/parity-rust/<pid>-<Date.now()>`; never `/tmp`, never the person's own
   `~/translationCore4`). An absent `--projects` would make `tc4` use the person's real folder.
2. Spawns `tc4 serve --app-data <appdata> --projects <projects> --ui <ui> --door43
   http://127.0.0.1:9/ --test-hooks` with `stdio: ['pipe', 'pipe', 'inherit']`, keeps stdin open
   (a closed stdin is a quit), and reads the first stdout line: `{url, token}`, or `{error: …}`
   (throw it, naming the flag when the error is the unknown `--test-hooks`). The dead `--door43`
   address (port 9, refused) keeps Door43 on the machine even if the switch were on; B9b replaces
   it with the fake. Leave a marked hook `scenarioArgs(scenario)` that returns `[]` today, for
   B1b's `--backend memory --scenario <.state dir>`.
3. `call(endpoint, body)`: a POST as `crates/tc-ui/app/smoke.mjs` `Server.call` makes it (Host,
   Origin, `X-Tc-Token`, `X-Tc-Api: 1`, JSON), resolving the `ok` value and rejecting any other
   status or `err` with the endpoint and the reply. `newRequestId()` is 16 random bytes as 32
   lowercase hex (`RequestId`, `crates/tc-api/src/ids.rs:245`).
4. Checks the hooks once: `call('test/quiet', {})`; a 404 means "start tc4 with --test-hooks from
   a build with `--features test-hooks`".
5. Returns the env:

```js
{
  scenario, origin, token, projects: PROJECTS[scenario],
  async prepare(context) { /* abort every request whose origin is not `origin`, recording its URL in `refused` */ },
  contextOptions: { ...CONTEXT_OPTIONS, extraHTTPHeaders: { 'X-Tc-Token': token } },
  tracks: rustTracks,
  failureTestIds: ['view-error', 'boot-error'],
  catalog: (locale) => JSON.parse(fs.readFileSync(`<repo>/crates/tc-ui/assets/i18n/${locale}.json`, 'utf8')),
  async load(page, { locale }) {
    await call('test/reset', { request: newRequestId() });
    if (locale !== 'en') await call('prefs/setLocale', { request: newRequestId(), locale });
    if (page.url().startsWith(origin)) await page.evaluate(() => { localStorage.clear(); sessionStorage.clear(); });
    await page.goto('about:blank');
    await page.goto(`${origin}/#t=${token}`);
  },
  async busy(page) {
    if ((await call('test/quiet', {})).busy) return true;
    return page.evaluate(() => window.__tcIdle === false).catch(() => false);
  },
  projectId: (repo) => { /* PROJECTS[scenario][repo]; throws naming the repo when absent */ },
  pushToDoor43: notYet('pushToDoor43', 'B9b'), dropNextClone: notYet(…, 'E1a'),
  teammatesChangeOneVerse: notYet(…, 'B10'), loseFromDoor43: notYet(…, 'B10'),
  refused: () => [...refused],
  async build() { /* hello's build.commit and api, and the ui build folder name */ },
  async close() { /* stdin {"quit":true}\n, wait up to 10 s for exit, SIGKILL after; remove runDir/<scenario> */ },
}
```

`notYet(step, unit)` returns an async function that throws `${step} needs the fake Door43 in the
Rust environment, which unit ${unit} adds (APP-DESIGN §7.1)`. `busy` takes the page because the
capture passes `() => env.busy(page)` to `settle`.

#### 3.2 `capture-rust.mjs`

```
node parity/app/capture-rust.mjs --unit <A5|B2|…> (--only a,b,… | --all) [--layers pixels,dom,meaning]
     [--tc4 target/debug/tc4] [--ui target/tc-ui-app] [--out parity/app]
```

- Playwright comes from `PLAYWRIGHT_MODULE`, default
  `parity/tc4-baseline/node_modules/playwright`, through `createRequire` (as `smoke.mjs` does):
  `parity/app` has no `node_modules`.
- `--unit` is required and must be a §10 unit id; it is written as `by`. `--layers` defaults to
  all three; `dom` alone is a DOM-first capture.
- Per scenario of the selected screens: `startRustEnv`, one `browser.newContext(env.contextOptions)`,
  `env.prepare(context)`, one page, `trackRequests(page, env.tracks)`, page errors collected
  (`pageerror` and `console` errors, as `scenarios.mjs` collects them), `appDriver({ page, env })`;
  per screen `captureScreen({ page, app, screen, inFlight, file, busy: () => env.busy(page),
  failureTestIds: env.failureTestIds })`, then `measureTargets` for the masks of that screen
  (resolving `<OBS>`/`<BIBLE>` with `env.projectId`), then the entry of 4.7 with the sha256 of
  the PNG and of the DOM file (`{ screen: name, ...dom }`, written as `capture.mjs` writes tC4's).
- Writes into `--out`: `screens/<name>.png`, `dom/<name>.json`, `captures.json` (entries merged
  into the existing file by name, in manifest order, as `capture.mjs` merges a partial run;
  `machine` from `machineFingerprint`; refuse the merge when the machine differs and the run does
  not cover every screen already in the file).
- Exit code 1 when any screen failed or any request was refused. Print the next step:
  `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`.

At the end of A6b nobody runs it for a baseline screen (none is due); the Playwright tests below
drive the same functions.

#### 3.3 `behaviour.mjs`

`export async function recordSequence({ page, app, screen, steps, inFlight, busy })` → the
record of 4.9: runs `screen.run(app)`, `settle`, `tagNodes`, snapshot `start`; for each step
(port the step kinds of `parity/ds-gallery/scenarios.mjs:22-29`: `click`, `hover`, `mouse`,
`move`, `key`, `type`, `wait`) performs it inside `#root`, waits `wait ?? 350` ms then `settle`,
and snapshots `{ label, dom: await page.evaluate(domSnapshot, { ids: true }), focus, overflow }`
with `pageState` ported from `scenarios.mjs:154-160`. `export const SEQUENCES = []` (A5 adds).
An entry `node parity/app/behaviour.mjs --env tc4|rust --only <names>` writes the recordings of
4.9; with no sequences it prints that there are none and exits 0.

#### 3.4 `env-rust.test.mjs`

Two groups. The pure group always runs (CI):

- `rustTracks counts the API and pictures and never the event stream`: `/api/v1/app/hello` yes,
  `/api/v1/events?after=0` no, `/api/v1/events` no, `/media/obs/x/01-01.jpg` yes,
  `/ui/abcd/i18n/ar.json` no, `/` no.
- `every Door43 step names the unit that brings it`: each of the four rejects with its unit.

The browser group runs only when `TC4_BIN` and `TC4_UI` are set and Playwright resolves; otherwise
each test is `skip`ped with the reason. Each test starts its own `tc4` (`startRustEnv`) and closes
it:

- `E1 home loads the page with the token, and the shot settles while the event stream stays
  open`: `env.load(page, { locale: 'en' })`; `[data-testid="runtime-ready"]` shows; `<html
  lang="en" dir="ltr">`; `settle(page, inFlight, { busy })` returns within 5 s; and a second
  tracker made before the load, `trackRequests(page, (url) => new URL(url).pathname ===
  '/api/v1/events')`, still holds exactly one request in flight when settle returns (a resource
  timing entry would not do: Chromium adds it only when a response ends).
- `E2 counting the event stream would make every shot wait out the deadline`: the same page with
  `trackRequests(page, (url) => new URL(url).pathname.startsWith('/api/'))`, which counts
  `/api/v1/events`; `settle` with `deadlineMs: 2000` rejects naming `/api/v1/events`. This proves
  the exclusion is what lets a shot settle.
- `E3 home saves the screen's language and forgets everything else`: `load({ locale: 'ar' })`
  → `<html lang="ar" dir="rtl">`; then `page.evaluate(() => localStorage.setItem('x', '1'))`,
  `call('net/set', { request, on: true })`; `load({ locale: 'en' })` → `<html lang="en">`,
  `localStorage.getItem('x')` is null, `call('net/state', {})` answers `on: false`.
- `E4 a screen whose server stays busy reaches the settle deadline and fails`:
  `call('test/fieldLog', { field: 'busy' })` → `current.token`; `call('test/hold', { request, at:
  'save' })`; send `test/fieldStore { request, field: 'busy', shown: token, text: 'x' }` without
  awaiting it (it waits at the hold, so `test/quiet` answers busy; an armed hold alone does not
  count, `Holds::holding`, `crates/tc-app/src/hooks.rs:248`); then `captureScreen` with a screen
  whose `run` is `env.load` and `settle` options `quietMs: 200, deadlineMs: 3000` (add a
  `settleOptions` pass-through to `captureScreen` for this) → `error` matches
  `/did not go quiet .* the server answered busy \(test\/quiet\)/`, and the PNG was still written.
  Then `env.load` again: `test/reset` released the hold, the store answers, and settle succeeds.
- `E5 nothing leaves the machine`: after E1's load, `env.refused()` is empty; a page script
  `fetch('https://example.org/')` is refused and listed (it fails in the page; nothing is sent).
- `E6 the recorder keeps node identity across a language switch`: `recordSequence` on a screen
  whose `run` is `env.load` and steps `[{ click: '[data-testid="runtime-locale"]' }, { key:
  'ArrowDown' }, { key: 'Enter' }]`; the last step's lines keep the `#n` of the `<select>` and of
  `runtime-sample` (A4a's switch remounts nothing), `focus` names the select, and the sample's
  text changed language.

Run them: `. ~/.tc-env && TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test
parity/app/env-rust.test.mjs`. Record the run (and E1's settle time) in the evidence (step 6).

Commit: "Drive tc4 serve from the parity driver: token, reset, quiet, and the event stream left
out of the wait".

### Step 4. The masks, measured on tC4

Files: `parity/app/measure-masks.mjs`, `parity/app/masks.json`.

`measure-masks.mjs [--only <mask ids>] [--grow <px>]` imports `env-tc4.mjs`, `driver.mjs`,
`screens.mjs` and `observe.mjs`; for each mask and each of its screens it starts the tC4 env of
that screen's scenario (reuse one env per scenario), runs the screen's `run` as `capture.mjs`
does (same context options, same `trackRequests` and `settle`), calls `measureTargets` with the
mask's `target` (resolving `<OBS>` and `<BIBLE>` with `env.projectId`, which is the repo path on
tC4), and writes the rect into `masks.json` (`--grow` pads it on every side, clipped to the
viewport; default 0). A target that is not found is an error naming the mask and the screen.
Needs `TC4_ROOT` (§2 item 4); writes nothing under `parity/tc4-baseline`.

Write `masks.json` with the five entries of 4.5, run the script, and commit the measured file.
Check by eye that each rect covers its element in `parity/tc4-baseline/screens/<screen>.png` (crop
it; the evidence names the rects).

Commit: "Declare the masks of P7 with their rulings, measured on tC4".

### Step 5. The compare and the gate in `cargo test`

Files: `Cargo.toml` (workspace dependency `png = "0.18.1"`, and dev-profile overrides if M3 needs
them), `crates/tc-ui/Cargo.toml` (host dev-dependencies `png`, `tempfile`, both from the
workspace), `crates/tc-ui/tests/screen_record.rs`,
`crates/tc-ui/tests/screen_record/{design.rs,inputs.rs,pixels.rs,dom.rs,meaning.rs,behaviour.rs,
report.rs,gate.rs,planted.rs}`, `parity/app/{units.json,cases.json,scopes.json,residuals.json,
captures.json,report.json}`. APP-DESIGN §7.3 layers 1 and 2, §7.5, §7.6 lead-in (D1, D2, D4–D8).

`screen_record.rs` starts like `gallery_record.rs` and `tests/app.rs`:

```rust
//! The gate of P7 for the app's screens (APP-DESIGN §7.5): …
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod screen_record {
    pub mod behaviour;
    pub mod design;
    pub mod dom;
    pub mod gate;
    pub mod inputs;
    pub mod meaning;
    pub mod pixels;
    pub mod report;
    mod planted;
}
```

The crate-level `cfg` keeps the whole test out of the wasm32 clippy runs (`--all-targets`
compiles tests for wasm32 too); `png` and `tempfile` go under
`[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`, beside `proptest`.

#### 5.1 `design.rs`: §7.6 and §10 from docs/APP-DESIGN.md

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UnitId(String);            // ^[A-G][0-9]+[ab]?$
pub enum Backend { Memory, MemoryScripted, Store }   // serde: "memory", "memory+scripted", "store"
pub enum Layer { Pixels, Dom, Meaning }              // serde: "pixels", "dom", "meaning"
pub struct ScreenPlan {
    pub name: String, pub scenario: String, pub locale: String,
    pub pixels: UnitId, pub dom: UnitId, pub backend: Backend,
    pub recapture: Option<UnitId>, pub pending_until: Option<UnitId>,
}
pub struct Design { pub units: BTreeSet<UnitId>, pub screens: Vec<ScreenPlan> }
pub fn parse(doc: &str) -> Result<Design, String>;
impl ScreenPlan { pub fn unit_of(&self, layer: Layer) -> &UnitId }  // meaning comes with pixels
```

- §10: the text between the line `## 10. Work breakdown` and the next `## `; every line that
  starts with `| ` and whose first cell's first word matches the unit pattern is a unit (48 at
  planning: A1–A11 with A4a/A4b/A6a/A6b, G0, B1a–B11, C0–C8 with C1a/C1b/C7a/C7b, D1, D2,
  E1a, E1b, E2, F1, G1, G2). The header row's first cell is `Unit`.
- §7.6: between `### 7.6` and `### 7.7`; rows `| \`<name>\` | <scenario>[, ar] | <first> |
  <recapture> | <notes> |`. `<first>` is either `<U>, M`, `<U>, M+S`, `<U>, M + fake` (pixels =
  dom = U; `M+S` is `MemoryScripted`, the others `Memory`), or `<U1> DOM; <U2> pixels` (dom = U1,
  pixels = U2, `Memory`). `<recapture>` is `—` or a unit. `pending_until` is the recapture of a
  `MemoryScripted` row (D7; parse error if it has none) and, when the notes say `` `pending_until:
  X` ``, X must equal it. Every unit named must be in §10. Any row that does not parse is an error
  naming the line.

#### 5.2 `inputs.rs`: the committed files

```rust
pub struct Paths { pub baseline: PathBuf, pub app: PathBuf, pub doc: PathBuf, pub plan: PathBuf /* docs/PLAN.md */ }
impl Paths { pub fn committed() -> Self }   // from CARGO_MANIFEST_DIR/../../
pub struct Record {
    pub paths: Paths, pub manifest: Manifest, pub captures: Captures, pub masks: Masks,
    pub residuals: Residuals, pub units: Units, pub scopes: Scopes, pub cases: Cases,
    pub tc4_behaviour: BTreeMap<String, Recording>, pub rust_behaviour: BTreeMap<String, Recording>,
}
impl Record { pub fn load(paths: Paths) -> Result<Record, String> }
pub struct Machine { pub platform: String, pub chromium: String, pub fonts: Option<FontList> }
pub struct FontList { pub count: u32, pub sha256: String }
pub struct Rect { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }
```

The serde types mirror §4 one to one (`camelCase`). `Manifest` reads only what the gate uses:
`tc4.commit`, `machine` (`Option`), `viewport`, and per screen `name`, `scenario`, `locale`,
`viewport`, `sha256`, `dom`, `domSha256`, `homeCards`, `syncToast`, `rebuildDiffers.ruling`.
Behaviour folders may be absent (empty maps).

#### 5.3 `pixels.rs`

```rust
pub struct Image { pub width: u32, pub height: u32, pub rgba: Vec<u8> }
pub fn read(path: &Path) -> Result<Image, String>;       // png crate; 8-bit RGB or RGBA, else Err
pub struct Count { pub mismatched: u64, pub outside_masks: u64 }
pub fn count(a: &Image, b: &Image, masks: &[Rect]) -> Result<Count, String>;  // size differs: Err
pub fn compare_files(tc4: &Path, rust: &Path, masks: &[Rect]) -> Result<Count, String>; // equal bytes: 0, 0 without decoding
pub fn diff_png(a: &Image, b: &Image, masks: &[Rect]) -> Vec<u8>;  // red outside masks, yellow inside, the tC4 image faded to 10 % elsewhere
```

A size mismatch (a 620 px screen captured at 1280) is not a panic and not an error of the
capture: `compare_files` returns it as `Count { mismatched: tc4 width × height, outside_masks:
the same }`, and the row's `pixels` block carries `"sizeError": "rust 1280x800, tC4 620x800"`
(an `Option<String>`, omitted when `None`), so the row is `differs` and says why.

#### 5.4 `dom.rs`

```rust
pub struct Snapshot { pub html: String, pub outside: Vec<String>, pub root: Vec<String>, pub styles: BTreeMap<String, String> }
pub enum Target { TestId(String), Text(String), Attr { attr: String, value: String, which: Which } }
pub enum Which { First, Last }
pub struct Side<'a> { pub snapshot: &'a Snapshot, pub rename: &'a [(String, String)] }  // Rust: folder → repo path; tC4: empty
pub struct DomResult { pub same: bool, pub declared: Vec<String>, pub first_diffs: Vec<String> }
pub fn compare(tc4: Side, rust: Side, scope: Option<&Target>, masks: &[(String, Target, Target)]) -> Result<DomResult, String>;
pub fn normalize(side: Side, scope: Option<&Target>) -> Result<Vec<String>, String>; // for behaviour and tests
```

Per side, in this order (each step its own function, each with a unit test in `planted.rs`):

1. **Expand**: replace ` style=@<key>` by ` style="<text>"` from `styles` (a missing key is an
   error). Lines are `<pad><tag attrs…[ style=…][ props]>` or `<pad>  "<text>"`; indentation is
   two spaces per depth.
2. **Select**: with a scope, the matching element line (`Attr`: the line's attributes include
   `attr="value"` exactly; `First` or `Last` in document order) and every following line indented
   deeper, re-indented so the scope element has none; otherwise `html`, then every `root` line,
   then each `outside` line prefixed `outside: `. No match is an error ("scope … matches nothing in
   the Rust DOM").
3. **Rename projects** (Rust side only): in element lines, inside attribute values, a token equal
   to a folder that starts the value or follows `-` or `/`, and ends the value, becomes the repo
   path (longest folder first, so `sample_burrito_obs` is never read as `sample_burrito`). tC4's
   project ids are only in `data-testid` today (239 lines); `project-`, `share-`, `share-card-`,
   `share-badge-`, `share-state-`.
4. **Renumber generated ids** (D4): over the selected lines in order, each distinct
   `tc-(f|g|dlg|cfm|rg)(\d+)` becomes `tc-<prefix>#<k>`, k counting first appearances per prefix;
   a derived id (`tc-f3-list`) keeps its suffix.
5. **Asset paths** (D5).
6. **Mask**: for each mask, find its target on this side (`TestId`: the element line with
   `data-testid="<value>"` after renaming; `Text`: the element whose first text-run child starts
   with the text; the first match), and move it and its subtree out into that mask's part. A mask
   whose target is not found on a side is an error naming the mask, the side and the screen.

Then: `same` when the remaining lines are equal and every mask's parts are equal; `declared` lists
the masks whose parts differ while the rest is equal; `first_diffs` holds the first five
index-wise differences of the remaining lines, as `line N: tC4 <…> | rust <…>`, each side cut to
300 characters (the ds-gallery's `firstDiffs` shape, `dom-diff.mjs:20-27`).

#### 5.5 `meaning.rs`

```rust
pub struct MeaningResult { pub same: bool, pub declared: Vec<String>, pub diffs: Vec<String> }
pub fn compare(tc4: &BaselineScreen, rust: &RustScreen, rename: &[(String, String)], declared: &[(String, BTreeMap<String, (String, String)>)]) -> MeaningResult;
```

`homeCards`: Rust keys renamed folder → repo; a missing map is `{}`; for each key in either side,
equal actions are fine; a pair equal to a declared `(tC4, rust)` pair of a mask applying to this
screen is listed under `declared`; anything else is a diff `homeCards[<repo>]: tC4 upload, rust
share`. `syncToast`: `null` vs not `null` is a diff; otherwise field by field over `ok`, `code`,
`text`, `received`, `newestWins`, `verseConflicts`, `from`, `restored`, `missingPins`, `links`.
`missingVersions` is left out: `capture.mjs` adds it on tC4's side by tC4's own rule
(`missingVersionsOf`), and the Rust capture has no such rule; say so in a comment and in §7.3.

#### 5.6 `behaviour.rs`

```rust
pub struct Recording { pub sequence: String, pub screen: String, pub steps: Vec<Step> }
pub struct Step { pub label: String, pub dom: dom::Snapshot, pub focus: String, pub overflow: String }
pub struct SequenceResult { pub sequence: String, pub steps: usize, pub same: bool, pub first_diff: Option<String> }
pub fn compare(tc4: &Recording, rust: &Recording, rename: &[(String, String)]) -> SequenceResult;
```

Step by step: equal labels, equal step counts, equal normalized lines (node-id markers included:
a node React keeps and the port draws again shows `#new` on one side only), equal `focus` and
`overflow`. The first difference names the step and the line.

#### 5.7 `report.rs`

```rust
pub enum Status { Same, Declared, Differs, Unported, Failed }   // serde lowercase
pub enum RowKind { Screen, Case }
pub struct Row { /* the fields of 4.8, Option where absent; skip_serializing_if = "Option::is_none" */ }
pub struct Report { pub about: String, pub baseline: BaselineInfo, pub rust: Option<RustInfo>, pub totals: Totals, pub rows: Vec<Row> }
pub fn build(record: &Record, design: &Design, diffs_out: Option<&Path>) -> Result<Report, String>;
pub fn to_json(report: &Report) -> String;   // serde_json::to_string_pretty + "\n"
```

Per baseline screen (manifest order), then per case:

1. No capture entry: `Unported`, `missing` = every layer with `plan.unit_of(layer)`.
2. An entry with `error`, a non-empty `pageErrors` or a non-empty `refused`: `Failed`, with the
   first of them as `error`; no layer compared.
3. Otherwise compare the claimed layers: pixels (only when both machines are equal and neither
   font list is `null`; otherwise the row is `Failed` with error "pixels from another machine:
   tC4 <fonts> on <chromium>, Rust <…>; recapture one side"), with the masks of this screen
   (`rects[screen]`); DOM with `scopes[screen]` when the entry does not claim pixels; meaning with
   the declared pairs; and every behaviour pair whose `screen` is this row. `missing` = the layers
   not claimed. Status: `Differs` if any compared layer or sequence differs; else `Declared` if
   any lists a declared mask; else `Same`.
4. With `diffs_out`, write `diffs/<name>.png` for each row with `mismatched > 0` and remove any
   other file in `diffs/`.

`totals` count the statuses. `rust` is `None` when `captures.screens` is empty.

#### 5.8 `gate.rs`

```rust
pub enum Rule {
    PlanDisagreesWithBaseline, UnknownUnit, DuplicateUnit, MissingRow, ExtraRow, StaleReport,
    CaptureFailed, DiffersWithoutResidual, StaleResidual, HashMismatch, StrayFile,
    LayerOverdue, CapturedByUnlandedUnit, ScriptedOverdue, MaskRuling, MaskRect, MaskMoved,
    MaskTargetMissing, ScopeNotDomFirst, BehaviourUnpaired,
}
pub struct Failure { pub row: Option<String>, pub rule: Rule, pub message: String }
pub fn check(record: &Record, design: &Design, committed: &str, now: &Report) -> Vec<Failure>;
```

The rules (each message says what to run or edit):

1. `PlanDisagreesWithBaseline`: §7.6's screens equal the manifest's, with the same scenario and
   locale (verified at planning: they agree).
2. `UnknownUnit` / `DuplicateUnit`: every id in `units.json`, `--unit`/`by`, residual `until`,
   case `until` is a §10 unit; no duplicate in `landed`.
3. `MissingRow` / `ExtraRow`: one row per baseline screen and per case, no other.
4. `StaleReport`: `committed` (the bytes of `report.json`) equals `to_json(now)`. Message: "run
   `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record` and review the diff".
5. `CaptureFailed`: every `Failed` row, whatever `residuals.json` says.
6. `DiffersWithoutResidual` / `StaleResidual`: a `Differs` row needs `rows[name]` with a non-empty
   `why`; an entry for a row that is not `Differs` fails.
7. `HashMismatch`: each capture entry's `sha256`/`domSha256` equals the sha256 of its committed
   file; each manifest screen's `sha256` equals its PNG (the baseline test checks this too; the
   gate does not rely on Node running).
8. `StrayFile`: every file in `parity/app/screens`, `dom`, `behaviour` belongs to an entry or a
   recording; every file in `diffs/` to a row with `mismatched > 0`.
9. `LayerOverdue`: a missing layer whose unit is in `landed` (also a tC4 behaviour recording with
   no Rust recording once its screen's DOM unit has landed).
10. `CapturedByUnlandedUnit`: a capture entry whose `by` is not in `landed`.
11. `ScriptedOverdue`: a row captured on `memory+scripted` whose `pendingUntil` is in `landed`.
12. `MaskRuling`: every mask's `ruling` is `build` or a `Q<n>` that docs/PLAN.md's "Where the
    rebuild differs from tC4 4.0 on purpose" lists as `(Q<n>)`; every manifest screen with
    `rebuildDiffers` has a mask citing that ruling, and a `Q<n>` mask applies only to screens whose
    `rebuildDiffers.ruling` is `Q<n>`.
13. `MaskRect`: every mask has a non-empty rect for each of its screens, inside that screen's
    viewport.
14. `MaskMoved`: in a captured row that claims pixels, each mask's Rust rect (`maskRects`) lies
    inside its tC4 rect; message "the masked element is outside its rectangle: widen the mask in
    masks.json, citing its ruling".
15. `MaskTargetMissing` and the scope errors from `dom::compare` surface as failures of their row.
16. `ScopeNotDomFirst`: a `scopes.json` screen whose §7.6 dom unit equals its pixels unit.
17. `BehaviourUnpaired`: a Rust recording without a tC4 recording of the same name.

#### 5.9 The tests in `screen_record.rs`

The real record:

- `the_committed_record_passes_the_gate`: `Record::load(Paths::committed())`, `design::parse`,
  `report::build` (with `diffs_out` only under `UPDATE_REPORT=1`, which also writes
  `report.json` first, then checks against what it wrote, as `golden.rs` does); assert `check`
  returns nothing, printing every failure and the totals on failure.
- `section_7_6_names_every_baseline_screen_with_its_units`: §7.6 parses; its 62 names, scenarios
  and locales equal the manifest's; `account-menu` has dom A5 and pixels B2; `sync-ok` is
  `MemoryScripted` with pending E1a; `home` recaptures at C1a.

The planted tests (`planted.rs`). A helper builds a planted record: the real baseline, doc and
PLAN, and a temp `app` folder (tempfile) holding copies of the real `units.json`, `cases.json`,
`scopes.json`, `masks.json`, `residuals.json`, plus planted `captures.json`, `screens/`, `dom/`
and a `report.json` equal to `to_json(build(…))` unless a test edits it. "Identity" captures copy
tC4's PNG as the Rust PNG and rewrite tC4's DOM file as the Rust app would write it: repo paths →
folders (`PROJECTS` of `env-rust.mjs`), every `tc-dlg<n>` → `tc-dlg<n+4>` and `tc-f<n>` →
`tc-f<n+7>`, `/clients/uw-tc4/assets/` → `/ui/0123456789abcdef/images/`, `/clients/uw-tc4/help/`
→ `/help/` (and recompute nothing else: the style keys are sha256 of the style text, so equal
texts keep equal keys). `homeCards` and `syncToast` copy the manifest's, with repo keys mapped to
folders. `maskRects` copies the rects of the masks that apply to the screen from `masks.json` (an
element where tC4 had it is inside its mask). The `machine` is the manifest's. `by` is `A6b`,
`backend` is `memory`. Every planted test filters the failures to its planted rows (the other
rows stay `unported`, which passes). The behaviour test also plants the identity capture of
`about`, because sequences are compared on captured rows.

| Test | Plants | Asserts |
|---|---|---|
| `an_identical_capture_is_same_on_every_layer` | identity of `about` and `shared-home`, all three layers | both rows `same`; no failure for them. This is the proof that the normalizations hide what they name (ids, folders, asset paths) and nothing more. |
| `a_planted_pixel_fails_the_gate` | identity `about`, then one pixel at (5, 5) of the Rust PNG changed (decode, flip the red byte, re-encode with `png`) | row `differs`, `mismatched` 1, `outsideMasks` 1, a `diffs/about.png` written when `diffs_out` is given; failure `DiffersWithoutResidual` naming `about` |
| `a_planted_pixel_inside_its_mask_is_declared` | one pixel at the centre of `about-build`'s rect | row `declared`, `outsideMasks` 0, masks `["about-build"]`; no failure |
| `a_planted_dom_difference_fails_the_gate` | identity `about`, then one attribute changed on a line outside every mask (for example the first `data-tone="neutral"` → `"accent"`) | row `differs`; `firstDiffs[0]` names that line on both sides; failure `DiffersWithoutResidual` |
| `a_changed_test_id_is_not_hidden_by_the_renaming` | identity, then `data-testid="about-copyright"` → `"about-copy"` | `differs` (the renaming touches only project folders) |
| `a_planted_meaning_difference_fails_the_gate` | identity `shared-home`; Rust `homeCards[<BIBLE>]` `sync` → `share` | `meaning.same` false, a diff naming the Bible project; `differs`; failure |
| `the_q2_card_is_a_declared_difference` | identity `shared-home`; Rust OBS action `sync` and the OBS card's line `data-action="sync"` and `title="Sync with Door43"` | row `declared`; meaning `declared: ["q2-obs-card"]`; DOM `declared: ["q2-obs-card"]`; no failure |
| `a_screen_that_reached_the_settle_deadline_fails_even_with_a_residual` | an entry with `error: "the app did not go quiet for 800 ms within 30 s; in flight: nothing now; the server answered busy (test/quiet)"` and a residual for it | `failed`; `CaptureFailed`; also `StaleResidual` |
| `a_page_error_or_a_refused_request_fails_the_screen` | `pageErrors: ["RuntimeError: unreachable"]`; separately `refused: ["https://example.org/"]` | `failed` both |
| `a_residual_excuses_only_a_row_that_differs` | the planted-pixel row plus a residual | no failure; then the residual on the identity row → `StaleResidual` |
| `an_edited_report_fails` | `report.json` with `about`'s `mismatched` changed by hand | `StaleReport` |
| `pixels_from_another_machine_fail` | Rust `machine.fonts.sha256` changed | row `failed` with the machine message |
| `a_dom_only_capture_compares_the_dialog_alone` | identity `about` with `layers: ["dom"]`, then a Home line outside the dialog changed | `same`, `missing` has pixels and meaning B2; then the same change with `layers` all three → `differs` |
| `a_layer_whose_unit_has_landed_must_be_captured` | `units.json` plus `B2` | `LayerOverdue` for `home` (and every B2 screen), none for B3's |
| `a_scripted_row_fails_once_its_recapture_unit_has_landed` | `sync-ok` identity with `backend: "memory+scripted"`, then `landed` plus `E1a` | before: no `ScriptedOverdue`; after: `ScriptedOverdue` |
| `a_capture_by_a_unit_that_has_not_landed_fails` | `by: "B2"` without B2 in `landed` | `CapturedByUnlandedUnit` |
| `a_masked_element_outside_its_rectangle_fails` | identity `about` with `maskRects["about-build"]` 20 px right of the mask | `MaskMoved` |
| `every_screen_the_rebuild_differs_on_has_a_mask_citing_its_ruling` | the real `masks.json` with `q3-where` removed | `MaskRuling` for `open-door43`; and a Q2 mask listing `about` → `MaskRuling` |
| `a_planted_behaviour_difference_fails_the_gate` | a tC4 and a Rust recording for `about` (three steps built from `dom/about.json`), the Rust one with `.focused` on another line at step 2; then one with `#new` where tC4 has `#12` | row `differs` with `firstDiff` naming step 2; then the node-identity difference |
| `stray_and_mismatched_files_fail` | an extra `screens/x.png`; a capture whose `sha256` is not its file's | `StrayFile`; `HashMismatch` |
| `unknown_units_are_refused` | `landed` with `B12` | `UnknownUnit` |
| `normalization_unit_cases` | small hand-made snapshots | each of the six steps of 5.4 on its own: style expansion, both scope kinds and `Last`, folder renaming with the prefix trap (`sample_burrito` vs `sample_burrito_obs`), id renumbering with `tc-f12` and `tc-f1-list`, same-origin paths only, mask by test id and by text |

These prove the row: the gate runs in `cargo test` (`the_committed_record_passes_the_gate`), and
planted pixel, DOM and meaning differences fail it; a screen that reached the settle deadline
fails (the planted test here, E4 at the capture, and the `observe.test.mjs` tests at `settle`).

Write the initial files: `units.json` (4.2), `cases.json` (4.3), `scopes.json` (4.4),
`residuals.json` (4.6), `captures.json` (4.7, empty), then `UPDATE_REPORT=1 cargo test -p tc-ui
--test screen_record` to write `report.json` (62 `unported` rows), and review it.

Commit: "Compare the Rust app's captures with tC4's in cargo test, and gate the report".

### Step 6. Evidence, the harness README, the design's built note, the final checks

1. `parity/app/README.md` (new): how the two sides are captured (`capture.mjs`, `capture-rust.mjs
   --unit …`), how the report is made (`UPDATE_REPORT=1 …`) and gated, each JSON file and who
   edits it, the machine rule and what to do when it fails (recapture both sides on one machine),
   and the duty of every unit that captures: add its id to `units.json`, capture its screens,
   rewrite the report, explain every `differs` row in `residuals.json`.
2. `docs/evidence/parity-harness-<date>.md`: the machine (cite
   docs/evidence/linux-machine-2026-10-18.md, `uptime` at each timing); §0's table and your own
   recapture's digests; the masks' rects; M3, M4 and M5 below; the `env-rust.test.mjs` run.
3. APP-DESIGN: the decisions D1–D10 in the sections their table names, and a built note at the end
   of A6b's Scope cell in §10, in the style of A4a's and A9's: *(A6b: built. D1 … D10, each in one
   sentence with its reason; evidence: docs/evidence/parity-harness-<date>.md.)* Also correct
   §7.3's sentence "the comparison applies the normalizations to both sides" to say the renaming
   of project folders applies to the Rust side (tC4's ids are the reference).
4. Final checks, in this order, all with `. ~/.tc-env && ` and `-j 6`:
   `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -j 6 -- -D warnings`;
   `cargo test --workspace -j 6`; `cargo clippy -p tc-ui --target wasm32-unknown-unknown
   --all-targets -j 6 -- -D warnings`, again with `--features gallery` and with `--features app`
   (no `harness` feature exists yet); `cargo clippy -p tc-api --target wasm32-unknown-unknown
   --all-targets -j 6 -- -D warnings`; the parity job's `node --test` line (with
   `env-rust.test.mjs` added); and the browser group of `env-rust.test.mjs`.

Commit: "Record the parity harness's evidence and what A6b built".

---

## 6. Invariants and edge cases to keep

- **The tC4 side changes no image except by machine.** After step 1's changes, the tC4 capture
  gives §0's digests; the DOM files never change. `capture.mjs` without `busy` behaves as today.
- **A failure is never a residual.** `failed` rows (settle deadline, page error, view or boot
  error, refused request, machine mismatch) fail the gate whatever `residuals.json` holds.
- **Pixels compare only within one machine.** A `null` font list (a non-Linux run) is never equal
  to anything, so such a capture can carry DOM and meaning but not pixels.
- **Masks are narrow.** A mask excludes one element's subtree from the DOM and one rectangle from
  the pixels, on the screens it lists; the Rust element must stay inside the rectangle; each mask
  cites a ruling the PLAN lists, or `build`.
- **Normalizations are listed and tested** (§7.3): project folders, generated ids by first
  appearance per prefix, same-origin asset paths. Nothing else is rewritten; a test id, a text, a
  style or an attribute that differs is a difference.
- **The report is a function of the committed files.** No timestamps, no absolute paths, no
  machine-dependent ordering in `report.json`; directory listings are sorted.
- **Nothing reaches the network.** `tc4 serve` gets a dead `--door43`; the browser context aborts
  other origins; `cargo test` reads files only.
- **No person's data.** Every `tc4` run gets `--app-data` and `--projects` under `target/`.
- **One machine's captures in `captures.json`.** A partial capture from another machine is
  refused.

---

## 7. Measurements to take

- **M1. The baseline here.** Your recapture's PNG and DOM digests against §0's (step 2), and two
  consecutive runs byte-identical if the PNG digest differs from §0's.
- **M2. The harness changed no tC4 image.** The recapture with step 1's code equals the capture
  of the unmodified code (§0's digest, or your own control run made before step 1).
- **M3. Gate time.** `time cargo test -p tc-ui --test screen_record` after a build, debug
  profile, three runs, with `uptime`. Targets: `the_committed_record_passes_the_gate` under 2 s;
  the whole file under 15 s. If PNG decoding dominates (the planted pixel tests decode about four
  1280×800 images), add `[profile.dev.package.png]`, `[profile.dev.package.fdeflate]` and
  `[profile.dev.package.miniz_oxide]` with `opt-level = 3` beside the existing `sha2` override,
  with a comment giving the measured before and after.
- **M4. The pixel count equals pixelmatch.** With `parity/ds-gallery`'s pixelmatch 7.1.0
  (threshold 0, `includeAA: true`), count the committed cloud PNG against this PC's PNG for
  `home` (planner: 52), `feedback` (1,281) and `help` (78,263) (take the cloud PNGs from git:
  `git show 01befe2:parity/tc4-baseline/screens/home.png > …`), and the planted one-pixel pair;
  the Rust count must equal pixelmatch's on each. Record the four pairs of numbers.
- **M5. Settle on the Rust page.** In E1, the time from `env.load` returning to `settle`
  returning (expect about `quietMs` plus the boot), and that no request but the event stream was
  in flight at the end.

---

## 8. Traps found while planning

1. **The committed baseline does not reproduce on this PC** (§0). Do not compare a Rust capture
   with a PNG captured on another machine; step 2 and the fingerprint exist for this.
2. **tC4's checkout has no `node_modules` and no `public/help`,** and `DEFAULT_TC4` points at the
   cloud path. Use a clone with `npm ci` and `TC4_ROOT` (§2). The help pages come from GitHub at
   build time; a capture without them changes `help` and must not be committed.
3. **No Playwright anywhere in the repository here.** `npm ci` in `parity/tc4-baseline` (and in
   `parity/ds-gallery` for M4). `parity/app` has no `node_modules`: resolve Playwright with
   `createRequire` from `parity/tc4-baseline/node_modules`.
4. **A `goto` to the page's own URL with a fragment does not reload.** Go through `about:blank`.
5. **`test/reset` has no scenario or locale** (`crates/tc-api/src/test.rs:211`): it resets prefs
   to defaults (`crates/tc-app/src/prefs.rs:792`). Save a non-English language with
   `prefs/setLocale` after it; `hello` reads the language from memory at once
   (`crates/tc-app/src/handlers.rs:66`).
6. **An armed hold does not make `test/quiet` busy;** only an operation waiting at it does
   (`Holds::holding`). E4 must send a `test/fieldStore` (token from `test/fieldLog`) after
   `test/hold { at: "save" }`.
7. **The design's id pattern `tc-[fg]\d+` misses `tc-dlg`, `tc-cfm` and `tc-rg`** (D4); every
   dialog screen would differ on ids alone.
8. **`pending_until` is written for only two of the five scripted screens** (D7).
9. **Eight screens are DOM-first** and need scopes (D8); `open-door43-internet-consent` has two
   dialogs.
10. **The Q3 note has no test id in tC4**, and its text is what the rebuild changes, so its mask
    targets text on tC4 and needs a Rust target from B9b/B10 (4.5).
11. **The event stream's URL has a query** (`/api/v1/events?after=0`): match the pathname.
12. **The `refused` list must stay empty**: `extraHTTPHeaders` would add the token to any origin,
    so the context's abort route must run for every other origin (D10).
13. **wasm32 clippy compiles tests** (`--all-targets`): the crate-level `cfg` and the
    target-gated dev-dependencies keep `png` and `tempfile` off wasm32.
14. **Debug PNG decoding is slow**; decode only when bytes differ (D2), and measure (M3).
15. **CI checks out one commit**, so "landed" cannot come from git history (D6).
16. **`syncToast.missingVersions` exists only on tC4's side** (5.5).
17. **The ds-gallery's screens were captured on the cloud machine too.** Leave them: its gate
    reads committed reports and recomputes nothing, and both of its sides were captured there.
    Mention it in the evidence as a known machine dependency for whoever recaptures the gallery.
18. **The README of the baseline names cloud paths** (`/home/user/…`, `/opt/pw-browsers`); update
    only the lines step 2 names, and keep the screen tables untouched (`baseline.test.mjs` checks
    them cell by cell).
19. **`phase0` may move under you** (A4b). Merge it, rerun the tests, add landed units to
    `units.json`.
20. **The storage worktree and branch belong to another team.** Nothing here touches them.
</content>
</invoke>
