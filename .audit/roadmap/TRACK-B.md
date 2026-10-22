# Track B: the package for a second developer

This file is for two readers: the developer who takes the package, and the main team that
prepares it and merges it back. It is complete on purpose: the developer should not need this
conversation, only the repository, this file and the cards it names.

## 1. The choice of package

The evaluation compared five ways to split the remaining work (`evidence/2026-10-21/split-seams.md`):

| Package | Verdict | Why |
|---|---|---|
| **B: Shell and release** (G5, G1, G3, G4) | **Recommended** | Its interface is already a written, tested contract; it adds new files and touches almost no shared ones; the developer can verify all of it alone; it needs a Mac and Windows, which this PC does not have |
| B′: Import core (the pure part of unit I) | Second choice, for a Rust developer without a Mac | Clean library seam, real fixtures, can run beside sync; but high data risk, so the main team reviews it adversarially (§8) |
| Exchange (X, E1a, E1b, E2) | Keep in the main team | Needs tc-store internals that S2 and S3 are still rewriting |
| Export (D2) | Keep in the main team | Small, waits for S2, touches the busiest app files |
| Import plus its screens (I, F1) | Split into B′ | The screens touch the hot tc-app and tc-ui files |

The owner picks between B and B′ by answering H2 in `ROADMAP.md`.

## 2. What Package B delivers

