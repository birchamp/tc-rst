---
id: C7b
title: Conflict choices: every other kind
track: A
lane: app
milestone: M3
state: todo
owner: none
branch: none
worktree: none
depends: [C7a, S2]
needs_owner: []
proves: [P2, P3, P7]
no_loss: true
size: 4.5-6.5 sessions plus the owner's wait (basis: B11 and C8; carried choices shrink to two, about -0.5 session, docs/STORAGE-DELTA-2026-10-21.md §6)
merged: none
updated: 2026-10-22
next: Claim this card after C7a and S2 merge; steps 1-2 need no ruling; step 3 waits for H5. H19 and H22 are ruled (rulings/2026-10-22-H19.md, -H22.md).
blocked_by: none
later: ["H5 from step 3"]
---

# C7b: Conflict choices: every other kind

## Goal

Every conflict kind outside the Text tab has its choices in the review: check, moved check,
alignment, settings and pins, the project's books, comment, carried file and path case. Detached
records can be dismissed, a held batch of changes can be applied ("Apply what was recovered"), and a
notice can be dismissed. The Received sheet is drawn for E2's *Receive work from a file* (Q-8). The
owner has signed off every rebuild-only screen (H5) before its choices are wired, and the screens
match what was signed off.

A carried file conflict has two choices only, by the owner's ruling of 2026-10-21 (CF 3): *Keep
this one*, on each version, and *Keep both*, which saves the second version under its own free
name. *Keep the other* is gone. Carried files stay (`rulings/2026-10-22-H19.md`, option A) and the
second file's name is `jon_01-Juan.wav` (`rulings/2026-10-22-H22.md` part 2).

Not here: the grouped row of one import reuses `OutsideEditLarge`'s three choices on the Text tab
(C7a). The notices of kept copies of outside-edited files, with *Import*, *Dismiss* and *Bring it
back*, are D1's (delta §6).

## Why it matters to a translator

When a teammate's sync or USB file brings a second check decision, alignment, setting or file, the
translator can decide which one stays. Today those tabs are read only. No choice moves a resource
pin without showing what it changes, and no rename loses a file.

## Before you start

