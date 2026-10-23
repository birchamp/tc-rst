---
id: INT-7
title: Prepare the Track B hand-off: freeze the shell contract
track: A
lane: integration
milestone: M0
state: building
owner: integration lane (Claude Code 3d07d5f8)
branch: track-INT-7
worktree: .worktrees/track-INT-7
depends: [INT-4]
needs_owner: []
proves: []
no_loss: true
size: 1 d (basis: two contract files, two small tests, an APP-DESIGN edit)
merged: none
updated: 2026-10-23
next: Step 4: edit docs/APP-DESIGN.md §10 and §5.3 and docs/TC4-ISSUES.md row #743.
blocked_by: none
---

# INT-7: Prepare the Track B hand-off: freeze the shell contract

## Goal

The interface between the app and the Electronite shell is written down as version 1 of
`docs/contracts/SHELL.md`, and two host tests fail if either side drifts from it: one checks the
draft buffer's reduction (`current_lines`) against shared vectors, the other checks the bridge
calls the page makes against a list of names. `docs/APP-DESIGN.md` §10 lists G3, G4 and G5, and
G1 no longer waits for D2. §5.3 says what the owner ruled: a window within 1 s, no splash screen.
This is `TRACK-B.md` §3 steps 3 to 6.

## Why it matters to a translator

The shell keeps a copy of every keystroke in `drafts-buffer.log` and hands it back after a crash.
If the shell and the page reduce that log differently, text a translator typed is offered wrong
or not at all. The vectors make both sides prove the same rules.

## Before you start

- `python3 -I .audit/tools/board.py` shows INT-4 as `done`, and
  `git -C "$(git rev-parse --show-toplevel)" merge-base --is-ancestor f9067bc claude/focused-ride-5uzj7p && echo in`
  prints `in`.
- `grep -n 'pub fn current_lines' crates/tc-ui/src/app/buffer.rs` prints
  `pub fn current_lines(lines: &[String]) -> Vec<String> {` (line 396 at f9067bc). The module
  `tc_ui::app::buffer` builds on the host: only `app::web` is wasm-only (`src/app/mod.rs`).
- `grep -o 'method("[A-Za-z]*")' crates/tc-ui/src/app/web/bridge.rs | sort -u` prints seven
  names: `bufferChanged`, `feedbackSend`, `onCloseRequested`, `openExternal`, `printPdf`,
  `setCanClose`, `takeBuffered`. `TRACK-B.md` §4 item 5 names eleven: the page does not call
  `chooseFolder`, `chooseFile`, `onDownloadDone` or `platform` yet.
- `crates/tc-ui/tests/app.rs` is the host test binary (`#![cfg(not(target_arch = "wasm32"))]`);
  new modules go in `crates/tc-ui/tests/app/` and are listed in its `mod app { ... }`.
- `crates/tc-api/tests/golden/types/buffer.json` holds real `BufferLine` objects under
  `"lines"`: copy keys and entries from there.
- `ls .audit/roadmap/rulings/ | grep -E -- '-H2(-.*)?\.md$'` (the pattern `board.py` uses)
  tells you whether the owner chose Track B or B′ (step 5).
- Claim nothing on Track B (`TRACK-B.md` §3 step 8): G1, G3, G4 and G5 stay `todo`.

## Funnel card

- **Invariants**:
  - INV-SHELL-CONTRACT-PINNED: the page's buffer reduction and the bridge members it uses equal
    `docs/contracts/SHELL.md` v1; a drift on either side fails a test.
  - INV-BUFFER-REMOVAL-NAMES-TEXT: a removal line that names a text ends an entry only while
    the entry holds that text (compared in NFC); a removal ends only its own server run's entry.
  - INV-TORN-LINE-SKIPPED: a line of `drafts-buffer.log` that does not parse is skipped, and
    the lines around it still count.
- **Wrong states**:
  - The shell deletes an entry on any removal, as the test stub `Bridge.current()` in
    `crates/tc-ui/app/no-loss.mjs` does: newer typed text is dropped at relaunch (split-seams S9).
  - The shell renames a bridge member the page calls: the call silently does nothing.
  - G1 waits for D2 although only `printPdf` needs D2 (split-seams S2).
  - §5.3 says a repeat launch shows no window, against the owner's ruling (TC4-ISSUES #743).
