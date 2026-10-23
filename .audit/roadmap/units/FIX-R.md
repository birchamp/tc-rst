---
id: FIX-R
title: Fix the tC4 bugs the rebuild repeats; reconcile TC4-ISSUES
track: A
lane: app
milestone: M1
state: claimed
owner: C7a session (Claude Code 3058c665)
branch: track-FIX-R
worktree: .worktrees/track-FIX-R
depends: [INT-4]
needs_owner: []
later: ["H12 from step 4"]
proves: [P1, P8]
no_loss: true
size: 2-3 d
merged: none
updated: 2026-10-23
next: Do step 1 (one direction source) on track-FIX-R.
blocked_by: none
---

# FIX-R: Fix the tC4 bugs the rebuild repeats; reconcile TC4-ISSUES

## Goal

Every tC4 bug that the 2026-10-21 reading found in the rebuild is fixed in its structure, with a
test named after the broken rule that fails if the bug comes back. `docs/TC4-ISSUES.md` tells
the truth again: no fixed bug is listed as open, every named test exists, and the counts are
recounted.

## Why it matters to a translator

A Hebrew or Arabic project reads in the right direction everywhere. A link that a save sends
back to the word bank is shown. A save that can never succeed does not trap the translator. On
Linux and macOS, having Dropbox installed does not make every project read-only.

## Before you start

Run each from the repository root on your branch:

- `git merge-base --is-ancestor f9067bc HEAD && echo landed` prints `landed` (INT-4 is in).
- `grep -n 'dir=dir_name(view.orig_dir)' crates/tc-ui/src/views/align/session.rs` prints the
  cards container (about 1136). `word_card` (about 313) draws the placed words in a body `div`
  (about 491) with no `dir`. The bank (about 856) uses `view.target_dir`.
- `grep -n 'write("textDirection"' crates/tc-app/src/settings.rs` prints Settings' only
  direction write (about 238). `grep -n scriptDirection crates/tc-app/src/home.rs
  crates/tc-app/src/align/open.rs` prints Home's `card_facts` (about 221) and Align's
  `project_dir` (about 87). The open reply (`understand.rs` about 523) uses `card_facts`' `dir`.
- `grep -n 'Held(Hold::ReadOnly(reason))' crates/tc-ui/src/app/save/mod.rs` prints about 972
  (inside `Saves::reply`, about 826-987): a read-only refusal is held and never blocks the
  gate. `grep -n BadToken crates/tc-ui/src/app/save/classify.rs` prints about 70: `BadToken`
  is `Failed`, so after one retry `gate()` (about 1211-1216) returns `Blocked`.
- `grep -rn '\.dropped' crates/tc-ui/src/views/align/` prints nothing: Align never draws
  `AlignerView.dropped` (`crates/tc-api/src/align.rs` about 273).
  `grep -n 'reflowed.value' crates/tc-store/src/commands.rs` shows the verse save keeps only
  the value.
- `grep -n 'join(".dropbox").exists()' crates/tc-store/src/platform.rs` prints about 137. The
  right rule is in a copy: `crates/tc-app/src/folders.rs` about 99-105.
- Read `.audit/roadmap/units/T.md`: its funnel card adds `DroppedLinks` to tc-store's reply.

## Funnel card

- **Invariants**:
  - INV-TEXT-DIRECTION-HAS-ONE-SOURCE: one function decides a project's direction: metadata
    `scriptDirection`, else settings `textDirection` (tC4 `src/state.jsx` 2144-2159). Settings
    writes the field it reads first. Placed words show in the translation's direction; a help
    card's text shows in its own.
  - INV-A-DROPPED-LINK-IS-LISTED: every link a save or an open sends back to the word bank is
    listed to the translator with its verse and word (owner answer Q30 (a)).
  - INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR: a refusal that no retry in this open can fix
    never blocks Home, and its text is kept for the next open.
  - INV-SYNCED-FOLDER-DETECTION-IS-EXACT: a folder counts as synced by a cloud client only when
    that client syncs it. Dropbox's settings folder `~/.dropbox` never marks a project.