| Card | What the developer builds | Proves |
|---|---|---|
| `units/G5.md` | A CI workflow that builds the app and runs every browser journey, and fails if any test is skipped | P1, and the P7 DOM layers on every push |
| `units/G1.md` | The Electronite shell: start and supervise `tc4`, the header filter, the typing mirror, one instance, the close guard, PDF, dialogs, external links, the feedback send | P4 (the shell's crash path), part of P8 |
| `units/G3.md` | A release pipeline that builds installers for Windows, macOS and Linux, fetches the OBS image pack, runs a smoke test after install, and signs when the owner allows | the "installs on the chosen platforms" part of done |
| `units/G4.md` | The P8 Graphite probe inside the shell on all three systems, and right-to-left project journeys (Arabic and Hebrew) | P8 |

Order: G5 first (it is small, it forces a full build of the app, and it is the most valuable
missing check), then G1, then G3 and G4.

## 3. Before the hand-off: what the main team does (card INT-7)

1. Land `resume-build-lane` on main (INT-4), so the developer starts from the newest app.
2. Make CI green (INT-2).
3. Write `docs/contracts/SHELL.md`, version 1: the frozen interface in §4 below.
4. Add `fixtures/shell/buffer-vectors.json` (input lines and the expected current text) and a
   Rust test in tc-ui that `tc_ui::app::buffer::current_lines` gives exactly that. The shell's
   node tests read the same file, so the two sides cannot drift.
5. Add `docs/contracts/shell-bridge.json` (the `window.tc4Shell` method names) and a host test
   that the `method("…")` calls in `crates/tc-ui/src/app/web/bridge.rs` equal it.
6. In `docs/APP-DESIGN.md` §10: G1 no longer depends on D2 (only its end-to-end PDF test waits
   for D2's `/print` route); add rows G3, G4 and G5. G1 is treated as a no-loss item (◆) from now
   on, because its typing mirror holds unsaved text.
7. Give the developer access: the owner adds them as a collaborator on `birchamp/tc-rst`
   (private), and confirms GitHub Actions minutes for Windows and macOS runners.
8. Claim nothing on Track B: the cards are left `todo` for the developer.

## 4. The frozen interface (SHELL.md version 1)

The developer codes against these. The main team does not change them while Track B is out;
a change needs a new version of SHELL.md that both sides approve.

| # | Item | Where it is defined and tested today |
|---|---|---|
| 1 | `tc4 serve --app-data <dir> --ui <dir> [--projects <dir>] [--door43 <url>] [--bundle <dir>]`. A release build has no test flags | `crates/tc4/src/args.rs`; `crates/tc4/tests/release.rs` (`release_has_no_test_hooks`) |
| 2 | The first stdout line: `{"url":"http://127.0.0.1:PORT/","token":"<43 base64url>"}`, or `{"error":{"type":"appLockHeld",...}}` with exit 3, or `{"error":{"type":"start",...}}` with exit 4. Usage error exit 2, server error exit 1 | `docs/APP-DESIGN.md` §5.1; `crates/tc4/tests/lifecycle.rs` |
| 3 | `{"quit":true}` on stdin answers `{"closed":true}`; closing stdin or SIGTERM also quits; a close takes at most 5 s | §5.1; `lifecycle.rs` (`quit_message_answers_closed_then_exits`, `sigterm_quits_like_the_quit_message`) |
| 4 | Headers `X-Tc-Token` and `X-Tc-Api`. The shell's filter adds the token only on paths outside `/api/`; the page sends it on `/api/` | `crates/tc-api/src/endpoint.rs`; `crates/tc-ui/app/boot.js` |
| 5 | `window.tc4Shell`: `printPdf`, `chooseFolder`, `chooseFile`, `openExternal`, `setCanClose`, `onCloseRequested` (5 s), `onDownloadDone`, `feedbackSend` → `{ok, reason?: offline | timeout | refused | not-configured}`, `bufferChanged`, `takeBuffered`, `platform`. The shell implements all eleven. The page calls six of them today (`bufferChanged`, `takeBuffered`, `setCanClose`, `onCloseRequested`, `openExternal`, `feedbackSend`) and only checks that `printPdf` exists; the main team wires the rest when D2, E2 and F1 need them | §5.7; `crates/tc-ui/src/app/web/bridge.rs` |
| 6 | `drafts-buffer.log`: one `BufferLine` per line (`tc_api::buffer`), fsynced at most once a second, off the main thread; reduced by `current_lines`; a torn last line is skipped | §5.7, §3.10; `fixtures/shell/buffer-vectors.json` (INT-7) |
| 7 | `TC4_BUILD_COMMIT` set for both the server build and the UI build; the UI is built by `crates/tc-ui/build-app.sh` with wasm-bindgen 0.2.129 | §3.10; the script's header |
| 8 | The English bundle: `cargo run -p tc-app --example bundle_store -- --from bundle/en --out <dir>`; the OBS image pack is fetched at its pinned commit and checked against `bundle/en/fetched.lock`; a build missing a pinned file fails | `bundle/en/README.md` |
| 9 | Electronite v37.1.0-graphite, pinned zips with sha256 | tC4 `docs/PACKAGING.md`; `.audit/tasks/g0-windows.md` |

## 5. The developer's setup

1. Clone `https://github.com/birchamp/tc-rst` and check out main.
2. Install the Rust toolchain that `rust-toolchain.toml` names (INT-2 adds it; until then
   Rust 1.99.1), with the `wasm32-unknown-unknown` target; `cargo install wasm-bindgen-cli
   --version 0.2.129`; Node 22.
3. `npm ci` in `parity/tc4-baseline` (Playwright 1.56.1), then
   `npx playwright install chromium`.
4. Build and run the journeys once, by hand:
   ```bash
   cargo build -p tc4 --features test-hooks,fixture-backend
   cargo build -p tc-door43-fake
   crates/tc-ui/build-app.sh
   crates/tc-ui/build-app.sh --test-hooks --out target/tc-ui-app-hooks
   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
     TC4_DOOR43_FAKE=target/debug/tc-door43-fake node --test parity/app/journeys/*.test.mjs
   ```
   A skipped test prints why. This run is the heart of G5.
5. Clone tC4 for reference only: `https://github.com/unfoldingWord/translationCore4`. Its
   `.github/workflows/package-desktop.yml`, `scripts/` and `docs/PACKAGING.md` are the model for
   G3. Never run its scripts against this repository.
6. Read, in order: `AGENTS.md`, this file, `FUNNEL.md`, `TRACKING.md`, `GLOSSARY.md`, then the
   card you start with. `docs/APP-DESIGN.md` §5 (the server, the shell bridge) and the G0 notes
   at the top of that file are the background for G1.

## 6. Rules for Track B

### What the developer may edit

- New files: `shell/` (the Electronite main, preload and pure library), `.github/workflows/`
  files other than `ci.yml` (for example `app-e2e.yml`, `release.yml`), `scripts/release/`,
  `docs/RELEASE.md`, `fixtures/shell/` (adding vectors), `parity/app/journeys/` (new right-to-left
  journeys for G4).
- Their own cards (`units/G1.md`, `G3.md`, `G4.md`, `G5.md`) and the minors list.

### What the developer must not edit

`crates/**`, `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml`, `docs/STORAGE.md`,
`docs/contracts/SHELL.md` (only by a new version both sides approve), `spikes/` (reference only),
other items' cards. When the work needs a change in those (a new flag in `tc4`, a new bridge
method), the developer writes a **request** in their card's log (`note` event, starting
"REQUEST:") and in the pull request; the main team makes the change in its own commit.

### What the main team must not change while Track B is out

The nine items of §4; the assertions of `crates/tc4/tests/lifecycle.rs` and `release.rs`; the
start-up protocol of `crates/tc-ui/app/prepare.html`; the `ui/<build>/` folder layout; the
`BufferLine` format; the `bundle_store` command line; the `fetched.lock` format. A new bridge
call is allowed only from the §4 item 5 list.

### The funnel for Track B

