---
id: T
title: The storage side of the tC4 issue sweep (what is left)
track: A
lane: storage
milestone: M2
state: gating
owner: S2 session (Claude Code add49644)
branch: track-T
worktree: /home/bot/tc-rst/.worktrees/track-T
depends: [S2]
needs_owner: []
proves: [P3, P10]
no_loss: true
size: 0.5-1 d (basis: 3ecdcae did two T items in one commit, +395 lines)
merged: none
updated: 2026-10-24
next: run scripts/gates.sh in .worktrees/track-T, then the three verify lenses, then wait for the orchestrator's merge turn.
blocked_by: none
---

# T: The storage side of the tC4 issue sweep (what is left)

## Goal

Most of T landed through other units (#582 and #767 in 3ecdcae, #789's reflow, #386). Left: the
alignment files and Align's rail judge "needs review" by one rule; an empty OBS frame's edit and
the `relationships` mirror write tC4's bytes; #747, #161, #179, #794, #812 and #820 get their tests;
and a link a Translate save cannot follow comes back in the save's reply (Q30 (a)).

## Why it matters to a translator

Today an alignment file can say "valid" where Align says the verse needs review (168 of 357
alignments in the Psalms corpus), and a link a verse edit drops goes back to the bank without the
list the owner asked for. Steps 1 to 3 change derived bytes: they must land before V freezes
format 1.0 (STORAGE §12.2 rule 3).

## Before you start

Lines in `crates/tc-store` tests and in `scan.rs`, `open.rs` are from `s2-proof-executor`
(3b075d7); all other lines are from `resume-build-lane` (f9067bc; main after INT-4). S2 changed
`tc-journal` and `tc-store`, so post-S2 main differs: find each place by name.

- `python3 -I .audit/tools/board.py` lists S2 as `done`.
- `grep -n "words_not_the_verse, 168" crates/tc-journal/tests/screens_and_fold.rs` prints one
  line (`:918` at f9067bc).
- `grep -rn "fn obs_frame_edits_project_other_bytes_than_tc4_writes_today" crates/` prints
  `crates/tc-app/src/backend/seed/tests.rs:723` (or a `crates/tc-import` path if I moved the seed).
- `grep -n "reflow_edit(&value, &before, &target).ok()?" crates/tc-store/src/commands.rs` prints
  one line (`:349` at f9067bc): there `Reflowed::dropped` is thrown away.
- `.audit/roadmap/units/FIX-R.md` owns the Align half of step 5 (showing the list).

## Funnel card

- **Invariants**:
  - INV-ONE-REVIEW-RULE: the alignment files' `invalid` and Align's rail decide "needs review"
    with one function; with no pinned source given, they agree for every alignment.
  - INV-A-DROPPED-LINK-IS-LISTED: every link a verse or section save cannot follow is in that
    save's reply, as a typed list; no code path discards it.
  - INV-DERIVED-BYTES-EQUAL-TC4: a frame edit and the `relationships` mirror project the bytes
    tC4's `writeFrame` and `relationshipsFromPins` make.
- **Wrong states**:
  - The fold's `invalid` reads only staleness (`value.is_valid`,
    `crates/tc-journal/src/fold/project.rs:782`); `tc_align::needs_review`
    (`crates/tc-align/src/value.rs:108`) also refuses a word the verse lacks (TC4-ISSUES row 6).
  - `splice_story` (`project.rs:2142`) writes `\n\n\n\nTexto nuevo.\n` into an empty frame where
    tC4 writes `\n\nTexto nuevo.\n\n`, so the file reads back as reformatted (seed-check §5.2).
  - `relationships` (`crates/tc-journal/src/fold/metadata.rs:136`) puts the extra scripture last
    (`GROUP_ORDER`, `:177`) and dedups whole rows (`:320-325`); tC4 puts it second and keeps the
    first row per repository (tC4 `journal/relationships.mjs:24-42`).
  - `reflow` (`crates/tc-store/src/commands.rs:302`) drops `reflowed.dropped` for the verse save
    (`:280`) and the section save (`:527`).
