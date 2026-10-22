# tc4-coverage

## Summary
Feature coverage against tC4 (tip b86270f; rc.2 tag a5ce0307, whose tree equals main b1d1f37). Most of tC4 4.0's screens are built. main af4eef3 has 29 of 48 app units and resume-build-lane f9067bc has 38. Shell, Home, the create and settings dialogs, resources, Understand, Translate, Check, Align, OBS and the Community Checking preview all exist.

Ten app units remain: C7a, C7b, D1, D2, E1a, E1b, E2, F1, G1 and G2. Of the user journeys, export (J7, J23) waits on S2. Import (J9) waits on I. Share, open, sync and USB (J11, J17, J24, J26, E2) wait on X. None of these storage units is built, and no tc-sync or tc-import crate exists on any branch.

Six gaps have no owner:
1. Packaging. tC4 ships a signed Windows installer, a macOS arm64 pkg and a Linux x64 zip, and its VISION promises all three. The rebuild's G1 is Windows-only, and installers, signing, smoke tests and notices appear only in TC4-ISSUES rows, not in G1's scope.
2. P6 Aquifer helps has no app unit.
3. The "reference move" to the new tC4 version (12 reference-change rows, #769 among them) has no unit.
4. Importing a tC4 4.0 project (P9) has no UI kind, and the "move the team" prompt has no unit.
5. P1's browser journeys skip in CI. Nothing in CI fails yet for any journey.
6. Seven tC4 changes after 9ad4d5c have no row: #807, #813/D110, #812, #820, the sync-refusal spec, D108's per-verse rows and D105's export skip.

STATUS.md is stale on both branches.

# Feature coverage of tC4 4.0 in the Rust rebuild (evaluator: features)

**Sources:**
- RBL: the resume-build-lane snapshot, f9067bc.
- MAIN: the main snapshot, af4eef3.
- tC4: `ext/tc4-tip`, origin/main b86270f.

All of these were read-only. Nothing was built or run.

**State codes:**
- **M**: built on main af4eef3.
- **L**: built only on resume-build-lane.
- **P:x**: planned unit x, not built.
- **NP**: no unit plans it.

**Proof codes:**
- **ci**: a Rust test that `cargo test --workspace` runs in CI.
- **local**: a browser journey in `parity/app/journeys/*.test.mjs`. These are skipped in CI, because `kit.mjs:22-23` skips when `TC4_BIN` is unset and the `parity-baseline` job in `.github/workflows/ci.yml` never sets it.

## 1. Where the units stand (verified with git)

- **On main (29 units):** A1–A11, G0, B1a, B1b, B2, B3, B4a, B5a, B6, B9a, B9b, B10, C0, C1a, C1b, C6 and C8. Each has a `Merge <unit>` commit, for example B4a 3889aad, B5a 846d9e9, B6 50ff2a9, C6 acba162 and C8 112f839.
- **Added on the lane (9 units):**
  - B4b (3fe7ebf), B5b (37da0b7), B7 (b230dad), B8 (cad4961), C4 (3eac396) and B11 (f07fd9e) arrived as merge commits.
  - C2, C3 and C5 were committed directly on the lane (for example 3067ce7 "Build C3").
  - That makes 38 of 48 on the lane.
- **Remaining (10 units):** C7a, C7b, D1, D2, E1a, E1b, E2, F1, G1 and G2.
- **Storage:** S1 is done. S2, P, T, X, I and V are not. No snapshot has a `tc-sync` or `tc-import` crate.
- **STATUS.md is stale on both branches:**
  - The lane's copy was generated at 3eac396 and says 37/48 with B11 "ready to start", but f07fd9e merged B11.
  - Main's copy was generated at fa46807 and says 24/48, but the five merges above all came after fa46807 (checked with `git merge-base --is-ancestor`).
- **Parity report** (RBL `parity/app/report.json`): 130 rows. 33 same, 79 differ, 0 declared, 18 unported. The unported rows are all gallery cases:
  - share-progress ×3 and share-done (E1b);
  - import-files, import-review ×2, import-working, import-failed and import-toast (F1);
  - feedback-sending, feedback-sent and five feedback-not-sent cases (G1);
  - home-checkpoint-error (D1).

## 2. Coverage table

### Shell and account

| # | Feature | tC4 | Rebuild | Proof |
|---|---|---|---|---|
| 1 | Top bar, mode switch, project chip | `App.jsx:106` | M A5 | local `top_bar_on_home_is_tc4s` |
| 2 | Save indicator with Retry | `App.jsx:42` | M A4b, A5 | local `save_indicator_states`; ci `tests/app/indicator.rs` |
| 3 | Sentence for a save refused after sync (#801, new after 9ad4d5c) | `App.jsx:91` | P:E1a, A4b (TC4-ISSUES:282, :403) | The named test `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` is not found by grep in RBL |
| 4 | View error boundary | `App.jsx:199` | M A5 | local `a_panic_reload_keeps_the_top_bar_and_back_to_home_gates` |
| 5 | Open overlay at 300 ms | `OpenProgress.jsx` | M B3 | ci `j15_a_slow_open_reports_from_300_ms_and_counts_up` (tc-app store_scenario.rs) |
| 6 | Account menu, internet switch, "Use the internet?" | `AccountMenu.jsx:73,405` | M A5, A8 | local account-menu (5 tests), internet-consent (16) |
| 7 | Door43 sign-in, Stay signed in, sign out | `ShareSignIn.jsx` | M A8 with a memory keychain; the OS keychain is P:E1a or G1 (Q-3) | local j11-sign-in (8) |
| 8 | Help and guides, offline | `AccountMenu.jsx:234` | M A5 | local help a–i; ci `tests/help_pages.rs` |
| 9 | About and License | `AccountMenu.jsx:296,327` | M A5; third-party notices P:G1 (#606, #554) | local about (3) |
| 10 | 10 app languages; Arabic right-to-left | `AccountMenu.jsx:351` | M A4a, A5 | ci `the_registry_names_the_ten_decided_locales_each_with_a_catalog`; local app-language a–j. **k is not ported**, and the in-project halves of help a and app-language h and j are open (APP-DESIGN:3071) |
| 11 | Report a problem: compose | `Feedback.jsx` | M A5 | local feedback (6) |
| 12 | Report a problem: send | `scripts/desktop-feedback.cjs` (SendGrid) | P:G1 (Q-2) | gallery feedback-* unported |

### Home and dialogs

| # | Feature | tC4 | Rebuild | Proof |
|---|---|---|---|---|
| 13 | Bible and OBS cards, badges, more than 12 books, recent stories | `Home.jsx:204,91` | M B2 | ci `more_than_twelve_books_show_in_progress_only_when_all_known`, `collapsed_stories_are_the_three_most_recent_newest_first` |
| 14 | Resume card | `Home.jsx:287` | M B2 | ci `resume_shows_only_a_record_its_project_holds`, `place_and_the_resume_record_survive_a_relaunch` |
| 15 | Conflicts chip | `Home.jsx:163` | M B10 | local conflict-review J18 (L) |
| 16 | QA label on a debug build's card (#506) | `Home.jsx` | M B2 | ci `debug_card_names_qa_server` |
| 17 | Checkpoint banner on Home | `Home.jsx` | P:D1 | gallery `home-checkpoint-error` unported |
| 18 | New Bible and New OBS, with language suggestions (J1, J20) | `NewBible.jsx`, `NewObs.jsx` | M B2, C6 | local create J1 and J20; language-suggestions (3) |
| 19 | Add a book, blank or from USFM | `AddBook.jsx` | M C6 | local create "Add a book from USFM files" |
| 20 | Project settings | `ProjectSettings.jsx` | M B9a, C6 | local settings-contract |
| 21 | Source texts, download, Use for checking | `SourceTexts.jsx` | M B9a | local source-texts (2) |
| 22 | Check for updates and upgrade (J12); gateway change (J13) | `ResourceUpdates.jsx`, `UpgradeSet.jsx`, `GatewayChange.jsx` | M C8 | ci `j12_an_upgrade_moves_one_sets_pins_and_carries_decisions_in_one_batch`, `j13_a_new_checking_language_moves_the_primary_set_and_keeps_the_english_fallback` |
| 23 | Guided fix: fetch, sideload, re-pin (J3) | `GuidedFix.jsx` | server M C8; UI L B5b | ci `j3_fetching_the_pinned_version_installs_that_commit_and_moves_no_pin`; local guided-fix (3) |
| 24 | First-launch seed and preparation window | `scripts/desktop-prepare.cjs` | M A9; window at the click (#743) P:G1 | ci `repeat_launch_reads_no_resource_content_file` |

### Understand, Translate, Check, Align, OBS

| # | Feature | tC4 | Rebuild | Proof |
|---|---|---|---|---|
| 25 | Understand: chips, units, cross-frame | `Understand.jsx:373` | M B3 | local understand (14) |
| 26 | Helps panel: Notes, Words, Questions, Simplified, article, resize | `HelpsPanel.jsx:554` | M B3 | local `helps_panel_width_follows_the_divider_drag` |
| 27 | Comment box (J16) and Comments tab | `Understand.jsx:77`, `HelpsPanel.jsx:346` | L C3 | local `j16_three_help_steps_then_three_pauses_and_a_blur_add_one_comment` |
| 28 | Translate grid, verse editor (J2) | `Draft.jsx:293,33` | M B4a; the production save is L C2 | local translate (11) |
| 29 | Section editor: Type and Place, spans | `SectionEditor.jsx` | L B4b, C2 | local section (22) |
| 30 | Alignment reflow after an edit | `state.jsx` `reflowAlignedVerses` | L C2 and tc-align | ci `tc-align/tests/tc4_reflow.rs`. **No test covers a replace (#820).** `properties.rs:556-600` generates insert-only or delete-only edits |
| 31 | Check picker, preflight, rail filters and sort | `Check.jsx:1436,980` | M B5a | local check-picker (2), check-keyboard (4) |
| 32 | Rail groups that fold, and "By figure" (#769) | `Check.jsx:980`; `state.jsx` `setRailFolded` | **NP** in practice: deferred to the "reference move" (TC4-ISSUES:217; B5a.md:15-16), and no `railFolded` or fold code exists in tc-ui | none |
| 33 | Item detail: compare card, drag-select, marks, comment, bookmark, Academy drawer (J4) | `Check.jsx:671,806,341` | L B5b, C4 | local check-detail (11); `j4_a_mark_on_dragged_words_a_comment_and_a_bookmark_are_one_decision_record_on_disk` |
| 34 | Invalidation after a verse edit (J6) | `Check.jsx:754` | L C4 | local `j6_a_verse_edited_in_translate_flags_its_checks_and_keeps_their_decisions` |
| 35 | Align: bank, cards, merge and split, lens, keyboard (J5) | `Align.jsx:331`; `Check.jsx:1159` | M B6 | local align (12), `align_by_keyboard_only` |
| 36 | Align saves, Mark valid, suggestions | `Align.jsx:535,575` | L C5 | local `align_suggestions_through_the_page`; ci `a_confirmed_save_joins_the_memory_at_once_and_a_resave_keeps_the_count` |
| 37 | OBS rail, Understand, Translate, Check (J21, J22, J25) | `StoryDraft.jsx:193`, `Check.jsx:508` | L B7, C3, C4 | local story (6), story-writes (5), the two j22 tests |

### Community Checking, exports and exchange

| # | Feature | tC4 | Rebuild | Proof |
|---|---|---|---|---|
| 38 | Community Checking preview and page setup | `CommunityChecking.jsx:159,96` | L B8 | local community-checking (6) |
| 39 | Export PDF | `ExportMenu.jsx`; `desktop-main.cjs:18` | P:D2, plus G1's printToPDF | `cc_the_export_menu_lists_the_exports_and_runs_none_until_d2` |
| 40 | USFM aligned and plain (J7); OBS Markdown (J23) | `data/export/*` | P:D2 (waits on S2). tc-export holds only the print model: `book.rs`, `page.rs`, `stories.rs`, `text.rs` | ci `weave_writes_what_tc4_writes` (the weave only) |
| 41 | Scripture Burrito zip export | ExportMenu | P:E2 (APP-DESIGN E2 row) | none |
| 42 | Export names the verses it skipped (D105, after 9ad4d5c) | `ExportMenu.jsx` `cc.exportSkipped` | P:D2 (TC4-ISSUES:127, keep the bytes and name the verse) | none |
| 43 | Import tC3, USFM, Burrito (J9a, J9c, J9d) | `Import.jsx` | kind step M B2; the rest P:F1 and I | gallery import-* unported |
| 44 | Share and first push (J11, J24); Copy link | `ShareDialog.jsx`; `state.jsx:6685` | steps M B9b; push P:E1b and X | local share-open (3) |
| 45 | Open from Door43 (J17) | `OpenFromDoor43.jsx` | form M B9b; P:E1b and X | local share-open, the third test |
| 46 | Sync with Door43 and its toast (J26), including the missing-pins Download | `SyncResult.jsx`; `state.jsx:6677` | scripted M B10 (`sync_result.rs:111`); the real sync P:E1a and X | local sync-button (2) |
| 47 | Conflict review: read, and Keep this version (J18) | `ConflictReview.jsx:133` | read L B11; Keep P:C7a | local conflict-review (3) |
| 48 | Checkpoints at mode switch and Home (J8) | `state.jsx` `go`, `backToProjects` | P:D1 (waits on S2) | none |

### Rebuild-only features (PLAN goals)

| # | Feature | Rebuild | Proof |
|---|---|---|---|
| 49 | Draft offers, read-only modes, Copy to this computer | M C1b | ci `a_banner_offers_the_copy_where_a_copy_helps` |
| 50 | Review of every conflict kind | read L B11; choices P:C7b, which waits for Q-9's designs | local conflict-review |
| 51 | USB exchange (P2) | P:E2 and X | none |
| 52 | Aquifer helps (P6); the Portuguese AI Bible (Q34 answer b) | **NP** for the app. APP-DESIGN:1505-1512 says "nothing in the app installs an Aquifer asset". Only library tests exist (tc-helps `tests/aquifer.rs`, tc-resources `tests/aquifer_install.rs`) | none in tc-app or tc-ui |
| 53 | Import a tC4 4.0 project (P9) | storage I planned. **The UI has no kind for it**: `ImportKind` lists only `Tc3`, `Usfm`, `Burrito` (RBL `tc-ui/src/app/forms.rs:506-513`), and F1's tests are J9a, J9c, J9d only | none |
| 54 | "Move the team from tC4" prompt (STORAGE §13.3) | **NP** for the app: grep for "Move the team" or §13.3 in APP-DESIGN and the plans finds nothing | none |

### Desktop shell and packaging (tC4 ships these)

| # | Feature | tC4 | Rebuild |
|---|---|---|---|
| 55 | Single instance; a second launch focuses the running window | `desktop-main.cjs:108,123` | P:G1 (named in its row) |
| 56 | Close guard, links open in the system browser, download report | `desktop-main.cjs:38-50,127-130` | P:G1 |
| 57 | Hidden native title bar (#639, #214, #642); zoom keys (#702) | `preload.cjs` | P:G1, but only in TC4-ISSUES:355, 391, 392 and 404 |
| 58 | Windows x64 signed per-user installer and zip | `scripts/tc4.iss`, `sign-windows.ps1`, D99 | P:G1 (TC4-ISSUES:395-396). G1's scope row does not mention it |
| 59 | macOS arm64 pkg (unsigned) and zip | `build-macos-pkg.zsh`, PACKAGING.md:333 | **NP** |
| 60 | Linux x64 zip | PACKAGING.md:403 | **NP** |
| 61 | Post-install smoke test on 3 systems, offline run, clean-machine witness | PACKAGING.md:758, 837 | untriaged, "likely G1" (TC4-ISSUES:426-427) |
| 62 | App auto-update | none in tC4: grep for updater, squirrel and auto-update finds nothing; the installer keeps no update channel | not needed |

## 3. Gaps

**A. Features with no unit:**
- #32: rail fold and By figure.
- #52: Aquifer in the app (P6).
- #53: a tC4 4.0 import kind.
- #54: the team-move prompt.
- The reference move itself: re-vendor the catalogs, recapture the 62 screens and port the 12 reference-change rows.
- app-language k.
- P8's Graphite probe: TC4-ISSUES #29 names it for G1, but G1's row does not.

**B. Platforms:**
- VISION ("Initial public release") lists Windows, macOS and Linux.
- tC4 rc.2 built `windows-x64.exe`, `macos-arm64-unsigned.pkg` and `linux-x64-unsigned.zip`, according to release/4.0's `docs/releases/4.0.0-rc.2.md`.
- G1's "done when" covers the Windows runner only. CI checks Linux and windows-gnu, never macOS.
- Only 2 files in RBL `crates/` have `target_os = "macos"` code.
- PLAN P8 asks for a Graphite probe "on the release pipeline for Windows and macOS", and no unit builds that pipeline.

**C. tC4 changes after 9ad4d5c (PRs into main b86270f, and into release/4.0 up to 9f06f99):**

| PR | Change | Rebuild |
|---|---|---|
| #785 (#784) | `\q1` on one line, then `\v N` on the next: that verse is indexed | Covered (TC4-ISSUES:150). Remove it from the declared differences |
| #804 (#792) | Drain the queue before a span save | Not applicable (:132) |
| #805 (#793, D109) | Alignment words in NFC | Listed (:131) |
| #806 (#791, D108) | Per-verse rows "{book} {ref}: {name}'s alignment replaced yours." | B10 shows a count and a Review instead. :133 says the per-verse rows "wait for tC4 to ship them", and tC4 has now shipped them. **E1a needs a note** |
| #799 (#794) | Inline alignment guard | Listed (:129) |
| #803 (#801) | New 646-line e2e `sync-refusal.spec.ts` | Rows :282 and :403 exist, but **the spec is in no port list** |
| #808 (#807) | Span save behind a refused alignment save | **No row** |
| #800 (#795) | One hash | Covered (:128) |
| #809, #811, #814 (#796, D105) | The export skips and names a verse | D2 todo (:127). This is a declared difference: the rebuild keeps the bytes and names the verse |
| #815 (#789, D106) | Link follows its word | Declared difference: the link goes to the bank, with no flag |
| #816 (#790, D107) | Source cards repaired from the pinned verse | Covered |
| #818 | NUL-escaped key separator | Not user-visible |
| #817 (#813, D110; #821, #822) | A Burrito import with no `vrs.json` gets `eng` | **No row.** I's T3 reads `vrs.json` as the oracle (STORAGE §13.1, on the storage branch) |
| #810 (#786, #812) | A line with three tabs becomes `\q2`; breaks after a verse's last words are kept | #786 covered (:161). **#812 has no row** |
| #824 (#820) | A replace of one copy of a repeated linked word flags the verse | **No row and no test** (table row #30) |
| #827, #828 | rc.2 cut; release notes | `v4.0.0-rc.2` = a5ce0307, whose tree equals main b1d1f37. The reference rule (PLAN:39-41) waits for a v4.0.0 tag, which D103 leaves without a date |

There are 6 new interface strings in each of the 10 catalogs (`git diff 9ad4d5c origin/main -- src/i18n/en.json`).

## 4. Pieces another developer could take (inferred from the coverage)

1. **Release and platforms.** G1's shell work, installers for 3 systems, signing, smoke tests, offline run and notices. It touches `spikes/electronite`, a new packaging folder and the `tc4` flags, and only printToPDF waits on D2.
2. **Reference move to rc.2.** Covers the catalogs, the baseline, #769 and the other reference-change rows. UI only.
3. **Aquifer app unit (P6).**
4. **The pure part of D2.** The USFM, Markdown and print producers in tc-export. Inference: only "checkpoint first" needs S2.


## Findings
- [F1] (critical/high) No browser journey runs in CI, so P1 (journeys checked in CI) is shown by no check that can fail. Every user-journey proof is a local Playwright run.
  - evidence: RBL parity/app/journeys/kit.mjs:22-23: a test skips when TC4_BIN or TC4_UI is unset; `git show resume-build-lane:.github/workflows/ci.yml`: the parity-baseline job runs `node --test ... parity/app/journeys/*.test.mjs` with no TC4_BIN; its comment says 'their browser tests skip here too'; APP-DESIGN §7.9: the app-e2e job is 'Later (G2)'
  - rec: Move a minimal app-e2e CI job (build tc4 with test-hooks plus the wasm UI, Playwright Chromium) ahead of G2, and run the journeys already built (create, translate, section, check-writes, align, story-writes, comments). It must fail on a broken journey and name the journey.
- [F2] (high/high) The rebuild has no plan for macOS or Linux packages, although tC4 4.0 ships a macOS arm64 pkg and a Linux x64 zip and tC4's VISION promises Windows, macOS and Linux.
  - evidence: tC4 release/4.0 docs/releases/4.0.0-rc.2.md: artifacts tC4-4.0.0-rc.2-windows-x64.exe, -macos-arm64-unsigned.pkg, -linux-x64-unsigned.zip; tC4 docs/VISION.md:106-118 'Windows, macOS, and Linux'; RBL APP-DESIGN G1 row: 'done when: the tests pass on the release pipeline's Windows runner'; RBL CI clippy runs for Linux and x86_64-pc-windows-gnu only; `grep target_os = "macos"` matches 2 files in crates/; PLAN P8 needs a Graphite probe 'on the release pipeline for Windows and macOS', and no unit builds that pipeline
  - rec: Ask the owner which platforms the rebuild ships. Add a unit G3 'Release pipeline' with runners for windows, macos-arm64 and ubuntu, packaging that matches PACKAGING.md (per-user Inno installer, pkg, zip), a post-install smoke test on each, and the P8 probe on Windows and macOS.
- [F3] (high/high) Installers, Windows signing, smoke tests, the offline run, third-party notices, window frames and zoom keys are assigned to G1 only in TC4-ISSUES rows. G1's scope row in APP-DESIGN §10 has none of them, so a builder working from §10 will not build them.
  - evidence: RBL docs/TC4-ISSUES.md:355 (#642), :357 (#606), :391 (#639), :392 (#214), :395 (#456), :396 (#44), :397 (#554), :404 (#702), :426-427 (#43, #45 untriaged); RBL APP-DESIGN G1 row: main script, supervision, buffer mirror, single instance, prep window, close guard, printToPDF, dialogs, openExternal, keychain or feedback; no installer, signing or notices
  - rec: Split G1 into G1a (shell runtime) and G1b (packaging, signing, notices, smoke, offline run). Copy each TC4-ISSUES row's regression test into G1b's 'Tests' column. Ask the owner for the Azure Artifact Signing access (tC4 D99).
- [F4] (high/high) P6 (helps from Aquifer in the same screens) has no app unit. Nothing in the app installs or offers an Aquifer asset, and no tc-app or tc-ui test mentions Aquifer.
  - evidence: RBL APP-DESIGN:1505-1512: '(B9a: no Aquifer asset gets a Download job yet ... nothing in the app installs an Aquifer asset'); tC4-DELTA-2026-10-19 Q34: the owner answered (b) 'Show Aquifer's Portuguese Bible as a reading pane'; no unit is named; grep in RBL: Aquifer tests only in tc-helps/tests/aquifer.rs and tc-resources/tests/aquifer_install.rs
  - rec: Add unit H1 'Aquifer helps in the app': Source texts lists Aquifer packages, a streamed Download job (A11), helps panes rendered from Aquifer, and the Q34 reading pane with its AI label. Prove P6 with one browser journey for DCS and one for Aquifer on recorded fixtures.
- [F5] (high/high) No unit owns the 'reference move' that 12 TC4-ISSUES rows wait for (#769 fold and By figure, #763 help pages, #766 pins at first open, among others). tC4 now has a tagged rc.2 whose tree equals main b1d1f37, and stable v4.0.0 has no date.
  - evidence: RBL TC4-ISSUES.md: `grep '| reference change |'` gives rows at :138, 139, 175, 217, 268, 302, 312, 314, 352; RBL .audit/plans/B5a.md:15-16: #769 'matched when the reference moves'; no railFolded or fold code in tc-ui/src; `git -C tc4-tip log -1 v4.0.0-rc.2` = a5ce0307; release notes: 'Its tree ... is the tree of main at b1d1f37'; RBL PLAN.md:39-41: the reference moves only when a v4.0.0 tag exists; tC4 ROADMAP D103: more rc.N before stable, no date
  - rec: Ask the owner to move the reference to v4.0.0-rc.2 now. Add unit R1: re-vendor the 10 catalogs (6 new keys since 9ad4d5c), the help pages (tc-website dbb7cd0) and CSS, recapture the 62 baseline screens and the gallery at rc.2, then build each reference-change row with its named test.
- [F6] (medium/high) Seven tC4 changes after 9ad4d5c have no TC4-ISSUES row or unit note: #807, #813/D110 (with #821 and #822), #812, #820, the new e2e sync-refusal.spec.ts, D108's per-verse alignment rows (now shipped), and the D105 export skip wording.
  - evidence: `git -C tc4-tip log --first-parent 9ad4d5c..origin/main`: PRs #785, #804, #805, #806, #799, #803, #808, #800, #809, #811, #814, #815, #816, #818, #817, #810, #824, #827; grep for #807, #813, #812, #820, #821, #822 and #825 in all four snapshots' docs: no hits; `git diff --stat 9ad4d5c origin/main -- e2e`: sync-refusal.spec.ts adds 646 lines; RBL TC4-ISSUES.md:133: 'the per-verse rows of D108 ... wait for tC4 to ship them'; tC4 SyncResult.jsx now renders `AlignReplaced`
  - rec: Rerun .audit/workflows tc4-issue-sweep with tip b86270f (and release/4.0 9f06f99). Add rows: #807 (likely not applicable), #812 (B4b check), #820 (tc-align test), #813/D110 (unit I rule), the sync-refusal spec (E1a port), and D108 rows (an E1a note to name book, ref and teammate per verse).
- [F7] (medium/high) No rebuild test covers tC4 #820: replacing one copy of a repeated linked word ('Dios Dios', one linked, saved as 'Dios Pablo'). The reflow property test generates only insert-only or delete-only edits.
  - evidence: RBL crates/tc-align/tests/properties.rs:556-600 `a_link_never_moves_to_another_copy_of_its_word` uses `one_way_edit` (only inserts or only deletes); RBL crates/tc-align/tests/tc4_reflow.rs test list (lines 66-494) has no replace-of-a-repeated-word case; tC4 commit 4760d266: the #820 fix with three red-first unit cases and a J5 journey; RBL crates/tc-align/src/reflow.rs:8-16: the rule that a link whose word cannot be followed for sure returns to the bank
  - rec: Add `a_replace_of_one_copy_of_a_repeated_linked_word_returns_its_link_to_the_bank` (and the mirror case) to tc-align/tests/tc4_reflow.rs. Widen the proptest edit generator to mixed insert, delete and replace edits.
- [F8] (high/high) Importing a tC4 4.0 project (part of P9) has no entry in the UI. Import offers only tC3, USFM and Burrito, F1's tests list only J9a, J9c and J9d, and STORAGE §13.3's 'Move the team to the new translationCore?' prompt has no app unit.
  - evidence: RBL crates/tc-ui/src/app/forms.rs:506-513 `enum ImportKind { Tc3, Usfm, Burrito }`; RBL APP-DESIGN F1 row tests: 'J9a, J9c, J9d; language-suggestions; ...'; storage snapshot docs/STORAGE.md:4427-4454 (§13.3, the prompt text); grep for 'Move the team' or '13.3' in RBL APP-DESIGN, PLAN and .audit/plans gives no hits
  - rec: Get an owner ruling and design (Q-9 style) for (a) how a translator brings a tC4 4.0 project in: a fourth import kind, or the Burrito kind detecting `checking/journal`; and (b) the team-move prompt. Add both to F1 and E1a or E1b, with a P9 journey that imports a tC4 4.0 fixture through the UI.
- [F9] (medium/medium) tC4 D110 (#813): a Burrito import with no vrs.json gets the eng scheme. A tC4 project imported before #813 has no vrs.json (#821). The rebuild's tC4 importer reads vrs.json as its oracle and has no rule for its absence. (Inference from the spec text; the I unit is not built.)
  - evidence: tC4 docs/DECISIONS.md:3133-3158 (D110, points 1, 2 and 5); storage snapshot STORAGE.md §13.1 T3: 'Read tC4's derived files ... resources.json, settings.json, vrs.json, metadata.json. They are the oracle.'; tC4 rc.2 release notes, known limitation #821
  - rec: Ask the owner: give a Burrito import with no vrs.json eng (as D110 does) or detect the scheme from the content (#822). Add an I-unit fixture: a tC4 project with a journal and no vrs.json imports with its text unchanged byte for byte.
- [F10] (critical/high) Exports (J7, J23), import (J9), share, open and sync (J11, J17, J24, J26) and USB (P2) are all unbuilt, and every one waits on a storage unit that is not built (S2, I, X). tc-export holds only the print model.
  - evidence: RBL STATUS/HANDOFF: S2, P, T, X, I, V 'to do'; no tc-sync or tc-import crate in any snapshot (`ls crates`); RBL crates/tc-export/src: book.rs, lib.rs, page.rs, stories.rs, text.rs (no USFM, Markdown or zip producer); APP-DESIGN rows: D2 depends on S2 and B8; E1a on X, B10, C7a, D1; E1b on X, E1a; E2 on X, C1a, D2; F1 on I, C1a; parity report.json: 18 unported gallery cases, all in E1b, F1, G1 and D1
  - rec: Treat X and I as the critical path. Start D2's pure producers (aligned and plain USFM via tc-usfm's weave, OBS Markdown, print HTML) now, against the model, so that only 'checkpoint first' waits on S2 (inference).
- [F11] (medium/high) The tracking files a handoff relies on are stale on both branches. The lane's STATUS.md says 37/48 with B11 'ready to start', though B11 is merged (38/48). Main's STATUS.md says 24/48, though main holds 29 unit merges.
  - evidence: RBL .audit/STATUS.md header: 'Generated ... from resume-build-lane at 3eac396'; `git log` shows f07fd9e 'Merge B11' after 3eac396; MAIN .audit/STATUS.md: 'Generated ... at fa46807', '24 of 48'; `git merge-base --is-ancestor` shows 3889aad, 846d9e9, 50ff2a9, acba162 and 112f839 all after fa46807
  - rec: Run .audit/tools/status.py in each merge commit, or as a CI step that fails when STATUS.md differs from the generated output.
- [F12] (medium/medium) TC4-ISSUES assigns tC4 #801 ('a refusal that can never succeed lets Home proceed and keeps the text') to A4b with the test `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text`. A4b is done, but no test of that name, and no reference to #801, exists in RBL crates/ or parity/.
  - evidence: RBL docs/TC4-ISSUES.md:403 (#801 row, unit A4b) and :282 (E1a, A4b, X); grep for 'a_save_refused_for_good', '#801' and 'project_replaced' in RBL crates/ and parity/: only i18n catalog lines match
  - rec: Check in a browser run whether a permanently refused save blocks Home today. Add the named test (port tC4 sync-refusal.spec.ts J2 and J6) to A4b's suite, or move the row to E1a.
- [F13] (low/high) Some tC4 app-language and help cases are unported and have no owning unit: app-language k (#650, helps of a left-to-right package under Arabic), and the in-project halves of help a and app-language h and j.
  - evidence: RBL APP-DESIGN.md:3071: '... and k and the in-project halves of help a and app-language h and j are still open'; RBL parity/app/journeys/app-language.test.mjs has 12 tests and none named k; tC4 e2e/app-language.spec.ts:890 test 'k. #650 ...'
  - rec: Assign them to R1 (the reference move) or to C3 and B3 as a small follow-up. Port tC4 case k as a browser journey.
- [F14] (medium/high) Some parity items need a person, not code: the Windows signing credential (D99), the help-desk (SendGrid) credential for Report a problem (Q-2), the platform choice (F2), the Q-9 designs that C7b waits on, and translations of rebuild-only keys into nine languages (Q-10).
  - evidence: tC4 scripts/desktop-main.cjs:51-63 and APP-DESIGN Q-2 at :3555 (SendGrid from Electron main); tC4 docs/DECISIONS.md D99 (Windows ships signed); RBL APP-DESIGN Q-9 at :3562 and Q-10 at :3563; PROGRESS.md 'What needs the owner'
  - rec: List these in HANDOFF under 'needs the owner', with a recommendation and a date each, so G1b, C7b and the release are not blocked late.

## Unverified
- None of the rebuild's tests were run (cargo and npm were forbidden), so 'built' means the code and test exist at f9067bc, not that they pass. Check: build tc4 with test-hooks and fixture-backend plus build-app.sh, then run `node --test parity/app/journeys/*.test.mjs` with TC4_BIN, TC4_UI and Playwright set.
- Whether the rebuild compiles or runs on macOS: there is no macOS CI and nothing was built. Check: `cargo check --workspace --target aarch64-apple-darwin` on a Mac runner.
- How tc_align::reflow_edit behaves on the #820 replace case ('Dios Dios' to 'Dios Pablo'): the expected result (link returns to the bank) is inferred from reflow.rs:8-16. Check: add and run the test proposed in F7.
- Whether some other code path already gives a permanently refused save a way to Home (#801): only grep was used. Check: a browser run that makes a save refuse for good and then clicks Home.
- Whether the Burrito import kind could detect a tC4 4.0 project once I exists: I and F1 are not built, so this is a design question, not a code fact.
- The storage branches (s2-proof-executor 3b075d7, storage dd63328) were checked only for crate lists. They were not examined for user-visible features, because their work is storage internals.
- tC4's final v4.0.0 contents: no v4.0.0 tag exists. rc.2 (a5ce0307) was the latest at the time of reading; later rc.N tags may add changes.
- The inventory of tC4 features comes from tC4 src/views, state.jsx action names, docs/JOURNEYS.md, PACKAGING.md, VISION.md and the rebuild's ui-inventory.md (written at d494a9d). Behaviour inside state.jsx not reflected in those may be missing. Check: diff the i18n keys of tC4 en.json at rc.2 against the keys the rebuild's tc-ui uses.
- Live Door43 and Aquifer behaviour was not exercised; the network is blocked here per PLAN.