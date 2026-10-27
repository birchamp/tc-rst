---
id: LINK-DROP
title: A Translate save that sends an alignment link back to the word bank says so (tC4 #825, #820)
track: A
lane: app
milestone: M5
state: verifying
owner: tc4-issues session (Claude Code dc817a96)
branch: track-LINK-DROP
worktree: .worktrees/track-LINK-DROP
depends: []
needs_owner: []
proves: [P2]
no_loss: true
size: 1 d (basis: FIX-R step 2, the Align half of the same list)
merged: none
updated: 2026-10-27
next: Step 4: the three lenses, loss first.
blocked_by: none
---

# LINK-DROP: A Translate save that sends an alignment link back to the word bank says so

## Goal

When a verse or section save in Translate moves an aligned word's link back to the word bank, the
translator is told which words lost their link, in the words Align uses for the same event, and
nothing else about the save changes.

## Why it matters to a translator

Today the link vanishes with no sign (tC4 #825, reproduced on main 58e8e10b:
`docs/evidence/tc4-issues-2026-10-27/repro/825-tie-drop.test.mjs`). The translator loses an
alignment they made and does not know to redo it.

## Before you start

- tc-store already lists the links: `StoreReply::dropped_links` (`crates/tc-store/src/reply.rs`),
  filled by `reflow` in `crates/tc-store/src/commands.rs`.
- `git grep dropped_links crates/tc-app` finds no reader: tc-app throws the list away.
- Align's list is `AlignView::dropped` and `keys::align::DROPPED_LINKS`
  (`crates/tc-ui/src/views/align/session.rs`, test id `align-dropped`).

## Funnel card

- **Invariants**:
  - INV-NO-SILENT-LINK-DROP: every link a Translate save sends back to the bank is named to the
    translator by that save's reply and on the Translate screen.
- **Wrong states**:
  - A save reply with `dropped_links` and a screen that shows nothing.
- **Code**:
  - tc-api: the verse and section save replies carry the dropped links per verse.
  - tc-app: reads `StoreReply::dropped_links` on every Translate save path.
  - tc-ui: Translate shows the list with Align's words until the translator dismisses it or
    leaves the chapter.
- **Checks**:
  - tc-app test: the issue's case (`Dios y Dios y y Dios`, first Dios linked, saved as
    `y y Dios y`) returns the dropped link in the reply. Fails on main.
  - Journey: the same case in the browser shows the list on Translate.
  - Proof it can fail: both fail on main 003aa087.
- **Prose**: Align's `DROPPED_LINKS` string, no new words.
- **Human**: none.

## Steps

### Step 1: Gate test that fails on main
### Step 2: Carry the list through tc-api and tc-app
### Step 3: Show it in Translate
### Step 4: Three lenses, loss first

## Gates before merge

`scripts/gates.sh --touched` with main merged in; three `verify` lines (loss, races and
protocol, browser and parity).

## Stop and ask when

- The fix would need the store to keep the list across a restart (a storage change).

## References

- tC4 #825; `docs/evidence/tc4-issues-2026-10-27/open.md`.
- tC4 #820 (folded in by the orchestrator, 2026-10-27): the same cause. Branch
  `tc4-fixed-issues-2026-10-27` holds an ignored repro,
  `tc4_820_a_translate_save_that_sends_a_link_back_to_the_bank_tells_the_translator`
  (`crates/tc-app/tests/tc4_fixed_2026_10_27.rs`, listed in `scripts/ignored-tests.txt`). It reads
  Align on reopen, which this card does not change; once LINK-DROP lands, that test and its
  ignored-tests line are replaced by `a_translate_save_names_the_link_an_ambiguous_replacement_sends_back`.

## Log

- 2026-10-27 11:00 EDT | tc4-issues session (Claude Code dc817a96) | created | none | from the orchestrator: fix the 4.1.0 blockers the tC4 issue sweep found, this one first | docs/evidence/tc4-issues-2026-10-27/open.md
- 2026-10-27 11:00 EDT | tc4-issues session (Claude Code dc817a96) | claim | none | claimed at creation | none
- 2026-10-27 10:40 EDT | tc4-issues session (Claude Code dc817a96) | step 1 | none | gate tests a_translate_save_names_the_link_a_tie_sends_back_to_the_bank (#825) and a_translate_save_names_the_link_an_ambiguous_replacement_sends_back (#820) in tc-app tests/app/align_store.rs; on the old wiring the reply is `dropped: []` while the link is gone ("left: [] right: [\"Dios\"]") | cargo nextest run -p tc-app --test app a_translate_save_names
- 2026-10-27 10:40 EDT | tc4-issues session (Claude Code dc817a96) | step 2 | none | tc-api `Ack.dropped` (per verse: book, chapter, verse, Align's DroppedLinkView); tc-app's backend reply keeps tc-store's dropped_links and the verse and section acks carry them; both tests pass | cargo nextest run -p tc-app --test app a_translate_save_names
- 2026-10-27 10:40 EDT | tc4-issues session (Claude Code dc817a96) | step 3 | none | Translate shows the links under the chapter bar in Align's words with the verse (`translate-dropped`), kept until Dismiss; the page's saves collect them from every acked reply, also one the field no longer waits for. Journey a_translate_save_names_the_link_it_sends_back_to_the_bank passes (no notice exists on main) | parity/app/journeys/translate.test.mjs
