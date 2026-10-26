---
id: HIST-1
title: History and Restore on the screens, and the Received sheet's replaced rows
track: A
lane: app
milestone: M3
state: done
owner: shell lane (Claude Code e1bb3a0c)
branch: track-HIST-1
worktree: .worktrees/track-HIST-1
depends: [C7b, UI-3b]
needs_owner: []
proves: [P2, P3]
no_loss: true
size: 2-3 sessions (basis: C7a's History view exists; this adds entry points and one list of rows)
merged: 144e595d
updated: 2026-10-26
next: none
blocked_by: none
---

# HIST-1: History and Restore on the screens, and the Received sheet's replaced rows

## Goal

Owner ruling H5 (`rulings/2026-10-24-H5.md`): outside verse text the newest version wins with no
question, and the older one stays in History, where *Restore this version* brings it back. C7b
built the rule, the record-keyed calls (`conflicts/history`, `conflicts/restore`) and the sync's
`replaced` rows. This card gives the translator the way back on the screens: History with
*Restore this version* on the Check screen (a check's decision and comment), in the aligner (a
verse's alignment) and for the project's settings and resource pins, and it draws the Received
sheet's rows ("Juan's alignment of Titus 1:1 replaced yours. Your version is in History.").

## Why it matters to a translator

A teammate's newer check or alignment arrives without a question. The translator sees what it
replaced and can put their own version back in two clicks.

## Before you start

- C7b is `done`: `grep -n '"conflicts/restore"' crates/tc-api/src/lib.rs` prints one line, and
  `grep -n 'pub struct ReplacedView' crates/tc-api/src/jobs.rs` prints one.
- UI-3b is `done` (it changes the check and align views first).
- Read C7a's History view in `crates/tc-ui/src/views/review.rs` (it is reused) and
  `crates/tc-app/tests/app/conflicts.rs`
  `a_record_outside_a_conflict_restores_an_older_version_from_history` (the calls in use).

## Funnel card

- **Invariants**:
  - INV-THE-LOSER-STAYS-IN-HISTORY (C7b): reached from a screen for every newest-wins kind.
  - INV-NO-SILENT-REPLACEMENT: every version a sync replaced with no question is named on the
    Received sheet (`SyncResultView.replaced`).
- **Code**: one History entry point component, given a record's token and `RecordRef`; the
  Check screen, the aligner and the settings and pins screen each place it. The Received sheet
  draws `replaced` rows; catalog keys for the words.
- **Checks**: the journey `conflict-kinds` (`newest_version_wins_and_history_restores_the_older` and the comment case; it extends C7b's
  `parity/app/journeys/conflict-kinds.test.mjs` with the restore half), and the same for an
  alignment and a setting; `the_received_sheet_names_each_replaced_version` (tc-ui).
- **Prose**: `docs/PLAN.md` declared differences already list History and the Received rows (C7b).
- **Human**: none; the words follow STORAGE §8.5's and H5's example.

## Steps

1. The History entry point on the Check screen, with its journey half. Done when C7b's
   conflict-kinds journey restores Teammate 1's decision and the decision file holds it.
2. The same in the aligner and for settings and pins.
3. The Received sheet's `replaced` rows, words in the catalogs; tests.

## Log

- 2026-10-25 | shell lane (Claude Code e1bb3a0c) | created | none | From C7b's parity lens blocker B1 and major M1; the orchestrator chose option (b): C7b gives the calls, this card the screens; it goes in REL-1's and DONE's depends; the G5 session takes it after UI-3b | .audit/roadmap/evidence/2026-10-24/C7b-lens-parity.md
- 2026-10-25 12:55 EDT | shell lane (Claude Code e1bb3a0c) | claimed | none | The orchestrator asked this lane to take HIST-1 now, cut from track-C7b, before C7b and UI-3b merge; UI-3b (track-UI-3b) also edits web/align.rs, web/check.rs, web/app_ctx.rs and views/check/detail.rs, so whichever lands second merges and adapts
- 2026-10-25 13:40 EDT | shell lane (Claude Code e1bb3a0c) | step 1 | none | New query conflicts/versions { at, shown }: the History of the record a screen shows, by its token (the token names the record, so no view needs a RecordRef); a check's version carries VersionValue::Check { selected, comment }. The Check screen draws a History toggle under the marks (views/check/history.rs, reusing review.rs's history_section); *Restore this version* goes through the decision write queue (Actions::restore_check, conflicts/restore) and reads the check again. conflict-kinds.test.mjs now restores Teammate 1's decision: the server, History, the comment field and TIT.json hold it; a_record_outside_a_conflict_restores_an_older_version_from_history checks conflicts/versions equals conflicts/history; a_checks_version_names_its_selection_and_comment (tc-ui) | node --test parity/app/journeys/conflict-kinds.test.mjs
- 2026-10-25 14:30 EDT | shell lane (Claude Code e1bb3a0c) | step 2 (aligner) | none | AlignerView.history: the alignment's token as last saved. The aligner draws History under the verse's reference (views/history.rs record_history, shared with the Check screen). The aligner holds the verse's links while open, so Restore (Actions::restore_alignment) saves the links typed, reads the verse's current token, writes conflicts/restore, closes the aligner and opens the verse again from the stored alignment. conflict-kinds.test.mjs restores Teammate 1's alignment of Titus 1:1: the shown version is the restored one, Teammate 2's stays in History; tc-app align tests pass. Settings and pins next | node --test parity/app/journeys/conflict-kinds.test.mjs
- 2026-10-25 15:10 EDT | shell lane (Claude Code e1bb3a0c) | step 3 | none | The sync toast draws SyncResultView.replaced: one sentence per row (tc-ui app/sync.rs replaced_text): the place, who replaced whose, and where the older version is (History for alignments, checks and comments; Settings for settings and resources; both files kept for a file; a removed book names who removed it). The person's own alignment of one verse uses tC4's syncResult.alignReplaced; 18 new rebuild keys, pending in the 9 other languages. the_received_sheet_names_each_replaced_version (tc-ui); conflict-kinds.test.mjs reads the toast's two rows | cargo test -p tc-ui --test app sync; node --test parity/app/journeys/conflict-kinds.test.mjs
- 2026-10-25 15:20 EDT | orchestrator (via shell lane (Claude Code e1bb3a0c)) | ruling | none | Settings and pins get no History list (option a): the Received row names the older value and points to Settings, and it stays readable later, not only in the toast; a declared difference
- 2026-10-25 15:40 EDT | loss lens (subagent of shell lane (Claude Code e1bb3a0c)) | verify | 04f40a53 | 0 blockers. M1 a click in the aligner during Restore saved over it; M2 a comment save could land over a check's Restore; M3 rows promised History for verse comments, which had none; M4 History rows did not say what differed (alignments drew nothing; a check's status, bookmark and no-selection were left out). Minors: silent aligner Restore failures; a failed check Restore said "Mark it again"; the aligner's History token lags one autosave; the file row said "replaced" beside "both kept"; settings rows named no older value | .audit/roadmap/evidence/2026-10-25/HIST-1-lens-loss.md
- 2026-10-25 16:30 EDT | shell lane (Claude Code e1bb3a0c) | fix | none | M1, M2: every Restore first passes the save gate (all fields and the aligner's links saved; a gate that fails restores nothing), and the aligner shows Loading until it opens again. M3: the comment box draws History with Restore (views/comment_box.rs); a comment row points at its verse, check or frame. M4: VersionValue::Check carries nothing_to_select, status and bookmarked; VersionValue::Alignment the pairs and marks. Minors: a failed Restore says "This version was not restored. Nothing was changed. Try again." on both screens; the file row says both versions are kept. Ruling (a): ReplacedView.was names a font, a direction or the differing pins (en_tn v90); door43/lastSync keeps each project's last sync result while the app runs, and the Home card's "Last sync: n versions replaced" shows its toast again, after a reload too. PLAN.md declares it. Tests: a_settings_row_names_the_older_value (tc-app), the_received_sheet_names_each_replaced_version and a_checks_version_names_its_selection_and_comment (tc-ui), conflict-kinds.test.mjs 2/2 (the comment journey new; the card's link after a reload) | node --test parity/app/journeys/conflict-kinds.test.mjs
- 2026-10-25 17:10 EDT | races lens (subagent of shell lane (Claude Code e1bb3a0c)) | verify | ca3ada86 | 0 blockers, 0 majors. Minors: an aligner Restore ignores a rail click made while it runs; a mark clicked while a check's Restore waits on the gate lands first; a lost reply after a saved Restore says it was not restored; the aligner's open History can be stale after an autosave or a sync; a doc comment on the wrong impl | .audit/roadmap/evidence/2026-10-25/HIST-1-lens-races.md
- 2026-10-25 17:10 EDT | parity lens (subagent of shell lane (Claude Code e1bb3a0c)) | verify | ca3ada86 | 0 blockers. M1: grouped rows ("Titus 1:1 and 4 more") hid which records were replaced, where tC4 lists each replaced alignment (#791), and PLAN.md said tC4 only counts. Minors: no aria-expanded on the toggle; identical Restore names; no dir on History text; labels in the scripture font; a check's History named only the verse; toast rows as spans after the notes; the Home link indented when it wrapped; "Settings can set it back." | .audit/roadmap/evidence/2026-10-25/HIST-1-lens-parity.md
- 2026-10-25 17:30 EDT | shell lane (Claude Code e1bb3a0c) | fix | none | Parity M1: one Received row per record (a direction's two records once); the rows are tC4's list right after "Changes from", scrolling past 30vh; PLAN.md's sentence on tC4 corrected. Minors fixed: aria-expanded, a Restore's aria-label names the version, dir=auto on History text, a check's History names its title, "Project settings can set it back.", the Home link does not wrap, the restore failure no longer says "Nothing was changed", the doc comment. The rest are in .audit/minors.tsv
- 2026-10-26 | shell lane (Claude Code e1bb3a0c) | verify | none | Main 1a0c8469 and C7b's tip merged in; the save gate's lint allows the three Restores. tc-api, tc-app and tc-ui tests, clippy (workspace and wasm32), and the journeys conflict-kinds 2/2, rtl-project, check-detail, check-writes, conflict-review, sync-button, align, story and translate pass | node --test parity/app/journeys/conflict-kinds.test.mjs
- 2026-10-26 11:30 EDT | integration lane (Claude Code 3d07d5f8) | merge | 144e595d | merged into main in batch 9 (C7b 39a6158c, HIST-1 144e595d, CHK-3 e36f49d2, and REL-1 waits for LOSS-1), gated by scripts/gates.sh --wait-ci batch-9 | scripts/gates.sh --wait-ci batch-9
