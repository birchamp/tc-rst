---
id: DOM-1
title: One network gate: all network code behind tc-door43
track: A
lane: domain
milestone: M1
state: building
owner: domain lane (Claude Code 9f599c21)
branch: track-DOM-1
worktree: .worktrees/track-DOM-1
depends: [S2]
needs_owner: []
proves: []
no_loss: false
size: 1-2 d
merged: none
updated: 2026-10-24
next: Step 7, run scripts/gates.sh (or --wait-ci DOM-1) on the branch.
blocked_by: none
---

# DOM-1: One network gate: all network code behind tc-door43

## Goal

Every request the app sends leaves through `tc-door43`, behind the internet switch and its
proxy finder. `tc-resources` asks `tc_door43::Http::get` instead of using reqwest itself. The
git network calls that sync needs (`fetch`, `push`, `ls_remote`) exist in `tc_door43::git`,
check the switch before and during each call, and take their proxy from the same finder as
every other request. Only `tc-door43` turns on git2's `https` feature; `tc-store` keeps git2
for local repositories only.

## Why it matters to a translator

When the internet switch is off, nothing leaves the computer, sync included. On a network
that needs a proxy (an office or a school with a PAC script), sync works as downloads already
do, because both use the same proxy finder (the FIX-proxy work).

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| S2 '` shows `done`.
- `grep -n "reqwest" crates/tc-resources/Cargo.toml` prints line 15 (a normal dependency).
- `grep -n "reqwest::" crates/tc-resources/src/*.rs` prints only `aquifer.rs` (lines 21, 22,
  209, 281, 303, 429 on resume-build-lane).
- S2's merge took git2's `https` out of the workspace line and tc-store's Linux
  `vendored-openssl` out (`units/S2.md`, merge table rows `Cargo.toml` and
  `crates/tc-store/Cargo.toml`). Check: `cargo tree --locked -p tc-store -e features | grep -c 'git2 feature "https"'`
  prints `0`. If it prints more, step 3 removes them as S2's table says.
- `grep -n "git2" crates/tc-door43/Cargo.toml` shows git2 only under `[dev-dependencies]`;
  `grep -rn "git2::" crates/tc-door43/src` prints nothing: no git network call exists.
- `grep -n "proxy_auto" docs/STORAGE.md` prints two lines in §9 (S3 fetch, §9.5 push): the
  design this item replaces.
- `grep -rn "\.fetch(\|\.push(&\|FetchOptions\|PushOptions" crates/tc-store/src` prints
  nothing: tc-store makes no network call.
- If `S3` is in an active state on the board, run `git log main..<S3's branch> -- crates/tc-store/Cargo.toml docs/STORAGE.md`.
  If it prints commits, stop and ask (see below).

## Funnel card

- **Invariants**:
  - INV-ONE-NETWORK-GATE: only `tc-door43` depends on reqwest or hyper as a normal
    dependency, or turns on git2's `https`; every request goes through `tc_door43::Http` or
    `tc_door43::git`.
  - INV-NO-REQUEST-WHILE-SWITCH-OFF: with the internet switch off, no call of `tc-door43`
    (HTTP or git) sends anything, and a switch turned off during a fetch stops it.
  - INV-ONE-PROXY-FINDER: git calls use the proxy that `Http`'s `ProxyFinder` names for the
    remote's origin, or the system's settings when it names none, as every HTTP request does.
- **Wrong states**:
  - A second network path: libgit2's `proxy_auto()` (STORAGE §9.1 S3 and §9.5) skips the
    PAC/WPAD finder (`crates/tc-door43/src/http.rs:47-69`), so sync fails on a proxied network
    where downloads work.
  - reqwest types crossing the crate edge (`HttpSpec.redirects` is a
    `reqwest::redirect::Policy`, `Http::with_finder` returns `reqwest::Error`,
    `Http::client()` returns `&reqwest::Client`, http.rs:148), so "reqwest only in
    tc-door43" cannot be checked.
  - git2 `https` back in `tc-store` (the S2 branch had it, `Cargo.toml:64` there), so
    storage code could open a remote unseen.
