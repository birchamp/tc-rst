# tC4 in Rust: frame and plan

This document frames the rebuild of translationCore 4 (tC4) in Rust. It states what
done means, how big the work is, how much rigor each part gets, and the order of work.
The decision log is `.audit/rebuild.tsv`. The research behind each claim is cited there.

## The goal

Rebuild tC4 in Rust so a translator gets the same outcome as tC4 4.0 today, plus three
things tC4 does not yet do:

1. Several team members work on one project offline and exchange work later, through
   Door43 or a USB drive, while others keep working. Nothing anyone wrote is lost.
2. Translation helps can come from Door43 (DCS) or from Aquifer.
3. Every project on disk, and every project that leaves the app, is a valid Scripture
   Burrito.

The guide is `docs/VISION.md` in the tC4 repository. tC4's decisions, its Burrito
specification and its internal structures inform the design but do not bind it. The
user-visible outcome binds it. The tC4 user interface and design system are kept.

## Definition of done

The rebuild is done when every statement below is true and each one is shown by a
check in CI that can fail.

- **P1. Journeys.** Every live tC4 journey has an automated end-to-end test that drives
  the Rust app through its UI in a browser engine and asserts the journey's
  user-visible end state and its on-disk end state. The live journeys are J1 to J8,
  J9a, J9c, J9d, J11 to J13, J16 and J20 to J25 (tC4 `docs/JOURNEYS.md`). Where tC4
  asserts an internal file shape, the Rust test asserts the equivalent fact in the new
  format.
- **P2. Team work.** New journeys pass end to end:
  - J17: receive a project, continue offline.
  - J18: keep one version of a conflicted verse.
  - Team sync through a Door43-compatible git server, with three users editing
    offline at the same time.
  - Exchange by USB file in both directions, while both sides keep editing.
- **P3. No silent loss.** A property test generates random concurrent histories for
  three or more devices and merges them in random orders and groupings. It shows that
  every merge order gives the same state and that no operation disappears without
  being shown. Every concurrent change to the same verse, alignment or check decision
  either merges without loss or appears as a conflict the user can see. Each silent-loss
  case found in tC4's design is a named regression test.
- **P4. Crash safety.** A test kills the writer at every write point of save, sync,
  import and checkpoint. After each kill, reopening the project gives either the state
  before the action or the state after it. Nothing is lost and nothing is corrupt.
- **P5. Valid burritos.** Every project state written by any test is validated against
  the Scripture Burrito 1.0 schema plus the checks the schema misses: ingredient md5
  and size, path rules, and scope coverage. This includes every commit, export and USB
  file. There are zero failures.
- **P6. Helps sources.** The same helps screens work with DCS resources and with
  Aquifer resources, pinned and offline, using recorded real data as fixtures.
- **P7. Same look.** The key screens are screenshotted in the Rust app and compared with
  the same screens in tC4. A human reviews the differences, and the differences are
  listed.
- **P8. Scripts.** Arabic and Hebrew text renders right-to-left and edits correctly in
  the browser engine used by tests. Graphite rendering is checked by a probe that also
  measures vertical glyph extent. That probe runs in the Electronite shell on the
  release pipeline, because this container cannot run Electronite.
- **P9. Carry-over.** A tC4 4.0 project, a tC3 project, raw USFM and a Scripture Burrito
  each import into a valid project with their text unchanged byte for byte.

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
- Electronite cannot run here, so the Graphite proof is open until it runs on the
  release pipeline.
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
