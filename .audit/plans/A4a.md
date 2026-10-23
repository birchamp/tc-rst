# Plan for design unit A4a: tc-ui runtime (transport, cache, language)

Planned 2026-10-18. Worktree `/home/user/tc-rst/.claude/worktrees/p0-a4a`, branch `p0-a4a`. The
planner created it from `phase0` at `227b0d9` (A2 merged), then, before any work, moved it to
`phase0` at `6468219`, because A3 (tc-app) and the A2/A3 join landed on `phase0` while the plan
was being written (the join's commit was amended once, `afe4f16` → `6468219`). **`tc4 serve` now runs tc-app's `App`, not tc-server's stand-in.** Commit
this file with your first commit.

The unit's row (docs/APP-DESIGN.md §10, phase 0):

> A4a tc-ui runtime: transport, cache, language | feature `app`; API client with scope drop;
> SSE-over-fetch reader; `QueryCache`; `Load<T>`; no tab guard, because the ruling of 2026-10-18
> removed the one-tab check *(A-19)*; `t()` with generated keys and rebuild keys; all ten catalogs
> at boot *(A-23)*; `<html lang dir>`; `build-app.sh`; web-sys features (`Storage`, `Headers`,
> `Request`, `RequestInit`, `ReadableStream`, `ReadableStreamDefaultReader`, `TextDecoder`,
> `AbortController`, `File`, `Blob`, `DragEvent`, `DataTransfer`, `HtmlAnchorElement`,
> `HtmlIFrameElement`, `PerformanceObserver`); vendor tC4's `src/ui.css` again | A1, A2 | — | host
> tests of the SSE parser and `t()`; `i18n-keys` port with floors; Playwright smoke: boot,
> `hello`, a locale switch with no request after boot | the smoke passes in this container

What the planner read, so you need not: AGENTS.md; docs/PLAN.md; docs/ARCHITECTURE.md;
docs/PERFORMANCE.md; APP-DESIGN "Owner's answers", "G0 results", §0–§2 (AD4, AD9, AD10, AD11,
AD21, AD22, AD23, AD25, AD30, AD35, AD37, AD39), §3.1–§3.10, §4.1, §4.3, §4.4, §5.1–§5.8,
§6.1–§6.12, §7.1–§7.3, §7.8, §7.9, §8.1–§8.4, §10 (intro, A1–A6b, order paragraph), the red team
table, RT-17, RT-19, RT-23, RT-30, RT-32 and "How each point was answered". Code: `crates/tc-ui`
(all), `crates/tc-api/src` (endpoint, app, event, query, error, ids, prefs, net, jobs, account,
refusal), `crates/tc-api/tests/refusals.rs` and `tests/common/mod.rs` (the catalog resolver),
`crates/tc-server/src` (statics, events, security, routes, stub, lifecycle), `crates/tc4/src`
(lib, args, join), `crates/tc-app/src` (events, handlers hello and setLocale, app set_internet,
prefs), `spikes/loopback` (the wasm fetch and Playwright pattern A2 used), `parity/app/*`,
`parity/ds-gallery/package.json`, `.github/workflows/ci.yml`. tC4 at `d310bf3`: `index.html`,
`src/main.jsx`, `src/ui.css`, `src/i18n/index.js`, `src/i18n/locales.js`, `src/App.jsx`
(SaveIndicator, ViewBoundary, ViewError), `test/i18n-keys.test.ts`,
`test/i18n-catalogs.test.ts` (the three `t()` cases), `e2e/app-language.spec.ts` (header).

---

## 0. Before you start

1. `git -C /home/user/tc-rst/.claude/worktrees/p0-a4a log --oneline -1` must show the tip of
   `phase0` (`6468219` at planning). If `phase0` moved and you have not committed yet,
   `git merge --ff-only phase0`; if it was rewritten (not a fast-forward) and you have no commit,
   `git reset --hard phase0` (this plan is untracked and survives). Once you have commits,
   merge `phase0` instead and say so in the merge message.
2. Disk: at planning time `df -h /home/user` showed **5.1 GB free**, under the 6 GB line. Your
   worktree had no `target/` yet. Check `df` before every big build; when it is under 6 GB and no
   rustc runs in this worktree (`pgrep -a rustc | grep p0-a4a`), run
   `python3 -I /home/user/tc-rst/.audit/tools/prune-target.py /home/user/tc-rst/.claude/worktrees/p0-a4a/target`.
   Never delete anything outside this worktree. Always `-j 2`.
3. Tools that are already here: `wasm-bindgen 0.2.129` and `wasm-bindgen-test-runner` in
   `~/.cargo/bin`; the `wasm32-unknown-unknown` target; node 22; Playwright 1.56.1 at
   `/home/user/tc-rst/parity/ds-gallery/node_modules/playwright`; Chromium build 1194 under
   `PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers`. `regex 1.13.1` is in `Cargo.lock` and in the
   registry (offline is fine). `wasm-bindgen-test` (the macro crate) is **not** in the registry:
   do not plan on wasm unit tests; everything with logic is a host test, and the browser is
   covered by the Playwright smoke.

## 1. What A4a is, and what it is not

A4a gives every later screen unit the browser runtime: a typed API client, the event stream
reader, a query cache with `Load<T>`, the server state that events carry, and the interface
language (`t()`, catalogs, `<html lang dir>`). It also gives the product its page: `index.html`,
a minimal `boot.js`, and `build-app.sh`, which builds the folder `tc4 serve --ui` serves. Until A5
ports tC4's `App.jsx`, the page mounts a **stand-in root** that proves the runtime works.

Not A4a (do not build): the one-tab guard (removed by the ruling of 2026-10-18, A-19; tC4
removed its own as dead code, D39); the save machine, draft buffer, panic hook with crash marker,
`boot.js` recovery, navigation gate, close guard, request-id minting for writes (A4b); the port of
`App.jsx`, TopBar, M16 app-language dialog, help pages (A5); the internet question flow (A8);
`/prepare.html` (A9); `env-rust.mjs`, `report.json`, the parity gate (A6b); the `harness` feature
and `ScriptedApi` (§7.8, a later unit). Do not touch the `storage` branch or its worktree.

No tC4 screen is ported here, so no parity row applies. Where A4a ports tC4 behaviour, it is
`src/i18n/index.js` (`t`, `setLocale`, `showOnDocument`), `locales.js`, `index.html` and the CSS
order of `main.jsx`; port those exactly (§3 says how).

## 2. Decisions this plan makes (and the design text you change for each)

The design is normative. Where this plan refines or corrects it, change the design text in the
same commit as the code and say why in the commit message (AGENTS.md; the task). Each item
names the place.

- **D1. Errors of a call.** `api.call(&req).await -> Result<E::Ok, CallError>`, with
  `CallError { Api(ApiError), Dropped, Transport(Transport) }` and
  `Transport { Unreachable(String), Status(u16), Unreadable(String) }`. `Load::Failed` holds a
  `CallError`. Why: AD9 keeps HTTP statuses and network failures below the API, so they are not
  `ApiError`s, and a reply dropped because its project closed must not look like a refusal.
  Edit §6.2 (its first bullet says `-> ReplyOf<E>` and `Failed(ApiError)`).
- **D2. Scope drop is on the read path only.** The query cache, and `Runtime::read_once` for a
  read an action makes itself, drop a reply whose open is no longer current or that the server
  answered with `ApiError::StaleScope`. **A write's reply is never dropped**: A4b's save machine
  needs every `Ack` and every `NotSaved::StaleScope` (§6.5, A-19). Edit §6.2's second bullet to
  say so.
- **D3. Server state that events carry whole lives in `ServerState` signals, not in refcounted
  cache entries**: internet, account, startup, the open sessions, the saved app language, jobs,
  the last checkpoint outcome, materialize-blocked paths and aligner save states. `hello` seeds
  them, events update them, and a `Resync` reads `hello` again. Refcounted reads (project data,
  Home) are in the `QueryCache`. Why: these values arrive whole in frames and must exist before
  any screen holds them; a refcounted entry with no holder would drop them. Edit §6.1's first
  source ("Server state in a `QueryCache`: … jobs, internet, account, prefs") to name both.
- **D4. The event stream at boot.** The UI's first connection asks `?after=0`; tc-app's bus
  then replays every frame since launch, or sends one `Resync` if the ring has wrapped
  (`crates/tc-app/src/events.rs:143-176`, `subscribe`). The UI reads `app/hello` only after the stream's
  response head has arrived, so the subscription is made before `hello`'s snapshot and no frame
  can fall between them. A `Resync` is numbered like the newest frame and may be lower than the
  reader's `after`, so it is never skipped as a repeat; any other frame with `seq <= after` is a
  repeat and is skipped. A frame that does not parse is handled as a `Resync` (refresh
  everything), never dropped. A frame from another `ServerInstance` is a restart (§3.10). No
  byte for 45 s (three 15 s heartbeats) aborts the connection; reconnects back off 250 ms,
  doubling to 5 s. Add these as one bullet to §5.4 (or §6.3).
- **D5. Catalogs.** English (tC4's `en.json` and the rebuild's `en.json`) and the nine rebuild
  `<id>.json` files are compiled into the wasm; the nine other vendored catalogs are fetched in
  parallel at boot, before the first render. Why: English is the fallback `t()` needs even when
  a fetch fails and build.rs reads it anyway; the rebuild files are a few bytes each. A failed
  fetch shows the boot error instead of a running app (A-23's point). Edit §6.9's third bullet
  ("All ten catalogs … are fetched").
- **D6. Key constants are nested modules**: `keys::app::SAVED`, `keys::net::ask::SIGN_IN`,
  `keys::refusal::open::LOCK_HELD`. Flat names collide: `upgrade.text.gateway` and
  `upgrade.textGateway` both become `UPGRADE_TEXT_GATEWAY` (checked: the nested form has no
  collision and no Rust keyword among the 1,045 keys). Edit §6.9's second bullet.
- **D7. The UI folder.** `build-app.sh` writes `index.html` and `ui/<build>/…`, where `<build>`
  is 16 hex digits of a SHA-256 over every file's path and bytes. A hashed folder, not hashed
  file names, because tC4's CSS imports `tokens/*.css` and fonts by relative URL. tc-server
  serves `/ui/` as `immutable` (`crates/tc-server/src/statics.rs:1-6`). Edit §3.10's second
  bullet ("every other UI file has a content-hashed name") and §5.3's `/ui/*` row.
- **D8. Build check.** `hello.build.commit` ≠ `tc_api::app::BUILD_COMMIT`, or `hello.api` ≠
  `API_VERSION`, or any reply that is `ApiError::Version`: reload once (a `sessionStorage`
  marker names what it reloaded for); the same mismatch again shows the boot error instead of
  reloading for ever. Add "once" and the second-mismatch rule to §3.10.
- **D9. The boot error** is a rebuild-only state (tC4 bundles everything and cannot fail to
  boot): tC4's view-error words, `app.viewError` and the technical reason, with `app.retry`,
  which reloads. Add it to §6.12's list of rebuild-only screens.
- **D10. "No request after boot" counts the app's own requests.** Showing Arabic makes the
  browser load the Arabic faces of tC4's fonts on first use, as tC4 does (its app-language case
  j paints with the bundled Noto Sans Arabic). The smoke fails on any `fetch`, `xhr`, document,
  script or stylesheet request after boot and lists font loads. Add one clause to §6.9.
- **D11. The stand-in root** (A5 deletes it): a sample line in the shown language, a plain
  `<select>` of the ten languages that calls the same `show_locale` M16 will call, and the
  internet state. It is the only way the smoke can switch the language before A5 exists, and it
  uses no test-only code path.

Finally, append an *(A4a: …)* note to the A4a row in §10 naming D1–D11 in a sentence each, as
A1, A2 and A3 did for theirs.

## 3. Facts the code must respect (read once, rely on)

### 3.1 The server, as `tc4 serve` runs it now

- Start: `tc4 serve --app-data <dir> --ui <dir> [--projects <dir>]`. Without `--projects` tc-app
  uses `~/translationCore4/projects` and may create it: **always pass `--projects`** in scripts.
  A `--projects` folder that does not exist stops the start (exit 4): create it first. Stdout's
  first line is `{"url":"http://127.0.0.1:PORT/","token":"<43 base64url>"}`; keep stdin open (a
  closed stdin quits); `{"quit":true}\n` on stdin answers `{"closed":true}` and exits 0.
- Every request needs `X-Tc-Token` (else 401), `Host: 127.0.0.1:PORT` (else 403), `Origin` equal
  to the server's origin on anything but GET (else 403), and `X-Tc-Api: 1` on every `/api/` path,
  **the event stream's GET included** (else 409 with a `Reply` body holding
  `ApiError::Version`). No CORS header ever. Repeated token headers are accepted when every value
  is the token.
