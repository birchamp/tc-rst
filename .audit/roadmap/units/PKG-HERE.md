---
id: PKG-HERE
title: Download package says when the package was already on this computer
track: A
lane: app
milestone: M5
state: done
owner: tester findings lane (Claude Code 76125a0f)
branch: track-REL1-tester
worktree: .worktrees/track-REL1-tester
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: one job outcome and one string)
merged: a3a5bc83
updated: 2026-10-27
next: none
blocked_by: none
---

# PKG-HERE: Download package says when the package was already on this computer

## Goal

A 4.1.0-preview tester read "✓ Titus · English is ready" as a download made with the internet
switch Off. It was not: the switch held (Download asks while Off; one "Turn on internet" keeps
it On for the session, tC4 D95), and English comes with the app, so nothing was fetched. The
owner chose (2026-10-27) to say "✓ {book} · {lang} is already on this computer" when every
repository was already here, and "is ready" when one was fetched.

## Why it matters to a translator

A translator who keeps the internet off on purpose must be able to trust that the app did not
go online.

## Funnel card

- **Invariants**:
  - INV-DOWNLOAD-SAYS-FETCHED: the finished line says "is ready" only when the job installed a
    repository.
- **Code**: `tc_api::resources::Downloaded { Fetched, AlreadyHere }` as the `Download` job's
  outcome (`JobOutcome::Downloaded`); tc-app's download job returns it from `fetched_any`;
  tc-ui's `Dl::Done` carries it to the footer.
- **Checks**: journeys `a package already on this computer says so and fetches nothing` (no
  request outside `/api/`, the new line) and J3 asserts "is ready" after a real fetch, in
  `parity/app/journeys/source-texts.test.mjs`; tc-ui `tests/app/jobs.rs` types the outcome.
  On main 020e0ddf the first journey fails: the line is "is ready".
- **Prose**: `sources.alreadyHere` in `crates/tc-ui/assets/i18n-rebuild/en.json`, pending in
  every other language; declared difference `download-already-here` in
  `parity/app/residuals.json`.
- **Human**: the owner chose the words.

## Steps

### Step 1: The outcome, the string and the journeys
### Step 2: One review; `scripts/gates.sh --touched` with main merged in

## Gates before merge

`scripts/gates.sh --touched` with main merged in; one `verify` line.

## References

- tC4 `src/state.jsx` `downloadPackage`; the audit session's relay of the owner's choice, 2026-10-27.

## Log

- 2026-10-27 12:55 EDT | tester findings lane (Claude Code 76125a0f) | created | none | Not reproduced as a network bug: Download asks while the switch is Off and fetched nothing; the owner chose new words for a package already here | parity/app/journeys/source-texts.test.mjs
- 2026-10-27 12:55 EDT | tester findings lane (Claude Code 76125a0f) | claim | none | Claimed with the change on track-REL1-tester | none
- 2026-10-27 13:17 EDT | tester findings lane (Claude Code 76125a0f) | verify | 8e92de5b | One review (general-purpose agent, FUNNEL §8): blocker. Eight tc-app tests still expected JobOutcome::Finished for Download. Logic passes: Ok only with no cancel, offline, adopt or partial failure; a present repo at another version is fetched; FixFetch unchanged; tc-api 151/151 incl. catalog checks. Minor: the key was out of order in the catalogs | review report relayed in the session
- 2026-10-27 13:17 EDT | tester findings lane (Claude Code 76125a0f) | fix | none | The tests assert Downloaded::Fetched and, on the second download, AlreadyHere (sources.rs); the upgrades helper accepts either, since its repos may be here; the key sits in order | crates/tc-app/tests/app/sources.rs a_download_installs_the_latest_release_and_counts_the_package
- 2026-10-27 14:05 EDT | tester findings lane (Claude Code 76125a0f) | gate | eb1c7aa6 | passed scripts/gates.sh --touched with main 829f63f4 merged: 2019 Rust tests, 31 journeys, board, claims and coverage | scripts/gates.sh --touched
- 2026-10-27 14:06 EDT | integration lane (Claude Code 3d07d5f8) | merge | a3a5bc83 | merged into main in batch 47 with the other tester findings, every screen recaptured on the batch tree; gated by scripts/gates.sh --wait-ci batch-47 | scripts/gates.sh --wait-ci batch-47
