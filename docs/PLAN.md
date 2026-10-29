# Plan: the promises every change keeps

**Answers:** what is always true of the app (the promises P1 to P10), and how a new feature
enters the documents. **Wins over:** the design documents on what must hold. **Does not
cover:** why (`docs/VISION.md`), what a person does (`docs/JOURNEYS.md`), where the app differs
from tC4 (`docs/DIFFERENCES.md`), or what is being built now (GitHub issues and milestones).

The Rust rebuild of tC4 4.0 is done (ruling `docs/rulings/2026-10-29-tc4-is-history.md`). It
gives a translator tC4 4.0's outcome plus three things tC4 4.0 does not do: team work that goes
further, helps from Door43 or Aquifer, and a valid Scripture Burrito for every project
(`docs/VISION.md` "What the rebuild adds"). From here the app grows from `docs/VISION.md` and
tC4's design system. tC4 4.0 is history: it no longer decides what the app does.

## The promises

Each statement below is true on `main`, and a check in CI that can fail shows it.
`docs/P-LEDGER.md` names the CI job and the tests for each. A change that breaks one does not
land. A promise that is not proven yet says so, with its issue.

- **P1. Journeys.** Every journey in `docs/JOURNEYS.md` of kind tC4 or rebuild has an automated
  end-to-end test that drives the app through its UI in a browser engine and asserts the
  journey's user-visible end state and its on-disk end state. `coverage.json` maps each journey
  to its tests (INV-P1-COVERED); a journey whose test is not built yet is marked pending there.
- **P2. Team work.** These journeys pass end to end:
  - J17: open a shared project on another computer, then continue offline. The project
    comes from Door43, through a Door43-compatible git server, or from an exchange file.
  - J18: keep one version of a conflicted verse. As in tC4, step 3 of J26 can prove it.
  - J26: sync one project with two teammates through Door43, against a Door43-compatible
    git server. Three users edit offline at the same time. The steps are tC4's. The
    rebuild adds one: B and C also change one check decision in two ways; the newest wins
    with no question, and the older stays in History (ruling H5, J28).
  - J27: exchange by USB file in both directions, while both sides keep editing. It needs
    no server.
- **P3. No silent loss.** A property test generates random concurrent histories for
  three or more devices and merges them in random orders and groupings. It shows that
  every merge order gives the same state and that no operation disappears without
  being shown. Every concurrent change to the same verse, alignment or check decision
  merges without loss, appears as a conflict the user can see (verse and frame text), or
  keeps the older version in History where the newest wins (ruling H5). Each silent-loss
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
  Aquifer resources, pinned and offline, using recorded real data as fixtures (J30).
  **Not proven yet:** the work is epic #711.
- **P7. tC4's design system.** Every screen, new or old, is built from tC4's design system:
  its tokens, CSS, fonts and components (`crates/tc-ui/src/ds/`). The design system's CSS, fonts,
  images and interface catalogs are tC4's byte for byte, as `crates/tc-ui/vendored-from-tc4.txt`
  records. The comparison of the app's screens with tC4's was done for 4.1.0
  (`docs/rulings/2026-10-27-p7-review.md`) and is history.
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

- **P10. Old projects keep working.** Every format version, once frozen, leaves a set of
  sample projects written by that version, which nobody may change afterwards. CI opens every
  one with the current build and shows that every verse, alignment, check decision, comment,
  conflict and history entry is still there and unchanged, that the project still validates,
  and that it can still be edited, saved and exchanged. CI also has the last frozen version
  open projects the current build wrote: it edits them, opens them read-only with a clear
  message, or refuses a newer major before writing, and never calls them damaged. A test that
  pins the format's bytes fails when the format changes without a new version and new sample
  projects (owner request, 2026-10-18; STORAGE §12, storage unit V).

## How a new feature enters

1. **Vision.** Name the commitment it serves (`docs/VISION.md`, V1 to V8). A feature that
   weakens V1, V2, V5 or V6 is redesigned or dropped. A feature the vision does not cover goes
   to the owner first.
2. **Journey.** Add a journey to `docs/JOURNEYS.md` with the next free number, kind `rebuild`:
   its actor, steps, end state and "must not" lines. Map it in `coverage.json`, pending until
   its test exists.
3. **Promise.** Most features fall under P1 to P10. A new kind of guarantee becomes a new
   promise here, P11 and on, with a row in `docs/P-LEDGER.md`, decided with the owner.
4. **Design, checks, code.** Down the funnel (`docs/FUNNEL.md`), in the design document for
   its area. A new screen uses tC4's design system (P7). A change to a screen or behavior that
   came from tC4 adds an entry to `docs/DIFFERENCES.md`.
5. **Work.** The feature is a GitHub issue, or an epic with sub-issues. Its order and its
   release are in GitHub, not here.

## Where the old plan went

- The reference version and the list of differences from tC4 4.0: `docs/DIFFERENCES.md`.
- Which parts get the strongest tests: `docs/FUNNEL.md` §5, "Where the checks are strongest".
- The architecture: `docs/ARCHITECTURE.md`.
- The size estimate and the limits of the build environment of 2026-10: in git history, at
  commit 9249a60d. A path written `tracker:<path>` names a file of the retired work tracker,
  at the same commit. The decision log before 2026-10-21 is `docs/rebuild.tsv`.
