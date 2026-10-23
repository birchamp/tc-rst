---
id: C7a
title: Conflict choices: text and passage
track: A
lane: app
milestone: M3
state: gating
owner: C7a session (Claude Code 3058c665)
branch: track-C7a
worktree: .worktrees/track-C7a
depends: []
needs_owner: []
proves: [P1, P2, P3]
no_loss: true
size: 3-4 sessions (basis: B11 and C4)
merged: none
updated: 2026-10-23
next: Read .enjoy-logs/c7a-gates.log in the worktree; on a pass, merge C7a into main as TRACKING says.
blocked_by: none
---

# C7a: Conflict choices: text and passage

## Goal

Every kind on the review's Text tab can be resolved: verse text, verse structure, passage, structure
issue, book header, chapter front, and the story kinds `tab_of` also puts there (story base, frame,
story reference). History has *Restore this version*. A bulk keep keeps one person's versions over
many conflicts. A refused choice says why in words. Every new button sends one request per press,
however often it is pressed.

## Why it matters to a translator

Today a joined verse, a passage or a chapter opening in conflict only says "Every version is kept".
After this the translator can choose, and no choice writes over a version they were not shown.

## Before you start

Run these in the worktree; each must print what is said.

- `git merge-base --is-ancestor f07fd9e HEAD && echo yes` prints `yes` (B11 is merged).
- `grep -c '^B11' .audit/minors.tsv` prints `2`. If `0`, finish B11's records first
  (`.audit/plans/HANDOFF-resume-lane.md`).
- Merge main (7285d9d or later) into `resume-build-lane` so `.audit/roadmap/` is on the branch;
  then claim this card in a commit whose subject starts `Claim C7a` (INT-4 step 1 looks for it).
  If `git log -1 --format=%ci resume-build-lane` and the worktree's status show no change for
  24 hours, the session that held this card may be gone: `TRACKING.md` "pick up an item someone
  else started" says how to decide and take over (ask the owner first).
- `grep -n 'pub keep: ChangeRef' crates/tc-api/src/conflicts.rs` prints line 382.
- `grep -n 'fn entry_keeps' crates/tc-ui/src/views/review.rs` prints line 301.
- `grep -n 'Choice::Candidate { head: keep }' crates/tc-app/src/conflicts.rs` prints line 599.
- `grep -n 'Command::Restore {\|Command::ResolveBulk {' crates/tc-store/src/commands.rs` prints
  two lines, 197 and 205 (storage already does every choice).

## Funnel card

- **Invariants**:
  - INV-A-CHOICE-NEVER-OVERWRITES-AN-UNSEEN-VERSION: a choice, a restore or a bulk keep sends one
    token per record it writes, each listing every version the screen showed; if a record has a
    version the screen did not show, nothing is written (`NotSaved::LookAgain`).
  - INV-ONE-RESOLVE-PER-PRESS: a second press before an answer sends the same request id; the reply
    cache gives the first reply and nothing more is written; the button is off while its request is
    in flight.
  - INV-EVERY-KIND-HAS-A-CHOICE: the offered choices come from one `match` on `ConflictKind` with no
    wildcard; a kind with no choice yet answers `Later` in its own arm.
- **Wrong states**:
  - A passage choice sent with one record's token only: it would overwrite a teammate's verse of the
    same passage (a teammate joined 1:4-5 while I changed 1:5).
  - A bulk keep that picks between two versions of one person, so a clock decides.
  - `busy == true` with `sent == None`, or an `outcome` while `busy`: representable in `ReviewState`
    today (`crates/tc-ui/src/app/review.rs:22-40`).
  - A refusal shown as raw text: `failure_text` has an `other =>` arm
    (`crates/tc-ui/src/app/web/review.rs:19-26`).