- **Code**:
  - Types: `HttpResponse` (to create; status, headers by name, content length, `chunk()`),
    `RedirectRule` (to create; allowed origins, allowed https hosts, a maximum count),
    `HttpError` (to create; `Offline`, `Timeout`, `Connect`, `Interrupted`, `Redirect`,
    `Other`) in `crates/tc-door43/src/http.rs`. `GitRemote` (to create; the remote's URL and
    the session's credentials) and `GitProgress` in `crates/tc-door43/src/git.rs` (to create).
  - One entry point: `Http::get(&self, url, accept, headers) -> Result<HttpResponse,
    HttpError>` for HTTP; `Http::proxy_for(&self, url) -> Result<Option<Url>, Offline>` (to
    create) reusing the finder's cached answers and `choose()` (http.rs:167); and
    `tc_door43::git::{fetch, push, ls_remote}`, each taking a `&git2::Repository` that
    tc-store opened and the `Http` whose switch and finder they use.
  - Removed: reqwest from `crates/tc-resources/Cargo.toml` `[dependencies]` (kept under
    `[dev-dependencies]` for the two tests that read the fake directly:
    `tests/dcs_install.rs:295`, `tests/updates.rs:188`); `reqwest::redirect::Policy` from
    `HttpSpec`; `Http::client()` and `reqwest::Error` from the public API.
  - Left to checks, and why: which crate may name reqwest or git2 `https` is a rule about
    manifests; the CHK-1 architecture test (`crates/tc-arch`) reads `cargo metadata` and holds
    it. A git push that has started sending its pack cannot be stopped by the switch: git2
    0.21's `push_transfer_progress` and `pack_progress` return nothing
    (`git2-0.21.0/src/remote_callbacks.rs:215-235`). The switch is checked before the call,
    in `credentials`, in `push_negotiation` (the last point before the pack is sent) and in
    `sideband_progress`. libgit2 1.9.7 has no SOCKS proxy (no `socks` in its transports); a
    SOCKS answer from the finder makes the git call fail as `Unreachable`, with that reason.
- **Checks**:
  - `every_call_is_refused_before_anything_is_sent_while_the_switch_is_off` in
    `crates/tc-door43/tests/internet_switch.rs:14`, extended with `get`, `fetch`, `push` and
    `ls_remote`; kind: unit through the public API; runs in: CI job `rust`,
    `cargo test --workspace`.
  - `a_fetch_stops_when_the_switch_goes_off` in `crates/tc-door43/tests/git.rs` (to create);
    kind: unit, against `tc-door43-fake`'s smart HTTP (`crates/tc-door43-fake/src/smart_http.rs`)
    with a held request (`FakeDoor43::hold`); runs in: CI `rust`.
  - `a_fetch_and_a_push_round_trip_through_the_fake` and
    `a_refused_push_reads_as_door43s_refusal` in `tests/git.rs` (to create); kind: unit;
    runs in: CI `rust`.
  - `a_git_fetch_goes_through_the_proxy_the_finder_names` in
    `crates/tc-door43/tests/proxy.rs` (to create, modelled on
    `a_request_goes_through_the_proxy_the_finder_names`, :72), with the fake proxy
    (`crates/tc-door43-fake/src/proxy.rs`); kind: unit; runs in: CI `rust`.
  - The Aquifer tests `crates/tc-resources/tests/aquifer_install.rs` and
    `aquifer_proxy.rs` pass unchanged.
  - CHK-1's `git_https_only_in_door43` and `network_libraries_only_in_door43_and_resources`
    (`crates/tc-arch/tests/architecture.rs`), the second narrowed by step 6 to tc-door43 with
    no exception for tc-resources.
  - Proof it can fail: (1) remove the switch check from `git::fetch`: the extended switch
    test fails with `fetch` and lists the request the fake received. (2) In `git::fetch` set
    `ProxyOptions::auto()` instead of the finder's URL: the proxy test fails (the fake proxy
    sees no `CONNECT`). (3) Put `reqwest` back in tc-resources' `[dependencies]`: the CHK-1
    test fails naming tc-resources. Record each output.
