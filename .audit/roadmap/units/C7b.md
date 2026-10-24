---
id: C7b
title: Newest wins outside verse text (H5), and the conflict screens that stay
track: A
lane: app
milestone: M3
state: building
owner: shell lane (Claude Code e1bb3a0c)
branch: track-C7b
worktree: .worktrees/track-C7b
depends: [C7a, S2]
needs_owner: []
proves: [P2, P3, P7]
no_loss: true
size: 3-5 sessions (basis: the design in the 2026-10-24 log; the fold already orders heads, so the rule is small, the tests and goldens are most of it)
merged: none
updated: 2026-10-24
next: Step 1, newest wins on records in the fold (tc-journal), then the storage lane reviews it.
blocked_by: none
---

# C7b: Conflict choices: every other kind

## Goal

Owner ruling H5 (`rulings/2026-10-24-H5.md`): only verse text and story frame text conflict. For
check decisions, moved checks, alignments, comments, settings, resource pins, book presence and
path-case clashes, the newest version wins with no question asked, and the older one stays in
History, where *Restore this version* brings it back. A carried file's newest version keeps the
name, and the other is kept beside it as `<stem>-<person>.<ext>` (`jon_01-Juan.wav`). A removal
that is the newest removes the book, and its edits stay in History; the last book never goes.
The Received sheet says what the newest version replaced. This card changes the fold and the
store to that rule, removes the conflict tabs and choice cards that no longer apply, and keeps
History with Restore, Detached, Notices, the Received sheet, the read-only banner, the draft
offer, the boot error and the first-launch failure.

## Why it matters to a translator

A teammate's newer check, alignment or setting arrives without a question to answer, the
translator's own older version is one click away in History, and the Received sheet says what
changed. Only a verse's words, where both versions may matter, still ask.

## Before you start

- `rulings/2026-10-24-H5.md` exists (on main, or merged from `track-RULING-H5`).
- The design: the shell lane's 2026-10-24 log line below and
  `side-session-notes/.../C7b-newest-wins-design.md`. "Newest" is `ChangeMeta::order() = (rank,
  at, id)` (`crates/tc-journal/src/fold/meta.rs:301`), the order the fold already uses for the
  provisional head, compared only between concurrent heads.

## Funnel card

- **Invariants**:
  - INV-NEWEST-WINS-CONVERGES: for every key of a newest-wins kind, every computer settles on the
    same head after any union order, and it is the head with the largest `(rank, at, id)`.
  - INV-THE-LOSER-STAYS-IN-HISTORY: every head that lost is listed in History and can be restored.
  - INV-ONLY-TEXT-CONFLICTS: the conflicts the fold lists are only the text kinds (verse, passage,
    structure, frame, book header, chapter front, story base and reference, the book restore
    race) and `Unknown`.
  - INV-A-CARRIED-FILE-KEEPS-BOTH: every present version of a carried file has one placement, at
    distinct paths that no other file holds.
