---
id: LIC-1
title: Move the rebuild to the MIT licence, as tC4 did
track: A
lane: integration
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: [SWEEP]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: a manifest line, two text files, ten catalog lines and one recapture)
merged: none
updated: 2026-10-23
next: Claim this card after SWEEP merges, then do step 1.
blocked_by: none
---

# LIC-1: Move the rebuild to the MIT licence, as tC4 did

## Goal

The rebuild is MIT-licensed, as tC4 is since its PR #838. `Cargo.toml` says `MIT`, the
repository holds an MIT `LICENSE` (copyright unfoldingWord) in place of `COPYING`, and the About
screen and its licence dialog say MIT in all ten languages, with the P7 captures and the about
journey updated.

## Why it matters to a translator

The About screen tells translators and their organisations what they may do with the app. It
must name the licence the app is really under, as tC4's does.

## Before you start

- `ls .audit/roadmap/rulings/ | grep -E 'licence-mit|tokenizer-relicense'` lists both rulings
  (`2026-10-23-licence-mit.md`, `2026-10-23-tokenizer-relicense.md`).
- `grep -n '^license' Cargo.toml` prints `license = "GPL-2.0-or-later"`.
- `grep -c 'about.licenseName' crates/tc-ui/assets/i18n/*.json` prints 1 for each of the ten
  catalogs.
- `grep -n 'GPL_TEXT\|license-gpl' crates/tc-ui/src/views/account_menu.rs` shows the dialog's
  GPL block.
- tC4's change to copy: `git -C ~/unfoldingword/tc4-sweep show 24188928 --stat` (#838, D111:
  About shows "License: MIT"; the dialog shows only `LICENSE`).

## Funnel card

- **Invariants**:
  - INV-ONE-LICENCE: `Cargo.toml`, the repository's `LICENSE`, the crates' NOTICE files and the
    About screen name the same licence.
- **Wrong states**:
  - The About screen says GPL while the code is MIT, or the other way round.
  - A NOTICE says "the rest of this crate is distributed under GPL-2.0-or-later".
- **Code**:
  - Types: none.
  - One entry point: `[workspace.package] license` in `Cargo.toml`; crates inherit it.
  - Removed: `COPYING`, `crates/tc-ui/assets/about/COPYING` and the dialog's GPL block.
  - Left to checks, and why: a licence is text in files; the about journey and the P7 captures
    hold what a translator sees.
- **Checks**:
  - `parity/app/journeys/about.test.mjs`; kind: journey; runs in: G5's app-e2e job.
  - `crates/tc-ui/tests/screen_record.rs` with the recaptured about and license screens; kind:
    parity gate; runs in: CI `test`.
  - Proof it can fail: before recapturing, `cargo test -p tc-ui --test screen_record` fails on
    the about screen.
- **Prose**: the NOTICE files' last line; any licence line in `docs/`.
- **Human**: none; the owner ruled (`rulings/2026-10-23-tokenizer-relicense.md`).

## Steps

### Step 1: Change the licence files

- Do: claim the card in its own worktree (`TRACKING.md`). Set `license = "MIT"` in
  `[workspace.package]`. `git rm COPYING`; add `LICENSE` with the MIT text, "Copyright (c)
  unfoldingWord". In `crates/tc-door43/NOTICE` and `crates/tc-align/NOTICE`, change the last
  line to say the rest of the crate is distributed under the MIT licence with this repository.
  `git grep -n 'GPL-2.0-or-later\|GNU GPL'` outside `.audit/` and `Cargo.lock`, and update every
  hit that describes the rebuild's own licence.
- Files: `Cargo.toml`, `COPYING`, `LICENSE`, the two NOTICE files, `docs/**`.
- Done when: `grep -n '^license' Cargo.toml` prints `license = "MIT"`; the grep prints only
  history and third-party lines.
- Record: a log line `step 1`.

### Step 2: The About screen and its dialog

- Do: set `about.licenseName` to tC4's wording in all ten catalogs (tC4's `src/i18n/*.json`
  at 24188928: `git -C ~/unfoldingword/tc4-sweep show 24188928:src/i18n/<lang>.json`). Replace
  `crates/tc-ui/assets/about/LICENSE` with the new `LICENSE`, delete
  `crates/tc-ui/assets/about/COPYING`, `GPL_TEXT` and the `license-gpl` block, as tC4's dialog
  shows only `LICENSE`. Update `about.test.mjs` and recapture the about and license screens
  (`parity/app/capture-rust.mjs`, as `parity/app/README` says; never set `UPDATE_REPORT` in a
  gate). Record the change as declared or matching in the P7 report, as tC4 #838 (D111).
- Files: `crates/tc-ui/assets/i18n/*.json`, `crates/tc-ui/assets/about/*`,
  `crates/tc-ui/src/views/account_menu.rs`, `parity/app/journeys/about.test.mjs`, `parity/app/`
  captures.
- Done when: `cargo test -p tc-ui --test screen_record` passes; `about.test.mjs` passes in the
  journeys; `grep -rn 'GPL' crates/tc-ui/assets crates/tc-ui/src` prints nothing.
- Record: a log line `step 2`.

### Step 3: Verify and merge

- Do: one verify pass (`FUNNEL.md` §8), then merge as `TRACKING.md` says, titled
  `Merge LIC-1: the app says MIT, as tC4 does`, trailer `Unit: LIC-1`.
- Done when: the gates pass on the merge commit and `board.py --check` shows no error for LIC-1.
- Record: log lines `verify` and `merge`.

## Gates before merge

`scripts/gates.sh --journeys` and `scripts/gates.sh --check-receipt` on the exact commit main
moves to.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A file outside the tokenizer port turns out to come from a GPL-only source.

## References

- `rulings/2026-10-23-licence-mit.md`, `rulings/2026-10-23-tokenizer-relicense.md`.
- `docs/TC4-DELTA-2026-10-23.md` C70 (#838, D111).

## Log

- 2026-10-23 | integration lane (Claude Code 3d07d5f8) | created | none | from the owner's licence answers of 2026-10-23 | rulings/2026-10-23-tokenizer-relicense.md