- **Code**:
  - Types (all to create):
    - `KeepPhase` in `crates/tc-ui/src/app/review.rs` (done, step 1): `Idle`, `InFlight(SentKeep)`,
      `ReplyLost { sent: SentKeep, error }`, `Answered { outcome: Option<KeepOutcome> }`, with
      `press`, `reply(KeepReply)`, `dismiss`, `busy`, `outcome`. `ReplyLost` keeps A-27: after a
      lost reply the same press resends the same request, and its `error` is what the screen says
      meanwhile. `SentKeep` holds the conflict, the choice and the request id, not a `Confirm`
      (`crates/tc-ui/src/app/confirm.rs`): `Confirm` has no "reply lost, press again" state, and
      nesting it would carry a second copy of the phase. `KeepReply` (`Kept`, `Refused`, `Lost`) is
      made from `Result<Ack, CallError>` by one match on every `CallError` variant.
    - `KeepChoice` in `crates/tc-api/src/conflicts.rs`: `Candidate { change }`, `Passage { picks }`,
      `Separate { picks }`, `Joined { picks }` (`picks: Vec<ChangeRef>`), and, after the review
      (fix 1), `Restore { from }`. `Resolve.keep` becomes `Resolve.choice: KeepChoice`.
      `Version.place` names each version's record (fix 2).
    - Withdrawn after the review (fix 1): `conflicts/restore` and `HistoryView.shown`. A restore
      through `Command::Restore` carries an edit token, which on a record in conflict names the
      shown head only, so a restore of the text the versions started from let the fold settle
      the conflict on the other side by its merge rule and hide the translator's version. In the
      review every History is on a record in conflict, so Restore is now `KeepChoice::Restore`
      through `choose`: the old version's record as the fold's `Values` over the entry's review
      tokens, kept over every head and refused when a head moved; its History cause is Kept.
      The former design was:
      `ResolveBulk { at, request, person: Who, conflicts: Vec<BulkConflict { conflict, review }> }`
      (`conflicts/resolveBulk`) with reply `BulkKept { ack, left: Vec<ConflictRef> }`;
      `HistoryView.shown: ShownToken` (done, step 2). Decision (step 2): `shown` is the record's
      edit token (`mint(key, None)`), the kind the store's `restore` takes (`ctx.edit`), so on a
      record in conflict a Restore writes over the shown version only and the conflict stays
      listed with the restored text as one side; a review token would need a change to
      `tc-store`, which this card does not make. `Who` now refuses unknown fields, as every
      request does.
    - `Choices` in `crates/tc-ui/src/app/review.rs`: `Candidate`, `PassageSides`,
      `JoinedOrSeparate`, `Later`, made by `fn choices(kind: ConflictKind) -> Choices` (done,
      step 4). `Press` (`Keep`, `Restore`, `KeepAll`) is what `KeepPhase` tracks, so Restore and
      the bulk keep get the double-submit rule from the same enum; every press goes through one
      `Actions::send`. `SaveRefusal::Choice { reason }` (tc-api) carries the store's resolve and
      unreadable refusals, so the review says why in words; `failure_text` matches every
      `ApiError`, `NotSaved` and `CallError` variant by name. Decision (step 4): the structure
      footer says "Keep the verses separate", not "Keep 4 and 5 separate": the entry does not
      name its range, and the words go to the owner in H5's packet (C7b step 2) anyway; a minor.
  - One entry point: `fn choose(conflict, review, &KeepChoice) -> Result<StoreCommand, ApiError>`
    (to create) in `crates/tc-app/src/conflicts.rs`. It maps every `KeepChoice` to
    `tc_journal::command::Choice` (`Candidate`, `KeepPassage`, `KeepSeparate`, `KeepJoined`) with no
    wildcard, and is the only caller of `StoreCommand::resolve`
    (`crates/tc-app/src/backend/mod.rs:551`).
  - Removed: `ReviewState.busy`, `.sent`, `.outcome`; `entry_keeps`; `failure_text`'s `other =>`
    arm.
  - Left to checks, and why: whether a token lists every head is known only to the fold, so the
    store checks it at run time (`check_review_tokens`, `crates/tc-store/src/commands.rs:1281`);
    tests prove the app sends one token per record.
