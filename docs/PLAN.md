# tC4 in Rust: frame and plan

This document frames the rebuild of translationCore 4 (tC4) in Rust. It states what
done means, how big the work is, how much rigor each part gets, and the order of work.
The decision log is `.audit/rebuild.tsv`. The research behind each claim is cited there.

## The goal

Rebuild tC4 in Rust so a translator gets the same outcome as tC4 4.0, plus three things
tC4 4.0 does not do.

tC4 4.0 includes Door43 team sync, "Open a project from Door43" and an Arabic interface
(tC4 decision D102). Team sync in tC4 4.0 is round 1 of its epic #24: one public repository,
the `main` branch only, Bible projects only, one "Sync with Door43" button, a result toast
and a review of verse-text conflicts.

The three things the rebuild adds:

1. Team work goes further. Several team members work on one project offline and exchange
   work later, while others keep working. Nothing anyone wrote is lost. Beyond tC4 4.0,
   the rebuild has:
   - exchange by USB drive or file;
   - saves that continue while a sync runs;
   - private repositories;
   - any default branch, not only `main`;
   - sync of Open Bible Stories (OBS) projects;
   - a review of every conflict kind, not only verse text;
   - carried files, such as audio, that travel with the project.
2. Translation helps can come from Door43 (DCS) or from Aquifer. tC4 plans Aquifer for
   4.1.0 (its epic #711).
3. Every project on disk, and every project that leaves the app, is a valid Scripture
   Burrito.

The guide is `docs/VISION.md` in the tC4 repository. tC4's decisions, its Burrito
specification and its internal structures inform the design but do not bind it. The
user-visible outcome binds it. The tC4 user interface and design system are kept.

### The reference version

tC4 4.0 has no tag yet. Until it has one, the reference is tC4 `main` at commit `d310bf3`
(2026-10-16). When the v4.0.0 tag exists on `release/4.0`, the reference moves to that tag.
`parity/tc4-baseline/README.md` records the commit that its screens show.
`docs/TC4-DELTA-2026-10-16.md` lists every tC4 change from `d494a9d` to `d310bf3`.

### Where the rebuild differs from tC4 4.0 on purpose

The project owner ruled on these points on 2026-10-16 (`docs/TC4-DELTA-2026-10-16.md` §5).

- OBS projects sync like Bible projects. The card reads "Sync with Door43", and the Share
  dialog shows the line about team access. tC4 4.0 syncs Bible projects only (Q2).
- A private repository opens and syncs through sign-in. The app fetches without
  credentials first. When Door43 answers 401 or 404 and nobody is signed in, the app
  shows the sign-in step and tries again with the token. tC4 4.0 opens public repositories
  only (Q3).
- A sync checks write access before it fetches. One API call reads `permissions.push`.
  Without write access the sync refuses, and nothing changes, as in tC4. Other refusals
  after the received work arrived keep that work and say so (Q8).
- The aligner shows the word bank and the words of each card in verse order everywhere:
  from the first view of a verse, after an edit, and after Clear. tC4 at `d310bf3` still
  opens a verse with no saved alignment in word-aligner order (Q15).
- A conflict between two check decisions is shown and can be resolved. tC4 keeps the
  newest decision and shows only a count (Q1, AGENTS.md).
- One damaged incoming file is salvaged. It does not stop the whole sync or open (Q4).
- Every other question in that §5 follows its recommendation.

The rebuild does not port tC4's sync mechanism: the scratch copy, the apply marker, the
ops log, `recoverySettled`, `unreadableSettings` and the sync generation. STORAGE §6 and §9
give the same outcomes, and STORAGE §9.9 keeps saves flowing while a sync runs. tC4 can
leave a torn `metadata.json` (tC4 RISKS #13). The rebuild cannot, because it writes every
file atomically (STORAGE L18).

## Definition of done

The rebuild is done when every statement below is true and each one is shown by a
check in CI that can fail.

- **P1. Journeys.** Every live tC4 journey has an automated end-to-end test that drives
  the Rust app through its UI in a browser engine and asserts the journey's
  user-visible end state and its on-disk end state. The live journeys are J1 to J8,
  J9a, J9c, J9d, J11 to J13, J16, J17, J18 and J20 to J26 (tC4 `docs/JOURNEYS.md`).
  Where tC4 asserts an internal file shape, the Rust test asserts the equivalent fact in
  the new format.
- **P2. Team work.** These journeys pass end to end:
  - J17: open a shared project on another computer, then continue offline. The project
    comes from Door43, through a Door43-compatible git server, or from an exchange file.
  - J18: keep one version of a conflicted verse. As in tC4, step 3 of J26 can prove it.
  - J26: sync one project with two teammates through Door43, against a Door43-compatible
    git server. Three users edit offline at the same time. The steps are tC4's. The
    rebuild adds one: A also resolves the conflict between two check decisions.
  - The rebuild's own journey: exchange by USB file in both directions, while both sides
    keep editing. It needs no server.
- **P3. No silent loss.** A property test generates random concurrent histories for
  three or more devices and merges them in random orders and groupings. It shows that
  every merge order gives the same state and that no operation disappears without
  being shown. Every concurrent change to the same verse, alignment or check decision
  either merges without loss or appears as a conflict the user can see. Each silent-loss
  case found in tC4's design is a named regression test.
- **P4. Crash safety.** A test kills the writer at every write point of save, sync,
  import and checkpoint. After each kill, reopening the project gives the state before
  the action, the state after it, or, for a sync or an import, a valid state between the
  two in which each received batch is whole or absent (STORAGE §6.4, rows Y2 and I2).
  Nothing is lost and nothing is corrupt. For a sync this is also tC4's G8: after a kill
  or a power loss at any point, the project opens, no own change is lost, no teammate's
  change is reverted or given to another person, and the next sync converges with
  Door43.
- **P5. Valid burritos.** Every project state written by any test is validated against
  the Scripture Burrito 1.0 schema plus the checks the schema misses: ingredient md5
  and size, path rules, and scope coverage. This includes every commit, export and USB
  file. There are zero failures.
- **P6. Helps sources.** The same helps screens work with DCS resources and with
  Aquifer resources, pinned and offline, using recorded real data as fixtures.
- **P7. Same look.** The key screens are screenshotted in the Rust app and compared with
  the same screens in tC4, with the interface in English (`en`) and in Arabic (`ar`). The
  tC4 screens are in `parity/tc4-baseline`, captured at the reference version. A human
  reviews the differences, and the differences are listed.
- **P8. Scripts.** Arabic and Hebrew text renders right-to-left and edits correctly in
  the browser engine used by tests. The app's own interface is right-to-left in Arabic,
  as in tC4 4.0, not only the scripture text. Graphite rendering is checked by a probe that
  also measures vertical glyph extent. That probe runs in the Electronite shell on the
  release pipeline for Windows and macOS. Electronite's Linux build also runs in this
  container under Xvfb (G0, 2026-10-17), so the probe can run here too.
- **P9. Carry-over.** A tC4 4.0 project, a tC3 project, raw USFM and a Scripture Burrito
  each import into a valid project with their text unchanged byte for byte. The tC4 4.0
  cases include:
  - a project that has synced with a team: its name events, a local history that differs
    from Door43 `main`, and the folders a sync leaves behind, which the import skips;
  - a project whose sync apply stopped part way: the import refuses it and says to open
    it in tC4 once, so tC4 can finish its sync (Q23);
  - a torn `metadata.json`;
  - the stamps that the platform writes into `metadata.json`.

## Scope

tC4 is about 43,000 lines of product code, made up of these parts:

- 8,000 lines of views.
- 4,400 lines of design system.
- 7,000 lines of application state.
- 18,500 lines of data layer.
- 5,100 lines of journal reference code.

It also has 43,000 lines of unit tests and 14,000 lines of end-to-end tests. The Rust
rebuild is expected to be 50,000 to 70,000 lines across about 15 crates. That number is
an estimate, not a measurement.

These constraints come from this build environment:

- The network policy blocks `git.door43.org`, `api.aquifer.bible` and `api.github.com`.
  Door43 behavior is therefore tested against a local git server and a fake Gitea API
  built from the Gitea swagger file and tC4's recorded fixtures. Live Door43 calls are
  open until a run on a machine with access.
- Electronite's Linux build runs here under Xvfb, as root with `--no-sandbox` (G0,
  2026-10-17, which saw Awami Nastaliq joined by Graphite). The Graphite proof on Windows
  and macOS is open until it runs on the release pipeline.
- The bundled English suite lives in `bundle/en/`. The OBS image pack is too large for git,
  so the release pipeline downloads it at its pinned commit and checks it against
  `bundle/en/fetched.lock`. The installer carries it, so nobody who installs the app
  downloads it. Read `bundle/en/README.md`.

## Rigor

Rigor is high wherever a mistake loses a translator's work or locks in a format:

- the journal and the merge;
- storage and crash recovery;
- sync and USB exchange;
- import;
- the Burrito writer.

These parts get property tests, crash-injection tests and an adversarial review by
several models before they merge. The UI port gets end-to-end journeys and screenshot
comparison. Resource parsers get tests against real fixtures from tC4 and Aquifer.

## Architecture

- **Core.** Rust library crates own all data: USFM, versification, the Burrito,
  the journal and fold, the project store, resources, checking, alignment, OBS,
  import, export and sync. No other process writes the project.
- **Server.** A Rust HTTP server on `127.0.0.1` exposes the core to the UI, guarded by a
  per-launch token. It serves the UI files.
- **UI.** Written in Rust and compiled to WebAssembly. It reuses tC4's CSS, design
  tokens, fonts and translation catalogs, so screens look the same. API types are
  shared with the server in one crate, so the two cannot drift.
- **Shell.** In production the window is Electronite, because it is the only shell
  shown to shape Graphite fonts. In development and tests any Chromium-based browser
  works.

Details and the reasons for each choice are in `docs/ARCHITECTURE.md`.