- **Code**:
  - Types: none new. `tc_api::buffer::BufferLine` is the line format; `current_lines` is the
    reduction.
  - One entry point: `tc_ui::app::buffer::current_lines` is the single statement of the
    reduction that the vectors pin; the shell's node test reads the same vectors file.
  - Removed: nothing.
  - Left to checks, and why: the shell is JavaScript in another process, so no Rust type can
    bind it; shared vectors and a names list are the strongest link across the language line.
- **Checks**:
  - `the_buffer_reduction_matches_the_shell_contract_vectors` in
    `crates/tc-ui/tests/app/shell_contract.rs` (to create); kind: contract; runs in: CI,
    `cargo test --workspace`.
  - `the_bridge_members_the_page_uses_match_the_shell_contract` in the same file (to create);
    kind: contract; runs in: CI.
  - Proof it can fail: (1) in `current_lines`, drop the condition `&& same_text(held, &text)`;
    the vector "a removal naming another text keeps the entry" fails with
    `INV-SHELL-CONTRACT-PINNED:`. (2) Remove `takeBuffered` from `calledByThePage` in the JSON;
    the names test fails. Restore both.
- **Prose**: `docs/contracts/SHELL.md` (the contract itself); `docs/APP-DESIGN.md` §5.3 and §10.
- **Human**: none new. H2 decides only whether step 5 runs.

## Steps

### Step 1: Write the buffer vectors and their test

- Do: claim the card (`TRACKING.md`) in a worktree of its own:
  `git -C "$(git rev-parse --show-toplevel)" worktree add -b track-INT-7 .worktrees/track-INT-7 claude/focused-ride-5uzj7p`.
  Create `fixtures/shell/buffer-vectors.json` (to create):
  ```json
  {
    "version": 1,
    "about": "Lines of drafts-buffer.log and which of them the reduction keeps (SHELL.md item 6). Order is not part of the contract.",
    "vectors": [ { "name": "...", "lines": ["<one BufferLine as compact JSON>", "..."], "keep": [0] } ]
  }
  ```
  `keep` lists indexes into `lines`. Each line is one compact JSON string, as the log holds it;
  build them from the objects in `crates/tc-api/tests/golden/types/buffer.json`, for example
  with `python3 -I -c 'import json,sys; print(json.dumps(json.load(sys.stdin), separators=(",",":"), ensure_ascii=False))'`.
  Use at least these vectors. K and K2 are two fields; A and B are two server runs (32 hex
  digits, like `abab…` in the golden); "chg K/A x" is a changed line with text x, "rm K/A" a
  removal, "rm K/A x" a removal naming text x:

  | Name | Lines | keep |
  |---|---|---|
  | the last change of a field wins | chg K/A a; chg K/A ab | [1] |
  | a removal without text ends the entry | chg K/A a; rm K/A | [] |
  | a removal naming the held text ends the entry | chg K/A a; rm K/A a | [] |
  | a removal naming another text keeps the entry | chg K/A ab; rm K/A a | [0] |
  | texts are compared in NFC | chg K/A é (U+00E9); rm K/A e + U+0301 | [] |
  | a removal ends only its own run's entry | chg K/A a; chg K/B b; rm K/B | [0] |
  | fields are kept apart | chg K/A x; chg K2/A y; rm K/A | [1] |
  | a change after a removal starts a new entry | chg K/A a; rm K/A; chg K/A b | [2] |
  | a torn last line is skipped | chg K/A a; `{"type":"changed","value":{"key":` | [0] |
  | a line that does not parse in the middle is skipped | chg K/A a; `not json`; chg K2/A y | [0, 2] |

  Then create `crates/tc-ui/tests/app/shell_contract.rs` (to create) and add
  `mod shell_contract;` to `mod app` in `crates/tc-ui/tests/app.rs`. The test
  `the_buffer_reduction_matches_the_shell_contract_vectors` reads the file from
  `concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/shell/buffer-vectors.json")`, and for
  each vector compares `current_lines(&lines)` with the kept lines, both as a
  `BTreeSet<String>`, with the message
  `INV-SHELL-CONTRACT-PINNED: vector {name:?}: current_lines kept {got:?}, the contract keeps {want:?}`.
  Clippy runs on tests with `-D warnings`: use `.get(i)` and `usize::try_from`, not `[i]` and
  `as`. Before you compare as sets, read `merge` in `crates/tc-ui/src/app/buffer.rs` and
  confirm it does not depend on the order of the mirror's lines; if it does, compare in order
  and drop "Order is not part of the contract".
  Then prove it can fail: in `current_lines`, drop `&& same_text(held, &text)` from
  `if let Some(Some((_, held))) = newest.get(&slot) && same_text(held, &text)` (the stub's
  behavior), run, restore; then change `continue` to `break` in the `let Ok(parsed) = ... else`
  at the top of the loop, run, restore.