- `POST /api/v1/<service>/<name>` answers 200 with `Reply<T>` JSON
  (`{"type":"ok","value":…}` / `{"type":"err","value":<ApiError>}`); 404 for an unknown path;
  413 over 8 MiB; 503 while closing; 500 if a handler panicked; 409 as above. Bodies of
  401/403/404/413/503/500 are plain text.
- `GET /api/v1/events?after=<seq>`: `text/event-stream`, first chunk `: tc4\n\n`, then per frame
  `id: <seq>\ndata: <one line of JSON EventFrame>\n\n`, a `:\n\n` heartbeat every 15 s; the
  connection ends when the reader lags (reconnect with the last seq). tc-app publishes
  `Internet { on }` when `net/set` changes the switch, `Job(view)`, `Startup(state)`.
- `GET /` serves `<ui>/index.html` with `no-store` and the CSP of §5.2 (no inline script;
  `'wasm-unsafe-eval'`; `style-src 'self' 'unsafe-inline'`; `connect-src 'self'`; fonts fall to
  `default-src 'self'`). `GET /ui/<path>` serves `<ui>/ui/<path>` as `immutable`; `.wasm` is
  `application/wasm`. A path segment that is empty, `.`, `..`, or holds `\`, `:` or NUL is 404.
- `app/hello` (`{}`) answers `HelloView { api, build { version, commit }, instance, internet,
  account, sessions, prefs { appLocale }, startup }` from tc-app's live state. `prefs/setLocale`
  (`{"request":"<32 hex>","locale":"ar"}`) is kept in memory for the run, so a later `hello`
  reports it: the smoke uses that to boot in Arabic. `sessions` is empty until B1b.

### 3.2 tc-api pieces you use

`tc_api::{Endpoint, Reply, ApiError, API_VERSION}`, `tc_api::endpoint::{API_VERSION_HEADER,
TOKEN_HEADER, EVENTS_PATH}` (there is no constant for the `/api/v1/` prefix: add
`const API_ROOT: &str = "/api/v1/"` in tc-ui with a test that `EVENTS_PATH` and
`tc_api::upload::UPLOADS_PATH` start with it), `tc_api::app::{Hello, HelloView, BUILD_COMMIT,
OpenSession}`, `tc_api::event::{EventFrame, Event, SessionEvent, ChangeScope, CheckpointOutcome,
StartupState}`, `tc_api::query::QueryKey` (`depends_on`), `tc_api::ids::{Scope, Epoch, ViewSeq,
ServerInstance, ProjectRef, LocaleId, LOCALE_IDS, JobId, AlignerId}`, `tc_api::jobs::{JobView,
JobState}`, `tc_api::account::AccountView`, `tc_api::align::AlignSave`. Every one is `Send +
Sync`, `Clone`, `PartialEq`; `EventFrame` and friends derive `proptest::Arbitrary` under tc-api's
`proptest` feature. tc-ui never enables tc-api's `server` feature.

### 3.3 tC4's language code, exactly

`tC4:src/i18n/index.js:76-95`:

```js
const own = catalogs[current][key];
let s = typeof own === 'string' && own !== '' ? own : catalogs.en[key];
if (s === undefined) {
  if (fallback !== undefined) return fallback;          // returned as given: no substitution
  if (!warned.has(key)) { warned.add(key); console.warn(`[i18n] missing key: ${key}`); }
  s = key;
}
if (vars) for (const [k, v] of Object.entries(vars)) s = s.replaceAll(`{${k}}`, String(v));
return s;
```

So: a target entry that is missing, empty or not a string falls back to English per key; an
English entry is used as it is, even empty; the fallback wins only when no catalog has the key
and is not substituted; a missing key warns once per key with exactly
`[i18n] missing key: <key>` and shows the key; each var replaces every occurrence of `{name}`,
**in the order given, one after another** (so a value that contains a later var's `{name}` is
replaced again; port it as is and pin it with a test). `setLocale(id)`: an id that is not
installed, or the shown one, changes nothing; otherwise set `<html lang>` to the id and
`<html dir>` to its `dir` **before** telling the listeners (`showOnDocument`, then listeners).
tC4 sets `lang="en" dir="ltr"` at module load, before the first paint. The rebuild key files
are read after the vendored catalog of the same language and never override it.

## 4. Files

New: `crates/tc-ui/build.rs`; `crates/tc-ui/src/app/{mod,boot,cache,client,events,load}.rs`;
`crates/tc-ui/src/app/web/{mod,api,stream,query,state,root,js}.rs`;
`crates/tc-ui/app/{index.html,boot.js,smoke.mjs}`; `crates/tc-ui/build-app.sh`;
`crates/tc-ui/assets/ui.css`; `crates/tc-ui/tests/{i18n_runtime.rs,i18n_keys.rs,app.rs}` and
`crates/tc-ui/tests/app/{sse,client,cache}.rs`; `docs/evidence/ui-runtime-<date>.md`.
Changed: `Cargo.toml` (workspace), `crates/tc-ui/{Cargo.toml,src/lib.rs,src/i18n.rs,
vendor-tc4.sh,vendored-from-tc4.txt,tests/vendored_assets.rs,tests/i18n_catalogs.rs}`,
`.github/workflows/ci.yml`, `docs/APP-DESIGN.md`.

## 5. Steps

Each step ends in a commit that builds and passes its tests. Run the narrow checks after each
step and the full list (§8) before the last commit.

### Step 1. Vendor tC4's `src/ui.css` again

The parity judge had it dropped because nothing loaded it (`parity/ds-gallery/JUDGE.md:234`);
the app's `index.html` now loads it after `ds/styles.css`, as `tC4:src/main.jsx:3-4` imports
them. It holds `html,body,#root{height:100%}` and the window-control rules that keep controls on
the physical side under RTL (§6.9). Its blob at `d310bf3` is
`699c4d131c0c5a621146f3b44648ad9e4685ac07`.

