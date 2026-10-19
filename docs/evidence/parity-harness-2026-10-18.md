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