- `python3 -I .audit/tools/board.py` shows C7a as `done`.
- `grep -n 'pub enum KeepChoice' crates/tc-api/src/conflicts.rs` prints one line (C7a's type).
- `grep -n 'fn tab_of' crates/tc-app/src/conflicts.rs` prints line 37 or near it.
- `awk '/pub enum ConflictKind/,/^}/' crates/tc-api/src/conflicts.rs | grep -c '^    [A-Z]'` prints
  `25` (24 kinds and `Unknown`).
- `python3 -I .audit/tools/board.py` shows S2 as `done`. S2 is in `depends` because
  `Command::ResolvePins` and `Choice::KeepBothCarried` exist only on the S2 branch today.
  `git grep -n 'ResolvePins {\|KeepBothCarried' HEAD -- crates/tc-journal/src/command.rs` prints
  both.
- `ls .audit/roadmap/rulings/2026-10-22-H19.md .audit/roadmap/rulings/2026-10-22-H22.md` prints
  both: carried files stay and *Keep both* is named `jon_01-Juan.wav`.
- For steps 3 to 8: `ls .audit/roadmap/rulings/*-H5.md` prints one file.

## Funnel card

- **Invariants**:
  - INV-EVERY-KIND-HAS-A-CHOICE: `choices(kind)` (C7a, `crates/tc-ui/src/app/review.rs`) and
    `choose` (C7a, `crates/tc-app/src/conflicts.rs`) match every `ConflictKind` and every
    `KeepChoice` with no wildcard; after this card only `Unknown` answers `Later`.
  - INV-NO-SILENT-PIN-MOVE: a pins choice moves a pin only through a plan the person saw, with its
    counts ("With v92: 1,204 checks valid, 37 to check again", STORAGE §8.3); a plan that changed
    since is refused, and nothing moves.
  - INV-A-RENAME-KEEPS-BOTH-FILES: a path-case rename and *Keep both* write the file at a path that
    holds no file, so both files stay. *Keep this one* removes only the other version of that same
    conflict; no choice removes any other carried file.
  - INV-A-CHOICE-NEVER-OVERWRITES-AN-UNSEEN-VERSION (from C7a) holds for every new choice, Dismiss
    included.
- **Wrong states**:
  - A pins choice that runs the carry-over without the counts, so checks turn invalid unannounced (a
    teammate pinned v93 while I pinned v92).
  - A rename onto a path that already holds a file: `rename_carried` in
    `crates/tc-journal/src/fold/resolve.rs` writes the new path with that path's heads as parents,
    which would replace the file there. (`keep_both_carried`, from S2, refuses such a
    path; `rename_carried` does not.)
  - A *Keep both* name with a space, such as the ruling's example `jon_01 (Juan).wav`: the path
    grammar forbids it (S2:1024). The name is `<stem>-<person>.<ext>` (`jon_01-Juan.wav`), the
    person part cleaned as an export file name is, with `-2`, `-3` when taken (delta §2.7;
    `rulings/2026-10-22-H22.md` part 2).
  - A *Keep the other* button, or a third carried choice: CF 3 gives two.
  - A Detached entry or a notice with no token or id, so Dismiss cannot name what it dismisses
    (`DetachedEntry` and `NoticeEntry` in `crates/tc-api/src/conflicts.rs` carry none today).
  - `tab_of` (`crates/tc-app/src/conflicts.rs:37`) remapping a kind to another tab: nothing fails
    today (B11's minor).
- **Code**:
  - Types: new `KeepChoice` variants (to create) for the choices `Candidate` cannot carry:
    `ApplyToMovedCheck`, `PerUnit { .. }`, `KeepBothComments`, `Pins { plan }`,
    `RestoreBook { mode }`, `Rename { file, to: FreePath }`, `KeepBothFiles { other, to: FreePath }`. `FreePath` (to create) is a path only
    `free_path(snapshot, wanted)` makes, and it never names a path the project holds. For *Keep
    both*, `wanted` is `<stem>-<Door43 user>.<ext>`, for example `jon_01-Juan.wav` (delta §2.7;
    `rulings/2026-10-22-H22.md` part 2: no spaces in project paths).
    `DetachedEntry.shown: ShownToken` and `NoticeEntry.id` (to create).
  - One entry point: C7a's `choose` gains one arm per variant, mapping to
    `tc_journal::command::Choice` (`Candidate`, `Values`, `ApplyToMovedCheck`, `RenameCarried`,
    and S2's `KeepBothCarried`),
    `Command::RestoreBook` (`RestoreMode::{KeepText, StartEmpty}`) or S2's `Command::ResolvePins`.
    `Candidate` already covers the book kinds, *Keep this one* for a carried file, and *Remove*
    for a path case (`keep_candidate`, in `crates/tc-journal/src/fold/resolve.rs`). `Values`
    records come only from the fold's own builders (`per_unit`, `keep_both_comments`, same file).
    Detached and Notices use `Command::{Dismiss, ReleaseBatch}` and the handle's `dismiss_notice`.
    `Command::MoveCheck` (re-key a decision, R7) stays unused unless the signed-off packet has a
    screen for it.
  - Notices after the delta: `StorageDamage` goes; `OutsideEditHeld` becomes "book changed
    outside, waiting"; `MetadataEditedOutside` becomes one notice for any rewritten generated file
    (delta §2.15). A kept copy's notice is D1's: its *Dismiss* deletes the copy, so C7b's generic
    dismiss never applies to it.
  - Removed: the `Later` arms of `choices` for every kind but `Unknown`; `later_bar` stays for
    `Unknown` only.
  - Left to checks, and why: the tab of each kind is a design decision no type can carry;
    `every_kind_is_listed_on_its_tab` holds it.
- **Checks**:
  - `every_kind_is_listed_on_its_tab` (to create) in `crates/tc-app/tests/app/conflicts.rs`; kind:
    unit on a fold built with tc-journal; runs in: CI `rust` job. Proof: move `Kind::Alignment` to
    `ReviewTab::Checks` in `tab_of`; the test must fail.
  - `align_and_check_show_the_two_versions_marker` (to create), same file; kind: unit. Proof: return
    `None` for the aligner's `marker`; the test must fail.
  - `a_pins_choice_moves_no_pin_until_its_plan_is_confirmed` and
    `a_stale_pins_plan_is_refused_and_moves_nothing` (to create), same file; kind: unit.
  - `a_rename_keeps_both_files` and `free_path_never_names_a_held_path` (to create), same file; the
    second is a `proptest` over random file lists. Proof: make `free_path` return `wanted`
    unchanged; the property must fail.
  - `keep_this_one_removes_only_the_other_version` and
    `keep_both_saves_the_second_version_under_a_free_name` (to create), same file; kind: unit.
    Proof: drop the `-<person>` part from `wanted`; the second test must fail. Carried files
    stay (`rulings/2026-10-22-H19.md`, option A), so these tests are built here.
  - `dismiss_needs_the_shown_token` (to create), same file; kind: unit.
  - `one_resolution_per_kind_through_the_ui` (to create) in
    `parity/app/journeys/conflict-kinds.test.mjs` (to create), one `browserTest` per group (a) to
    (d); kind: journey; runs in: G5's job once G5 lands (they skip in CI until then).
  - `rebuild_only_screens_equal_their_signed_off_capture` (to create) in
    `crates/tc-ui/tests/screen_record.rs`; kind: parity gate; runs in: CI `rust` job.
  - J26's check-decision step (to create) in `parity/app/journeys/team-sync.test.mjs`: A resolves
    the conflict between two check decisions, B and C sync and see the kept decision (PLAN P2
    J26); kind: journey; runs in: G5's job. Step 7b says which of C7b and E1a writes it.
- **Prose**: `docs/PLAN.md` declared differences for each new screen; `docs/TC4-ISSUES.md` rows #25
  and #791.
- **Human**: H5 (ROADMAP.md): sign off the rebuild-only screens as one packet. Options: approve as
  drawn (the choices are wired as shown); ask for changes (the card redraws, then asks again);
  approve some (only those groups are wired). Recommendation: approve the packet. Ruling file:
  `.audit/roadmap/rulings/<date>-H5.md`.
- **Human, ruled 2026-10-22**: H19 (delta Q3): S2's carried code stays, without its git backups
  (`rulings/2026-10-22-H19.md`, option A). H22 (delta Q4): the second file's name under *Keep
  both* is `<stem>-<Door43 user>.<ext>`, `jon_01-Juan.wav` (`rulings/2026-10-22-H22.md` part 2).

## Steps

### Step 1: Test the read side of every tab

- Do: Add a scripted teammates' change for each kind group (a check decision, a comment, an
  alignment, a setting, a pin, a book presence, a carried file and a path case), beside
  `teammates_edit_verse` (`crates/tc-app/src/backend/mod.rs:1087`, `memory.rs` module `teammates`,
  `scenario_store.rs`), made with tc-journal's encoder as `Teammates::edit_verse` does, and
  reachable from `tc_api::test::ScriptedSync::Synced` (to create: a field). These kinds reach a
  translator only by sync or USB; this stands in for X. Write `every_kind_is_listed_on_its_tab` and
  `align_and_check_show_the_two_versions_marker`. A carried file conflict is two versions that
  replaced the same version (CF 3); S2's code makes them (`rulings/2026-10-22-H19.md`).
- Files: as named; `crates/tc-api/src/test.rs`; `crates/tc-api/tests/golden/endpoints/test/*`;
  `crates/tc-app/tests/app/conflicts.rs`.
- Done when: `cargo test -p tc-app --test app app::conflicts::` passes; each proof mutation of the
  funnel card fails its test (paste the first failure line into the log);
  `cargo test -p tc-api --test golden` passes.
- Record: log line `step 1`; set `state: building`; update `next`.

### Step 2: Send the H5 packet, then stop

- Do: Draw each proposed screen as a pure view: the Alignments, Checks, Settings, Detached and
  Notices tabs with their choice footers (STORAGE §8.3 words), and the Received sheet (STORAGE
  §8.5). Add a test-hooks page `?probe=q9&screen=<name>` (to create) in
  `crates/tc-ui/src/app/web/probe.rs` that draws one screen from fixed data; it never builds in a
  release. Write `parity/app/q9-packet.mjs` (to create), which opens each screen with the journeys
  kit and saves `<name>.png` and `<name>.json` into `docs/evidence/q9-<date>/`. Include the screens
  built earlier that have no tC4 design: History and Restore, the bulk keep (C7a), the draft offer,
  the read-only banner, the first-launch failure and the boot error. E2's USB screens do not exist
  yet: list them as "follows E2" in the packet. Draw the carried file card with *Keep this one* on
  each version and *Keep both* naming `jon_01-Juan.wav` (`rulings/2026-10-22-H19.md` keeps carried
  files; `rulings/2026-10-22-H22.md` part 2 names the file). Draw no *Keep the other*. Write `docs/evidence/q9-<date>.md`: one line per
  screen, what the translator sees, and what each button does. End that file with a section
  `## Question for the owner`: the H5 question in short plain sentences, with the options and the
  recommendation from the funnel card. Tell the owner the file's path in your session's last message.
- Files: `crates/tc-ui/src/views/review.rs`, `crates/tc-ui/src/app/web/probe.rs`,
  `parity/app/q9-packet.mjs`, `docs/evidence/q9-<date>/`, `docs/evidence/q9-<date>.md`.
- Done when: `ls docs/evidence/q9-<date>/*.png | wc -l` equals the number of lines in the packet's
  list; `grep -c '^## Question for the owner' docs/evidence/q9-<date>.md` prints `1`; the four tc-ui
  wasm32 clippy commands pass.
- Record: log line `step 2`; add `H5` to `needs_owner`; set `state: blocked`,
  `blocked_by: waiting for the owner's H5 ruling`,
  `next: when rulings/<date>-H5.md exists, do step 3`.

### Step 3: Group (a): check and moved check

- Do: When the ruling file exists, remove `H5` from `needs_owner`, set `state: building`, and log
  `ruling`. Add `KeepChoice::{ApplyToMovedCheck, PerUnit, KeepBothComments}`: *Keep mine*, *Keep
  <person>'s* (Candidate), *Choose per line*, *Keep both* for a check comment, *Apply <person>'s
  change to the moved check*, *Keep <person>'s check where it was* (Candidate). Map them in
  `choose`. Wire the Checks tab's buttons. Add the group's `one_resolution_per_kind_through_the_ui`
  test.
- Files: `crates/tc-api/src/conflicts.rs`, `crates/tc-app/src/conflicts.rs`,
  `crates/tc-ui/src/app/review.rs`, `crates/tc-ui/src/views/review.rs`,
  `parity/app/journeys/conflict-kinds.test.mjs`, the goldens.
- Done when: `cargo test -p tc-api`, `cargo test -p tc-app --test app app::conflicts::` and
  `cargo test -p tc-ui --test app app::review::` pass; the journey prints `# fail 0` and
  `# skipped 0` (commands as in step 8).
- Record: log line `step 3`; update `next`.

### Step 4: Group (b): alignment

- Do: *Use this alignment* (Candidate); *Open in the aligner starting from this one* (its save is
  the resolution); bulk *Dismiss alignments made for text that was not kept*. Each card says whether
  it was made for the current text (STORAGE §8.3).
- Files: as step 3; `crates/tc-ui/src/views/align/session.rs` for the open.
- Done when: as step 3, for the Alignments tab.
- Record: log line `step 4`; update `next`.

### Step 5: Group (c): settings, pins and the project's books

- Do: *Keep this one* for Setting, ProjectMeta, Versification, BookLayout and DeviceProfile. Pins: a
  `conflicts/planPins` read (to create) that returns the plan of each version with its counts, and
  `KeepChoice::Pins { plan }` through S2's `Command::ResolvePins`; a missing release offers
  *Download* first. Books: *Restore <book>*, *Keep it removed* and *Keep <book>* through `Candidate`
  (`keep_candidate` handles `BookRemovedWhileEdited`, `LastBookRemoved` and `BookPresence`);
  *Restore the text* and *Start empty* through `KeepChoice::RestoreBook { mode }` and
  `Command::RestoreBook` (STORAGE §8.3). Write the two pins tests.
- Files: as step 3; `crates/tc-app/src/plans.rs` (reuse C8's plan reading).
- Done when: as step 3, plus `a_pins_choice_moves_no_pin_until_its_plan_is_confirmed` and
  `a_stale_pins_plan_is_refused_and_moves_nothing` pass, and each fails with the confirm skipped
  (temporary edit, reverted).
- Record: log line `step 5`; update `next`.

### Step 6: Group (d): comments, Detached, notices, then carried files and path cases

- Do, no new ruling needed: Comment: *Keep this one*, *Keep both*. Detached: *Dismiss* with the
  entry's token (`conflicts/dismiss`, to create). Notices: *Apply what was recovered* for
  `IncompleteChanges` (`conflicts/releaseBatch`, to create) and dismiss for every notice but a
  kept copy's (D1's) through `session/dismissNotice` (whichever of D1 and C7b lands first creates
  it; the other reuses it). Wire the Received sheet's view for E2.
- Do, carried files and path cases (ruled: `rulings/2026-10-22-H19.md` option A keeps S2's
  carried-file code, so these are wired here): CarriedFile: *Keep this one* on each version
  through `Candidate` (the other version's file is removed, and the change names every head);
  *Keep both* through `KeepChoice::KeepBothFiles` and `Choice::KeepBothCarried { keep, other, to }`,
  with `to` from `free_path`. PathCase: *Rename* (with `FreePath`) and *Remove*. *Keep both*'s
  name is `<stem>-<Door43 user>.<ext>`, for example `jon_01-Juan.wav` (`rulings/2026-10-22-H22.md`
  part 2; project paths cannot hold spaces).
- Files: as step 3; `crates/tc-api/src/lib.rs` and `crates/tc-api/tests/endpoints.rs` (`PHASE_0`)
  for the new rows.
- Done when: as step 3, plus `dismiss_needs_the_shown_token` passes and
  `cargo test -p tc-server --test routes` passes; `a_rename_keeps_both_files`,
  `free_path_never_names_a_held_path`, `keep_this_one_removes_only_the_other_version` and
  `keep_both_saves_the_second_version_under_a_free_name` pass.
- Record: log line `step 6`; update `next`.

### Step 7: Hold the screens to the signed-off packet

- Do: Commit the signed-off captures as the reference for the rebuild-only rows. Add
  `rebuild_only_screens_equal_their_signed_off_capture` to `crates/tc-ui/tests/screen_record.rs`:
  each rebuild-only screen's DOM capture equals its signed-off one, or is explained in
  `parity/app/residuals.json`.
- Files: `crates/tc-ui/tests/screen_record.rs`, `parity/app/*`.
- Done when: `cargo test -p tc-ui --test screen_record` passes, and fails when one signed-off DOM
  file is edited by one attribute (temporary edit, reverted).
- Record: log line `step 7`; update `next`.

### Step 7b: Give J26's check-decision step an owner

- Do: Run `python3 -I .audit/tools/board.py` and read E1a's state. If E1a is `done`: add the
  check-decision conflict step to `parity/app/journeys/team-sync.test.mjs` (A resolves the
  conflict between two check decisions, B and C sync and see the kept decision; PLAN P2 J26) and
  delete the minor E1a step 6 filed for it from `.audit/minors.tsv`'s open list (append a row that
  says it is closed; the file is append-only). If E1a is not `done`: append a `note` line to
  E1a's log that C7b has landed and E1a step 6 must include the step, and say so in this card's
  log. One of the two cards always writes the step.
- Files: `parity/app/journeys/team-sync.test.mjs`, `.audit/minors.tsv` or `units/E1a.md`.
- Done when: either `grep -n "check decision" parity/app/journeys/team-sync.test.mjs` prints the
  new step and the journey passes three runs, or E1a's log holds the `note`.
- Record: log line `step 7b`.

### Step 8: Run the journeys and write down the differences

- Do: Build and run the journeys three times. Add each new screen to `docs/PLAN.md` "Where the
  rebuild differs from tC4 4.0 on purpose" and update `docs/TC4-ISSUES.md` rows #25 and #791 with
  the test names.
- Files: `docs/PLAN.md`, `docs/TC4-ISSUES.md`, `docs/APP-DESIGN.md` (C7b's row, line 3397).
- Done when: after `cargo build -p tc4 --features test-hooks,fixture-backend`,
  `cargo build -p tc-door43-fake` and `crates/tc-ui/build-app.sh`, the command
  `TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake node --test parity/app/journeys/conflict-kinds.test.mjs parity/app/journeys/conflict-review.test.mjs`
  prints `# fail 0` and `# skipped 0` three runs in a row.
- Record: log line `step 8`; set `state: verifying`; update `next`.

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
- 2026-10-26 | shell lane (Claude Code 6f39d119), for CHK-3 | note | none | your rows of docs/STORAGE.md §15 are `(pending: C7b)`; when you write a test, replace the marker with its name in the same commit (claims.test.mjs checks §15 now) | docs/STORAGE.md