- **Code**:
  - Types (to create): in `crates/tc-journal/src/alignment.rs`, `verse_words(content) ->
    Vec<TargetWord>` and `AlignmentValue::needs_review(&self, at: &AlignedRun, verse:
    &[TargetWord]) -> bool`; in `crates/tc-store/src/reply.rs`, `DroppedLinks { book, chapter,
    verse, links: Vec<tc_align::DroppedLink> }` and `StoreReply.dropped_links`.
  - One entry point: `AlignmentValue::needs_review`; `tc_align::needs_review` calls it and adds
    only the pin check; the fold calls it instead of `is_valid`.
  - Removed: the fold's `is_valid` call; the `.ok()?` at `commands.rs:349`; whole-row dedup.
  - Left to checks, and why: the pin half stays in tc-align, because the fold holds only the
    source's sha (`ol_sha`), not its words; byte layouts have no type, so tests compare them
    with tC4's writers.
- **Checks** (all in the `rust` job, `cargo test --workspace`):
  - `the_psalms_corpus_files_are_what_the_screens_compute` (exists, `screens_and_fold.rs:901`)
    asserts 0; `the_alignment_files_and_the_rail_agree_on_invalid` (to create); kind:
    differential.
  - `an_empty_frame_is_spliced_as_tc4_writes_it` and
    `relationships_follow_tc4_order_and_keep_the_first_row_per_repository` (to create,
    `crates/tc-journal/tests/projection.rs`); `obs_frame_edits_project_the_bytes_tc4_writes`
    (renamed from the tc-app test above); kind: unit.
  - `a_translate_save_that_drops_an_unclear_link_lists_it`,
    `a_section_save_that_drops_an_unclear_link_lists_it` (to create,
    `crates/tc-store/tests/store/reflow.rs`); kind: unit through the public API.
  - Step 4's tests and proptest.
  - Proof it can fail: each step names the revert or mutation; quote its failure in the log.
- **Prose**: `docs/TC4-ISSUES.md` rows (step 6).
- **Human**: none.

## Steps

### Step 1: Make the fold and the rail use one review rule

- Do:
  1. Red first: set `:918` to expect `0`, and add `the_alignment_files_and_the_rail_agree_on_invalid`
     to `screens_and_fold.rs`: a Titus record stamped with the current hash that holds "siervo"
     for "Pablo, esclavo de Dios" renders `invalid: true`, and `verse_status` says Invalid. Quote
     the failure.
  2. Add `verse_words` (the words of `tc_usfm::target_words(content)` as `TargetWord`s, as
     `TargetVerse::new` builds them) and `needs_review`: `!self.is_valid(at)`, or a word of
     `word_bank` or of a group's `bottom_words` that is not a distinct verse word by (word,
     occurrence, occurrences), as `value.rs:114-117`. That word walk is tc-align's trait method
     `all_target_words` (`crates/tc-align/src/edit.rs:104`, `:176`): make it an inherent method
     of `AlignmentValue` in tc-journal and have the trait call it.
  3. `tc_align::needs_review` calls it with `verse.words()` and keeps its pin check;
     `TargetVerse::new` (`crates/tc-align/src/words.rs:429`) takes its words from `verse_words`;
     `project.rs:782` calls `value.needs_review(&run, &verse_words(content))`.
- Files: `crates/tc-journal/src/{alignment.rs,record.rs,fold/project.rs}`,
  `crates/tc-align/src/{value,words,edit}.rs`, `crates/tc-journal/tests/screens_and_fold.rs`.
- Done when: `cargo test -p tc-journal --test screens_and_fold` passes, and fails with 168 when
  `project.rs` calls `is_valid` again; `cargo test -p tc-align` and
  `cargo test -p tc-journal --test operation_counts` pass.
- Record: log line `step 1`; `next: do step 2`.

### Step 2: Splice an empty frame as tC4's writeFrame does