- Files: `fixtures/shell/buffer-vectors.json`, `crates/tc-ui/tests/app/shell_contract.rs`,
  `crates/tc-ui/tests/app.rs`.
- Done when: `cargo test -p tc-ui --test app shell_contract` passes; each mutation makes it fail
  with a message that starts `INV-SHELL-CONTRACT-PINNED:` and names a vector; afterwards
  `git diff crates/tc-ui/src` is empty.
- Record: a log line `step 1` quoting both failure messages.

### Step 2: Write the bridge names and their test

- Do: create `docs/contracts/shell-bridge.json` (to create):
  ```json
  {
    "version": 1,
    "methods": ["bufferChanged", "chooseFile", "chooseFolder", "feedbackSend", "onCloseRequested",
                "onDownloadDone", "openExternal", "platform", "printPdf", "setCanClose", "takeBuffered"],
    "calledByThePage": ["bufferChanged", "feedbackSend", "onCloseRequested", "openExternal",
                        "printPdf", "setCanClose", "takeBuffered"]
  }
  ```
  Add `the_bridge_members_the_page_uses_match_the_shell_contract` to `shell_contract.rs`. It
  asserts, each with a message starting `INV-SHELL-CONTRACT-PINNED:`:
  1. the set of names in `method("…")` calls in `crates/tc-ui/src/app/web/bridge.rs` (regex
     `method\("([A-Za-z]+)"\)`, the `regex` dev-dependency) equals `calledByThePage`;
  2. `calledByThePage` is a subset of `methods`;
  3. every name after `tc4Shell.` or `tc4Shell?.` in `crates/tc-ui/app/boot.js` (regex
     `tc4Shell\??\.([A-Za-z]+)`) is in `methods`;
  4. every name in `methods` appears as `` `name` `` in `docs/contracts/SHELL.md` (written in
     step 3; until then this assertion fails, which is expected). Commit steps 2 and 3
     together, so every commit passes its tests (`AGENTS.md` "Commits").
- Files: `docs/contracts/shell-bridge.json`, `crates/tc-ui/tests/app/shell_contract.rs`.
- Done when: after step 3, `cargo test -p tc-ui --test app shell_contract` passes; removing
  `"takeBuffered"` from `calledByThePage` makes assertion 1 fail; adding `"chooseFolder"` to it
  makes assertion 1 fail. Restore both.
- Record: a log line `step 2` quoting the failures.

### Step 3: Write SHELL.md version 1