- **Checks** (kind: unit through `App::request` on the store, in
  `crates/tc-app/tests/app/conflicts.rs`, runs in CI's `rust` job, unless marked):
  - `keep_passage_resolves_every_record_of_the_group` (to create). Proof: send only
    `entry.review[0]` (temporary edit); verse 1:5's value must fail the test.
  - `restore_writes_over_every_shown_head` (to create). Proof: in the store's `restore`
    (`crates/tc-store/src/commands.rs:1390`), pass `&[]` as parents (temporary edit).
  - `restore_writes_a_new_version_and_history_keeps_the_old` (to create, #78). Finding (step 3):
    a Restore in a conflict of the text the versions started from is no edit against their base,
    so the fold settles the conflict on the other side's edit by its merge rule (§7.3); the
    no-loss test restores an earlier edit of this computer's instead, and C7b's History words
    may say so.
  - `bulk_keep_leaves_out_conflicts_with_two_candidates` (to create). Proof: resolve every listed
    conflict in the handler (temporary edit).
  - `every_kind_has_its_choices` and `a_look_again_refusal_says_another_version_was_kept` (to
    create) in `crates/tc-ui/tests/app/review.rs`; the first asserts `choices` for all 25
    `ConflictKind` variants. The existing `a_second_press_of_one_choice_sends_the_first_request` and
    `a_press_after_an_answer_is_a_new_request` (:207, :228) keep passing.
  - `a_restore_costs_its_record` and `a_bulk_keep_costs_its_conflicts` (to create) in
    `crates/tc-app/tests/op_counts/conflicts.rs`; kind: count.
  - Three new `browserTest`s (to create: passage keep, Restore, double press) in the existing
    `parity/app/journeys/conflict-review.test.mjs`; kind: journey; they skip in CI until G5.
  - No translator can make a newer version arrive while the review is open on this branch (no
    outside-edit check before S2 and D1), so the look-again journey is D1's step 5; the store's
    refusal is tested (`resolve_with_moved_heads_is_look_again`, `store_cases.rs:290`).
- **Prose**: `docs/PLAN.md` "Where the rebuild differs from tC4 4.0 on purpose": the rebuild-only
  lines on the Text tab (B11's minor), the passage and structure choices, Restore, bulk keep.
- **Human**: none now. History with Restore, the bulk button and the passage footer go into H5's
  packet, which C7b step 2 sends.

## Steps

### Step 1: Replace the keep flags with KeepPhase

- Do: In `crates/tc-ui/src/app/review.rs`, replace `busy`, `outcome` and `sent` (lines 32-37) with
  `keep: KeepPhase`; keep `SentKeep.change: ChangeRef` for now; move `request_for` and `answered`
  onto `KeepPhase`. In `crates/tc-ui/src/app/web/review.rs`, `keep_version` sets `InFlight` before
  the call, `Answered` on `Ok` or `CallError::Api`, and `ReplyLost` on `CallError::Transport` or
  `CallError::Dropped`. `failure_text` matches every `CallError` variant by name.
- Files: `crates/tc-ui/src/app/review.rs`, `crates/tc-ui/src/app/web/review.rs`,
  `crates/tc-ui/src/views/review.rs`, `crates/tc-ui/tests/app/review.rs`.
- Done when: `cargo test -p tc-ui --test app app::review::` passes all 14 tests;
  `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings`
  passes; `grep -n 'pub busy\|pub sent\|pub outcome' crates/tc-ui/src/app/review.rs` prints nothing.
- Record: append a log line `step 1`; set `state: building`, `updated` and `next`.

### Step 2: Widen the wire and add the one entry point

- Do: Add the tc-api types of the funnel card. Add `command "conflicts/restore"` and
  `command "conflicts/resolveBulk"` after `conflicts/resolve` in `crates/tc-api/src/lib.rs` (line
  174) and in `PHASE_0` of `crates/tc-api/tests/endpoints.rs`; add their examples in
  `crates/tc-api/tests/golden.rs`. In `crates/tc-app/src/conflicts.rs`, add `choose` and call it
  from the `Resolve` handler; add a `Restore` handler (`Command::Restore` with the token History
  sent) and a `ResolveBulk` handler (the devices whose `FoldState::person_labels` label has
  `person`, then `Command::ResolveBulk`; reply with the conflicts left out). Add
  `StoreCommand::restore` and `StoreCommand::resolve_bulk` (to create) in
  `crates/tc-app/src/backend/mod.rs`. In tc-ui, `SentKeep.change` becomes `choice`.
- Files: as named; `crates/tc-api/tests/golden/endpoints/conflicts/`.
- Done when: `UPDATE_GOLDEN=1 cargo test -p tc-api --test golden` writes `restore.json` and
  `resolveBulk.json`; then `cargo test -p tc-api`,
  `cargo test -p tc-server --test routes every_endpoint_is_routed_once` and
  `cargo test -p tc-app --test app app::conflicts::` pass;
  `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings` passes.
- Record: log line `step 2`; update `next`.

### Step 3: Prove each choice on the store

- Do: Add a scripted teammates' edit that writes several verses of one chapter in one batch (a join
  of Titus 1:4-5), beside `teammates_edit_verse` (`crates/tc-app/src/backend/mod.rs:1087`,
  `memory.rs` module `teammates`, `scenario_store.rs`), reachable from
  `tc_api::test::ScriptedSync::Synced` (to create: a field). Copy the shape of
  `a_join_against_an_edit_of_a_joined_verse_is_one_passage_conflict`
  (`crates/tc-journal/tests/fold.rs:171`). Write the tc-app tests and the count tests. Run each with
  its proof edit, paste the first failure line into the log, then revert.
- Files: as named; `crates/tc-api/src/test.rs`; `crates/tc-app/tests/app/conflicts.rs`;
  `crates/tc-app/tests/op_counts/conflicts.rs`.
- Done when: `cargo test -p tc-app --test app app::conflicts::` and
  `cargo test -p tc-app --test op_counts op_counts::conflicts::` pass, with
  `conflict_tab_costs_its_conflicts` unchanged; each proof edit failed its test.
- Record: log line `step 3` with the failure lines; update `next`.

### Step 4: Draw the choices

- Do: Add `Choices` and `choices`. In `crates/tc-ui/src/views/review.rs`, delete `entry_keeps` and
  draw the footer per `Choices`: *Keep this version*; *Keep <person>'s passage*; *Keep the joined
  verses* and *Keep <n> and <m> separate*, each asking which version where a record holds two
  (STORAGE §8.3); `later_bar` only for `Later`. Add *Restore this version* to each History row, and
  a bulk button *Keep all of <person>'s* whose result names how many conflicts were left out. Put
  the words in `crates/tc-ui/assets/i18n-rebuild/en.json` and each key in every `*.pending.json`.
  These three screens have no tC4 design. Do not capture them here: C7b step 2 draws them on its
  probe page and sends them to the owner as part of H5's packet.
- Files: `crates/tc-ui/src/app/review.rs`, `crates/tc-ui/src/views/review.rs`,
  `crates/tc-ui/src/app/web/review.rs`, `crates/tc-ui/assets/i18n-rebuild/*.json`,
  `crates/tc-ui/tests/app/review.rs`.
- Done when: `cargo test -p tc-ui --test app app::review::` and
  `cargo test -p tc-api --test refusals` pass; the four tc-ui wasm32 clippy commands pass.
- Record: log line `step 4`; update `next`.

### Step 5: Journeys and captures

- Do: Add a scripted passage to the environment (`parity/app/env-rust.mjs`, beside
  `teammatesChangeOneVerse`). In `parity/app/journeys/conflict-review.test.mjs`, add three
  `browserTest`s: a passage keep after a scripted sync that joined 1:4-5 while this computer changed
  1:5; Restore from History; a double press on each new button writes one resolution. Add `"C7a"`
  to `landed` in `parity/app/units.json` (the capture refuses a unit not listed there). The bulk
  button and Restore change the DOM of the three tC4 review cases, so capture them again:
  `node parity/app/capture-rust.mjs --unit C7a --case conflict-review,conflict-review-selected,conflict-review-mine --backend memory+scripted`
  (the backend B11 captured them with, `parity/app/captures.json`), then
  `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`. Explain each new DOM line in the three
  cases' entries in `parity/app/residuals.json` (they already explain B11's History button).
- Files: `parity/app/env-rust.mjs`, `parity/app/journeys/conflict-review.test.mjs`,
  `parity/app/units.json`, `parity/app/captures.json`, `parity/app/dom/`, `parity/app/screens/`,
  `parity/app/report.json`, `parity/app/residuals.json`.
- Done when: the journey run (Gates) of `conflict-review.test.mjs` passes three times in a row;
  `cargo test -p tc-ui --test screen_record` passes.
- Record: log line `step 5` with the three run summaries; update `next`.

### Step 6: Write down the differences

- Do: Add C7a's differences and B11's minor (the group, incomplete-history and unreadable lines on
  the Text tab) to `docs/PLAN.md` "Where the rebuild differs from tC4 4.0 on purpose". Put the new
  test names in `docs/TC4-ISSUES.md` rows #78, #25 and #679, and the refinements in C7a's row of
  `docs/APP-DESIGN.md` (line 3396).
- Files: `docs/PLAN.md`, `docs/TC4-ISSUES.md`, `docs/APP-DESIGN.md`.
- Done when: `cargo test -p tc-ui --test screen_record` passes (it reads PLAN's rulings); for each
  test name you wrote into TC4-ISSUES, `grep -rn '<name>' crates parity` finds it.
- Record: log line `step 6`; set `state: verifying`; update `next`.

## Gates before merge

Until INT-3 lands: every command of AGENTS.md "Tests" plus the Windows step of
`.github/workflows/ci.yml`, listed here. After INT-3: `scripts/gates.sh` (to create in INT-3).

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, then the
   same with `--features gallery`, `--features app` and `--features test-hooks`.
5. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
6. `cargo clippy -p tc-app --lib -- -D warnings`
7. `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
8. `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`

Plus:

- The journey run: `cargo build -p tc4 --features test-hooks,fixture-backend`,
  `cargo build -p tc-door43-fake`, `crates/tc-ui/build-app.sh`, then
  `TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake node --test parity/app/journeys/conflict-review.test.mjs`.
  It prints `# fail 0` and `# skipped 0`, three runs in a row.
- `cargo test -p tc-ui --test screen_record` and
  `cargo test -p tc-app --test op_counts op_counts::conflicts::` pass.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A choice would write a record of a conflict without that record's token: that breaks
  INV-A-CHOICE-NEVER-OVERWRITES-AN-UNSEEN-VERSION. Stop and write it in the log.
- You need to change `tc-store` or `tc-journal`: write the need in the log and ask the storage
  lane's owner.
- A required gate fails twice in a row and you cannot tell why.

## References

- `.audit/roadmap/evidence/2026-10-21/app-remaining.md` (C7a; F2), `health-app.md` (CH-4, CH-9, §8
  item 3), `tc4-bugs.md` (§2 row C7a)
- `docs/STORAGE.md` §8.3 and §8.5 (the choices and their words); `docs/APP-DESIGN.md:3382` (B11) and
  `:3396` (C7a)
- `crates/tc-journal/src/command.rs:266` (`Choice`); `crates/tc-journal/src/fold/resolve.rs:80`
  (`resolution`); `crates/tc-store/src/commands.rs:1304-1345, 1390` (resolve, bulk, restore)

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/app-remaining.md
- 2026-10-21 | roadmap | note | f9067bc | next in the resume lane session's own plan (.audit/plans/HANDOFF-resume-lane.md); it adopts this card at its next boundary | .worktrees/resume-build-lane/.audit/plans/HANDOFF-resume-lane.md
- 2026-10-22 | roadmap session (Cursor, review of 2026-10-22) | note | none | `KeepPhase` (step 1) is the first `NetEnd<T>` in the UI; UI-3 step 2b generalises it, so a verifier of this card does not ask for the generalisation here. Last sign of life of the lane: f9067bc 2026-10-21 12:16; the gone rule is TRACKING.md "pick up" | .audit/roadmap/evidence/2026-10-21/review-2026-10-22.md
- 2026-10-22 | roadmap session (Cursor) | takeover | 43496296 | the owner confirmed at 12:53 that the tC4 Rebuild session (Enjoy d8a954f9) has ended; its two uncommitted minors rows are committed as WIP on resume-build-lane (43496296); the card is released (todo, no owner) because C7a's step 1 was never started; INT-4 lands the branch; `depends` stays empty because B11 is done and C7a's code starts from main after INT-4 | .worktrees/resume-build-lane
- 2026-10-22 20:50 EDT | C7a session (Claude Code 3058c665) | claim | none | claimed before INT-4 lands: track-C7a is cut from resume-build-lane 43496296 (B11, the two B11 minors rows) with main 81cedaca merged in (7c84cef0, HANDOFF.md took main's text); the merge into main waits for INT-4, as the start session said | .worktrees/track-C7a
- 2026-10-22 21:20 EDT | C7a session (Claude Code 3058c665) | step 1 | none | `ReviewState.busy/.outcome/.sent` are one `KeepPhase`; `keep_version` presses it and feeds it a `KeepReply` built by a match on every `CallError` variant; the 14 review tests pass plus `a_press_while_a_keep_is_in_flight_sends_nothing` (15); wasm clippy with `--features app` passes; decision logged in the funnel card: `SentKeep` holds the request id, not a `Confirm` | cargo test -p tc-ui --test app app::review::
- 2026-10-22 22:05 EDT | C7a session (Claude Code 3058c665) | step 2 | none | `KeepChoice`, `Resolve.choice`, `Restore`, `ResolveBulk`/`BulkKept` and `HistoryView.shown` on the wire; `choose` is the only caller of `StoreCommand::resolve`; `Restore` and `ResolveBulk` handlers and the two `StoreCommand` constructors; goldens `restore.json` and `resolveBulk.json` written; tc-api (all), routes, tc-app `app::conflicts::` (7) and tc-ui `app::review::` (15) pass; both wasm clippy runs pass. The journey `conflict-review.test.mjs` still reads `sent[0].keep`: step 5 moves it to `choice` | cargo test -p tc-api; cargo test -p tc-app --test app app::conflicts::
- 2026-10-22 23:40 EDT | C7a session (Claude Code 3058c665) | step 3 | none | the scripted join (`ScriptedSync::Synced.joined`, `Teammates::join_verses`, one pack per batch) and `Teammates::sign_in`; tc-app `app::conflicts::` passes 12 (5 new: keep passage both sides, two restores, bulk); `op_counts::conflicts::` passes 4 (2 new). Proofs, each reverted: `review: vec![entry.review[0].clone()]` → `keep_passage_resolves_every_record_of_the_group` panicked `called Result::unwrap() on an Err value: NotSaved(LookAgain)`; store `restore` with `&[]` parents → `restore_writes_over_every_shown_head` failed `the shown version is replaced, the others stay: left: ["Pablo, mi primera versión.", "Pablo, mi segunda versión.", "Pablo, versión del compañero 1.", "Pablo, versión del compañero 2."] right: [3 texts]` (`restore_writes_a_new_version_and_history_keeps_the_old` still passed: it proves #78, not the parents); handler over every person's devices → `bulk_keep_leaves_out_conflicts_with_two_candidates` failed `left: Unchanged right: Written`. A bulk keep's work shows as `changes_indexed` (one per record kept), not `keys_updated`. Note: `tc-api --test roundtrip` takes 288 s here (CHK-4's budget is 300 s; it was over it while shrinking) | cargo test -p tc-app --test app app::conflicts::; cargo test -p tc-app --test op_counts op_counts::conflicts::
- 2026-10-23 00:35 EDT | C7a session (Claude Code 3058c665) | step 4 | none | `Choices`/`choices` (25 arms), `Press`, the footer per kind (`passage_bar`, `structure_bar`), *Restore this version* on every History row but the shown one, the bulk row `conflict-bulk` with *Keep all of <person>'s*, the outcome callout per `KeepOutcome`, 14 words in `en.json` and every `*.pending.json`; `entry_keeps` deleted; `SaveRefusal::Choice`; tc-ui `app::review::` passes 17 (new: `every_kind_has_its_choices`, `a_look_again_refusal_says_another_version_was_kept`), `tc-api --test refusals` passes, the four wasm clippy runs and the native clippy of tc-api, tc-app, tc-ui, tc-server, tc4 pass | cargo test -p tc-ui --test app app::review::; cargo test -p tc-api --test refusals
- 2026-10-23 02:20 EDT | C7a session (Claude Code 3058c665) | step 5 | none | `env.teammatesJoinVerses`, three `browserTest`s in `conflict-review.test.mjs` (passage keep, Restore from History, bulk keep; each double-pressed sends one request), J18's assertion moved to `choice`, `"C7a"` in `landed`. Three runs in a row: `# pass 6 # fail 0 # skipped 0` each (target/journey-run1..3.log, PLAYWRIGHT_MODULE=/home/bot/tc-rst/parity/tc4-baseline/node_modules/playwright). Captures: `capture-rust.mjs` refuses a partial run because this machine's font list changed since B11's captures (535 to 542 fonts; a partial recapture would mix machines); recaptured into a scratch folder instead and diffed the DOM of the three cases against `parity/app/dom/`: the only differences are the UI build hash in the logo path and my own version's date, both already in residuals.json, so C7a changes none of the three screens (History closed, one conflict: no Restore row, no bulk row) and `captures.json`, `dom/`, `screens/` and `residuals.json` stay as B11 left them. CHK-6 recaptures everything on one machine. `cargo test -p tc-ui --test screen_record` passes (48) | node --test parity/app/journeys/conflict-review.test.mjs
- 2026-10-23 02:25 EDT | C7a session (Claude Code 3058c665) | step 6 | none | PLAN.md "Where the rebuild differs" gains the review's lines (B11's minor row 72) and C7a's passage, structure, Restore, bulk and refusal-words differences; TC4-ISSUES rows #679, #78 and #25 name the new tests (each found by grep in crates/parity); APP-DESIGN's C7a row carries the refinements; the minors row of step 4 had a stray dash in its unit column, corrected; `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record` passes and changes report.json not at all | cargo test -p tc-ui --test screen_record
- 2026-10-23 03:30 EDT | loss lens (Claude Code subagent of 3058c665) | verify | 3a3dba06 | Code: structure holds for Resolve and ResolveBulk (one entry point, tokens checked by the store), not for Restore, whose edit token lists the shown head only. Checks: the named tests exist, run in CI's `app` bin, each proof recorded; the bulk test failed once in three runs on the project's lock. Reach: Text tab, History, bulk row; dropped a sync during the review. Siblings: the edit-token writers share the rule; only Restore is offered on a record in conflict. No loss: one blocker, a Restore of the versions' base text settles the conflict on the other side's edit and hides the translator's version. Prose and Human: differences declared; the Restore behaviour needs words or withholding. Verdict: not a pass until the Restore blocker is fixed | the lens's report, quoted in this line
- 2026-10-23 03:30 EDT | races and protocol lens (Claude Code subagent of 3058c665) | verify | 3a3dba06 | Code: `KeepPhase` and one `Actions::send` carry INV-ONE-RESOLVE-PER-PRESS; one gap, a reply is not matched to its request. Checks: the three press tests and the four server tests pass, each double send through the real reply cache. Reach: double press, lost reply, steps and tab changes, a top-bar leave while in flight, a reload. Siblings: `Confirm` and Create deferred to UI-3 as the card says. No loss: every write's tokens are checked or skipped by the store before any write. Prose and Human: reasons in comments; no owner question. Verdict: no blocker; one major, a reply after a top-bar leave and reopen can be shown for the wrong press; minors F2-F6 | the lens's report, quoted in this line
- 2026-10-23 03:30 EDT | browser and parity lens (Claude Code subagent of 3058c665) | verify | 3a3dba06 | Code: `choices` has no wildcard, `failure_text` one arm per variant; gaps: a refusal's reason is a free string that can print a record key, and a structure press can be sent with no pick. Checks: 17 tc-ui tests; the journeys press every new button twice. Reach: passage and structure conflicts from a sync, Restore, the bulk keep, read-only. Siblings: refusal-by-words matches `check_item.rs`. No loss: every write carries what the screen showed. Prose and Human: five bullets in PLAN, APP-DESIGN's row; four differences undeclared. Verdict: not a pass until F1 (a passage's version cards do not name their verse) and F2 (a structure press with no pick comes back as key text) are fixed; F3-F10 minors | the lens's report, quoted in this line
- 2026-10-23 04:20 EDT | C7a session (Claude Code 3058c665) | fix | none | fix 1 (loss blocker): Restore is `KeepChoice::Restore` through `choose` (the fold's `Values` over the entry's tokens, every head a parent); `conflicts/restore`, `HistoryView.shown` and `StoreCommand::restore` removed; `restore_writes_over_every_shown_head` now proves it on a three-version conflict (proof: `Choice::Candidate { head: from }` in its place → `NotSaved(Refused(Choice { reason: "the chosen version is not one of the conflict's versions" }))`, reverted). fix 2 (browser F1, major): `Version.place`; a card names its verse when it is not the conflict's ("Titus 1:5"), the journey asserts it. fix 3 (browser F2, major): the structure footer's buttons are off until a version is clicked, both secondary. fix 4 (races F1, major): `KeepPhase::reply(request, reply)` ignores a reply to another request; a test asserts it. fix 5 (loss F3): the bulk test waits for the closed project's lock; `app::conflicts::` passed 3 runs of 11. fix 6 (browser F3, words): "Your version was kept in {kept} of {sent} conflicts." and "{person}’s version was kept in {kept} of {sent} conflicts."; four more differences declared in PLAN. Count tests: `a_restore_costs_its_record` on a listed conflict, 4 pass. Clippy: native and the four wasm runs pass | cargo test -p tc-app --test app app::conflicts::; cargo test -p tc-ui --test app app::review::
- 2026-10-23 04:50 EDT | C7a session (Claude Code 3058c665) | block | none | after the fixes the journey passes three runs in a row (`# pass 6 # fail 0 # skipped 0`, target/journey3-run1..3.log; the first rerun failed on my own assertion: both versions of verse 5, the covered one and mine, name "Titus 1:5", as they should). Every step is done and verified; the merge waits for INT-4 to land resume-build-lane on main. Then: merge main into track-C7a, run "Gates before merge", merge. Eleven minors appended to .audit/minors.tsv; a hand-over note is in the shared notes folder (C7a-session-notes.md) | node --test parity/app/journeys/conflict-review.test.mjs
- 2026-10-23 10:05 EDT | C7a session (Claude Code 3058c665) | unblock | b9491277 | INT-4 landed resume-build-lane on main (d293375a); main merged into track-C7a, the one conflict (.audit/minors.tsv) kept every row of both sides | git log -1 b9491277
- 2026-10-23 10:06 EDT | C7a session (Claude Code 3058c665) | gate | none | started `. ~/.tc-env && PLAYWRIGHT_MODULE=/home/bot/tc-rst/parity/tc4-baseline/node_modules/playwright scripts/gates.sh --journeys` in .worktrees/track-C7a (second heavy run beside INT-4's gate) | .worktrees/track-C7a/.enjoy-logs/c7a-gates.log