- Do:
  1. Red first: add `an_empty_frame_is_spliced_as_tc4_writes_it` to `projection.rs`, built like
     `a_story_splices_its_frames_and_reference_into_the_template` (`:250`; helper
     `story_one_with`, `:301`) with frame 3 empty: after a Frame change to `Texto nuevo.`, the
     bytes between the frame 3 and frame 4 image lines are `\n\nTexto nuevo.\n\n`.
  2. Make `splice_story` replace a frame's region with a blank line, the text's lines and a blank
     line (an empty text: the template's empty form), as `tc_obs::story::write_frame`
     (`crates/tc-obs/src/story.rs:433`) does.
  3. Rename the tc-app test to `obs_frame_edits_project_the_bytes_tc4_writes`; assert
     `projected == tc4` for the frame edit and no reformatted frame. Change only that test.
- Files: `crates/tc-journal/src/fold/project.rs`, `crates/tc-journal/tests/projection.rs`,
  `crates/tc-app/src/backend/seed/tests.rs`.
- Done when: both tests pass, and fail with the old `splice_story`; `cargo test -p tc-journal`
  and `cargo test -p tc-app` pass.
- Record: log line `step 2`; `next: do step 3`.

### Step 3: Order relationships as tC4 does

- Do: red first, add `relationships_follow_tc4_order_and_keep_the_first_row_per_repository` to
  `projection.rs` (pins in all five groups; one repository pinned twice with two versions:
  expect tC4's order and the first row only). Then move `PinGroup::ExtraScripture` to second in
  `GROUP_ORDER` and dedup by `id` (`metadata.rs:320-325`).
- Files: `crates/tc-journal/src/fold/metadata.rs`, `crates/tc-journal/tests/projection.rs`.
- Done when: the test passes and fails with either change reverted; `cargo test -p tc-journal`
  and `cargo test -p tc-store` pass (a changed fixture is regenerated in the same commit).
- Record: log line `step 3`; `next: do step 4`.

### Step 4: Add the missing tests (#747, #161, #179, #794, #812, #820)

- Do: write each, run it, apply its mutation, see it fail, restore.
  - #747: extend `a_file_that_cannot_be_an_ingredient_is_moved_out_and_kept`
    (`crates/tc-store/tests/store/outside.rs:899`) with `ingredients/TIT.usfm.bak`,
    `ingredients/notes.txt~`, `ingredients/draft.tmp`: each moved into `.tc/external/` with its
    bytes, no notice, no `.bak` in the next checkpoint's tree. Mutation: delete the `.bak` line of
    `is_junk` (`crates/tc-store/src/scan.rs:45`).
  - #161: `a_new_device_copies_the_profile_and_both_group_as_mine` (to create, `held.rs`): give
    the device a profile record (`DeviceProfileValue`, `crates/tc-journal/src/record.rs:482`)
    with a Door43 username (`teammate_batch`, `tests/store/commands.rs:106`, passing this
    device); then mint a new device as `a_complete_temp_that_cannot_be_placed_rotates_the_device`
    (`held.rs:197`) does; `person_labels(me)` (`crates/tc-journal/src/fold/people.rs:32`) labels
    both `PersonLabel::Mine`. Mutation: remove `project.copy_profile(old)?` (`open.rs:759`).
  - #179: `loss_unknown_parent_verse_edit_keeps_standing_text` (to create,
    `crates/tc-journal/tests/loss.rs`): a teammate's verse edit with an unknown parent gives two
    heads, the standing text shows, the conflict says incomplete history. Mutation:
    `continues_missing` (`crates/tc-journal/src/fold/attach.rs`) returns `false`.
  - #812: `every_bundled_ult_and_ust_verse_body_holds_every_target_word` (to create,
    `crates/tc-usfm/tests/corpus.rs`): over every book of `bundle/en` ULT and UST, each verse's
    editable body holds every word of its `target_words`, so a mid-verse `\p`, `\ts\*`, `\b`,
    `\q3` or `\s` line never cuts the verse off; plus 1SA 2:2, 1SA 4:1, REV 7:12 and MAT 27:9
    saved whole through `tc-store` and read back equal. Mutation: make `split_verse`
    (`crates/tc-usfm/src/codec.rs:130-141`) stop at the first layout line.
  - #794: `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment` (to create,
    `tests/store/reflow.rs`): a verse holding `\zaln-s` markup and an alignment record; a verse
    save writes no Alignment change. Mutation: remove the `NoReflow::AlignedInText` return
    (`crates/tc-align/src/reflow.rs:104`).
  - #820: `replacing_one_copy_of_a_repeated_linked_word_keeps_no_unsure_link` (to create,
    `crates/tc-align/tests/tc4_reflow.rs`): "Dios Dios", each linked, edited to "Dios Pablo";
    `reflow_edit` keeps neither link and lists both as dropped. And
    `follow_matches_every_longest_common_subsequence` (to create), a proptest in a
    `#[cfg(test)]` module of `crates/tc-align/src/reflow.rs` (`follow`, `:237`, is private): lists
    of 0 to 8 words over 2 or 3 distinct words; `Some(j)` exactly when every longest common
    subsequence, enumerated by brute force, matches old word `i` to new word `j`. It adds
    replace edits to `a_link_never_moves_to_another_copy_of_its_word`
    (`crates/tc-align/tests/properties.rs`, one-way edits only). Mutation for both: `follow`'s
    `(Some(j), None) if !left_out` arm (`reflow.rs:272`) becomes `(Some(j), _)`.