- Do: create `docs/contracts/SHELL.md` (to create) with: a header "The shell contract, version
  1", the date, and the rule from `TRACK-B.md` §9: a change is a pull request that raises the
  version and that both sides approve. Then a table with one row per item of `TRACK-B.md` §4:
  the contract in words (copy §4's wording), where it is defined, and what tests it:

  | # | Defined in | Tested by |
  |---|---|---|
  | 1 | `crates/tc4/src/args.rs` (`USAGE`) | `crates/tc4/tests/release.rs` `release_has_no_test_hooks` (ignored: run after a release build, as its doc comment says) |
  | 2 | `crates/tc4/src/lib.rs` (`EXIT_SERVER_ERROR` 1, `EXIT_USAGE` 2, `EXIT_LOCK_HELD` 3, `EXIT_START_FAILED` 4) | `crates/tc4/tests/lifecycle.rs` `the_address_line_comes_first_and_names_loopback`, `new_tc4_waits_for_the_old_one_then_names_the_holder`, `bad_arguments_exit_2`, `a_ui_folder_that_is_missing_is_a_start_error`; exit 1 has no test: say so |
  | 3 | APP-DESIGN §5.1 | `lifecycle.rs` `quit_message_answers_closed_then_exits`, `closed_stdin_quits_within_5_s`, `sigterm_quits_like_the_quit_message` |
  | 4 | `crates/tc-api/src/endpoint.rs` (`TOKEN_HEADER`, `API_VERSION_HEADER`); `crates/tc-ui/app/boot.js` | `crates/tc-server/tests/security.rs` (`request_without_token_is_401` and its neighbours) |
  | 5 | `docs/contracts/shell-bridge.json`; `crates/tc-ui/src/app/web/bridge.rs` | `the_bridge_members_the_page_uses_match_the_shell_contract` (step 2) |
  | 6 | `crates/tc-api/src/buffer.rs` (`BufferLine`); `tc_ui::app::buffer::current_lines`, with the vectors' rules in words | `the_buffer_reduction_matches_the_shell_contract_vectors` (step 1); `crates/tc-api/tests/buffer.rs` |
  | 7 | `crates/tc-api/src/app.rs` (`BUILD_COMMIT`); `crates/tc-ui/build-app.sh` (wasm-bindgen 0.2.129) | `crates/tc-ui/tests/app/client.rs` `the_same_build_proceeds`; `crates/tc-server/tests/routes.rs` (`hello.build.commit`) |
  | 8 | `bundle/en/README.md` "Fetched when the app is packaged"; `bundle/en/fetched.lock` | `crates/tc-resources/tests/fetched_lock.rs` (`the_bundled_lexicons_match_the_trees_door43_reported`, `the_fixture_folder_matches_its_lock`); the OBS pack check is G3's (to create) |
  | 9 | tC4 `docs/PACKAGING.md` line 917 (tC4 b86270f); `.audit/tasks/g0-windows.md` step 4 | G3's release pipeline (to create) |

  Add the list of what the main team must not change while Track B is out (`TRACK-B.md` §6).
- Files: `docs/contracts/SHELL.md`.
- Done when: `cargo test -p tc-ui --test app shell_contract` passes, and every path the file
  names exists: check each with `ls` or `git ls-files <path>`.
- Record: a log line `step 3`.

### Step 4: Fix APP-DESIGN §10 and §5.3

- Do, in `docs/APP-DESIGN.md`:
  1. §10 "Phase 6", G1 row, "Depends on": `A2, A4b, A9, C1b, D2, G0` becomes
     `A2, A4b, A9, C1b, G0`, and add to its scope: "`printPdf(url)` is part of the shell
     contract (`docs/contracts/SHELL.md`), so D2 is not needed to build the shell; only its
     end-to-end PDF test waits for D2's `/print` route". Rename the row's unit cell
     `G1 Electronite shell` to `G1 ◆ Electronite shell`, and add G1 to the "No-loss rigor" list
     at the top of §10 (line 3337 at f9067bc: "A4b, B4b, C0, C1b, C2, C3 and C5"), because its
     typing mirror holds unsaved text (`TRACK-B.md` §3 step 6).
  2. G2 row: remove "the `app-e2e` CI job" (it moves to G5).
  3. Add three rows after G2, in the table's six columns (Unit, Scope, Depends on, Waits for,
     Tests, Done when), from `TRACK-B.md` §2 and `evidence/2026-10-21/split-seams.md` §3.3:
     G3 "Release pipeline and installers" (depends G1; waits for the owner's platforms and
     signing, ROADMAP H3 and H4, and Windows and macOS runners); G4 "P8 Graphite probe and
     right-to-left project journeys" (depends G1; the spike's `spikes/electronite/page/probe.js`
     run inside Electronite on three systems); G5 "Browser journeys in CI" (an `app-e2e`
     workflow that runs every `parity/app/journeys/*.test.mjs` and fails on any skip; depends
     on nothing unbuilt).
  4. §5.3, paragraph "Who opens the first-launch window": replace "The shell opens
     `/prepare.html#t=<token>` when `hello.startup` is `Preparing` and still is 300 ms later, and
     `/#t=<token>` otherwise, so a repeat launch shows no window (PERF rule 4; tC4 shows its
     window on every packaged launch, until its main window is ready: declared difference)."
     with "The shell shows its window on every launch within 1 s of the click, painted in the
     app's background colour, with no splash screen (owner ruling, 2026-10-18;
     `docs/PERFORMANCE.md` "Start-up"; tC4 #743). This section decides only which page the
     window loads: `/prepare.html#t=<token>` when `hello.startup` is `Preparing` and still is
     300 ms later, and `/#t=<token>` otherwise."
  5. §10, A9 row, its note D10: replace "the shell (G1) opens the window only when
     `hello.startup` is still `Preparing` 300 ms after launch, so a repeat launch shows none, a
     declared difference from tC4, which shows it on every launch" with "the shell (G1) shows
     its window on every launch within 1 s, and loads `/prepare.html` in it only when
     `hello.startup` is still `Preparing` 300 ms after launch (owner ruling, 2026-10-18; tC4
     #743)".
  The §5.3 sentence of item 4 is wrapped over lines 1727-1730 at f9067bc; match it across the
  line breaks.
  In `docs/TC4-ISSUES.md`, row `#743`: delete "Fix the §5.3 and D10 wording now." from its
  Action; in the list near the end, mark "fix §5.3 and A9 D10 (#743)" as done by INT-7. Keep
  the row's status `unit todo`: G1 still builds the window.
- Files: `docs/APP-DESIGN.md`, `docs/TC4-ISSUES.md`.
- Done when: `grep -n 'repeat launch shows no window\|repeat launch shows none' docs/APP-DESIGN.md`
  prints nothing; `grep -cE '^\| G[345] ' docs/APP-DESIGN.md` prints 3; the G1 row no longer
  contains `C1b, D2, G0`; `grep -c '^| G1 ◆ Electronite shell' docs/APP-DESIGN.md` prints 1;
  `grep -c 'Fix the §5.3 and D10 wording now' docs/TC4-ISSUES.md`
  prints 0.
- Record: a log line `step 4`.

### Step 5: Freeze the import core's skeleton, only if the owner chose B′

- Do: only if a ruling for H2 picks B′ (`TRACK-B.md` §8): add `crates/tc-import` (to create)
  with §8's final signatures (`identify`, `review`, `convert`, `verify`, `trait TagResolver`,
  `pub mod tc4` with `convert_segments`), each returning `Err(ImportRefusal::NotBuilt)`, a
  `thiserror` error type, no `tc-store` dependency, and one test per function. Otherwise log
  "step 5 skipped: H2 is <B / unanswered>"; a later B′ makes this the first step of `units/I.md`.
