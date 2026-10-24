---
id: LIC-1
title: Move the rebuild to the MIT licence, as tC4 did
track: A
lane: integration
milestone: M1
state: done
owner: integration lane (Claude Code 3d07d5f8)
branch: track-LIC-1
worktree: .worktrees/track-LIC-1
depends: [SWEEP]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: a manifest line, two text files, ten catalog lines and one recapture)
merged: 819efb32
updated: 2026-10-24
next: none
blocked_by: none
---

# LIC-1: Move the rebuild to the MIT licence, as tC4 did

## Goal

The rebuild is MIT-licensed, as tC4 is since its PR #838. `Cargo.toml` says `MIT`, the
repository holds tC4's MIT `LICENSE` (copyright unfoldingWord) in place of `COPYING`, and the
NOTICE files, fixture notes and the tokenizer citation say MIT. The About screen and its licence
dialog are vendored byte for byte from tC4, so they move to MIT with REF1's re-vendor from tC4
`release/4.0` (REF1 step 2); until then About still says GPL, which is known and short-lived.

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
    About screen name the same licence (not in code: REF1's about check holds the About half).
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

### Step 2: The About screen and its dialog (moved to REF1)

- Moved to REF1 step 2 on 2026-10-24: the catalogs, `assets/about` and the dialog are vendored
  from one tC4 commit (`crates/tc-ui/vendored-from-tc4.txt`), so they change with REF1's
  re-vendor from tC4 `release/4.0` d25b7871, which has #838.

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
- 2026-10-24 14:21 EDT | integration lane (Claude Code 3d07d5f8) | claim | none | claimed on track-LIC-1, cut from track-SWEEP (the card and its rulings are there); SWEEP is not merged yet, so the merge waits for it (batch 2) | python3 -I -B .audit/tools/board.py
- 2026-10-24 14:24 EDT | integration lane (Claude Code 3d07d5f8) | note | none | scope change, decided here (technical): the About line, the licence dialog and the ten catalogs are vendored byte for byte from one tC4 commit (crates/tc-ui/vendored-from-tc4.txt, tests/vendored_assets.rs). Moving them needs a re-vendor from a tC4 commit with #838 (release/4.0 d25b7871 has it; rc.2 does not), which also adds keys and renames check.sort.byCategory: REF1's job. So step 2 moves to REF1, recommended to the orchestrator; LIC-1 keeps the repository's own licence. Also found: string-punctuation-tokenizer 2.2.0 is already MIT and equals 2.0.0 (tC4 D111), so owner action A5 is not needed | git -C ~/unfoldingword/tc4-sweep show 24188928:docs/DECISIONS.md (D111)
- 2026-10-24 14:24 EDT | integration lane (Claude Code 3d07d5f8) | step 1 | none | Cargo.toml (and the two spikes) say license = "MIT"; COPYING removed; LICENSE is tC4's MIT text at 24188928 (Copyright (c) 2017-2026 unfoldingWord, with the trademark notice); the two NOTICE files say MIT; the fixture READMEs name MIT for this repository and tC4's code; tc-usfm's words module cites tokenizer 2.2.0 (MIT); ROADMAP A5 marked not needed. Left for REF1: the ten catalogs, assets/about, account_menu.rs comments, about.json captures, about.test.mjs; help-pages.txt's Q-19 note follows vendor-help.sh | git grep -n 'GPL-2.0-or-later' -- ':!.audit' ':!Cargo.lock'
- 2026-10-24 17:43 EDT | integration lane (Claude Code 3d07d5f8) | verify | none | one pass by a fresh read-only reviewer agent: licence MIT in the workspace and both spikes, every crate inherits it; LICENSE equals tC4's at 24188928 byte for byte; COPYING gone and read by nothing; remaining GPL mentions are third-party, history or REF1's vendored About; the tokenizer citation matches tC4 D111 (2.2.0's MIT licence rests on tC4's evidence, 2.0.0 is all that is installed here). §8: code = one entry point (the workspace licence) and a deletion; checks: none for INV-ONE-LICENCE once step 2 moved; reach: About is reachable; siblings: two stale doc lines. One major: About keeps saying GPL after this merge and REF1's card did not carry the change; minor: TC4-ISSUES #838 stale | reviewer report in this session
- 2026-10-24 17:43 EDT | integration lane (Claude Code 3d07d5f8) | fix | none | the major: REF1 step 2 now vendors release/4.0 d25b7871 (with #838) and drops the GPL dialog block, and REF1's Checks carry the about check for INV-ONE-LICENCE; LIC-1's Goal and step 2 say the About half is REF1's. The minor: the #838 row says the rebuild is MIT since LIC-1 and About follows with REF1 | .audit/roadmap/units/REF1.md
- 2026-10-24 18:35 EDT | integration lane (Claude Code 3d07d5f8) | merge | 819efb32 | merged into main in batch 2b (INT-6 a4dc42af, FIX-J f1b865b9, G1 137164e8, LIC-1 819efb32, CHK-2 a9c3710f, CHK-1 64ad5158, I 22f77f0d, RULING-H5 8bd03b4b), gated by scripts/gates.sh --wait-ci batch-2b on the batch's last commit. | scripts/gates.sh --wait-ci batch-2b