- **Wrong states**:
  - Hebrew verse: the words placed on a card read backwards (tC4 #830).
  - Settings switches to right to left; Home, the open project and Align stay left to right,
    even after a restart (#832.1). Settings shows no language name for a tC4 project (#832.2).
  - English questions in an Arabic set are laid out right to left (#836).
  - A Translate save sends a link back to the bank and nobody is told (Q30 (a), #825).
  - A save refused for good keeps Home blocked (#801).
  - Linux or macOS with Dropbox installed: every project under home opens read-only (R28).
  - An offline update check shows a raw error (#833.1).
  - OBS: one bad row of the questions table hides every question.
  - A Psalm verse on `\q` lines is drafted on Home and undrafted in Check (H12).
- **Code**:
  - Types: `Hold::RefusedForThisOpen(NotSaved)` (to create) beside `Hold::ReadOnly`; a dropped
    list in the verse-save reply (T's `DroppedLinks`, or created here if T has not landed); an
    update-check failure enum that maps to fixed sentences (to create).
  - One entry point: `tc_app::direction::project_direction` (to create, pure) for every
    reader of a project's direction; tc-store's `cloud_client` taking the environment as a
    value, called by tc-store and by tc-app's first-launch rule.
  - Removed: the direction reads in `home.rs` `card_facts` and `align/open.rs` `project_dir`;
    tc-app's copy of the cloud check (`folders.rs` `cloud_client`, `dropbox_roots`,
    `placeholder`); the `_ =>` arm in `crates/tc-resources/src/load.rs` (about 417) that sends
    OBS questions to the strict reader; after H12, the second drafted rule.
  - Left to checks, and why: the direction a browser draws text in. Types reach the `dir`
    attribute, not what the page renders, so journeys check it.
- **Checks** (all "(to create)"):
  - `a_direction_changed_in_settings_shows_on_home_and_in_every_tool_after_restart` and
    `an_imported_project_shows_its_metadata_language_name_in_settings` in
    `crates/tc-app/tests/app/create.rs`; unit through `App::request`; CI `cargo test --workspace`.
  - `an_ot_card_shows_placed_words_in_the_translation_direction` in
    `parity/app/journeys/align.test.mjs`; journey; local until G5 runs journeys in CI.
  - `an_english_question_under_an_arabic_set_reads_left_to_right` in
    `parity/app/journeys/understand.test.mjs`; journey; local until G5.
  - `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` in
    `crates/tc-ui/tests/app/save.rs`; unit on the save machine; CI.
  - `a_link_sent_back_to_the_bank_is_listed`: reply half in `crates/tc-app/tests/app/translate.rs`
    (CI); screen half in `parity/app/journeys/align.test.mjs` (local until G5).
  - `a_dropbox_settings_folder_in_home_never_makes_a_project_read_only` in
    `crates/tc-store/tests/store/platform.rs` (to create); unit; CI.
  - `an_offline_update_check_shows_one_plain_sentence` in `crates/tc-app/tests/app/upgrades.rs`.
  - `one_bad_obs_question_row_hides_no_other_question` in `crates/tc-resources/tests/load.rs`.
  - Proof it can fail: for each, `git stash push -- <the fixed file>`, run the test, paste the
    failing line into the log, `git stash pop`.
- **Prose**: `docs/PLAN.md` "Where the rebuild differs from tC4 4.0 on purpose" (about line
  45): one bullet each for #830, #832, #836 (tC4 4.0 still has them) and #801.
- **Human**: H12 (`ROADMAP.md`). Recommendation: words on any line count, everywhere. Step 4
  waits for its ruling file; the other steps do not.

## Steps

### Step 1: Give a project's direction one source and draw text in its own direction

- Do:
  1. Red first, the two Rust tests. Direction: change it in Settings, start a new app on the
     same folders (copy `relaunch` from `crates/tc-app/tests/app/check_item.rs` about 509),
     assert Home's card, the open reply and Align's `target_dir` are all right to left.
     Language name: use `fixtures/tc4/sample-burrito` (its
     `ingredients/checking/settings.json` has no `languageName`; its metadata names `es-419`)
     and assert Settings shows the metadata's name.
  2. Red first, the Align journey: route `**/api/v1/align/open` (as `align.test.mjs` about 396
     does) and set `origDir` to `rtl`. Assert each placed word's computed `direction` is `ltr`
     and the words read left to right in order.
  3. Red first, the helps journey: route `**/api/v1/helps/passage` as `understand.test.mjs`
     about 284-290 does, set the `questions` slot's `dir` to `rtl`, keep its English text.
     Assert the question's computed `direction` is `ltr`.
  4. Create `crates/tc-app/src/direction.rs` with `project_direction`. Make `card_facts`,
     `project_dir` and the Settings dialog call it. Delete their own reads.
  5. Settings: write `languages[0].scriptDirection` through the store's `SetMeta` command (add
     a `StoreCommand::set_meta` constructor beside `set_setting`,
     `crates/tc-app/src/backend/mod.rs` about 402), then `textDirection` as today.
  6. `settings_values` (`settings.rs` about 42-47): when `languageName` is empty, show the
     metadata language's name.
  7. `session.rs`: pass the target direction into `word_card` and set
     `dir=dir_name(target_dir)` on its body `div`. `helps_panel.rs` `questions_tab` (about
     686): give each question `dir="auto"` and keep the pane's `dir` (about 743) for layout.
     Do the same in `notes_tab` (about 531) and `words_tab` (about 621).
- Files: `crates/tc-app/src/{direction.rs,settings.rs,home.rs,align/open.rs,backend/mod.rs,lib.rs}`,
  `crates/tc-ui/src/views/align/session.rs`, `crates/tc-ui/src/views/helps_panel.rs`, the test
  files above.
- Done when: `cargo test -p tc-app --test app -- direction_changed language_name` passes and
  fails with the fix stashed. The journeys pass with
  `. ~/.tc-env && cargo build -p tc4 --features test-hooks,fixture-backend &&
  crates/tc-ui/build-app.sh && TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test
  parity/app/journeys/align.test.mjs parity/app/journeys/understand.test.mjs`, the new ones
  fail with the fix stashed, and `arabic_helps_panes_are_rtl_in_an_english_interface` still
  passes.
- Record: append a log line `step 1` (TRACKING.md format) with each failing output; set
  `next` to step 2.

### Step 2: Never trap a translator behind a save; list every link sent back

- Do:
  1. `grep -rn 'NotSaved::' crates/tc-app/src crates/tc-store/src`. For each variant decide:
     does sending the same text again in this open fail the same way? Write the table in the
     log line.
  2. Red first: `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text`: a field
     refused with a for-this-open reason (for example `BadToken`), one gate retry, then
     `gate()` must be `GateState::Pass` and the field's text must still be buffered.
  3. In `classify.rs` add one exhaustive match (no wildcard) that says whether a retry can fix
     each reason. In `Saves::reply` put a for-this-open refusal into
     `FieldSave::Held(Hold::RefusedForThisOpen(..))` with `send_on_open = true`, as the
     `ReadOnly` arm does at about 971-973.
  4. Dropped links: if T has merged `DroppedLinks`, use it. Otherwise add the dropped list to
     the verse-save reply in tc-store (`commands.rs` `fn reflow`) and to tc-api's `Ack`
     (`crates/tc-api/src/write.rs` about 19), and add a `note` log line to T's card saying so.
     Show the list in Translate after a save; draw `AlignerView.dropped` in the Align session.
     New strings go in `crates/tc-ui/assets/i18n-rebuild/`.
  5. Test vector (tc4-coverage F7, tC4 #820): a verse "Dios Dios" with one copy linked, saved
     as "Dios Pablo". Second case: the TIT 1:1 setup of `docs/TC4-ISSUES.md` §1 row 10 (#789).
  6. #833.1: red first `an_offline_update_check_shows_one_plain_sentence` (Door43 at an address
     nothing listens on, like `DEAD_DOOR43` in `parity/app/env-rust.mjs` about 69). Then map
     the error at `crates/tc-app/src/upgrades.rs` about 729-731 to the failure enum and show
     its sentence in `crates/tc-ui/src/app/upgrades.rs` `check_failed` (about 185).
- Files: `crates/tc-ui/src/app/save/{classify.rs,mod.rs}`, `crates/tc-api/src/write.rs`,
  `crates/tc-store/src/commands.rs` (only if T has not landed),
  `crates/tc-ui/src/views/align/session.rs`, `crates/tc-ui/src/views/translate.rs`,
  `crates/tc-app/src/upgrades.rs`, `crates/tc-ui/src/app/upgrades.rs`, the test files.
- Done when: `cargo test -p tc-ui --test app -- refused_for_good` and
  `cargo test -p tc-app --test app -- link_sent_back offline_update_check` pass, and each new
  test fails with its fix stashed; `the_gate_resolves_false_while_a_failure_stands_after_one_retry`
  and `a_held_field_does_not_block_the_gate` (`crates/tc-ui/tests/app/save.rs`) still pass.
- Record: log line `step 2` with the refusal table and the failing outputs; `next` to step 3.

### Step 3: Make cloud-folder detection exact; read OBS questions leniently

- Do:
  1. Check that no claimed storage card holds `crates/tc-store/src/platform.rs`:
     `grep -l 'platform.rs' .audit/roadmap/units/*.md`, then read each match's `state` and
     `owner`. If one does, do part 4 first and stop and ask for the rest.
  2. Red first: `a_dropbox_settings_folder_in_home_never_makes_a_project_read_only` in
     `crates/tc-store/tests/store/platform.rs`; register it in `crates/tc-store/tests/store.rs`
     with `#[path = "store/platform.rs"] mod platform;`. Cover Linux and macOS (`~/.dropbox`
     holding `info.json` is the settings folder), Windows (`%APPDATA%` and `%LOCALAPPDATA%`
     `Dropbox\info.json`), and a root holding a `.dropbox` marker, which does count.
  3. Make tc-store's `cloud_client` take the environment as a value (a small struct) with the
     rule of `crates/tc-app/src/folders.rs` about 99-105. `StdPlatform::folder` passes the
     process environment. tc-app's `HostFolders` calls tc-store's function. Delete tc-app's
     copy and point `crates/tc-app/tests/app/folders.rs` at the new function.
  4. OBS questions: skip if DOM-2's card says it moved the reader. Otherwise move
     `parse_story_questions` (`crates/tc-obs/src/questions.rs` about 40) into tc-helps (both
     tc-obs and tc-resources depend on tc-helps), give `HelpsPart::ObsQuestions` its own arm
     in `load.rs`, and delete the old copy. Red first:
     `one_bad_obs_question_row_hides_no_other_question`.
- Files: `crates/tc-store/src/platform.rs`, `crates/tc-store/tests/store.rs`,
  `crates/tc-store/tests/store/platform.rs`, `crates/tc-app/src/folders.rs`,
  `crates/tc-app/tests/app/folders.rs`, `crates/tc-resources/src/load.rs`,
  `crates/tc-resources/tests/load.rs`, `crates/tc-helps/src/tsv.rs`, `crates/tc-obs/src/questions.rs`.
- Done when: `cargo test -p tc-store --test store -- platform` and
  `cargo test -p tc-resources --test load -- obs_question` pass and fail with the fix stashed;
  `grep -n 'fn dropbox_roots' crates/tc-app/src/folders.rs` prints nothing.
- Record: log line `step 3`; `next` to step 4.

### Step 4: One drafted rule, after the owner answers H12

- Do:
  1. `ls .audit/roadmap/rulings/ | grep H12`. If nothing prints, add a `note` log line
     ("step 4 waits for H12"), do step 5, and come back when the ruling exists.
  2. Red first, in `crates/tc-app/tests/app/check.rs`: open a Psalm verse whose words sit on a
     `\q1` line (`fixtures/tc4/test-fixtures/en_ult/PSA-3.usfm`) and assert what the ruling
     says Check shows.
  3. With "yes, everywhere": make Check (`grep -n is_drafted crates/tc-app/src/check/item.rs`),
     the store (`grep -n is_drafted crates/tc-store/src/commands.rs`) and the fold
     (`grep -n is_drafted crates/tc-journal/src/fold/project.rs`) use the words-on-any-line
     rule `words_drafted` (`crates/tc-app/src/drafted.rs` about 75). Move that rule to one
     crate all three depend on.
  4. Rename what is left so the two rules cannot be mixed up: `tc_usfm::codec::is_drafted`
     (`crates/tc-usfm/src/codec.rs` about 239) gets a name that says it is tC4's indexer rule,
     or is deleted if nothing calls it.
  5. The store and the fold are storage-lane files. If a claimed storage card holds them, stop
     and ask.
- Files: the files above, `docs/PLAN.md` (the drafted bullet, about 146-148).
- Done when: the new Check test passes and fails with the fix stashed;
  `grep -rn 'is_drafted(' crates --include=*.rs` prints only the renamed rule's callers.
- Record: log line `step 4`; `next` to step 5 (or back to the open step).

### Step 5: Write the orphaned rows' tests and reconcile docs/TC4-ISSUES.md

- Do:
  1. Write each missing test, red first where the behavior is missing; where it is right,
     show it fails when the behavior is broken on purpose:
     #576 `last_edited_names_the_span_then_its_first_verse` (`crates/tc-app/tests/app/home.rs`);
     #731 `feedback_attachment_is_english_and_explained_in_the_app_language` and
     `platform_line_names_the_architecture` (`crates/tc-app/tests/app/feedback.rs`);
     #599 `switcher_stays_centred_while_the_save_indicator_changes`
     (`parity/app/journeys/shell.test.mjs`, within 1 px, en and ar);
     #795 `an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen`
     (`crates/tc-app/tests/app/align_store.rs`);
     #612 a letter-spacing assertion for Hindi on Home and the top bar
     (`parity/app/journeys/app-language.test.mjs`; reuse the scan at about 264-276);
     #831 `resume_card_names_the_verse_of_its_text_and_never_shows_the_stub`
     (`crates/tc-app/tests/app/home.rs`; C2 is built, so no other card writes it);
     #833.5 `source_texts_lists_the_installed_version` (`crates/tc-app/tests/app/sources.rs`;
     B9a is built, so no other card writes it).
  2. Move rows you do not build here, each with a `note` line in the receiving card's log if it
     is unclaimed: #613 `hindi_label_renders_in_the_bundled_face_offline` to G1 (no Devanagari
     face is vendored: `grep -ic deva crates/tc-ui/vendored-from-tc4.txt` prints 0; do not
     fetch one); #278 `about_commit_matches_the_release_record` to G3; #763 and Q31
     `help_home_shows_no_rc1_build_text_offline` to REF1.
  3. Fix the names: #507's journey is
     `j25_a_story_opens_with_its_rail_its_units_and_writable_comment_boxes`
     (`parity/app/journeys/story.test.mjs` about 26).
  4. Set each "rebuild bug" row fixed by a finished unit to "covered" with its test. Set each
     "unit todo" row that names only built units to "covered" with its test, or to the card
     that now owns it. Recount the Counts table (about lines 29-38) and date it.
  5. Add the PLAN bullets from the Prose line. Tell the owner, in two plain sentences, which
     tC4 bugs the rebuild now fixes on purpose.
  6. SWEEP edits the same two files. Whichever merges second rebases and recounts.
- Files: the test files above, `docs/TC4-ISSUES.md`, `docs/PLAN.md`.
- Done when: for every row you changed, `grep -rn '<its test name>' crates parity` finds the
  test; CHK-3's TC4-ISSUES test passes if CHK-3 has landed; `grep -c '| rebuild bug |'
  docs/TC4-ISSUES.md` prints 0, or each remaining row names a card.
- Record: log line `step 5`; set `state: verifying` and `next` to the review.

## Gates before merge

The full gate list: `scripts/gates.sh` once INT-3 lands. Until then, each must pass:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and
  again with `--features gallery`, `--features app` and `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`
- `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`

Plus: the journeys in `align.test.mjs`, `understand.test.mjs`, `shell.test.mjs`,
`app-language.test.mjs` and `translate.test.mjs`, run as step 1 says, with the summary line in
the log. This item is no-loss: the review uses three lenses (FUNNEL.md §8).

## Stop and ask when

- A claimed storage or domain card holds `crates/tc-store/src/platform.rs`,
  `crates/tc-store/src/commands.rs`, `crates/tc-journal/src/fold/project.rs` or
  `crates/tc-resources/src/load.rs` when you need it.
- The list of sent-back links needs more than a notice line in Translate and a list in Align.
  Ask the owner how it should look, with a picture.
- A gate fails twice in a row and you cannot tell why.

## References

- `.audit/roadmap/evidence/2026-10-21/tc4-bugs.md` T2, T3, T4, T5, T7, T11, T15, §4, §6
- `.audit/roadmap/evidence/2026-10-21/tc4-coverage.md` F7, F12; `app-remaining.md` F10;
  `health-app.md` CH-11; `health-domain.md` F5
- `docs/APP-DESIGN.md` R28 (about 3280) and risk 23 (about 3541)
- `docs/TC4-DELTA-2026-10-19.md` Q30, Q31, Q36
- tC4 `src/state.jsx` 2144-2159 (`projectPresentation`, the direction rule); tC4 commit
  095144ce (#801)

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/tc4-bugs.md
- 2026-10-23 10:40 EDT | C7a session (Claude Code 3058c665) | claim | none | claimed on track-FIX-R from main d293375a (INT-4 landed) while C7a's gate waits for a heavy slot; every "Before you start" line prints as the card says, except that `word_card` takes `rtl_orig: bool`, not a direction | .worktrees/track-FIX-R