- `crates/tc-ui/vendor-tc4.sh`: add `src/ui.css` to the first `git ls-tree` list; map
  `src/ui.css) echo "ui.css"` in `asset_path`; add `"$ASSETS/ui.css"` to the `rm -rf` line (it
  is outside the three folders the line removes). Update the header comment.
- Run `crates/tc-ui/vendor-tc4.sh /home/user/unfoldingword/translationcore4 d310bf3`. Only
  `assets/ui.css` and one new line in `vendored-from-tc4.txt` may change (`git status` proves
  it; the checkout is read-only and the script reads the commit, not the working tree).
- `tests/vendored_assets.rs`: `asset_path` gains `if tc4_path == "src/ui.css" { return
  "ui.css".into() }` before the prefix loop.
- The gallery copies `assets/` whole but links only `ds/styles.css`, so its pixels do not move.
- Check: `cargo test -p tc-ui -j 2 --test vendored_assets`.

### Step 2. Dependencies, generated keys and the language core

**Workspace `Cargo.toml`**: `[workspace.dependencies] regex = "1.13.1"`.

**`crates/tc-ui/Cargo.toml`**:

```toml
[features]
gallery = ["dep:wasm-bindgen-futures", "dep:console_error_panic_hook"]
# The application's runtime and page (APP-DESIGN §6): the API client, the event stream, the
# query cache, the server state and the language. Until unit A5 it mounts a stand-in root.
app = ["dep:wasm-bindgen-futures", "dep:console_error_panic_hook"]

# Every target: the pure parts (event parser, cache bookkeeping, call errors, language) are
# host-tested by `cargo test --workspace`, which enables no feature.
[dependencies]
tc-api = { path = "../tc-api" }
tc-core = { path = "../tc-core" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }   # one error type per module, as AGENTS.md asks

[build-dependencies]
serde_json = { workspace = true }
```

In the wasm32 table remove `serde_json` (now above) and add to `web-sys` features, beside the
existing ones: `AbortController`, `AbortSignal`, `Blob`, `DataTransfer`, `DragEvent`, `File`,
`Headers`, `HtmlAnchorElement`, `HtmlIFrameElement`, `HtmlSelectElement`, `Performance`,
`PerformanceObserver`, `ReadableStream`, `ReadableStreamDefaultReader`, `Request`,
`RequestInit`, `Storage`, `TextDecoder`. (The row's list plus what A4a's code calls:
`AbortSignal`, `HtmlSelectElement`, `Performance`. `TextDecoder` stays enabled although the
parser of step 4 reads bytes; later units may use it.) Host dev-dependencies gain `regex = { workspace = true }`
and `tc-api = { path = "../tc-api", features = ["proptest"] }` (the `Arbitrary` frames of the
SSE round trip). `cargo build` must keep `Cargo.lock` changes to the new edges only.

**`crates/tc-ui/build.rs`** (runs on the host for every target; the workspace lints apply to it,
so no `unwrap`, `expect` or `panic!`: `fn main() -> Result<(), Box<dyn std::error::Error>>`):

- Read `assets/i18n/en.json` and `assets/i18n-rebuild/en.json` (relative to
  `CARGO_MANIFEST_DIR`), each a JSON object; print `cargo:rerun-if-changed=` for both.
- Fail the build (return `Err` with a message naming the key) when: a key is in both files (the
  rebuild never shadows tC4, as tc-api's `rebuild_keys_never_shadow_vendored_keys` also checks);
  a key is not `segment(.segment)*` with segments of ASCII letters, digits, `-` and `_` that
  start with a letter; two keys give one Rust path.
- Names: split each segment into words at `-`, `_` and lower-or-digit→upper boundaries
  (`syncResult` → `sync`,`Result`; `lock-held` → `lock`,`held`). Every segment but the last is a
  module in snake case (`sync_result`, `open_door43`); the last is a constant in SCREAMING snake
  case (`SAVE_ERROR`, `LOCK_HELD`). Prefix a module that is a Rust keyword with `r#` (none
  today).
- Write `$OUT_DIR/i18n_keys.rs`: nested `pub mod`s with `pub const NAME: crate::i18n::Key =
  crate::i18n::Key("dotted.key");` sorted by key, each with a doc comment holding the English
  text (rustdoc then shows what a key says), plus at the root `pub const ALL: &[crate::i18n::Key]
  = &[…];` in key order. The generated code must pass `cargo clippy -D warnings` (no
  `#[allow]` needed if names follow the rules above; escape doc text that contains `*`/`[`… by
  writing it as a `#[doc = "…"]` string literal through `{:?}`).

**`crates/tc-ui/src/i18n.rs`** (keep `TextDir`, `Locale`, `LOCALES`, `DEFAULT_LOCALE`,
`locale()`, `placeholders()`; everything below compiles on every target):

```rust
/// A catalog key that exists in English. Only the generated `keys` make one from text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(&'static str);
impl Key { pub const fn as_str(self) -> &'static str }

/// One constant per key of `assets/i18n/en.json` and `assets/i18n-rebuild/en.json`.
pub mod keys { include!(concat!(env!("OUT_DIR"), "/i18n_keys.rs")); }

/// The UI locale of an API locale id. Total: a test proves both lists hold the same ten ids.
pub fn locale_of(id: tc_api::ids::LocaleId) -> &'static Locale
/// `<html lang>` and `<html dir>` for the shown language.
pub fn html_lang_dir(id: LocaleId) -> (&'static str, &'static str)

/// One language's messages: tC4's catalog, then the rebuild's file, which never overrides it.
#[derive(Debug, Clone, Default)]
pub struct Catalog { /* HashMap<Box<str>, Box<str>>: string entries only, empty ones kept */ }
#[derive(Debug, thiserror::Error)] pub enum CatalogError { NotAnObject { which: &'static str }, Json(..) }
impl Catalog {
    pub fn parse(vendored: &str, rebuild: &str) -> Result<Catalog, CatalogError>;
    pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool;
    pub fn get(&self, key: &str) -> Option<&str>;
}
/// The rebuild's own text, compiled in: (id, the file's text) for the ten languages.
pub const REBUILD: [(&str, &str); 10] = [("en", include_str!("../assets/i18n-rebuild/en.json")), …];
/// tC4's English catalog, compiled in (the fallback of every key).
pub const ENGLISH_VENDORED: &str = include_str!("../assets/i18n/en.json");

pub type Vars<'a> = &'a [(&'a str, &'a dyn std::fmt::Display)];

/// tC4's `t(key, vars)` in the shown language.
pub fn t(key: Key, vars: Vars<'_>) -> String;
/// tC4's `t(key, vars, fallback)` for a key that is not a constant: one a tc-api type names
/// (`Message::key()`, `NetKind::ask_key()`) or one made at run time.
pub fn t_dyn(key: &str, vars: Vars<'_>, fallback: Option<&str>) -> String;
/// Makes `catalog` the messages of `locale` (boot installs the nine fetched ones).
pub fn install_catalog(locale: LocaleId, catalog: Catalog);
/// tC4's `setLocale`: false, and nothing changes, when `locale` is already shown. On wasm with
/// `app` it sets `<html lang dir>` first, then tells every view that reads `t()`.
pub fn show_locale(locale: LocaleId) -> bool;
/// The shown language; a view that reads it renders again when it changes.
pub fn shown_locale() -> LocaleId;
/// Where `[i18n] missing key: <key>` goes: the console on wasm by default, nothing on the host
/// until a test sets a sink.
pub fn set_missing_key_sink(sink: impl Fn(&str) + 'static);
```

Implementation notes:

- State is a `thread_local!` `RefCell` holding the shown `LocaleId`, the English `Catalog`
  (parsed lazily from `ENGLISH_VENDORED` + `REBUILD`'s `en`; build.rs parsed the same bytes, so a
  failure cannot happen; use `unwrap_or_default()` with that comment, never `unwrap`), the
  installed catalogs, the warned-key set and the sink. Never hold the borrow while calling the
  sink (collect, release, then call).
- Reactivity: under `#[cfg(all(target_arch = "wasm32", feature = "app"))]` a thread-local
  `ArcRwSignal<LocaleId>` (reactive_graph 0.2.16; it needs no owner) mirrors the shown locale;
  `t()`, `t_dyn()` and `shown_locale()` call `.track()` on it; `show_locale` sets the
  attributes on `document.documentElement` (`set_attribute("lang", …)`, `("dir", …)`) and then
  `.set()`s the signal. On the host and in the gallery build there is no tracking. This keeps one
  `t()` that host tests call directly (each test thread has its own state).
- Resolution order for shown locale `L` and key `k`: `L`'s vendored entry if a non-empty string;
  else `L`'s rebuild entry if a non-empty string; else English (vendored, then rebuild), used as
  it is; else the fallback (as given) if any; else warn once and return the key. Then the vars,
  sequentially, as §3.3.

**Tests** — new `crates/tc-ui/tests/i18n_runtime.rs` (`#![cfg(not(target_arch = "wasm32"))]`,
`#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`), loading the real catalogs
from `assets/` (the fixtures are the shipped files):

- `t_reads_the_shown_catalog_and_a_selection_shows_at_once` — tC4's case: `account.language`
  in en, then fr, then hi equals each catalog's text, and fr's differs from en's.
- `t_falls_back_to_english_per_key_when_a_target_entry_is_absent_or_empty` — es-419 with
  `common.close` removed, then set to `""`: English both times; another es-419 key stays Spanish.
- `t_warns_once_per_missing_key_and_shows_the_key` — tC4's i18n-keys case: two calls, one
  `[i18n] missing key: no.such.key.for.issue12` through the sink; `app.name` warns nothing.
- `an_explicit_fallback_wins_only_over_a_missing_key_and_is_not_filled` — fallback with
  `{n}` stays literal; a present key ignores the fallback.
- `vars_replace_every_occurrence_in_the_order_given` — `{n} of {n}`; and the sequential quirk:
  vars `[("name", "{n}"), ("n", "3")]` on `{name}` gives `3`, as tC4 does.
- `a_rebuild_key_reads_in_english_until_it_is_translated` — `upload.notWritten` with
  `{reason}` in ar equals the English rebuild text filled.
- `a_rebuild_key_never_overrides_a_vendored_key` — `Catalog::parse` of the real `fr.json` and a
  rebuild text `{"common.close":"X"}` keeps tC4's text.
- `showing_the_shown_locale_again_changes_nothing` — `show_locale` returns false.
- `every_ui_locale_is_an_api_locale_in_the_same_order` — `LOCALES` ids equal `LOCALE_IDS`;
  `locale_of` round-trips each.
- `lang_and_dir_follow_the_locale` — `html_lang_dir` of all ten: lang is the id; only ar is rtl.
- `every_catalog_parses_with_every_english_key` — each of the ten parses with ≥ 1,029 entries.
- `the_compiled_rebuild_files_are_the_files_on_disk` — every `REBUILD` text equals the file's
  bytes, and `ENGLISH_VENDORED` equals `assets/i18n/en.json` (guards the `include_str!` paths).
- `a_catalog_that_is_not_an_object_is_refused`.

Also replace the header of `tests/i18n_catalogs.rs` ("the rebuild has no `t()`…") with a pointer
to `i18n_runtime.rs` and `i18n_keys.rs`.

Commit: "Add the interface language: t() as tC4 resolves it, key constants generated from the
catalogs, and the shown language".

### Step 3. The `i18n-keys` port with floors

New `crates/tc-ui/tests/i18n_keys.rs`, a port of `tC4:test/i18n-keys.test.ts`. It scans
`crates/tc-ui/src/**/*.rs` (walk with `std::fs`; the generated file is in `OUT_DIR` and is not
scanned). "Catalog" means the merged English (`assets/i18n/en.json` + `assets/i18n-rebuild/
en.json`, 1,045 keys today); "namespace" means a first segment of a catalog key.

- `every_key_constant_names_a_catalog_key_and_every_key_has_one` — `keys::ALL` as strings equals
  the catalog's key set (the generator saw both files).
- `every_catalog_namespaced_string_in_the_ui_source_is_a_catalog_key` — regex
  `"([A-Za-z][\w-]*(?:\.[\w-]+)+)"` (double quotes only: in Rust `'…'` is a char); a match
  whose first segment is a namespace must be a key. This is tC4's indirect-key check, and it
  catches a `t_dyn("x.y", …)` typo. False positives go in an `ALLOWED` list with a reason each,
  like tc-api's `UNUSED_TC4_CODES` (none expected; the strings A4a adds, `"tc4.token"`,
  `"tc4.reloadedFor"`, have no namespace first segment).