- Files: `crates/tc-import/**` (to create), `Cargo.toml` (`members`).
- Done when: the skip is logged, or `cargo test -p tc-import` passes and
  `cargo tree -p tc-import | grep -c tc-store` prints 0.
- Record: a log line `step 5`.

### Step 6: Verify with three lenses, then merge

- Do: this is a no-loss item (`FUNNEL.md` §8 point 5): run one verification pass with three
  lenses: (1) the vectors cover every rule `current_lines` states in its doc comment; (2) every
  pointer in SHELL.md matches the code it names; (3) nothing in the §5.3 and D10 edits changes
  what happens to a translator's unsaved text. Fix blockers and the majors that harm a
  translator's data or view; others go to `.audit/minors.tsv`. Then merge `track-INT-7` into
  main as `TRACKING.md` says, titled `Merge INT-7: the shell contract is frozen, so the
  shell keeps typed text the way the app does`, trailer `Unit: INT-7`.
- Files: this card, `.audit/minors.tsv`.
- Done when: the gates pass on the merged tree, CI is green on main, and
  `python3 -I .audit/tools/board.py --check` prints no error for INT-7.
- Record: log lines `verify` and `merge`.

## Gates before merge

`scripts/gates.sh` (INT-3; it holds the tc-ui wasm clippy steps) and
`scripts/gates.sh --check-receipt` on the exact commit main moves to, plus: the failures of
step 1's mutations and step 2's name edits are quoted in the log.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A vector shows that `current_lines` breaks a rule its own doc comment states: that is a
  no-loss finding. Stop, log `block`, and write a red-first test for it before anything else.