- **Prose**: `docs/STORAGE.md` §9 (crate line, S3, §9.5) and its dependency list say where
  git TLS and the proxy come from. One doc comment on `tc_door43::git` says why the switch
  cannot stop a pack already being sent.
- **Human**: none. The roadmap decided where git network calls live (`ROADMAP.md`,
  "Decisions I made", first row).

## Steps

### Step 1: Give tc-door43 a typed GET, red first

- Do:
  1. Write `crates/tc-door43/tests/http_get.rs` (to create) against `tc-door43-fake`:
     `a_get_while_the_switch_is_off_sends_nothing`, `a_redirect_the_rule_does_not_allow_is_refused`,
     `a_body_over_its_limit_is_refused`. Run it; it does not compile yet.
  2. In `crates/tc-door43/src/http.rs`, add `RedirectRule`, `HttpResponse`, `HttpError` and
     `Http::get(url, accept, headers: &[(&str, &str)])`. Build reqwest's redirect policy from
     the rule inside `Http::with_finder` (http.rs:106). Classify reqwest errors once, inside
     tc-door43, as `transport_error` does today (`crates/tc-resources/src/aquifer.rs:429`).
  3. Change `HttpSpec.redirects` to `RedirectRule` and move `Door43Client`'s policy
     (`crates/tc-door43/src/client.rs:190-205`) onto it. Make `Http::with_finder` return
     `Result<Self, HttpError>` (today `reqwest::Error`) and `Http::client()` `pub(crate)`, so
     no reqwest type is public. Make `pub mod http` (`crates/tc-door43/src/lib.rs:49`)
     `pub(crate) mod http` once `Http` is the entry point (no caller outside the crate uses the
     module today; CHK-1's INV-ONE-PARSER-OWNER table lists it with no allowed reader, and the
     Code-layer form needs no allow line).
- Files: `crates/tc-door43/src/http.rs`, `crates/tc-door43/src/client.rs`,
  `crates/tc-door43/src/lib.rs`, `crates/tc-door43/tests/http_get.rs`.
- Done when: `cargo test -p tc-door43` passes, and the three new tests failed before 1.2
  (paste both in the log).
- Record: a log line `step 1`; set `next` to step 2.

### Step 2: Move Aquifer onto Http::get and take reqwest out of tc-resources

- Do:
  1. In `crates/tc-resources/src/aquifer.rs`, replace `get` (:279-299), `body` (:301-330),
     the redirect policy (:209-227) and `transport_error` (:429) with `Http::get`,
     `HttpResponse::chunk` and a `RedirectRule` (the API and asset hosts, `MAX_REDIRECTS`).
     Keep the `X-GitHub-Api-Version` header and the per-chunk switch check.
  2. In `crates/tc-resources/Cargo.toml`, move `reqwest` from `[dependencies]` to
     `[dev-dependencies]`.
- Files: `crates/tc-resources/src/aquifer.rs`, `crates/tc-resources/Cargo.toml`.
- Done when: `cargo tree -p tc-resources -e normal --depth 1 | grep -c reqwest` prints `0`;
  `cargo test -p tc-resources --test aquifer_install --test aquifer_proxy` passes with no
  change to those test files.
- Record: a log line `step 2`; set `next` to step 3.

### Step 3: Turn on git2's https in tc-door43 only

- Do:
  1. If the check under "Before you start" printed more than `0`: set the root
     `Cargo.toml` workspace git2 to `features = ["vendored-libgit2"]` and delete tc-store's
     Linux `vendored-openssl` block, as S2's merge table says.
  2. `crates/tc-door43/Cargo.toml`: move git2 from `[dev-dependencies]` to `[dependencies]`
     with `features = ["https"]`, and add a Linux-only `git2 = { workspace = true, features
     = ["vendored-openssl"] }` with a comment: libgit2's TLS on Linux is OpenSSL, built from
     source, for the fetch and push of sync.
