---
id: LINK-DROP
title: A Translate save that sends an alignment link back to the word bank says so (tC4 #825, #820)
track: A
lane: app
milestone: M5
state: done
owner: tc4-issues session (Claude Code dc817a96)
branch: track-LINK-DROP
worktree: .worktrees/track-LINK-DROP
depends: []
needs_owner: []
proves: [P2]
no_loss: true
size: 1 d (basis: FIX-R step 2, the Align half of the same list)
merged: c2f2d00f
updated: 2026-10-27
next: none
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
  - tc-ui: Translate shows the list of the open project with Align's words, each with its
    verse, until the translator dismisses it; another project's list is not shown there.
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

- 2026-10-27 09:07 EDT | tc4-issues session (Claude Code dc817a96) | created | none | from the orchestrator: fix the 4.1.0 blockers the tC4 issue sweep found, this one first | docs/evidence/tc4-issues-2026-10-27/open.md
- 2026-10-27 09:07 EDT | tc4-issues session (Claude Code dc817a96) | claim | none | claimed at creation | none
- 2026-10-27 10:40 EDT | tc4-issues session (Claude Code dc817a96) | step 1 | none | gate tests a_translate_save_names_the_link_a_tie_sends_back_to_the_bank (#825) and a_translate_save_names_the_link_an_ambiguous_replacement_sends_back (#820) in tc-app tests/app/align_store.rs; on the old wiring the reply is `dropped: []` while the link is gone ("left: [] right: [\"Dios\"]") | cargo nextest run -p tc-app --test app a_translate_save_names
- 2026-10-27 10:40 EDT | tc4-issues session (Claude Code dc817a96) | step 2 | none | tc-api `Ack.dropped` (per verse: book, chapter, verse, Align's DroppedLinkView); tc-app's backend reply keeps tc-store's dropped_links and the verse and section acks carry them; both tests pass | cargo nextest run -p tc-app --test app a_translate_save_names
- 2026-10-27 10:40 EDT | tc4-issues session (Claude Code dc817a96) | step 3 | none | Translate shows the links under the chapter bar in Align's words with the verse (`translate-dropped`), kept until Dismiss; the page's saves collect them from every acked reply, also one the field no longer waits for. Journey a_translate_save_names_the_link_it_sends_back_to_the_bank passes (no notice exists on main) | parity/app/journeys/translate.test.mjs
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | verify | b3e39421 | loss lens (read-only subagent): 1 Code: the list is copied onto two acks by hand; no shared type forces it. 2 Checks: verse tests fail on revert; the section path had no test. 3 Reach: verse, section, crash before the reply, project switch. 4 Siblings: session/keepDraft also drops the list. 5 No loss: not met for keepDraft and for a crash between write and reply. 6 Prose: the card's "leaves the chapter" was untrue. Findings: F1 major keepDraft, F2 major crash gap, F3 minor notice crosses projects, F4 minor no section test | workflow wf_4e7c5d86-f9d
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | verify | b3e39421 | races-and-protocol lens (read-only subagent): blocker, tc-api golden files lack the new `dropped` key (11 endpoints); major, the page-wide list shows project A's links on project B's Translate; major, section_ack reverted passes the whole tc-app suite; minors: the golden Ack shows no DroppedLinks, the late-reply collection has no test. The reply cache keeps the Ack bytes, so a retry still names the links | workflow wf_4e7c5d86-f9d
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | verify | b3e39421 | browser-and-parity lens (read-only subagent): verse and section saves show the notice in Chromium; role=status; Tab reaches Dismiss and Enter dismisses; Arabic page stays rtl (align.droppedLinks was already untranslated, as before). tC4 d310bf3 says nothing after a save (state.jsx:1164-1176). Findings: major keepDraft (as F1); minors: PLAN and APP-DESIGN not updated, the card's "leaves the chapter" | workflow wf_4e7c5d86-f9d
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | fix | none | blocker: golden files regenerated, and the golden Ack now carries one DroppedLinks so the wire shape shows in review | crates/tc-api/tests/golden/endpoints/editor/saveVerse.json
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | fix | none | major (project crossing): each collected link keeps the open it was saved in; Translate lists only the open's own and Dismiss clears only those. The journey passes | crates/tc-ui/src/views/translate.rs dropped_links
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | fix | none | major (section path): a_section_save_names_the_link_it_sends_back_to_the_bank; with section_ack's list emptied it fails ("left: []") | crates/tc-app/tests/app/align_store.rs
- 2026-10-27 12:45 EDT | tc4-issues session (Claude Code dc817a96) | note | none | keepDraft (F1) is not reachable: no screen calls session/keepDraft (`grep -rn keepDraft crates/tc-ui shell` finds none), so it is a minor; the crash gap (F2) is the owner's declared limit for the preview (orchestrator, 2026-10-27, under rulings/2026-10-22-acceptable-loss.md), declared in docs/PLAN.md and docs/APP-DESIGN.md, with card LINK-MARK after 4.1.0 | .audit/roadmap/units/LINK-MARK.md
- 2026-10-27 14:10 EDT | tc4-issues session (Claude Code dc817a96) | gate | 426acf60 | scripts/gates.sh --touched with main 69c7dc39 merged: journeys 102/102, no skips; Rust 2009/2011; failed tc-api check's the_psalms_forms_that_a_numeric_reference_refused_read_back_as_written (its wire JSON lacked `dropped`; fixed, tc-api check passes) and tc-ui screen_record captures_match_the_current_ui_source (every capture records the tc-ui source tree; recapture on the batch tree with POEM-TAB) | /tmp/claude-1000/-home-bot-tc-rst/dc817a96-3f26-478a-9cb7-3304a28410ab/scratchpad/ld-touched.log
- 2026-10-27 11:49 EDT | integration lane (Claude Code 3d07d5f8) | merge | c2f2d00f | merged into main in batch 45 with LINK-DROP and POEM-TAB together, every screen recaptured on the batch tree; gated by scripts/gates.sh --wait-ci batch-45 | scripts/gates.sh --wait-ci batch-45
