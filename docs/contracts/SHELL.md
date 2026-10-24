# The shell contract, version 1

Frozen on 2026-10-23 (INT-7), for the Track B hand-off (`.audit/roadmap/TRACK-B.md`).

This is the interface between the app (the `tc4` server and the page it serves) and the
Electronite shell. Both sides code against it. A change is a pull request that raises this
file's version and that both sides approve (`TRACK-B.md` §9). Two tests make any other drift
fail CI: `the_buffer_reduction_matches_the_shell_contract_vectors` and
`the_bridge_members_the_page_uses_match_the_shell_contract`, both in
`crates/tc-ui/tests/app/shell_contract.rs`.

## The items

| # | The contract | Defined in | Tested by |
|---|---|---|---|
| 1 | `tc4 serve --app-data <dir> --ui <dir> [--projects <dir>] [--door43 <url>] [--bundle <dir>]`. A release build has no test flags | `crates/tc4/src/args.rs` (`USAGE`) | `crates/tc4/tests/release.rs` `release_has_no_test_hooks` (ignored: run after a release build, as its doc comment says) |
| 2 | The first stdout line: `{"url":"http://127.0.0.1:PORT/","token":"<43 base64url>"}`, or `{"error":{"type":"appLockHeld",...}}` with exit 3, or `{"error":{"type":"start",...}}` with exit 4. Usage error exit 2, server error exit 1 | `crates/tc4/src/lib.rs` (`EXIT_SERVER_ERROR` 1, `EXIT_USAGE` 2, `EXIT_LOCK_HELD` 3, `EXIT_START_FAILED` 4); `docs/APP-DESIGN.md` §5.1 | `crates/tc4/tests/lifecycle.rs` `the_address_line_comes_first_and_names_loopback`, `new_tc4_waits_for_the_old_one_then_names_the_holder`, `bad_arguments_exit_2`, `a_ui_folder_that_is_missing_is_a_start_error`. Exit 1 has no test |
| 3 | `{"quit":true}` on stdin answers `{"closed":true}`; closing stdin or SIGTERM also quits; a close takes at most 5 s | `docs/APP-DESIGN.md` §5.1 | `crates/tc4/tests/lifecycle.rs` `quit_message_answers_closed_then_exits`, `closed_stdin_quits_within_5_s`, `sigterm_quits_like_the_quit_message` |
| 4 | Headers `X-Tc-Token` and `X-Tc-Api`. The shell's filter adds the token only on paths outside `/api/`; the page sends it on `/api/` | `crates/tc-api/src/endpoint.rs` (`TOKEN_HEADER`, `API_VERSION_HEADER`); `crates/tc-ui/app/boot.js` | `crates/tc-server/tests/security.rs` (`request_without_token_is_401` and its neighbours) |
| 5 | `window.tc4Shell` has eleven members (below). The shell implements all eleven. The page calls six of them (`bufferChanged`, `takeBuffered`, `setCanClose`, `onCloseRequested`, `openExternal`, `feedbackSend`) and only checks that `printPdf` exists; `calledByThePage` lists those seven. The main team wires the rest when D2, E2 and F1 need them | `docs/contracts/shell-bridge.json`; `crates/tc-ui/src/app/web/bridge.rs`; `docs/APP-DESIGN.md` §5.7 | `the_bridge_members_the_page_uses_match_the_shell_contract` |
| 6 | `drafts-buffer.log`: one `BufferLine` per line, fsynced at most once a second, off the main thread, reduced by the rules below | `crates/tc-api/src/buffer.rs` (`BufferLine`); `tc_ui::app::buffer::current_lines` in `crates/tc-ui/src/app/buffer.rs`; `fixtures/shell/buffer-vectors.json` | `the_buffer_reduction_matches_the_shell_contract_vectors`; `crates/tc-api/tests/buffer.rs` |
| 7 | `TC4_BUILD_COMMIT` is set for both the server build and the UI build; the UI is built by `crates/tc-ui/build-app.sh` with wasm-bindgen 0.2.129 | `crates/tc-api/src/app.rs` (`BUILD_COMMIT`); `crates/tc-ui/build-app.sh`; `docs/APP-DESIGN.md` §3.10 | `crates/tc-ui/tests/app/client.rs` `the_same_build_proceeds`; `crates/tc-server/tests/routes.rs` (`hello.build.commit`) |
| 8 | The English bundle: `cargo run -p tc-app --example bundle_store -- --from bundle/en --out <dir>`; the OBS image pack is fetched at its pinned commit and checked against `bundle/en/fetched.lock`; a build missing a pinned file fails | `bundle/en/README.md` "Fetched when the app is packaged"; `bundle/en/fetched.lock` | `crates/tc-resources/tests/fetched_lock.rs` (`the_bundled_lexicons_match_the_trees_door43_reported`, `the_fixture_folder_matches_its_lock`). The OBS pack check is G3's (to create) |
| 9 | Electronite v37.1.0-graphite, pinned zips with sha256 | tC4 `docs/PACKAGING.md` line 917 (tC4 b86270f), "The wrapper is Electronite"; `.audit/tasks/g0-windows.md` "Prerequisites" item 4 | G3's release pipeline (to create) |
| 10 | The test harness G1 and G5 code against: `tc4` features `test-hooks` and `fixture-backend`; `--scenario <dir>`; the `/?probe=fields` page and `build-app.sh --release --test-hooks --out` with `CARGO_JOBS`; `kit.mjs`'s `TC4_BIN`, `TC4_UI`, `TC4_UI_HOOKS`, `TC4_DOOR43_FAKE`; `app/hello`'s `startup` values and `build.commit`; `net/state`; AD11's filter path list (G1 step 5) | `crates/tc4/Cargo.toml` `[features]`; `crates/tc4/src/args.rs` (`scenario`); `crates/tc-ui/build-app.sh`; `crates/tc-ui/src/app/web/probe.rs`; `parity/app/journeys/kit.mjs`; `crates/tc-api/src/app.rs` (`startup`); `crates/tc-api/src/lib.rs` (`net/state`) | G5's `journeys` job (`.github/workflows/app-e2e.yml`) and the shell's e2e jobs |
| 11 | The help-desk key: the release pipeline writes it as `help-desk.key` beside `shell/main.cjs` in the installed layout, from the repository secrets H15 names; the shell reads it only in the main process, never the page | G1 step 7 (reads); G3 step 3 (writes); `docs/RELEASE.md` (G3, to create, names the secrets) | G3's release pipeline (to create) |