- Files: `Cargo.toml`, `crates/tc-store/Cargo.toml`, `crates/tc-door43/Cargo.toml`,
  `Cargo.lock`.
- Done when: `cargo metadata --format-version 1 --no-deps | python3 -I -c "import json,sys; d=json.load(sys.stdin); print(sorted(p['name'] for p in d['packages'] for x in p['dependencies'] if x['name']=='git2' and x['kind'] is None and 'https' in x['features']))"`
  prints `['tc-door43']`; `cargo test -p tc-store` passes (local git still works without
  TLS); the Windows clippy step passes.
- Record: a log line `step 3`; set `next` to step 4.

### Step 4: Add the git calls behind the switch and the proxy finder

- Do:
  1. Add `Http::proxy_for(&self, url: &Url) -> Result<Option<Url>, Offline>`: the same
     lookup `client()` runs (http.rs:148-163), then `choose()`; `None` for a loopback origin.
  2. Create `crates/tc-door43/src/git.rs` with `fetch`, `push` and `ls_remote`, each taking
     `(&Http, &git2::Repository, &GitRemote, …)`. Each one: refuses with
     `Door43Error::Offline` before anything when the switch is off; sets
     `ProxyOptions::url` from `proxy_for` (never `auto()`); checks the switch in
     `credentials`, `transfer_progress` (fetch: return `false`), `push_negotiation` (push:
     return an error) and `sideband_progress`; maps git2 errors to the existing
     `Door43Error` kinds (`AuthFailed`, `NotFound`, `Unreachable`, and a refused push through
     `refused_push`, `crates/tc-door43/src/push.rs`). They are blocking calls: the doc says
     the caller runs them on a blocking thread.
  3. Write `crates/tc-door43/tests/git.rs` with the three tests of the funnel card, using
     `FakeDoor43::start_seeded`, `clone_url`, `issue_token` and `hold`
     (`crates/tc-door43-fake/src/lib.rs`).
- Files: `crates/tc-door43/src/http.rs`, `crates/tc-door43/src/git.rs`,
  `crates/tc-door43/src/lib.rs`, `crates/tc-door43/tests/git.rs`.
- Done when: `cargo test -p tc-door43 --test git` passes; proof mutation (1) fails it.
- Record: a log line `step 4`; set `next` to step 5.

### Step 5: Prove the switch and the proxy for every call

- Do:
  1. Extend `every_call_is_refused_before_anything_is_sent_while_the_switch_is_off`
     (`crates/tc-door43/tests/internet_switch.rs:14`) with `get`, `fetch`, `push` and
     `ls_remote` on a temporary repository; its last line already asserts that the fake
     received nothing.
  2. Add `a_git_fetch_goes_through_the_proxy_the_finder_names` to
     `crates/tc-door43/tests/proxy.rs`, with an `https://` remote name, the `Always` finder of
     that file and `FakeProxy`: assert the proxy saw `CONNECT <host>:443`.
- Files: `crates/tc-door43/tests/internet_switch.rs`, `crates/tc-door43/tests/proxy.rs`.
- Done when: `cargo test -p tc-door43 --test internet_switch --test proxy` passes; proof
  mutations (1) and (2) fail them (paste both in the log).
- Record: a log line `step 5`; set `next` to step 6.

### Step 6: Update the storage design and the architecture check