- The page needs a bridge member outside the eleven: the contract changes; ask the owner only
  if a translator would notice, otherwise record the change as SHELL.md version 2.
- Another card has claimed `docs/APP-DESIGN.md` §10 or §5.3 for an edit at the same time.

## References

- `TRACK-B.md` §3 (steps 3 to 8), §4 (the nine items), §6, §8 and §9.
- `evidence/2026-10-21/split-seams.md` §3.1, §3.2, §3.3 and findings S2, S9, S11.
- `evidence/2026-10-21/app-remaining.md` "G1 Electronite shell" and finding F11.
- `docs/TC4-ISSUES.md` row `#743` (line 344 at f9067bc); `.audit/rebuild.tsv` line 94 (the
  ruling of 2026-10-18T23:12Z); `docs/PERFORMANCE.md` "Start-up".
- `crates/tc-ui/src/app/buffer.rs` `current_lines`; `crates/tc-ui/src/app/web/bridge.rs`.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/split-seams.md
- 2026-10-23 13:33 EDT | integration lane (Claude Code 3d07d5f8) | claim | none | claimed on track-INT-7 from main d293375a; INT-4 is done and H2 chose Track B, so step 5 will be skipped | python3 -I -B .audit/tools/board.py
- 2026-10-23 13:45 EDT | integration lane (Claude Code 3d07d5f8) | step 1 | none | ten vectors in fixtures/shell/buffer-vectors.json and the_buffer_reduction_matches_the_shell_contract_vectors pass; merge keeps the first of equal-`at` mirror lines per slot and current_lines yields one line per slot, so order is not part of the contract. Mutation 1 (drop `&& same_text`): `INV-SHELL-CONTRACT-PINNED: vector "a removal naming another text keeps the entry": current_lines kept {}, the contract keeps {…"text":"ab"…}`. Mutation 2 (`continue` to `break`): `INV-SHELL-CONTRACT-PINNED: vector "a line that does not parse in the middle is skipped": current_lines kept {…verse "a"…}, the contract keeps {…checkComment "y"…, …verse "a"…}`. Source restored, `git diff crates/tc-ui/src` empty | cargo test -p tc-ui --test app shell_contract
- 2026-10-23 14:05 EDT | integration lane (Claude Code 3d07d5f8) | step 2 | none | docs/contracts/shell-bridge.json (11 methods, 7 called by the page) and the_bridge_members_the_page_uses_match_the_shell_contract pass. Removing "takeBuffered" from calledByThePage: `INV-SHELL-CONTRACT-PINNED: bridge.rs calls {…, "takeBuffered"}, shell-bridge.json calledByThePage says {"bufferChanged", "feedbackSend", "onCloseRequested", "openExternal", "printPdf", "setCanClose"}`. Adding "chooseFolder": `INV-SHELL-CONTRACT-PINNED: bridge.rs calls {…}, shell-bridge.json calledByThePage says {"bufferChanged", "chooseFolder", …}`. Both restored | cargo test -p tc-ui --test app shell_contract
- 2026-10-23 14:05 EDT | integration lane (Claude Code 3d07d5f8) | step 3 | none | docs/contracts/SHELL.md v1 written with all eleven items of TRACK-B.md §4 (the card's table shows nine; items 10, the test harness, and 11, the help-desk key, were added to §4 later and are frozen too), the reduction rules in words and the §6 freeze list. Every path it names exists except docs/RELEASE.md, marked "G3, to create" | git ls-files on each path
