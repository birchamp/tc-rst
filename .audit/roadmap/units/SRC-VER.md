---
id: SRC-VER
title: Get source texts names the versions that are installed
track: A
lane: app
milestone: M5
state: fixing
owner: tc4-issues session (Claude Code dc817a96)
branch: track-SRC-VER
worktree: .worktrees/track-SRC-VER
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: FIX-R step 5)
merged: none
updated: 2026-10-27
next: Step 3: one browser-and-parity review lens, then scripts/gates.sh --touched with main merged in.
blocked_by: none
---

# SRC-VER: Get source texts names the versions that are installed

## Goal

The gateway package cards on Get source texts name the notes, words and academy versions installed on this computer (or none when not installed), never a fixed number.

## Why it matters to a translator

Today the English card says v89 while v91 is installed and pinned, so a translator thinks their helps are old. Reproduced on main 58e8e10b: `docs/evidence/tc4-issues-2026-10-27/repro/833-5-source-texts-versions.test.mjs`.

## Before you start

- `GATEWAYS` in `crates/tc-resources/src/gateways.rs` holds `("v89", "v89", "v89")` as `Seen`.
- Find where the card's line is built: `graft ask "Get source texts package card notes words academy version" --source`.

## Funnel card

- **Invariants**:
  - INV-SOURCE-CARD-NAMES-INSTALLED: a Get source texts package card names, for its notes, words
    and academy, the release installed on this computer (the project's own pin when it is this
    package's and installed, else the install a language set would pin), else tC4's registry
    release; never a release this computer lacks for one it has.
- **Code**: The card's line reads the installed versions; the fixed `Seen` numbers go if nothing else needs them.
- **Checks**: A test on the en-pins scenario: the English card names v91 for notes, words and academy.
  - Proof it can fail: the test fails on main 003aa087.
- **Prose**: no new words unless the step says so.
- **Human**: none.

## Steps

### Step 1: A gate test that fails on main
### Step 2: The fix
### Step 3: Review lens(es) the card needs, then `scripts/gates.sh --touched` with main merged in

## Gates before merge

`scripts/gates.sh --touched` with main merged in; the `verify` lines FUNNEL §8 asks for.

## Stop and ask when

- The fix would change what a translator sees beyond the issue (a new word, a new flow).

## References

- tC4 #833 item 5; `docs/evidence/tc4-issues-2026-10-27/open.md`.

## Log

- 2026-10-27 09:10 EDT | tc4-issues session (Claude Code dc817a96) | created | none | from the orchestrator: fix the 4.1.0 blockers the tC4 issue sweep found | docs/evidence/tc4-issues-2026-10-27/open.md
- 2026-10-27 10:05 EDT | tc4-issues session (Claude Code dc817a96) | claim | none | claimed for the tc4-issues session in .worktrees/track-SRC-VER | python3 -I .audit/tools/board.py
- 2026-10-27 11:40 EDT | tc4-issues session (Claude Code dc817a96) | step 1 | none | gate test `a_package_card_names_the_installed_releases_else_tc4s` (tc-app, en-pins) fails on main 003aa087: "assertion `left == right` failed: English: what is installed; left: (\"v89\", \"v89\", \"v89\"), right: (\"v91\", \"v91\", \"v91\")"; journey `a package card names the notes, words and academy releases installed here` added beside it (the repro's check, which failed on main 58e8e10b) | tc-heavy cargo test -p tc-app --test app -- sources::a_package_card
- 2026-10-27 11:45 EDT | tc4-issues session (Claude Code dc817a96) | step 2 | none | each card names the notes, words and academy release installed here (the install a set would pin, `gateways::installed_release`), else tC4's registry release, as tC4 shows a package not installed; declared in docs/PLAN.md (#833.5); source-texts and the five upgrade cases recaptured, residuals explain the English line (source-texts: pixels and DOM; the cases: DOM); tc-app sources 10/10, tc-resources all, tc-ui screen_record 49/49, journey source-texts 5/5 | parity/app/residuals.json
- 2026-10-27 12:30 EDT | tc4-issues session (Claude Code dc817a96) | verify | ddd398df | browser-and-parity lens by a read-only subagent (aa44137a): 1 Code: one function decides each value; the registry fallback stays, so no type can carry it; 2 Checks: tc-app test and journey exist, fail-before logged, deterministic, in CI; name and message lacked the invariant (minor, fixed); 3 Reach: English v91 reachable, and after an online helps update the card named the bundled v91 while the project pins v92 (major, view); 4 Siblings: `package()` in upgrades.rs carries the registry numbers but no screen shows them; 5 No loss: not a no-loss card; 6 Prose and Human: no new words, comments give reasons; parity captures, residuals and the PLAN.md entry accurate; journey source-texts 5/5 | the lens's report in this session; parity/app/diffs/source-texts.png
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | fix | none | the major: the current package's card names the project's own pin when it is installed, so an update's v92 shows, never a pinned release this computer lacks (`inv_source_card_names_installed_the_projects_release_first`, tc-resources); the gate test renamed `inv_source_card_names_installed_releases_else_tc4s`; the partly-installed case went to minors.tsv; tc-app sources 10/10 | tc-heavy cargo test -p tc-resources --lib inv_source