- `every_template_key_family_matches_a_catalog_key` — regex over `format!(` whose literal holds a
  `.` and a `{…}`; the literal's `{…}` parts become `[\w-]+` and the pattern must match at least
  one key (tC4's template check). There are none in A4a; the test passes empty and later units
  (Check, Align) fill it.
- `the_scan_sees_each_call_shape` — the three scanners on a fixed Rust snippet holding
  `t(keys::app::SAVED, &[])`, `t_dyn("home.title.missing", &[], None)`, a role table
  `name: "sources.roleNotes"`, `format!("net.ask.{kind}")` and decoys (`"app.json"` inside an
  `ALLOWED` example, `'.'`, `"0.5"`), with exact expected finds. This is what keeps the scan from
  passing vacuously while the real source is small.
- `the_scan_finds_at_least_its_floors` — `KEY_CONSTANT_USES_FLOOR` (uses of `keys::…` paths),
  `NAMESPACED_FLOOR`, `TEMPLATE_FLOOR`, set to exactly what the scan finds when A4a is done
  (about 5, 0, 0). A doc comment says: each screen unit raises them to its new counts and never
  lowers one without a reason in its commit; tC4's floors (100 literal, 100 namespaced) are the
  end state once every screen is ported.

Commit: "Port tC4's i18n-keys scan with floors that later screens raise".

### Step 4. The event stream parser and reader (`src/app/events.rs`, pure)

`src/lib.rs` gains `pub mod app;` and a doc line. `src/app/mod.rs` starts with
`#![deny(clippy::indexing_slicing)]` (§6.5, A-15: the app modules index with `get`), declares the
pure modules on every target, and the `web` module under
`#[cfg(all(target_arch = "wasm32", feature = "app"))]`.

```rust
/// One server-sent event (WHATWG "interpreting an event stream").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent { pub id: Option<String>, pub data: String }

/// Server-sent events from bytes in chunks of any size: a chunk may end inside a line, inside a
/// UTF-8 character, or between the CR and the LF of one line end.
#[derive(Debug, Default)]
pub struct SseParser { /* line: Vec<u8>, after_cr: bool, started: bool, data: String,
                          has_data: bool, last_id: Option<String> */ }
impl SseParser {
    pub const MAX_LINE: usize = 16 * 1024 * 1024;
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, LineTooLong>;
}
#[derive(Debug, thiserror::Error)] #[error("a line of the event stream is over 16 MiB")]
pub struct LineTooLong;
```

Rules (the spec, trimmed to what matters): lines end at CRLF, LF or CR (a CR at a chunk's end
swallows an LF that starts the next chunk); a complete line is decoded with
`String::from_utf8_lossy`; one U+FEFF at the very start of the stream is dropped; an empty line
dispatches the pending event if any `data` line came (data lines joined with `\n`, no trailing
newline), and resets the data buffer; a line starting with `:` is a comment; `field: value`
drops one space after the colon; a line with no colon is a field with an empty value; `data`
appends; `id` sets the last id unless the value contains NUL (the last id persists across
events, as the spec says); `event`, `retry` and other fields are ignored; at the end of a
connection a half-read event is discarded. A line over `MAX_LINE` is an error: the reader ends
the connection and reconnects (tc-server never writes one; it bounds memory).

```rust
/// What the reader hands the runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Frame(EventFrame),
    /// A frame that did not parse. The runtime refreshes everything, as for `Resync` (D4).
    Unreadable { seq: Option<u64>, error: String },
    /// A frame of another server run (§3.10).
    Restart(ServerInstance),
}

/// The UI's position in the stream, and its rules for repeats (D4).
#[derive(Debug)]
pub struct EventReader { /* parser, after: u64, instance: ServerInstance */ }
impl EventReader {
    /// `instance` is `hello`'s; frames are read only after `hello` (D4).
    pub fn new(instance: ServerInstance) -> Self;
    /// The `after` of the next connection: the last frame read, 0 at first.
    pub fn after(&self) -> u64;
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Incoming>, LineTooLong>;
    /// A new connection starts a new stream: the half-read line and event are dropped.
    pub fn reconnected(&mut self);
}
```

`push` decodes each event's `data` as `EventFrame`. Not JSON or not a frame → `Unreadable`, and
`after` moves to `id` if it parses as `u64` greater than `after`. A frame of another instance →
`Restart(new)`; the reader adopts the new instance and `after` restarts at that frame's seq. A
`Resync` frame is always returned and sets `after = seq` even when lower. Any other frame with
`seq <= after` is skipped; otherwise returned and `after = seq`.

Tests in `crates/tc-ui/tests/app/sse.rs` (with `tests/app.rs` as the one binary: `#![cfg(not(
target_arch = "wasm32"))]`, the allow line, `mod app { mod sse; mod client; mod cache; }`, as
`crates/tc-app/tests/app.rs` does):

- `reads_the_frames_tc_server_writes` — the bytes `: tc4\n\n`, two frames written exactly as
  `crates/tc-server/src/events.rs:82` writes them (`format!("id: {}\ndata: {data}\n\n", frame.seq)`
  with `data = serde_json::to_string(&frame)`), a `:\n\n` heartbeat: two frames, in order.
- `comments_and_heartbeats_dispatch_nothing`.
- `any_split_of_the_stream_gives_the_same_frames` — proptest: 0..8 `EventFrame`s from tc-api's
  `Arbitrary`, seqs made increasing, interleaved comments, line ends LF, CRLF or CR per stream,
  split at random byte boundaries (which cut UTF-8 characters: include a frame whose
  `ChangeCause::OutsideEdit { path }` is Hebrew and Arabic text) → the same frames.
- `a_cr_lf_split_across_chunks_ends_one_line`, `data_lines_join_with_a_newline_and_one_space_is_dropped`,
  `an_event_is_dispatched_only_at_its_blank_line`, `a_bom_at_the_start_is_dropped_once`,
  `fields_other_than_data_and_id_are_ignored`, `an_id_holding_nul_is_ignored`,
  `a_line_over_the_limit_is_an_error`.
- Reader: `after_follows_the_last_frame_read`, `a_repeated_frame_is_skipped`,
  `a_resync_is_never_skipped_and_resets_after` (a `Resync` numbered below `after`),
  `an_unreadable_frame_is_reported_and_after_moves_past_it`,
  `a_frame_of_another_server_run_is_a_restart`, `a_reconnect_drops_the_half_read_event`,
  `replays_with_repeats_are_read_once` (proptest: a frame list delivered with random reconnects
  that resend a suffix → each frame once, in order).

Commit: "Add the event stream parser and reader, host-tested on the server's own bytes".

### Step 5. Call errors, boot decisions and `Load<T>` (pure)

`src/app/client.rs`:

```rust
pub const API_ROOT: &str = "/api/v1/";
/// `/api/v1/<service>/<name>` of an endpoint.
pub fn api_url<E: Endpoint>() -> String;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    #[error(transparent)] Api(ApiError),
    /// A read whose open closed or was replaced before its reply came; the reply was dropped.
    #[error("the project was closed or opened again")] Dropped,
    #[error(transparent)] Transport(Transport),
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Transport {
    /// No reply: the server is gone or restarting, or the page is leaving.
    #[error("the app's server did not answer: {0}")] Unreachable(String),
    /// A status the API itself never answers with (401, 403, 404, 413, 500, 503).
    #[error("the app's server answered {0}")] Status(u16),
    #[error("the app's server sent a reply that could not be read: {0}")] Unreadable(String),
}
/// A reply as the server sent it: 200 carries `Reply<T>`, 409 carries `ApiError::Version`.
pub fn interpret<T: DeserializeOwned>(status: u16, body: &str) -> Result<T, CallError>;
```

For 409 parse `Reply<serde::de::IgnoredAny>`; an `Err(ApiError)` inside is `Api`, anything else
is `Transport(Status(409))`.

`src/app/boot.rs`:

```rust
/// The launch's token (§5.2): 43 base64url characters.
#[derive(Clone, PartialEq, Eq)] pub struct Token(String);   // Debug prints "Token(..)"; never logged
impl Token { pub fn parse(text: &str) -> Option<Token>; pub fn as_str(&self) -> &str }
/// The fragment's `t=` wins (a new launch), else the token kept in `sessionStorage` (a reload).
pub fn token_from(fragment: &str, kept: Option<&str>) -> Option<Token>;
pub const TOKEN_KEY: &str = "tc4.token";          // sessionStorage
pub const RELOADED_FOR_KEY: &str = "tc4.reloadedFor";
pub enum BuildCheck { Same, ReloadOnce { marker: String }, Stale { ui: String, server: String } }
/// D8. `reloaded_for` is the marker a reload left.
pub fn check_build(server_commit: &str, server_api: u32, reloaded_for: Option<&str>) -> BuildCheck;
```

The marker is `"<commit>/<api>"` of the server; `ReloadOnce` when it differs from ours and the
marker is not already that value; `Stale` when it is. Fragment parsing: strip `#`, split on `&`,
take the `t=` value; only a valid `Token` counts. Do not strip the fragment from the address:
a reload keeps it and the shell gives every launch a new one.

`src/app/load.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Load<T> { Loading, Ready(T), Refreshing(T), Failed(CallError) }
impl<T> Load<T> {
    pub fn value(&self) -> Option<&T>;          // Ready or Refreshing
    pub fn is_pending(&self) -> bool;           // Loading or Refreshing
    pub fn error(&self) -> Option<&CallError>;
    /// A fetch starts: Ready keeps its value as Refreshing; Loading and Failed become Loading.
    pub fn refreshing(self) -> Self;
    pub fn as_ref(&self) -> Load<&T>; pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Load<U>;
}
```

"Failed" is never drawn as "absent": there is no variant that could be.

Tests in `tests/app/client.rs`: `an_ok_reply_is_its_value`, `an_err_reply_is_the_api_error`,
`a_409_carries_the_version_error`, `other_statuses_are_transport_failures` (401, 403, 404, 413,
500, 503 with tc-server's plain-text bodies), `an_unreadable_body_is_a_transport_failure`,
`api_paths_are_under_the_api_root` (`api_url::<Hello>() == "/api/v1/app/hello"`,
`EVENTS_PATH`, `UPLOADS_PATH`), `the_fragment_token_wins`, `a_malformed_fragment_falls_back_to_the_kept_token`
(wrong length, a `+`, empty), `no_token_anywhere_is_none`, `the_token_never_shows_in_debug`,
`the_same_build_proceeds`, `another_build_reloads_once_then_stops`,
`another_api_version_reloads_once_then_stops`, `refreshing_keeps_the_value_and_a_failure_does_not`.

Commit: "Add the call errors, the boot's token and build checks, and Load".

### Step 6. The query cache's bookkeeping (`src/app/cache.rs`, pure)

All decisions of the cache live here, so they are host-tested; the wasm glue (step 7) only
executes them.

```rust
/// A read, by endpoint and request: equal requests share one fetch and one value (§6.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryId { pub path: &'static str, pub body: String }
impl QueryId { pub fn of<E: Endpoint>(request: &E) -> Result<QueryId, serde_json::Error> }

/// What makes a read stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOf {
    /// A read of the app: refreshed by these topics and by `Resync`.
    App(Vec<AppTopic>),
    /// A read of one open project: dropped when the open ends, refreshed by a `Changed` at
    /// `at` whose scope one of `keys` depends on (`QueryKey::depends_on`), and by `Resync`.
    Project { at: Scope, keys: Vec<QueryKey> },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppTopic { Projects, HomeCard(ProjectRef) }

/// A read a screen keeps in the cache. Later units implement it per query endpoint.
pub trait CachedQuery: Endpoint<Ok: Send + Sync> + Clone {
    fn read_of(&self) -> ReadOf;
    /// The snapshot a reply was built from; an older one never replaces a newer one.
    fn view(_ok: &Self::Ok) -> Option<ViewSeq> { None }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Ticket(u64);   // one fetch attempt
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Start { pub id: QueryId, pub ticket: Ticket }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing { pub apply: bool, pub then: Option<Ticket> }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed { Value(Option<ViewSeq>), Error, StaleScope }

#[derive(Debug, Default)]
pub struct Registry { /* reads: HashMap<QueryId, Read>, newest: HashMap<ProjectRef, Epoch>,
                         dead: HashSet<Scope>, next: u64 */ }
impl Registry {
    pub fn hold(&mut self, id: &QueryId, read: ReadOf) -> Option<Ticket>;     // first holder fetches
    pub fn release(&mut self, id: &QueryId) -> bool;                        // true: forgotten
    pub fn changed(&mut self, at: &Scope, scope: &ChangeScope) -> Vec<Start>;
    pub fn topic(&mut self, topic: &AppTopic) -> Vec<Start>;                // ProjectsChanged: every
                                                                            // `Projects` read; HomeCard
    pub fn resync(&mut self) -> Vec<Start>;
    pub fn opened(&mut self, at: Scope) -> Vec<QueryId>;                    // forgets older-epoch reads
    pub fn closed(&mut self, at: &Scope) -> Vec<QueryId>;
    pub fn restarted(&mut self) -> Vec<QueryId>;
    pub fn is_current(&self, at: &Scope) -> bool;
    pub fn landed(&mut self, id: &QueryId, ticket: Ticket, landed: Landed) -> Landing;
}
```

Rules and invariants:

- A read is `Idle` or `Running { ticket, again }`. Making a read stale: `Idle` → a new ticket
  (a `Start`); `Running` → `again = true` and no new fetch. **Invariant: a change that reaches a
  read after its fetch started always causes another fetch after that one lands.** This is what
  keeps a teammate's change from being hidden behind a reply computed before it.
- `is_current(at)`: `at` is not in `dead` and no newer epoch of its folder is known. Epochs are
  learnt from `opened` (hello's sessions, `Session { Opened }`), and from `hold` of a project
  read. `closed` and a `StaleScope` landing put the scope in `dead`. Tracking dead scopes, not
  live ones, means a unit that forgets to announce an open still gets its replies.
- `landed`: the read is gone, or the ticket is not its running one → `apply: false, then: None`
  (a late reply of a forgotten read does nothing). `StaleScope` → the scope is dead, its reads
  are forgotten, nothing applied. A project read whose scope is no longer current → not
  applied, forgotten. `Value(Some(v))` older than the read's last applied view → not applied.
  Otherwise applied (an `Error` is applied as `Failed`). Then `again` → `then: Some(new ticket)`.
- `changed` visits each live read once; its cost is the number of live reads, never the size
  of the project (PERFORMANCE rule 1).

Tests in `tests/app/cache.rs`, using real `QueryKey`s, `ChangeScope`s and `Scope`s:
`two_holders_of_one_read_share_one_fetch`;
`a_change_refetches_only_the_reads_that_depend_on_it` (TIT 1 changed: `Chapter(TIT 1)` and
`Book(TIT)` refetch; `Chapter(TIT 2)`, `Book(PSA)` and a read of another folder or epoch do
not); `everything_refetches_every_read_of_that_open_only`;
`a_change_during_a_fetch_fetches_again_after_it_lands`;
`many_changes_during_one_fetch_cost_one_more_fetch`; `resync_refetches_every_live_read`;
`topics_refetch_their_reads` (`Projects`, `HomeCard(p)` only for `p`);
`a_reply_for_a_closed_open_is_dropped`; `a_newer_epoch_forgets_the_reads_of_the_older_one`;
`a_stale_scope_reply_is_dropped_and_kills_the_scope`;
`an_older_view_never_replaces_a_newer_one`;
`the_last_release_forgets_the_read_and_its_late_reply`;
`restart_forgets_every_read_and_open`; and the property
`no_change_is_hidden_behind_an_earlier_reply` — random interleavings of hold, release,
changed, resync and landed over three reads and two scopes; when every fetch has landed, each
held read's last fetch started after the last change that touched it.

Commit: "Add the query cache's bookkeeping: shared reads, refetch on change, scope drop".

### Step 7. The browser runtime (`src/app/web/*`, wasm32 with `app`)

Everything here is glue over steps 2–6. Rules for all of it: never hold a `RefCell` borrow
across an `.await` or while calling out (collect, drop the borrow, act); signals the cache and
`ServerState` own are `ArcRwSignal`s (reference counted, so a late completion never meets a
disposed signal; §6.5 asks `try_*` of arena signals for the same reason); Leptos 0.8's
`provide_context` and `on_cleanup` need `Send + Sync`, so wrap `Rc` values in
`send_wrapper::SendWrapper` (already a dependency; wasm is single-threaded); no `unwrap`,
`expect`, `panic!` or indexing.

**`web/js.rs`**: `window() -> Result<Window, JsValue>`, `session_get(key) / session_set(key,
value)` (every access a `Result`; a failure is ignored, as §6.6 asks of storage),
`mark(name)` (`performance.mark`, ignored on error), `sleep(ms) -> JsFuture` (a `Promise` over
`setTimeout`), `race_with_timeout(promise, ms) -> Option<JsValue>` (`Promise.race` with a
timer that resolves to a sentinel), `set_idle(bool)` (`Reflect::set(window, "__tcIdle", …)`).

**`web/api.rs`**:

```rust
#[derive(Clone)]
pub struct Api { /* token: Rc<str>, busy: Rc<Busy> */ }
impl Api {
    /// One request (§6.2, D1). Headers: Content-Type: application/json, X-Tc-Api, X-Tc-Token.
    pub async fn call<E: Endpoint>(&self, request: &E) -> Result<E::Ok, CallError>;
    /// GET of a static file under the build folder (the catalogs), with the token.
    pub(crate) async fn get_text(&self, url: &str) -> Result<String, CallError>;
    /// GET /api/v1/events?after=n, with an AbortController; Ok once the response head is in.
    pub(crate) async fn open_events(&self, after: u64, abort: &AbortController)
        -> Result<ReadableStreamDefaultReader, CallError>;
}
```

`call` builds `Request::new_with_str_and_init(&api_url::<E>(), &init)` with method POST and the
JSON body (`serde_json::to_string`), awaits `fetch_with_request` (a rejection is
`Transport(Unreachable)`), reads `text()`, and returns `interpret(status, &text)`. It counts
itself in `Busy` with a guard that decrements on every path. On `Err(Api(ApiError::Version
{..}))` it runs D8's reload-once. The browser adds `Origin` to the POST; never set `mode`.

**`web/stream.rs`**: the reader loop, spawned once after boot with the already-open reader
(D4):

```
loop {
  read: race_with_timeout(reader.read(), 45_000):
    timeout            -> abort the controller; break to reconnect
    {done: true}/error -> break to reconnect
    {value: Uint8Array} -> event_reader.push(&value.to_vec()) -> for each Incoming: runtime.dispatch(..)
                          (LineTooLong -> abort; reconnect)
  reconnect: event_reader.reconnected(); sleep(backoff); open_events(event_reader.after());
             backoff 250 ms doubling to 5 s, reset after the first chunk of a connection
}
```

Read `{done, value}` with `js_sys::Reflect` as `spikes/loopback/src/lib.rs:66-80` (`read_all`) does.

**`web/state.rs`** (D3):

```rust
#[derive(Clone)]
pub struct ServerState {
    pub instance: ArcRwSignal<ServerInstance>,
    pub internet: ArcRwSignal<bool>,
    pub account: ArcRwSignal<AccountView>,
    pub startup: ArcRwSignal<StartupState>,
    pub sessions: ArcRwSignal<Vec<OpenSession>>,
    pub saved_locale: ArcRwSignal<LocaleId>,       // hello.prefs.appLocale; M16 rolls back to it (A5)
    pub jobs: ArcRwSignal<BTreeMap<JobId, JobView>>,
    pub checkpoints: ArcRwSignal<HashMap<Scope, CheckpointOutcome>>,
    pub blocked: ArcRwSignal<HashMap<Scope, Vec<String>>>,
    pub align_saves: ArcRwSignal<HashMap<AlignerId, AlignSave>>,
    pub restarted: ArcRwSignal<bool>,
}
```

**`web/query.rs`**:

```rust
#[derive(Clone)]
pub struct QueryCache { /* Rc<RefCell<{ registry: Registry, slots: HashMap<QueryId, Rc<dyn Slot>> }>>, api */ }
impl QueryCache {
    /// The read `request`, held while the calling reactive owner lives (`on_cleanup` releases).
    pub fn read<Q: CachedQuery>(&self, request: Q) -> Signal<Load<Arc<Q::Ok>>>;
}
```

A `Slot` is a type-erased `TypedSlot<Q> { request: Q, value: ArcRwSignal<Load<Arc<Q::Ok>>> }`
(`as_any` + downcast; `QueryId.path` fixes the type). Starting a ticket sets the value to
`refreshing()` and spawns `api.call(&request)`; on landing it asks the registry and, if
`apply`, sets `Ready(Arc::new(ok))` or `Failed(err)`; `then` starts again. A serialization
failure of the request (cannot happen with tc-api's types) sets `Failed(Transport(Unreadable))`.
No phase-0 endpoint is a cached read (internet, account and jobs are `ServerState`, D3), so the
first real user is B2; keep this file thin, because no browser test runs it before then.

**`web/mod.rs`** — the runtime:

```rust
#[derive(Clone)]
pub struct Runtime(Rc<Inner>);   // api, cache, state, listeners, busy, base
pub fn use_runtime() -> Option<Runtime>;   // from context
impl Runtime {
    pub fn api(&self) -> &Api; pub fn cache(&self) -> &QueryCache; pub fn state(&self) -> &ServerState;
    /// A read an action makes itself, with the scope drop of D2.
    pub async fn read_once<Q: CachedQuery>(&self, request: &Q) -> Result<Q::Ok, CallError>;
    pub fn is_current(&self, at: &Scope) -> bool;
    /// Every frame, after the runtime has applied it; A4b's save machine needs `Changed`
    /// with `LocalSave` (AD37). The listener lives until `unlisten`.
    pub fn listen(&self, f: impl Fn(&EventFrame) + 'static) -> ListenerId;
    pub fn unlisten(&self, id: ListenerId);
}
#[wasm_bindgen] pub fn run_app(base: String);
```

`dispatch(Incoming)` is one exhaustive `match` with no wildcard:
`Unreadable` → as `Resync`; `Restart(i)` → `cache` forgets everything (`restarted`), `state.
restarted` true, `state.instance` = `i`, read `hello` again. `Frame` by `event`:
`Resync` → `registry.resync()` starts + read `hello` again (replace internet, account, startup,
sessions, saved_locale; not the shown language); `Internet { on }`; `Account(v)`; `Startup(s)`;
`Job(v)` (insert; recompute `Busy`); `ProjectsChanged` → `topic(Projects)`; `HomeCard {
project }` → `topic(HomeCard(project))`; `Session { at, event }`: `Opened` → `registry.opened`;
`Closed` → `registry.closed`, remove from `sessions`, `checkpoints`, `blocked`; `Mode(m)` → set
the session's mode; `Changed { scope, .. }` → `registry.changed(&at, &scope)` starts;
`Checkpoint(o)`; `MaterializeBlocked { paths }`; `AlignSave { aligner, state }`. Then every
listener gets the frame. The check rail's row patching (AD17) is B5a's, through a listener.

`Busy` counts API calls and catalog fetches in flight (not the event stream) and jobs whose state
is `Queued`, `Running` or `Cancelling`; every change calls `set_idle(fetches == 0 && jobs ==
0)`. `window.__tcIdle` is false from `run_app` until the root is mounted or the boot error is
shown (§6.2; A4b adds saves).

`run_app(base)` (`base` is `/ui/<build>/`; refuse anything else with the boot error), in
order, marking `tc4:boot`, `tc4:catalogs`, `tc4:hello`, `tc4:mounted`:

1. `console_error_panic_hook::set_once()` (A4b replaces it with its hook); `set_idle(false)`.
2. Token: `token_from(location.hash, session_get(TOKEN_KEY))`; none → boot error ("this window
   was opened without its key"); else `session_set(TOKEN_KEY, …)`.
3. Start the nine catalog fetches in parallel: `get_text(&format!("{base}i18n/{id}.json"))`
   for every `LocaleId` but `en`, each `Catalog::parse(text, REBUILD's id)`.
4. `open_events(0, …)` and await its head (D4). Then `api.call(&Hello {})`.
5. `check_build(...)`: `ReloadOnce` → set the marker, `location.reload()`, stop; `Stale` → boot
   error naming both commits.
6. Await the catalogs: any failure → boot error naming the file and the reason (D5). Install
   them; `show_locale(hello.prefs.app_locale)` (sets `<html lang dir>` before anything mounts).
7. Build `ServerState` from `hello`, the registry's opens from `hello.sessions`, the
   `EventReader` with `hello.instance`; spawn the stream loop.
8. `mount_to(#root, …)` providing the runtime as context (`.forget()` the handle, as the gallery
   does); `set_idle` from `Busy`.

The boot error (D9) mounts into `#root`: tC4's `ViewError` shape — the DS `Callout` (tone warn,
`role="alert"`, `data-testid="boot-error"`) with `<strong>{t(keys::app::VIEW_ERROR)}</strong>`,
the reason, and a DS `Button` (`size` sm, `variant` outline, `data-testid="boot-error-retry"`)
saying `t(keys::app::RETRY)` that reloads. Shown in the saved language when its catalog is
installed, else English.

**`web/root.rs`** — the stand-in (D11), with a doc comment "unit A5 replaces this with the port
of `App.jsx`":

```html
<div data-testid="runtime-ready" style="padding:24px;display:flex;flex-direction:column;gap:12px">
  <p data-testid="runtime-sample">{t(keys::account::LANGUAGE)}</p>
  <select data-testid="runtime-locale" aria-label={t(keys::account::LANGUAGE)}>
    one <option value=id> per LOCALES entry, label = Locale.label; prop:value = shown_locale()
  </select>
  <p data-testid="runtime-internet" data-on={internet}>{internetOn | internetOff text}</p>
</div>
```

`on:change` parses the value with `LocaleId::parse` and calls `show_locale`, marking
`tc4:locale`. Every text is a `move ||` closure, so a switch updates text nodes in place; nothing
remounts.

**Checks** for this step: `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets
-j 2 -- -D warnings` three times: no feature, `--features app`, `--features gallery`. Add the
`--features app` line to `.github/workflows/ci.yml` after the gallery line.

Commit: "Add the UI runtime: the API client, the event stream reader, the query cache, the
server state and the boot, behind the app feature".

### Step 8. The page, `boot.js` and `build-app.sh`

`crates/tc-ui/app/index.html` (tC4's `index.html` with the CSS `main.jsx` imports, in its
order; no inline script, which the CSP refuses):

```html
<!DOCTYPE html>
<html lang="en" dir="ltr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>translationCore 4</title>
<link rel="icon" href="/ui/@BUILD@/images/logo-mark-color.png">
<link rel="stylesheet" href="/ui/@BUILD@/ds/styles.css">
<link rel="stylesheet" href="/ui/@BUILD@/ui.css">
</head>
<body>
<div id="root"></div>
<script type="module" src="/ui/@BUILD@/boot.js"></script>
</body>
</html>
```

`crates/tc-ui/app/boot.js`:

```js
// The UI's entry (APP-DESIGN §5.2, §6.8). The page's CSP refuses inline scripts, so the page
// loads this file. It loads the wasm and starts the app with the build folder it was served
// from, where the catalogs are. Unit A4b adds, before the import, the capture listener that
// writes each keystroke to the draft buffer, and the panic recovery (AD30).
import init, { run_app } from './tc_ui.js';

await init();
run_app(new URL('./', import.meta.url).pathname);
```

`crates/tc-ui/build-app.sh [--release] [--out DIR]` (model it on `build-gallery.sh`: the same
wasm-bindgen version check and `cargo metadata` target-dir lookup; `CARGO_JOBS` default 2;
default out `$ROOT/target/tc-ui-app`):

1. `cargo build -p tc-ui --lib --target wasm32-unknown-unknown --profile <dev|wasm-release>
   --features app`.
2. Stage in `$TARGET_DIR/tc-ui-app-stage` (inside the worktree's target, never `/tmp`), cleaned
   first: `wasm-bindgen --target web --no-typescript --out-dir <stage>` (gives `tc_ui.js`,
   `tc_ui_bg.wasm`); copy `app/boot.js`, `assets/ui.css`, `assets/ds` → `ds`, `assets/images` →
   `images`, `assets/i18n` → `i18n`.
3. `BUILD=$(cd stage && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum |
   sha256sum | cut -c1-16)`.
4. `rm -rf OUT; mkdir -p OUT/ui; mv stage OUT/ui/$BUILD; sed "s/@BUILD@/$BUILD/g"
   app/index.html > OUT/index.html`. Print the folder and the wasm size.

`TC4_BUILD_COMMIT`, when set, reaches both the wasm and `tc4` builds through the environment
(§3.10). Hand check: build, run `tc4 serve` with a temp `--app-data` and an existing
`--projects` folder under `target/`, and `curl -H "X-Tc-Token: …" -H "Host: 127.0.0.1:PORT"`
`/` and one `/ui/<build>/…` file: 200, `no-store` and the CSP on `/`, `immutable` on the file.

Commit: "Add the app's page, a minimal boot.js and build-app.sh, which builds the folder tc4
serve serves".

### Step 9. The Playwright smoke (the row's "done when")

`crates/tc-ui/app/smoke.mjs`; run as

```sh
cargo build -p tc4 -j 2
crates/tc-ui/build-app.sh
node crates/tc-ui/app/smoke.mjs --tc4 target/debug/tc4 --ui target/tc-ui-app
```

Playwright from `PLAYWRIGHT_MODULE` (default the parity copy, as `spikes/loopback/run.mjs:40-42`
does); `chromium.launch()` headless. Start `tc4 serve` with `--app-data` and `--projects` under
`target/tc-ui-smoke/<run>/` (create the projects folder first); stdin piped and kept open; read
the address line; in a `finally`, send `{"quit":true}` and, if it has not exited in 10 s, kill
it, and remove the run folder. Node requests to the API (for set-up) send `Host`, `Origin` (the
server's origin), `X-Tc-Token`, `X-Tc-Api: 1`, `Content-Type`, and a fresh 32-hex `request`
(see `nodeBaseline`, `spikes/loopback/run.mjs:74-118`).

The token reaches the page as the shell gives it (AD11): `page.goto(url + '#t=' + token)`, and a
`context.route` adds `x-tc-token` to every request **whose path is not under `/api/`** (the
shell's filter lists `/`, `/ui/*`, `/help/*`, …). Do not use `extraHTTPHeaders`: it would add the
token to `/api/` too and hide a client that forgets it. (The wasm-bindgen glue fetches the
`.wasm` with `fetch`; it is under `/ui/`, so the route covers it as the shell's filter does.)
Record every request with `page.on('request')` (URL, method, resource type, `allHeaders()`, time)
and every console message and page error.

Cases (each a named check; exit 1 on the first failure, print a JSON summary):

- **A. Boot.** `[data-testid=runtime-ready]` within 60 s (dev wasm is large). `<html lang="en"
  dir="ltr">`. Exactly nine catalog GETs, `/ui/<build>/i18n/<id>.json` for every id but `en`, each
  answered 200. Exactly one `POST /api/v1/app/hello`. One `GET /api/v1/events?after=0`, sent
  **before** the hello. Every `/api/` request carries `x-tc-token` equal to the token and `x-tc-api:
  1`. `runtime-sample` equals `account.language` of `en.json` read from disk. No page error, no
  console error, no `[i18n] missing key`. `window.__tcIdle === true` within 5 s.
  `sessionStorage['tc4.token'] === token`.
- **B. A locale switch sends nothing.** Hold an element handle of `runtime-sample`; clear the
  request log; select `ar`. Then: `<html lang="ar" dir="rtl">`; the sample equals `ar.json`'s
  `account.language`; the handle is still connected (no remount); no request of type `fetch`,
  `xhr`, `document`, `script` or `stylesheet` since the switch (D10: list `font` requests in the
  summary). Select `en` back: English and `ltr`, still no such request.
- **C. Events reach the page.** From Node `net/set { on: true }`: `runtime-internet` has
  `data-on="true"` within 2 s; `net/set { on: false }`: `"false"`. The page made no request.
- **D. Boot in the saved language.** From Node `prefs/setLocale { locale: "ar" }`; a new page with
  an init script whose `MutationObserver` records `documentElement.lang` and `dir` when `#root`
  first gets a child: they are `ar` and `rtl` (set before the root mounts), and the sample is
  Arabic. Set `en` back from Node.
- **E. A catalog that cannot load stops the boot.** New context whose route aborts
  `**/i18n/fr.json`: `[data-testid=boot-error]` shows with `app.viewError`'s English text and the
  file's name, `runtime-ready` never shows, `__tcIdle` becomes true.
- **F. A load without the fragment uses the kept token.** In case A's page, `page.goto(url)`
  without `#t=`: ready again.
- **G. Quit.** `{"quit":true}` → `{"closed":true}`, exit code 0.

The run must pass in this container; paste its summary into the commit message. If `phase0`
gains changes that move `hello` or the stream before you finish, run it again after merging.

Commit: "Add the runtime's Playwright smoke: boot, hello, events, and a locale switch with no
request".

### Step 10. Measurements

`smoke.mjs --measure 20 --out <file>` (release: `build-app.sh --release`, and `cargo build
--release -p tc4 -j 2`; one release build at a time, check `df` first), each load in a fresh
context:

- wasm bytes (`tc_ui_bg.wasm`) for the dev and `wasm-release` builds; total bytes of
  `ui/<build>/`; bytes the page fetched at boot (sum of response sizes, fonts apart).
- Boot, median and p95 of 20 loads, from `performance.timeOrigin` to the marks `tc4:catalogs`,
  `tc4:hello`, `tc4:mounted` (read `performance.getEntriesByType('mark')`).
- Locale switch, median and p95 of 20 switches alternating en↔ar: from the `change` dispatch to
  the first `requestAnimationFrame` after the sample's text changed (measure in the page with an
  init script). Any `longtask` over 50 ms during a switch (`PerformanceObserver`, §8.3) is
  reported; there must be none in the release build.

Write `docs/evidence/ui-runtime-<date>.md`: machine (CPU model from `/proc/cpuinfo`, 4 vCPU
container, load average before and after), commit, Chromium build, Playwright version, the
numbers, and how they relate to PERFORMANCE (the runtime's share of the proposed 3 s repeat
launch; 16 ms per frame). Rule 5: 20 runs, median and p95. No budget gates these numbers; if
the locale switch is over 16 ms in release, say why (catalog parse, text nodes) and what A5
must keep in mind.

Commit: "Measure the UI runtime's boot and locale switch, and record them".

### Step 11. Design text

The edits of section 2 (D1–D11) if they did not already go in with their steps, plus: §6.9
"`<html lang dir>` … set before the first paint" stays true (step 7.6); §6.2's tab-guard bullet
stays as it is. Commit message lists each edit with its reason.

## 6. Invariants to keep (reviewers will check these)

1. **No write reply is ever dropped** by the client or the runtime; scope drop is for reads (D2).
2. **No change is hidden**: a change after a read's fetch started causes another fetch (step 6);
   an unreadable frame refreshes everything; a `Resync` is never skipped; a gap in the stream is
   healed by `after` or `Resync` (D4).
3. **The token never leaves its header**: it is not logged, not in `Debug`, not in the DOM;
   `Token`'s `Debug` prints `Token(..)`. tc-ui never enables tc-api's `server` feature, so it
   cannot build or read a `ShownToken` (AD3).
4. **Language**: a missing translation shows English, never a blank and never a key while English
   has it; one warning per missing key; `<html lang dir>` change before the views re-render.
5. **Nothing panics** in tc-ui code: `get` not indexing (`#![deny(clippy::indexing_slicing)]` in
   `app`), `Result` around every storage, fetch and DOM call, no `unwrap`/`expect`/`panic!`
   outside tests.
6. **No test-only path in the product**: the stand-in root uses the same `show_locale` M16 will;
   the smoke drives the page as the shell does (header only outside `/api/`).
7. PERFORMANCE rule 3: nothing in the runtime blocks on the network; every call is async; the
   catalog parse happens once, at boot, before the first render.

## 7. Traps found while planning

- The worktree was behind `phase0` (A3 and the join landed during planning); it is now at
  `6468219`. `tc4 serve` answers from tc-app: `prefs/setLocale` and `net/set` work, and the event
  bus is tc-app's, not the stand-in's (which never sends a frame).
- `tc4 serve` without `--projects` creates `~/translationCore4/projects`; with a missing
  `--projects` folder it refuses to start.
- `X-Tc-Api: 1` is required on the event stream's GET too.
- `extraHTTPHeaders` in Playwright adds the token to `/api/` and hides a client that forgets it.
- tC4's CSS imports by relative URL, so hash the folder, not the files.
- `index.html` cannot hold an inline script (CSP): `boot.js` is a file under the hashed folder.
- The gallery has `#[wasm_bindgen(start)]`; the app must not: `boot.js` calls the exported
  `run_app`, so the two features never fight over the start function.
- `serde_json` is an optional, wasm-only dependency today, and `gallery` enables it with
  `dep:serde_json`; moving it to `[dependencies]` means removing that from the feature list.
- `vendor-tc4.sh`'s `rm -rf` names three folders; `ui.css` sits beside them. Its path mapping
  exists twice (the script and `tests/vendored_assets.rs`).
- `cargo test --workspace` enables no tc-ui feature: anything a host test needs must not sit
  behind `app`. Test files need `#![cfg(not(target_arch = "wasm32"))]`, because wasm clippy
  builds `--all-targets`.
- The workspace lints apply to `build.rs`: return errors, do not `expect`.
- Leptos 0.8: `provide_context` and `on_cleanup` need `Send + Sync`; `Signal<T>` needs `T: Send
  + Sync`; arena signals panic after disposal (use `ArcRwSignal`).
- A CR at the end of one chunk and an LF at the start of the next are one line end; a UTF-8
  character can straddle chunks: parse bytes, decode whole lines.
- A `Resync` frame can carry a number below the reader's `after`.
- After `after=0` the bus replays every frame since launch; applying them in order converges on
  the current state (internet, jobs), so do not suppress replayed frames.
- Showing Arabic loads font files lazily; that is not a request the app makes.
- Flat key constants collide (`upgrade.text.gateway` / `upgrade.textGateway`).
- Rust strings in `src/` such as `"app.json"` would match the namespaced scan (first segment
  `app` is a namespace): name storage keys `tc4.*`.
- Disk is short (5.1 GB at planning): dev wasm builds are large; prune when under 6 GB.

## 8. Before the last commit

All from the worktree, `-j 2`:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -j 2 -- -D warnings
cargo test --workspace -j 2
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 2 -- -D warnings
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 2 --features gallery -- -D warnings
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 2 --features app -- -D warnings
cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 2 -- -D warnings
crates/tc-ui/build-app.sh && cargo build -p tc4 -j 2 && \
  node crates/tc-ui/app/smoke.mjs --tc4 target/debug/tc4 --ui target/tc-ui-app
```

(`harness` does not exist yet; when it does, it gets its own line.) If the gallery's pixel
report should be re-checked because `assets/` gained `ui.css`, `parity/ds-gallery/capture-rust.mjs`
is the tool; it should show no change, since the gallery page does not link it.

## 9. What later units rely on (keep these names)

- A4b: `Api::call` for writes with `Transport` kept apart from `Api` errors; `Runtime::listen`
  for `Changed { cause: LocalSave }`; `ServerState::sessions` for the reattach after a reload;
  `boot.js` (it adds the capture listener and recovery before the import); `__tcIdle` (it adds
  saves); `TOKEN_KEY`. A4b mints `RequestId`s (web-sys `Crypto`) when it first writes.
- A5: `use_runtime()`, `t`, `t_dyn`, `show_locale`, `shown_locale`, `ServerState::{internet,
  account, saved_locale}`, the boot error, and `web/root.rs`, which it replaces.
- A6b: `build-app.sh [--release] [--out DIR]` and its output layout (`index.html`,
  `ui/<build>/`); `<html lang>` set before the root mounts.
- B2 onward: `CachedQuery` per query endpoint, `QueryCache::read`, `Load`, `ReadOf`,
  `AppTopic`, `Runtime::read_once`; raise the i18n-keys floors with each screen.