| Layer | What it means here |
|---|---|
| Code | The shell's lifecycle is one typed state machine: `Starting`, `Preparing`, `Running`, `Restarting`, `Closing`, `Quitting`, with one pure `next()` in `shell/lib.cjs` and an executor in `main.cjs`. One entry point per bridge method. The release pipeline is one script per platform, called by one workflow |
| Checks | `node --test shell/**/*.test.cjs` (pure decisions, contract vectors, bridge names); Electronite end-to-end tests under Xvfb and on Windows and macOS runners; the smoke test after install; G5's zero-skip journeys; the P8 probe with numeric limits |
| Prose | `docs/RELEASE.md`; comments only for reasons |
| Human | Signing identities, platforms, the help-desk key, runner minutes: questions H2, H3, H4, H15 in `ROADMAP.md`, asked through the owner |

## 7. Branches, pull requests and merging back

1. The developer works on branches named `ext/<card>-<short>` (for example `ext/g5-app-e2e`) in
   their own clone, and pushes them to `birchamp/tc-rst`. Never push main, `storage`, or any
   branch the developer did not create.
2. One pull request per deliverable (G5, G1, G3, G4), as a series of coherent commits, each of
   which builds and passes its own tests (AGENTS.md "Commits").
3. Merge `origin/main` into the branch at least once a week and before opening the pull request.
   Never rebase commits that are already pushed. Before asking for review,
   `git merge-tree --write-tree origin/main HEAD` must print no conflicts.
4. The pull request body links the card, lists the acceptance tests and their results, and lists
   every REQUEST the work needs from the main team.
5. **The main team integrates** (one person or session, the integration lane):
   1. Fetches the branch into a separate worktree and merges it into an up-to-date main there.
   2. Runs the full gate list (INT-3) plus the branch's own checks (`node --test shell/**`, the
      new workflows on the pull request), plus `.audit/tools/winvm/gates.ps1` for G1.
   3. Runs one verification pass with the lenses in `FUNNEL.md` §8 (three lenses for G1, which
      holds unsaved text). The developer fixes blockers and the majors that harm a translator's
      data or what they see; other findings go to `.audit/minors.tsv`.
   4. Merges with a merge commit `Merge G<n>: <what the translator gets>`, trailer `Unit: G<n>`,
      the card set to `done` with `merged` filled in, in the same commit.
   5. Pushes main under the push rule.
6. The first integration (G5) is the pilot: both sides note in G5's log anything in this
   protocol that did not work, and this section is updated.

## 8. The alternative: Package B′, the import core

Use this instead of B when the developer works in Rust and has no Mac (decision H2).

- **What it is.** A new crate `tc-import` that turns a tC4 4.0, tC3, USFM or Scripture Burrito
  project into an `ImportPlan` (packs, carried files, `project.json` fields, the report), checks
  it (fold it, project it, validate it, compare the scripture bytes), and does no I/O on project
  folders. It does not depend on `tc-store`.
- **Contract the main team freezes first** (in INT-7, a skeleton crate whose functions have their
  final signatures and return `Err(ImportRefusal::NotBuilt)` until B′ fills them in):
  `identify(&SourceTree) -> SourceKind`, `review(&SourceTree) -> ImportReview` (the dry run),
  `convert(&SourceTree, progress, cancel) -> Result<ImportPlan, ImportRefusal>`,
  `verify(&ImportPlan, &SourceTree) -> Result<(), Vec<Mismatch>>`, `trait TagResolver` for the
  tC3 pin lookup, and `pub mod tc4::convert_segments` for the team move.
- **The main team keeps**: `Store::create_imported` (the store write, after S3's core), F1 (the
  screens), the team move (§13.3, with X), and V.
- **Deliverables**: `docs/STORAGE.md` §13.1 T2 to T8, §13.2 (moved out of
  `tc-app/src/backend/seed*`), §13.4, §13.5; the four P9 tC4 4.0 cases; a table-driven test over
  `fixtures/tc4/conformance-fixtures/import/MANIFEST.json`; `memory_seed_equals_tc_import_seed`.
- **Risk**: imported history becomes permanent at the format freeze. The merge gate is the P9
  byte-for-byte tests, a determinism test, a model test, and a review by three lenses.
- **Card**: `units/I.md` steps I-1 to I-5 and I-7; tracking as in §7, branch `ext/i-import`.

## 9. Communication

- A 30-minute call each week, plus the cards' logs, which are the written record.
- Questions for the owner go through the main team, in short plain sentences, with options,
  their effect on translators, and a recommendation.
- A contract change: a pull request that bumps `docs/contracts/SHELL.md`'s version, approved by
  both sides; the vector and name tests make any unapproved drift fail CI.

## 10. When Track B ends

Each card is `done` and merged. `docs/RELEASE.md` says how to cut a release, who holds the
signing access, and how to rerun the smoke tests. The release pipeline's owner is written in
`.audit/HANDOFF.md`.