- Files: the test files above, and `crates/tc-align/src/reflow.rs` (test module only).
- Done when: `cargo test -p tc-store --test store -- outside:: held:: reflow::`,
  `cargo test -p tc-journal --test loss` and `cargo test -p tc-align` pass; each test failed
  under its mutation.
- Record: log line `step 4` with each failure quoted; `next: do step 5`.

### Step 5: Return the links a save cannot follow in its reply

- Do:
  1. Red first: add the two `..._drops_an_unclear_link_lists_it` tests to `tests/store/reflow.rs`
     (TIT 1:1 with the #820 edit): `dropped_links` names the verse and both links.
  2. `reflow` returns the change and `reflowed.dropped`; the command's plan carries the list to
     `StoreReply.dropped_links`. Fill it at every site
     `grep -n "StoreReply {" crates/tc-store/src/*.rs` prints (three, in `project.rs` and
     `handle.rs`) and in the tc-app test helper `crates/tc-app/src/backend/store.rs:1988`.
  3. Log a `note` for FIX-R: the field, its type, the tests. FIX-R makes tc-app's `reply()`
     (`backend/store.rs:969`) destructure `StoreReply` and shows the list in Align (tc-api has
     `DroppedLinkView`, `crates/tc-api/src/align.rs:169`).
- Files: `crates/tc-store/src/{commands,reply,project,handle}.rs`, `tests/store/reflow.rs`,
  `crates/tc-app/src/backend/store.rs` (one line).
- Done when: both tests pass, and fail when `reflow` returns an empty list; `cargo test -p tc-store`
  and `cargo test -p tc-app` pass.
- Record: log line `step 5`; `next: do step 6`.

### Step 6: Close the rows in TC4-ISSUES

- Do: in `docs/TC4-ISSUES.md` mark covered, with test names: row 6 (#796, its "Left for storage"
  sentence, `:58`), #179 (`:164`), #747 (`:285`), #161 (`:304`; X creates and keeps
  `copied_username_is_replaced_by_the_first_signed_in_fetch`), #794 (`:59`, `:129`), #789 and
  Q30 (a) (`:62`, `:135`). Add a row for #820 with its two tests.
- Files: `docs/TC4-ISSUES.md`.
- Done when: `grep -rn "fn <name>" crates/` finds every test the changed rows cite; CHK-3's check
  passes if it has landed.
- Record: log line `step 6`; `state: gating`.