## Item 5: the bridge

`window.tc4Shell`: `printPdf`, `chooseFolder`, `chooseFile`, `openExternal`, `setCanClose`,
`onCloseRequested` (5 s), `onDownloadDone`, `feedbackSend` → `{ok, reason?: offline | timeout |
refused | not-configured}`, `bufferChanged`, `takeBuffered`, `platform`.

## Item 6: how `drafts-buffer.log` is reduced

`takeBuffered` returns what this reduction keeps. Order is not part of the contract.

1. The last change line per field and server run wins.
2. A removal without a text ends that field's entry in that run.
3. A removal that names a text ends the entry only while the entry holds that text, compared
   in Unicode NFC. Otherwise the entry stays: newer typed text is never dropped.
4. A removal ends only its own server run's entry; fields are kept apart.
5. A change after a removal starts a new entry.
6. A line that does not parse (a torn last line, or garbage in the middle) is skipped, and the
   lines around it still count.

`fixtures/shell/buffer-vectors.json` holds one vector per rule. The shell's node tests read
the same file; Track B may add vectors.

## What the main team must not change while Track B is out

The eleven items above, item 10 (the test harness) included; the assertions of
`crates/tc4/tests/lifecycle.rs` and `crates/tc4/tests/release.rs`; the start-up protocol of
`crates/tc-ui/app/prepare.html`; the `ui/<build>/` folder layout; the `BufferLine` format; the
`bundle_store` command line; the `fetched.lock` format. A new bridge call is allowed only from
the item 5 list.
