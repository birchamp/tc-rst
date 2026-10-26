---
id: FIX-R
title: Fix the tC4 bugs the rebuild repeats; reconcile TC4-ISSUES
track: A
lane: app
milestone: M1
state: verifying
owner: shell lane (Claude Code 6f39d119)
branch: track-FIX-R4
worktree: .worktrees/track-FIX-R4
depends: [INT-4]
needs_owner: []
later: []
proves: [P1, P8]
no_loss: true
size: 2-3 d
merged: none
updated: 2026-10-26
next: ready for a batch on track-FIX-R4 (step 4, its own small merge; steps 1-3 and 5 are on main). Then done.
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
- 2026-10-23 11:40 EDT | C7a session (Claude Code 3058c665) | note | none | step 1, the Rust side: `tc_app::direction::{project_direction, script_direction, of_fold}` is the one reader (card_facts, the open reply, Align and both Settings paths call it; `ProjectPeek.text_direction` carries settings.json's field to Home's card); Settings writes `languages[0].scriptDirection` through the new `StoreCommand::set_meta` before `textDirection`; Settings shows the metadata's language name when settings.json names none; the aligner's card body takes the translation's `dir`, and each question, note and word text reads `dir="auto"`. The two tests live beside the align store rig (`crates/tc-app/tests/app/align_store.rs`), not in create.rs: the store world's sample names its language "English" and holds no original-language resources, so Align cannot open there. Proofs: with Settings' metadata write and the name fallback cut, `a_direction_changed_in_settings_shows_on_home_and_in_every_tool_after_restart` failed `Home's card in this app: left: Ltr right: Rtl` and `an_imported_project_shows_its_metadata_language_name_in_settings` failed `left: "" right: "Español (Latinoamérica)"`. `settings_from_home_mints_only_for_shown_values` now injects the arriving change into the metadata field, since `textDirection` alone no longer changes a direction the metadata names. The two journeys are written and wait for a heavy slot | cargo test -p tc-app --test app -- direction_changed language_name
- 2026-10-23 12:35 EDT | C7a session (Claude Code 3058c665) | step 2 | none | Refusal table (does the same text in this open fail the same way?): Disk yes-a-retry-can-fix; Refused(InvalidText) fixable by the person in the field, so it stays Failed with its reason; Refused(IncoherentView), Refused(LastBook), LookAgain, PlanStale, BadToken, StaleScope can never be fixed by a retry (StaleScope is re-attached before it reaches here); ReadOnly is held as before. `classify::retry_can_fix` holds this table as one match with no wildcard; `Hold::RefusedForThisOpen(NotSaved)` keeps the text for the next open's offer and the gate passes (`Hold` lost `Copy`; nothing relied on it). Align draws `AlignerView.dropped` as `align-dropped` with the words sent back. #833.1: `UpdateCheckFailure` (Unreachable, Refused, Server) from one exhaustive match on `Door43Error`, three fixed sentences, also for `texts_error`. Proofs: hold arm cut → `left: Some(Failed(NotSaved(BadToken))) right: Some(Held(RefusedForThisOpen(BadToken)))`; mapping cut → `left: ... { why: Server } right: ... { why: Unreachable }`. Not done: the verse save's dropped links (Translate's half and its reply test `a_link_sent_back_to_the_bank_is_listed` in tc-app) need tc-store's `reflow` to return them, and S2 (verifying, storage lane) holds commands.rs: waits for S2, per "Stop and ask". The Align journey of the same name is written | cargo test -p tc-ui --test app -- refused_for_good; cargo test -p tc-app --test app -- offline_update_check
- 2026-10-23 13:30 EDT | C7a session (Claude Code 3058c665) | interrupt | none | usage limit. Step 3 part 3 done: tc-store's `platform::synced_by(env, path)` is the one cloud rule (moved from tc-app, which now calls it; tc-app's copy deleted); `a_dropbox_settings_folder_in_home_never_makes_a_project_read_only` passes, and with the old marker rule failed `a project under home is not synced: left: Some("Dropbox") right: None`; tc-app's 10 folder tests pass. Part 4 written: the lenient OBS reader moved to `tc_helps::tsv::parse_obs_questions` (tc-obs re-exports it), the loader has its own OBS arm and no wildcard; its test is not finished (see next) | cargo test -p tc-store --test store -- platform
- 2026-10-23 16:20 EDT | C7a session (Claude Code 3058c665) | step 3 | none | finished: `one_bad_obs_question_row_hides_no_other_question` passes (its edited table carries its own size and md5 in the export's metadata); with the loader's OBS arm back on the strict reader it failed `Missing(Unreadable { ..., problem: "translation questions TSV line 674 has 8 columns; the header has 7" })`. tc-resources, tc-obs and tc-helps tests pass; clippy passes; `grep -n 'fn dropbox_roots' crates/tc-app/src/folders.rs` prints nothing | cargo test -p tc-resources --test load -- obs_question
- 2026-10-23 16:20 EDT | C7a session (Claude Code 3058c665) | note | none | step 4 waits for H12: `ls .audit/roadmap/rulings/ | grep H12` prints nothing | ls .audit/roadmap/rulings/
- 2026-10-23 17:10 EDT | C7a session (Claude Code 3058c665) | note | none | step 5 part 1: #576 and #831 were bugs, fixed in the one recorder (`record_translate_edit` names a verse by the span it now holds; a stub is no snippet; a section save passes its first verse's stored words, not the section text). Red first: `last_edited_names_the_span_then_its_first_verse` failed `a join names the span: left: "9" right: "9-10"`; `resume_card_names_the_verse_of_its_text_and_never_shows_the_stub` failed with snippet "9 10 no defraudando sino mostrando toda buena fe"; both pass now. #731 is covered by existing tests under other names (`the_attachment_is_english_in_every_app_language`, `platform_line_names_the_os_and_the_architecture`, which failed `left: "Linux Win32"` with a tC4-style line). Step 1's Settings change updated two sources tests to the one-direction rule and the metadata language name (declared differences #832.1, #832.2). All 672 tc-app app tests pass | cargo test -p tc-app --test app
- 2026-10-23 17:50 EDT | C7a session (Claude Code 3058c665) | note | none | step 5 part 2: #795 `an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen` (align store rig) passes; it asserts the fold's own rule (the alignment's recorded `target_verse_md5` equals `verse_text_md5_v1` of the stored verse) as well as the rail. Proof: the aligner's `TargetVerse::new` hashing the NFD form failed `one hash for the aligner and the alignment files: left: Md5(62f5…) right: Md5(8e12…)`; two weaker mutations (the store keeping NFD; the aligner's word NFC) did not fail it, as the shared `TargetText` makes them agree. #833.5 needs no test: the rebuild's Get source texts lists no version, only "n installed", so tC4's wrong-version label cannot occur; the row says so | cargo test -p tc-app --test app -- an_nfd_typed
- 2026-10-23 18:30 EDT | C7a session (Claude Code 3058c665) | note | none | step 5 part 3: docs/TC4-ISSUES.md reconciled (a subagent's pass, checked here): rebuild bug → covered 14 (fixed by the earlier FIX-text, FIX-align, FIX-misc, FIX-helps merges), unit todo → covered 28, two rows "done" → covered; #613 → G1, #278 → G3, #763 and Q31 → REF1, #796 (review rule) and #179 → T; test names fixed (#795, #507, #612, #765, #378); Counts recounted 2026-10-23: `| All area rows | 1 | 52 | 9 | 147 | 2 | 60 | 271 |`; the one remaining rebuild bug (#796, review rule) names T. Check: every test a covered row names exists but three that predate this pass (#529 and #646 name card I's tests, #612 a test file). Left undecided: #569, #505 (A5 built; no test or card names them). FIX-R's own fixes (#830, #832, #836, #801, #833.1, R28, OBS questions, Q30 (a)) have no rows in TC4-ISSUES; their tests are named in PLAN's new section and this log. PLAN gains "tC4 4.0 bugs the rebuild fixes on purpose"; `cargo test -p tc-ui --test screen_record` passes | docs/TC4-ISSUES.md; docs/PLAN.md
- 2026-10-23 19:20 EDT | C7a session (Claude Code 3058c665) | note | 54216164 | journeys: align, understand, shell, app-language and translate run once: 62 tests, 58 pass, 4 fail (.enjoy-logs/fixr-journeys.log). Two were mine: the align routes edited the `Opened` enum's envelope, not the view inside `ready` (so #830's journey passed without testing anything), and the en-pins scenario installs no questions (slot `fetch`); fixed in 54216164, the routes edit `reply.value.value` and answer a ready questions slot. The three FIX-R UI journeys pass, and each failed on the UI of main d293375a: `placed words read in the translation's direction: rtl rtl rtl rtl rtl rtl`, `align-dropped` never shown, `the English question reads left to right`. `switcher_stays_centred_while_the_save_indicator_changes` (en, ar) and `hindi_home_and_top_bar_keep_normal_letter_spacing` passed. The other two failures, `j15_the_overlay_shows_at_300_ms_of_a_held_open_with_no_event` (`shown 816 ms after the click`) and `a_quick_open_never_shows_the_overlay`, are wall-clock assertions on a loaded machine, which the 2026-10-23 ruling (tests prove behavior, not speed) leaves to CHK-4; FIX-R touches no open overlay code | node --test parity/app/journeys/{align,understand,shell,app-language,translate}.test.mjs
- 2026-10-23 20:40 EDT | loss lens (Claude Code subagent of 3058c665) | verify | 7be0b74e | Code: one direction reader, one cloud rule in tc-store, an exhaustive refusal table; the held state is a guard, not a classification. Checks: the #801 test covers BadToken only; the save model generates only Disk failures; nothing checks the languages record's other fields or a half-written Settings save. Reach: "look again" on a field save after a second sync over a merged verse. Siblings: aligner saves keep their own failure path. No loss: no path loses text; the held text stays in localStorage and the mirror. Prose: the hold's doc overstates the next open. Verdict: no blocker; majors: (1) a "look again" text can never be stored, even by Keep at the next launch (tc-store `displayed()` answers LookAgain over moved multi-head tokens); (2) a held refusal shows as plain Unsaved; (3) no model case for a for-good refusal | the lens's report, quoted in this line
- 2026-10-23 20:40 EDT | races and protocol lens (Claude Code subagent of 3058c665) | verify | 7be0b74e | Code: `retry_can_fix` names every variant; Settings' direction is two commands; the StaleScope arm cannot run. Checks: the #801 test never exercises the gate's retry; no test types after the hold. Reach: LookAgain after a second sync or a second tab over a merged verse; BadToken and IncoherentView only by bugs. Siblings: Api and Transport failures still block; `pump_draft` and `worst()` miss the new hold. No loss: no double write, no loss in retry, lost-reply or StaleScope paths; Home and Settings read both files in one peek; the Resume record reads a snapshot that holds the save. Prose: a displaced doc comment. Verdict: one major, a held field parks later typing silently and is resent to the open that refused it; minors | the lens's report, quoted in this line
- 2026-10-23 20:40 EDT | browser and parity lens (Claude Code subagent of 3058c665) | verify | 7be0b74e | Code: one direction reader, an exhaustive update-check mapping, an exhaustive refusal table; an invalid `scriptDirection` wins over textDirection unlike tC4. Checks: the three UI journeys failed on main's UI; no journey covers notes or answers; #599 and #612 journeys lack a break-on-purpose proof. Reach: an Arabic set in an English UI and the reverse. Siblings: short notes and the answer line missed `dir="auto"`. No loss: not this lens. Prose: a wrong claim about tC4 in settings.rs, PLAN overclaims the save half of Q30 (a). Verdict: majors: (1) the answer line takes the interface label's direction; (2) short notes have no `dir="auto"`, Check notes lay out unevenly; (3) "Sign in again" on an anonymous update check; (4) a for-good refusal shown only as Unsaved | the lens's report, quoted in this line
- 2026-10-23 21:10 EDT | C7a session (Claude Code 3058c665) | fix | none | the held state is gone: a for-good refusal stays `Failed` with its reason, Retry and drafts as before, and only `gate()` changed: after its one retry it blocks only on a failure a retry can fix (`classify::save_error_can_be_fixed`); this answers races 1, loss 2, browser 4, and the draft and resend minors. `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` now uses LookAgain and the gate's retry; with the gate change cut it failed `the gate lets the person go`. The save property test generates LookAgain refusals (loss 3); 3 property tests pass, 388 tc-ui app tests pass
- 2026-10-23 21:10 EDT | C7a session (Claude Code 3058c665) | fix | none | browser 1 and 2: the answer's `dir="auto"` is on the answer text, not the line with the interface label; both branches of `expandable_note` (short and long, Understand and Check) carry `dir="auto"`. Browser 3: "Door43 turned the request away. Try again later."; a 404 maps to "did not answer properly". An invalid `scriptDirection` no longer counts, as in tC4 (`state.jsx` 1815-1831, the module's citation corrected). The wrong tC4 claim in settings.rs is corrected and PLAN's Q30 (a) bullet says the Translate half waits for S2; the displaced doc comment is back on `setting_text`. Clippy: native and four wasm runs pass
- 2026-10-23 21:10 EDT | C7a session (Claude Code 3058c665) | note | none | loss major 1 is not FIX-R's to fix: tc-store `displayed()` (commands.rs about 105-118) answers LookAgain to an edit over a merged verse whose heads moved, so that text can never be stored, before and after FIX-R; the no-loss rule wants it kept as a conflict. commands.rs is S2's; the finding is handed to the storage lane (s2-proof) and logged as a minor naming S2 and T. FIX-R now keeps such a text with its reason and lets the person go Home
- 2026-10-23 22:10 EDT | C7a session (Claude Code 3058c665) | note | 9e9cc08a | after the review fixes, understand, check-detail, check-keyboard, check-picker, check-writes, community-checking and shell journeys: `# tests 55 # pass 55 # fail 0 # skipped 0` (.enjoy-logs/fixr-journeys2.log). Every blocker and data or view major of the three lenses is fixed or handed on (the store's LookAgain to S2/T, logged); FIX-R waits for its merge turn from the orchestrator. Step 4 waits for H12; step 2's Translate half waits for S2 | node --test parity/app/journeys/{understand,check*,shell}.test.mjs
- 2026-10-23 23:43 EDT | checks lane (Claude Code 2198ee19) | note | none | From CHK-3 (claims ledger): docs/TC4-ISSUES.md rows #795, #507 and #765 now read (pending: FIX-R), and 29 unit-todo rows that wait only on finished units end their unit cell with "FIX-R reconciles". .audit/tools/claims.test.mjs fails once FIX-R is done if any of them still waits: write each test and replace the marker with its name in the same commit (step 5) | node --test .audit/tools/claims.test.mjs
- 2026-10-24 00:20 EDT | C7a session (Claude Code 3058c665) | ruling | none | H12 answered "poetry - yes, everywhere" (rulings/2026-10-23-H12.md). Step 4 must change the Check reader, the store (commands.rs about 767) and the fold (project.rs about 288) together: Check alone would offer a check whose decision the store refuses. S2 holds commands.rs (and changes it heavily) until it merges, so step 4 follows S2 as its own small merge; `later` now names S2 from step 4 | .audit/roadmap/rulings/2026-10-23-H12.md
- 2026-10-24 01:10 EDT | C7a session (Claude Code 3058c665) | fix | none | Q37 (rulings/2026-10-23-Q37-needs-review.md, on track-SWEEP): an edit in Align keeps "needs review" and the draft-changed notice; only Mark valid clears them. `settle` keeps `marked_invalid` and leaves `done` off while it is set; `edit()` no longer resets `stale`. Red first: `an_edit_keeps_the_review_mark_until_mark_valid` failed `an edit keeps the mark`; with the old clear put back, `a_record_flagged_for_re_review_opens_as_it_is_and_keeps_the_flag_until_mark_valid` fails at aligner.rs:349. tc-align passes, 37 tc-app align tests pass; TC4-ISSUES' #796 row now states the ruling | cargo test -p tc-align
- 2026-10-24 01:30 EDT | C7a session (Claude Code 3058c665) | test | none | Q38 (rulings/2026-10-23-Q38-raw-rc-links.md, on track-SWEEP): keep-it test `a_bare_rc_link_shows_as_its_raw_text` (tc-helps blocks.rs): a bare rc link and a `[[rc://…]]` link show as their raw address, the brackets dropped as tC4's own renderer drops them (the vectors' Titus intro holds one). A first version that expected the brackets kept failed, so the test reads the renderer's output; no code change was needed. TC4-ISSUES' two #628 rows now state the ruling | cargo test -p tc-helps --test blocks
- 2026-10-24 | C7a session (Claude Code 3058c665) | fix | none | the checks lane's c3_reload case (shared note 2026-10-24-c3-story-ref-retry.md; seed `kind = Reference, steps = [Type(2), Blur, Type(0), Deliver(false), Blur, Type(2), Cut]`) failed at save_machine.rs:1224 here too. No loss: the first save stored "é" and its reply was lost, the empty text was refused, and the person typed "é" again, which the store already holds; Retry rightly sends nothing and lets the field go as saved (`retry_field` on a field that is not dirty). The test wanted a resend in every case; it now wants none, and the field Clean, when the store holds the correction, and a fresh-request resend otherwise. The seed is kept in save_machine.proptest-regressions; the save machine tests pass with PROPTEST_CASES=256 | cargo test -p tc-app --test app save_machine
- 2026-10-24 | C7a session (Claude Code 3058c665) | merge | e5765cfa | steps 1-3 and 5 with Q37, Q38 and the save-machine test fix merge into main on the orchestrator's turn (main 0189cee4 merged in; TC4-ISSUES conflicts resolved to FIX-R's reconciled rows, five rows fixed for the claims ledger, #505 kept as "FIX-R reconciles"). The card stays open for step 4, so `merged` stays none (board: merged needs done) | scripts/gates.sh --wait-ci FIX-R
- 2026-10-26 10:15 EDT | shell lane (Claude Code 6f39d119) | takeover | none | took the card over from the C7a session, whose claim is 1.8 days stale (orchestrator's word); step 4 on track-FIX-R4 from main 1a0c8469, where S2 is merged (33d2378d) and no claimed storage card holds commands.rs or fold/project.rs | .worktrees/track-FIX-R4
- 2026-10-26 10:50 EDT | shell lane (Claude Code 6f39d119) | step 4 | fc057ed9 | one drafted rule: `tc_usfm::codec::words_drafted` (moved from tc-app, which re-exports it) decides for Check's reader (check/item.rs), the store's decision judge (commands.rs) and the fold's check texts (fold/project.rs), and for outside.rs's copy; tC4's rule is `tc4_indexer_drafted`, called only by the two tests that record tC4 (derivations.rs, tests/app/drafted.rs). `grep -rn 'is_drafted(' crates --include=*.rs` prints nothing. The test is `a_verse_on_poetry_lines_is_drafted_in_check_and_its_mark_is_saved` in `crates/tc-app/tests/app/check_item.rs`, not check.rs: the store rig with `save_verse` and `decide` lives there. Red before the fix: `Check shows the poem's words: TargetWords { words: [], selected: [], verse: None }  left: None right: Some("Pablo")`. STORAGE §7.7 and §16's crate table, and PLAN's drafted bullet, say the one rule | cargo test -p tc-app --test app -- poetry_lines drafted check_item
- 2026-10-26 11:00 EDT | loss lens (subagent of the shell lane) | verify | fc057ed9 | no findings. 1 one predicate, tC4's renamed apart; 2 the poem round trip through Check, the store and the fold is tested; 3 a Psalm on \q lines and prose edited into poetry are reachable; 4 every drafted caller agrees (Home, Translate, export, Align already used the words rule); 5 decision records are never rewritten: `assess` is pure, so a verse that becomes drafted changes only derived flags, and a decision whose selected words are gone is flagged with them kept (§7.7); 6 the prose declares the difference | git show fc057ed9
- 2026-10-26 11:05 EDT | browser and parity lens (subagent of the shell lane) | verify | fc057ed9 | no blocker, no major, five minors. tC4 (d310bf3) reads the indexer body for Check (`verseTextIndex`, state.jsx:352), Home (`draftPercent`, indexer.ts:218) and the aligner (state.jsx:1183), so all three show a poem verse as not drafted; Check draws `data-drafted="0"` with no words to tap. The page needs no change: tc-ui draws `data-drafted` from the words the server sends (views/check/detail.rs). Minors: STORAGE said tC4's Home counts such a verse; PLAN said tC4's Check leaves it out; APP-DESIGN's declared difference named the tiles only; the Align half was declared nowhere; no journey pins it | /home/bot/unfoldingword/tc4-ref
- 2026-10-26 11:10 EDT | races lens (subagent of the shell lane) | verify | fc057ed9 | no findings. 1 the verse token is checked before any text is read (commands.rs `ctx.edit(verse_shown)`), so a Translate save after Check showed the item is LookAgain whatever the draftedness; 2 Check, the store and the fold read the same record's words with one predicate; 3 nothing caches or counts by draftedness (`verse_edit_reassesses_its_checks_only` counts checks); 4 a teammate's poem verse re-projects through the same `check_texts`; 5 no fix; 6 a decision stamped md5("") on an undrafted verse that later reads as a poem shows "verse edited", which is §7.7's rule for a changed verse | cargo test -p tc-app --test app -- poetry look_again verse_changed
- 2026-10-26 11:11 EDT | shell lane (Claude Code 6f39d119) | fix | none | the parity lens's prose minors: STORAGE §7.7 says tC4's Check, Home and aligner all read the indexer body; PLAN's bullet names Home, Check and Align and what tC4 shows; APP-DESIGN's summary bullet names the one rule and all of tC4's readers. The journey is a minor in .audit/minors.tsv. The H12 ruling's option text has the same wrong claim about tC4's Home; rulings are the owner's record, so it is left | docs/PLAN.md
- 2026-10-26 11:32 EDT | shell lane (Claude Code 6f39d119) | fix | none | the crate run found `edits_joins_renumbers_and_pins_change_files_and_screens_alike` (tc-journal screens_and_fold.rs) asserting tC4's rule: its model of Check's draft index trimmed tC4's indexer body, and 1:2 (`\q1` on its own line) and 1:3 (the stub then a `\q2` line) had no text. The model now keeps tC4's index keys and decides drafted by the words on every line (H12), and 1:2 and 1:3 have their text; the five screens-and-fold tests pass. `the_no_text_rule_reads_the_body_tc4s_indexer_reads` (shared_rules.rs) asserted the same old rule; it is now `the_no_text_rule_reads_the_words_on_every_line`, and the poem, stub-then-poem and blank-first-line verses are judged like the others. tc-usfm, tc-journal, tc-store and tc-app tests pass (tc-app 695), and clippy -D warnings on the four crates | cargo test --no-fail-fast -p tc-usfm -p tc-journal -p tc-store -p tc-app