- **Wrong states**: a loser no screen can reach (a moved check's old key); a winner that differs
  between computers; a beside file written over another file; a project left with no book; a
  Received sheet that says nothing about a replaced version.
- **Code**: one pure predicate `newest_wins` in `crates/tc-journal/src/fold/merge.rs`; at each
  conflict site the fold returns `Local::ByRule(provisional)` with `AutoRule::Newest`. Books in
  `books.rs` (`status_of`, `last_book`). Carried beside names in `attach::stage_paths`, read by
  tc-store's checkpoint from the fold's placements, never from file names. `moved_into` for
  History across check moves. tc-app: a pure `replaced(snapshot, me, received)` for the Received
  sheet. Removed: the newest-wins kinds from `ConflictKind`, `KeepChoice`, `ReviewTab::{Alignments,
  Checks}`, and their choice cards; INV-EVERY-KIND-HAS-A-CHOICE and INV-NO-SILENT-PIN-MOVE (made
  obsolete by H5).
- **Checks**: `newest_wins_converges`, `the_loser_stays_in_history`, `only_text_conflicts`,
  `carried_keeps_both`, `a_newer_removal_removes_and_a_newer_edit_keeps` (property tests in
  `crates/tc-journal/tests/properties.rs`); `a_moved_checks_older_decision_is_restored` (tc-store);
  `only_text_kinds_reach_the_review` and `received_says_what_was_replaced` (tc-app); the
  conflict-kinds journey. Proof it can fail: the min instead of the max fails
  `newest_wins_converges`; no free-path check fails `carried_keeps_both`; no last-book rule fails
  the book test.
- **Prose**: STORAGE I2, I3, §7.2, §7.6, §8.1, §8.3, §8.5, §11.4, §12; PLAN.md declared differences.
- **Human**: H5 is ruled. Seven detail questions went to the owner on 2026-10-24 with defaults (log).

## Steps

### Step 1: The fold: newest wins on records (tc-journal)
- Do: `newest_wins`, `AutoRule::Newest`, the `Local` rule, unreadable keys still registered; the
  three property tests; update `conservation`, `concurrent_differences_are_visible`,
  `no_survival_by_time_or_identity` (rename `heads_and_text_conflicts_read_no_clock`),
  `resolution_always_possible` and the scenario tests and fixtures that asserted these kinds.
- Done when: `cargo test -p tc-journal` passes; the min instead of the max fails
  `newest_wins_converges`. The storage lane reviews the diff.

### Step 2: The fold: books
- Do: `status_of` and `last_book` by newest, the book property test, STORAGE §7.6, I2, I3.
- Done when: `cargo test -p tc-journal` passes; dropping the last-book rule fails it.

### Step 3: Carried files and path case (tc-journal, tc-store)
- Do: the beside map and names, `checkpoint.rs` reading the fold's placements, the newest keeps the
  name in a case clash, `carried_keeps_both`, the tc-store carried tests.
- Done when: `cargo test -p tc-journal -p tc-store` passes; no free-path check fails
  `carried_keeps_both`.

### Step 4: History across check moves (tc-journal, tc-store)
- Do: `moved_into`, Restore of a version whose key moved into the shown key, and
  `a_moved_checks_older_decision_is_restored`. Ships with step 1.

### Step 5: STORAGE.md
- Do: I2, I3, §7.2, §7.6, §8.1, §8.3, §8.5, §11.4 and §12 to the rule.

### Step 6: tc-api and tc-app
- Do: remove the newest-wins kinds and choices, the restore race to the Text tab, the Received
  sheet's `replaced` rows (drop `kept_newest`), the scripted kinds of step 1 (2026-10-24) asserting
  newest-wins rows, the goldens.
- Done when: `cargo test -p tc-api -p tc-app` passes.

### Step 7: tc-ui
- Do: remove the Alignments, Checks and Settings tabs, their cards and markers, and the dropped
  packet screens; draw the Received sheet's replaced rows; keep History and Restore, Detached,
  Notices, the banners, the draft offer, the boot error and the first-launch failure.
- Done when: the four tc-ui wasm32 clippy runs and `cargo test -p tc-ui` pass.

### Step 8: Journeys and declared differences
- Do: `conflict-kinds.test.mjs` becomes newest wins (B and C see A's newer check and alignment;
  History restores the older one); J26's check-decision step or E1a's note; PLAN.md's declared
  differences. Three green runs.
- Then: three verification lenses (no-loss card), and the tc-journal and tc-store diff to the
  storage lane.

## Gates before merge

Until INT-3 lands, run every command in AGENTS.md "Tests" plus the Windows step of
`.github/workflows/ci.yml`. After INT-3, run `scripts/gates.sh` (to create in INT-3) instead.

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`
5. The same with `--features gallery`, then `--features app`, then `--features test-hooks`.
6. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
7. `cargo clippy -p tc-app --lib -- -D warnings`
8. `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
9. `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`

Plus:

- The H5 ruling file exists and every wired screen is in it.
- Step 6's carried part is wired with two choices only, *Keep this one* and *Keep both*
  (`jon_01-Juan.wav`), as `rulings/2026-10-22-H19.md` and `-H22.md` part 2 say.
- Step 8's journeys green three runs in a row with no skips.
- `cargo test -p tc-ui --test screen_record` passes.
- `cargo test -p tc-app --test op_counts op_counts::conflicts::` passes.
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- Step 2 is done: stop until the H5 ruling file exists. Do not wire a screen the owner has not
  signed off.
- The owner asks for a change to a screen: redraw it, add it to a new packet, and ask again.
- A screen would offer a third carried choice, or *Keep the other*: CF 3 gives two. Stop.
- A choice would move a pin, remove a file or overwrite a version the screen did not show. That
  breaks a rule; stop and write it in the log.
- You need to change `tc-store` or `tc-journal` (for example, to make `rename_carried` refuse a held
  path). Write what you need in the log and ask the storage owner.

## References

- `.audit/roadmap/evidence/2026-10-21/app-remaining.md` (C7b; F7)
- `.audit/roadmap/evidence/2026-10-21/health-app.md` (CH-4, CH-9)
- `.audit/roadmap/evidence/2026-10-21/tc4-bugs.md` (§2 row C7b)
- `docs/STORAGE.md` §8.3 and §8.5 on the S2 branch (each kind's choices and words)
- `docs/STORAGE-DELTA-2026-10-21.md` §2.7 (carried choices and the *Keep both* name), §2.15
  (notices), §6 (C7b row), §7 Q3 and Q4
- `.audit/roadmap/rulings/2026-10-21-carried-files.md` (CF 3),
  `.audit/roadmap/rulings/2026-10-21-outside-edits.md`, `rulings/2026-10-22-H19.md` (carried
  files stay) and `rulings/2026-10-22-H22.md` part 2 (`jon_01-Juan.wav`)
- `docs/APP-DESIGN.md:3397` (C7b's row) and the Q-8, Q-9 rows (:3561, :3562)
- `crates/tc-journal/src/command.rs` (`MoveCheck`, `Restore`, `ReleaseBatch`, `Dismiss`,
  `Choice::RenameCarried`; `ResolvePins` on the S2 branch)
- B11's minor "no test covers the Alignments and Checks tabs or the markers" (committed to
  `.audit/minors.tsv` with B11's records; `evidence/2026-10-21/app-remaining.md` B11 close-out)

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/app-remaining.md
- 2026-10-21 | roadmap | note | none | updated to the owner's rulings of 2026-10-21 and docs/STORAGE-DELTA-2026-10-21.md | .audit/roadmap/rulings/
- 2026-10-22 | roadmap review (completeness review 2026-10-22) | ruling | none | Step 6 and the funnel card follow H19 (carried files stay) and H22 part 2 (*Keep both* is `jon_01-Juan.wav`); the STOP lines are gone | rulings/2026-10-22-H19.md, rulings/2026-10-22-H22.md
- 2026-10-22 | roadmap review (completeness review 2026-10-22) | note | none | Step 7b gives PLAN P2 J26's check-decision step an owner whichever of C7b and E1a lands last; `later` names H5 from step 3 | .audit/roadmap/evidence/2026-10-21/review-2026-10-22.md
- 2026-10-24 15:03 EDT | shell lane (Claude Code e1bb3a0c) | claim | none | The orchestrator gave C7b to this lane (owner ruling 2026-10-24, finish in days); cut from origin/track-C7a d07d6612 with main 0189cee4 merged in, because C7a is in merge batch 1 and not on main yet (depends C7a: the merge waits for it). Kept to the review and conflict screens: the app lane does UI-1 meanwhile. One review, per the ruling | git log -2 track-C7b
- 2026-10-24 15:32 EDT | shell lane (Claude Code e1bb3a0c) | step 1 | none | tc_api::test::ScriptedSync::Synced gains kinds: Vec<TeammatesKind> (serde default; Check, Comment, Alignment, Setting, Pins, BookPresence, CarriedFile, PathCase); Teammates::change_kind (test-hooks) has Teammates 1 and 2 change one record of each kind from its heads with tc-journal's encoder, as edit_verse does (a carried file: Teammate 1 adds it, both replace it, CF 3). cargo test -p tc-app --test app app::conflicts:: 13 passed, with every_kind_is_listed_on_its_tab and align_and_check_show_the_two_versions_marker; cargo test -p tc-api --test golden 9 passed (script.json regenerated). Proofs: Alignment moved to ReviewTab::Checks fails 'conflicts.rs:1042:5: left: [] right: [Alignment]'; the aligner's marker None fails 'conflicts.rs:1112:5: left: None right: Some(ConflictMarker { .., versions: 2 })'. The carried and path-case records arrive without their file contents (filling the git store is tc-store's), so the Notices tab also shows FileMissing notices; the tab test leaves Notices out and says why. Written by a subagent of this lane, reviewed here | cargo test -p tc-app --test app app::conflicts::
- 2026-10-24 16:07 EDT | shell lane (Claude Code e1bb3a0c) | step 2 | none | The H5 packet: docs/evidence/q9-2026-10-24.md (20 screens, one line each, and '## Question for the owner') with 20 pictures and DOM files in docs/evidence/q9-2026-10-24/, captured by parity/app/q9-packet.mjs from the test-hooks page ?probe=q9&screen=<name> (crates/tc-ui/src/app/q9.rs, web/probe.rs) and the real app. Proposed: alignments, checks, checks-moved, checks-comment, settings-pins, -setting, -books, -book-removed, -last-book, -carried (Keep this one on each version, Keep both naming jon_01-Juan.wav, no Keep the other), -path-case, detached, notices, received; built earlier without a tC4 design: draft-offer, read-only, boot-error, bulk-keep, history, first-launch-failure; E2's USB screens follow E2. No choice wired. Gates: the four tc-ui wasm32 clippy runs, fmt, cargo test -p tc-ui --test app app::review (19 pass). The orchestrator asked to keep building while the owner looks, so the state stays building with H5 in needs_owner. Note: app::save tests fail in this worktree with or without this step (a_failure_keeps_the_text_and_the_error, save.rs:136): to look at before the merge | docs/evidence/q9-2026-10-24.md
- 2026-10-24 18:10 EDT | shell lane (Claude Code e1bb3a0c) | ruling | none | H5 ruled (rulings/2026-10-24-H5.md, merged from track-RULING-H5 79e28c52): only verse text conflicts; everything else newest wins, the loser in History; carried files keep both. The card is rewritten to it: the choice tabs and cards go, the store and fold change. The step 3 work on check choices (WIP 7d04420d) is reverted (9ecb4c13). Steps 1 and 2 above stay as history: step 1's scripted kinds are reused; step 2's packet was the question
- 2026-10-24 18:10 EDT | shell lane (Claude Code e1bb3a0c) | note | none | Design (a Plan agent of this lane): conflicts are computed by the fold, never stored (I1); losers are already heads that FoldState::history lists and Restore names, so old projects need no migration. Newest = ChangeMeta::order() (rank, at, id), the order of today's provisional head; only concurrent heads compare. Seven detail questions sent to the owner through the orchestrator with defaults: headings, chapter fronts, story base and reference stay text; the book restore race stays a conflict; book layout is newest-wins; a version from outside with no known base never wins; the whole newest version wins; the newest keeps the name in a case clash; a beside file with no person name uses the version's short id. Risks told: a wrong clock wins races (ClockAhead warns past 24 h only); builds before and after H5 must not be mixed on one team | side-session-notes C7b-newest-wins-design.md