## Gates before merge

Until INT-3 lands, every command below; after INT-3, `scripts/gates.sh` (to create in INT-3):

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and the
  same with `--features gallery`, `--features app`, `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` (the
  Windows step of `.github/workflows/ci.yml`; needs `gcc-mingw-w64-x86-64` and `nasm`)

Plus `cargo test -p tc-align --release --test suggestion_budget -- --ignored --nocapture` (the
`alignment-budget` job; step 1 changes `TargetVerse::new`).

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- V froze format 1.0 before steps 1 to 3 merged: their bytes then need a `minReaderMinor` bump
  (STORAGE §12.2 rule 3), which a translator notices. Stop and ask.
- A card that claims `crates/tc-app/src/backend/seed/tests.rs` or `backend/store.rs` is active:
  ask its owner before your one-test or one-line change.
- Step 1 flips `invalid` for an alignment in real data under `fixtures/`: list them first.

## References

- `.audit/roadmap/evidence/2026-10-21/storage-remaining.md` §3, ST-5, ST-6; `tc4-bugs.md` §2 (T row), T5, T6.
- `docs/evidence/state-seed-check-2026-10-18.md` §4.3, §5.2; `docs/TC4-ISSUES.md`; `docs/PLAN.md` (#789, Q30 (a)).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/storage-remaining.md
- 2026-10-24 17:55 EDT | S2 session (Claude Code add49644) | claim | none | claimed on track-T from main 33d2378d (S2 on main), as the integration lane asked: the storage lane carries on with T, P, S3, DOM-1, DOM-2 in that order; also to decide here: C7a's finding that a save of a merged verse whose heads moved can never be stored (S2 card log, 2026-10-24 09:30) | .audit/roadmap/units/S2.md
- 2026-10-24 19:40 EDT | S2 session (Claude Code add49644) | step 1 | none | the fold's `invalid` and the Align rail now share AlignmentValue::needs_review (staleness, or a held word the verse lacks or holds twice); tc_align::needs_review adds only the pin check; verse_words and all_target_words moved to tc-journal. Red first: the corpus test failed `words_not_the_verse: 168`, and with project.rs calling is_valid again both the corpus test and the_alignment_files_and_the_rail_agree_on_invalid fail. The loss-test fixture `alignment()` banked the whole phrase as one word; it now banks the verse's own words. screens_and_fold, tc-journal, tc-align, operation_counts (release) and workspace clippy pass | crates/tc-journal/src/alignment.rs
- 2026-10-24 21:10 EDT | S2 session (Claude Code add49644) | step 2 | none | splice_story replaces a frame's whole region, from its image line to the next image or reference line, as tC4's writeFrame: a blank line, the text and a blank line, or the template's empty form (3 blank lines, 6 at the end). New an_empty_frame_is_spliced_as_tc4_writes_it; with the old splice it fails at projection.rs:375 (the bytes between frames 2 and 3 were not `\n\nTexto nuevo.\n\n`). The tc-app test is renamed obs_frame_edits_project_the_bytes_tc4_writes and asserts equality with write_frame and no reformatted frame; seed-check §5.2 marked fixed. tc-journal tests pass | crates/tc-journal/src/fold/project.rs
- 2026-10-24 21:15 EDT | S2 session (Claude Code add49644) | step 3 | none | `relationships` follows tC4's relationshipsFromPins: GROUP_ORDER puts the extra scripture second, and rows dedup by `id` (first row per repository wins). New relationships_follow_tc4_order_and_keep_the_first_row_per_repository fails with either change reverted: the old order puts fr_glt last; the old whole-row dedup keeps both `dcs::uW/en_ult` v1 and v2. Not changed: inside one group the rows follow the slot names' sorted order (PinGroupValue is a BTreeMap), where tC4 follows the JSON object's insertion order; a group with two scripture slots whose names sort apart from their order in resources.json still differs. tc-journal tests pass | crates/tc-journal/src/fold/metadata.rs
- 2026-10-24 21:25 EDT | S2 session (Claude Code add49644) | step 4 | none | six tests, each failed under its mutation and passes restored. #747: a_file_that_cannot_be_an_ingredient_is_moved_out_and_kept also puts `TIT.usfm.bak`, `notes.txt~`, `draft.tmp`; with the `.bak` line of is_junk removed it fails at outside.rs:863 (5 moved, not 6). #161: a_new_device_copies_the_profile_and_both_group_as_mine (held.rs); without `project.copy_profile(old)?` it fails at held.rs:283 (the old device is no longer Mine). #179: loss_unknown_parent_verse_edit_keeps_standing_text (loss.rs); with continues_missing returning false it fails at loss.rs:2023 (`conflict.incomplete_history`). #812: every_bundled_ult_and_ust_verse_body_holds_every_target_word went to crates/tc-usfm/tests/resources.rs, which already reads the bundle, not to a new corpus.rs: 47,723 verses; with split_verse stopping at the first layout line it fails with `661 verses cut off`; and verses_with_a_layout_line_inside_save_whole (tc-store tests/store/commands.rs, zip added to tc-store's dev-dependencies) adds the ULT 1SA, REV and MAT and saves 1SA 2:2, 1SA 4:1, REV 7:12, MAT 27:9; under the same mutation the book cannot be added (commands.rs:2010). #794: a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment (tests/store/reflow.rs); without the AlignedInText return it fails at reflow.rs:167 (the save wrote an Alignment change). #820: replacing_one_copy_of_a_repeated_linked_word_keeps_no_unsure_link (tc-align tests/tc4_reflow.rs) and the proptest follow_matches_every_longest_common_subsequence (test module of crates/tc-align/src/reflow.rs, brute force over lists of 0 to 8 words on 2 or 3 words); with `(Some(j), _) => Some(j)` both fail (tc4_reflow.rs:526, reflow.rs:322) | crates/tc-store/tests/store/outside.rs
- 2026-10-24 22:20 EDT | S2 session (Claude Code add49644) | step 5 | none | `StoreReply.dropped_links: Vec<DroppedLinks>` (tc-store `reply.rs`; `DroppedLinks { book, chapter, verse, links: Vec<tc_align::DroppedLink> }`, the saved verse's place): the store's reflow pushes `reflowed.dropped` into the plan, and the verse save, the section save and a kept draft carry it to the reply; every StoreReply site fills it. New a_translate_save_that_drops_an_unclear_link_lists_it and a_section_save_that_drops_an_unclear_link_lists_it (TIT 1:1 "Dios Dios" to "Dios Pablo"): with the list left empty both fail (reflow.rs:327, :371). tc-store, tc-app, tc-journal, tc-align, tc-usfm and tc-api tests pass (73 binaries) | crates/tc-store/src/reply.rs
- 2026-10-24 22:20 EDT | S2 session (Claude Code add49644) | note | none | for FIX-R: `tc_store::StoreReply.dropped_links` is a `Vec<tc_store::DroppedLinks>`, one entry per verse whose save sent links back to the bank, each with the `tc_align::DroppedLink`s (card, its source words, the word); tests a_translate_save_that_drops_an_unclear_link_lists_it and a_section_save_that_drops_an_unclear_link_lists_it in crates/tc-store/tests/store/reflow.rs. tc-app's `reply()` (backend/store.rs) does not read it yet; FIX-R maps it to tc-api's `DroppedLinkView` and shows it in Align | crates/tc-store/src/reply.rs
- 2026-10-24 22:55 EDT | S2 session (Claude Code add49644) | fix | none | decided C7a's finding (S2 log, 2026-10-24 09:30) here, as the claim said: a save whose token shows a merged verse (several heads) refused with LookAgain once a sync moved those heads, so the edit could never be stored. That is reachable: two teammates edit different parts of a verse, the app shows the merge, a third change syncs in before the translator saves. Now the fold rebuilds the merge of the token's heads (`FoldState::merged_at`, `merge::resolve_heads`, which `resolve_local` now calls for the current heads), and the save writes with the token's heads as parents, so it stands beside the newer text as a conflict and both texts are kept. A token whose heads conflict still gets LookAgain. New a_save_of_a_merged_verse_whose_heads_moved_is_stored_as_a_conflict (tc-store tests/store/commands.rs); before the change it fails with `stored, not refused: LookAgain`. STORAGE §5.1 step 2 says so. The outside-edit reader's own `shown_value` still treats moved, differing heads as an unknown base (recorded without parents, nothing lost); not changed | crates/tc-store/src/commands.rs
- 2026-10-24 23:00 EDT | S2 session (Claude Code add49644) | step 6 | none | docs/TC4-ISSUES.md: §1 row 6's "Left for storage" sentence now says T step 1 did it; #796 (review rule), #794, #789 (with Q30 (a), StoreReply.dropped_links), #179, #747 and #161 (X keeps copied_username_is_replaced_by_the_first_signed_in_fetch) are `covered` with their tests; a new #820 row. Every test the changed rows cite exists (`grep -rn "fn <name>" crates/`). There is no CHK-3 check in scripts/gates.sh. The Counts table was already out of step with the rows before this change (for example Align: 7 rebuild bugs and 13 covered in the table, 7 and 16 in the rows); left as it is and added to .audit/minors.tsv. Step 1 flipped no alignment in real data: the corpus disagreements were in the generated Psalms corpus, 0 in real data (TC4-ISSUES row 6) | docs/TC4-ISSUES.md
- 2026-10-24 23:10 EDT | S2 session (Claude Code add49644) | gate | none | started scripts/gates.sh in .worktrees/track-T on 24bf9f83 plus this line; log /tmp/claude-1000/-home-bot-tc-rst/add49644-fea9-4f0f-8b94-948bea6cb07c/scratchpad/t-gates.log | scripts/gates.sh
- 2026-10-24 23:40 EDT | S2 session (Claude Code add49644) | verify | 28665c77 | three read-only lens reviews of 33d2378d..28665c77 (they ran nothing, a gate run held the worktree). Loss: fix_first, one major new (a save from a screen a sync overtook reflowed the teammate's current alignment from the old text and replaced it with no conflict; it already happened for single-head tokens, the merged-verse change widened it), one major old (a story whose base lost its reference line to a record never wrote the line again), one minor. Races and protocol: merge, three minors (the same reflow point; no test for a twice-sent merged save; STORAGE §5.1 step 2 says a conflict token stands for its candidate, the save refuses it). Parity: fix_first, one major (relationship rows inside a group came in slot-name order; tC4 writes LS_SLOTS order, so every gateway-language project's metadata.json differed), minors (the alignment files' rule and #820 not declared in PLAN; APP-DESIGN's seed-check line). The gate run on 28665c77 was stopped at its test step to fix these | .audit/roadmap/units/T.md
- 2026-10-24 23:45 EDT | S2 session (Claude Code add49644) | fix | none | (1) the store's reflow runs only when the verse's heads are still the token's; a save from an overtaken screen leaves the alignment as it is, and its hash marks it for review (STORAGE §5.1 step 5). New a_save_from_a_screen_a_sync_overtook_keeps_the_teammates_alignment; without the guard it fails at reflow.rs:441. (2) relationships read a group's slots in tC4's resources.json order (`slot_rank`, now also with tC4's OBS slots, so resources.json of an OBS project orders them as tC4 too); the relationships test uses real slot names and fails without the sort (projection.rs:581). Left: extraScripture panes keep their id order where tC4 keeps first-pin order (minor). (3) splice_story writes the reference line after the last frame, as tC4's writeRef, when the base has none; new a_story_whose_base_has_no_reference_line_writes_its_reference failed before the fix (the line was missing). (4) PLAN.md declares that the alignment files carry the review rule and cites #820. The other minors are in .audit/minors.tsv | crates/tc-store/src/commands.rs