- Do:
  1. `docs/STORAGE.md` §9: check that the crate line (around line 3603) and the dependency
     list (around 5196) say TLS comes with tc-door43 (S2's merge changed them); in S3
     (around :3627) and §9.5
     (around :3770) replace `proxy_options(proxy_auto())` with the proxy that
     `tc_door43::Http::proxy_for` names, and say the switch is checked before the call and in
     the callbacks.
  2. If `crates/tc-arch` exists (CHK-1), narrow `network_libraries_only_in_door43_and_resources`
     to tc-door43 only: delete its "reqwest in tc-resources until DOM-1" exception and rename
     the test `network_libraries_only_in_door43`. If it does not exist, write a log line
     `note` that CHK-1 must hold the rule with no exception for tc-resources.
- Files: `docs/STORAGE.md`, `crates/tc-arch/` (if present).
- Done when: `grep -n "proxy_auto" docs/STORAGE.md` prints nothing; `cargo test -p tc-arch`
  passes (if present); proof mutation (3) fails it.
- Record: a log line `step 6`; set `next` to step 7.

### Step 7: Run the full gates

- Do: run the full gate list.
- Files: none.
- Done when: every gate passes.
- Record: a log line `gate`; set `state: verifying` and `next`.

## Gates before merge

Run `scripts/gates.sh` (INT-3 creates it; it lands before this item can start). If it is
missing, run every command of `AGENTS.md` "Tests" and the Windows step:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and
  again with `--features gallery`, `--features app` and `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`
- `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
  (it must still build libgit2 with `https` for Windows, now through tc-door43)

Plus: `cargo test -p tc-door43 -p tc-resources` with the network blocked, if CHK-5's
no-network job exists (it proves no test reaches past the fakes).

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- S3 is active and its branch has changed `crates/tc-store/Cargo.toml` or `docs/STORAGE.md`
  §9 (the check under "Before you start"): those files are on its path.
- A git call needs a type or a step that X's card owns (opening the repository through
  `tc_store::git::Git`, whose `repo` field is private, `crates/tc-store/src/git.rs:148-149`):
  leave it to X and log a `note`.
- The Windows clippy step cannot build libgit2 with `https` from tc-door43.
- A gate fails twice and you cannot tell why.

## References

- `.audit/roadmap/evidence/2026-10-21/health-domain.md` §2, §6 item 4, F2, F3;
  `checks-inventory.md` C5.
- `crates/tc-door43/src/http.rs:47` (`ProxyFinder`), `:60` (`system_proxy_finder`), `:72`
  (`HttpSpec`), `:87` (`Http`), `:106` (`with_finder`), `:148` (`client`);
  `crates/tc-door43/src/client.rs:190-205`; `crates/tc-door43/src/push.rs`;
  `crates/tc-door43/tests/internet_switch.rs:14`; `crates/tc-door43/tests/proxy.rs:72`;
  `crates/tc-door43-fake/src/smart_http.rs`, `proxy.rs`; `crates/tc-resources/src/aquifer.rs:195-330`, `:429`.
- `docs/STORAGE.md` §9 (lines 3601-3640, 3758-3780 on s2-proof-executor), the dependency list
  (around 5196); `docs/ARCHITECTURE.md:89`.
- `ROADMAP.md` "Decisions I made" (git network calls in tc-door43).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/health-domain.md
- 2026-10-24 07:21 EDT | domain lane (Claude Code 9f599c21) | claim | none | Claimed in .worktrees/track-DOM-1 from main 29048562. Before-you-start: S2 done; reqwest only in aquifer.rs (21,22,209,281,303,429); git2 has no https anywhere (workspace line vendored-libgit2 only); tc-door43 has git2 in dev-deps only; proxy_auto at STORAGE.md 3716 and 3864; crates/tc-arch absent; S3 not active | python3 -I .audit/tools/board.py
- 2026-10-24 07:40 EDT | domain lane (Claude Code 9f599c21) | step 1 | none | Http::get, HttpResponse (status, header, body(limit) with the per-chunk switch check), RedirectRule and HttpError in tc-door43; HttpSpec.redirects is a RedirectRule; with_finder returns HttpError; client() and the http module are crate-private; reqwest errors are classified once (From<reqwest::Error> for HttpError, used by Door43Client too). Before: `cargo test -p tc-door43 --test http_get` failed to compile (E0432 HttpError, RedirectRule; E0599 no method get). After: 3 passed, `cargo test -p tc-door43` all pass. Deviation: HttpError has TooLarge and HttpResponse has body(limit) instead of a public chunk(), so the switch check per chunk lives in tc-door43 | crates/tc-door43/tests/http_get.rs
- 2026-10-24 07:45 EDT | domain lane (Claude Code 9f599c21) | step 2 | none | aquifer.rs sends through Http::get with a RedirectRule (API and web origins; asset hosts unless loopback; MAX_REDIRECTS); reqwest moved to tc-resources' dev-dependencies. `cargo tree -p tc-resources -e normal --depth 1 | grep -c reqwest` prints 0; `cargo test -p tc-resources` passes with no test file changed | cargo test -p tc-resources
- 2026-10-24 08:05 EDT | domain lane (Claude Code 9f599c21) | step 3 | none | The check under "Before you start" printed 0 (S2 already took https out), so 3.1 had nothing to do. tc-door43 depends on git2 with `https`, and on Linux with `vendored-openssl`; the workspace comment says only tc-door43 adds TLS. The metadata check prints ['tc-door43']; `cargo test -p tc-store` passes; `cargo clippy -p tc-door43 -p tc-store --all-targets --target x86_64-pc-windows-gnu -- -D warnings` passes | cargo metadata (card step 3)
- 2026-10-24 08:20 EDT | domain lane (Claude Code 9f599c21) | step 4 | none | Http::proxy_for (blocking; finder lookups share one thread-with-timeout body with the async client path) and tc_door43::git::{fetch, push, ls_remote} with GitRemote {server, repo, session}; Door43Client::http() hands the gate to sync. Switch checked before each call (through proxy_for), in credentials, transfer_progress, push_negotiation and sideband_progress; a failure while the switch is off reads as Offline. Token offered once (libgit2 would ask forever). SOCKS refused as Unreachable. GitProgress not made: no caller needs progress yet (X adds it if its UI does). Proof: with transfer_progress and sideband_progress both ignoring the switch, a_fetch_stops_when_the_switch_goes_off fails with `Ok(())` at tests/git.rs:170 (either check alone stops it) | cargo test -p tc-door43 --test git
- 2026-10-24 08:30 EDT | domain lane (Claude Code 9f599c21) | step 5 | none | The switch test now covers Http::get, git::fetch, git::push and git::ls_remote; a_git_fetch_goes_through_the_proxy_the_finder_names added. Mutation (1), git calls skip proxy_for's switch check: every_call_is_refused_… fails with `ls_remote: Ok(())` (internet_switch.rs:136). Mutation (2), ProxyOptions::auto() instead of the finder's URL: the proxy test fails, `left: [] right: ["CONNECT door43.test:443 HTTP/1.1"]` (proxy.rs:119) | cargo test -p tc-door43 --test internet_switch --test proxy
- 2026-10-24 08:40 EDT | domain lane (Claude Code 9f599c21) | step 6 | none | docs/STORAGE.md §9's crate line, S3, §9.5, the crate table and the dependency list say TLS and the proxy come from tc-door43 (`Http::proxy_for`), with the switch checked before and in the callbacks; `grep -n proxy_auto docs/STORAGE.md` prints nothing. tc-door43's crate doc says it is the only crate that uses the network | grep -n proxy_auto docs/STORAGE.md
- 2026-10-24 08:40 EDT | domain lane (Claude Code 9f599c21) | note | none | crates/tc-arch does not exist yet: CHK-1 must hold INV-ONE-NETWORK-GATE with no exception for tc-resources (its card's table row "`tc-resources` until DOM-1" and the "until DOM-1" exception at CHK-1.md:115, :169, :265 are void once DOM-1 merges). Proof mutation (3) waits for that test | .audit/roadmap/units/CHK-1.md
