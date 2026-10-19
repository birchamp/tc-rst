# Plan for design unit A4b ◆: the tc-ui no-loss core

Planned 2026-10-18. Worktree `/home/bot/tc-rst/.claude/worktrees/p0-a4b`, branch `p0-a4b`, cut
from `phase0` at `01befe2` ("Merge p0-a4a: the tc-ui runtime …"). Commit this file with your first
commit. This is a **no-loss unit** (PLAN.md rigor, APP-DESIGN §10 ◆): property tests, crash and
reload injection at every point, and three independent verifiers (lenses: lost text; races and
protocol; browser and parity, `.audit/workflows/app-phase0.js:23-25`).

The unit's row (docs/APP-DESIGN.md §10, phase 0):

> A4b ◆ tc-ui no-loss core: saves, buffer, panics | the join of A2 and A3 (order paragraph), if no
> earlier unit made it; the save machine (§6.5: `sent`, the NFC clean rule, retries with the first
> body *(A-2)*, `StaleScope` *(A-19)*); the refresh rule for open editors (AD37) *(A-6)*; the draft
> buffer v2 keyed by folder and project id *(A-9)*, with the bridge calls `bufferChanged` and
> `takeBuffered` *(A-3)*; `boot.js`: capture listener, reload, panic counting by page load, static
> callout, drafts posted with `fetch` *(A-15)*; panic hook and crash marker; the navigation gate
> and close guard *(A-26)*; `try_` forms and `clippy::indexing_slicing` in app modules *(A-15)* |
> A1, A2, A3, A4a | — | port of tC4's scheduler tests; proptest: no `current` dropped;
> `reload_anywhere_stores_the_last_text_once` against `test/fieldStore`, with reloads and
> `test/crash` at every point; the AD37 refresh cases; Playwright: a panic in an input handler
> keeps the keystroke, one panic with trapping closures reloads once, three panics show the static
> callout and post the drafts | the tests pass and the adversarial review's findings are fixed

The workflow adds: every red-team answer that names A4b: RT-1, RT-2, RT-3, RT-6, RT-9, RT-10,
RT-15, RT-19, RT-20, RT-27 (and RT-26 for the gate).

What the planner read, so you need not: AGENTS.md; docs/PLAN.md; docs/ARCHITECTURE.md;
docs/PERFORMANCE.md; APP-DESIGN "Owner's answers", "G0 results", §0–§2 (AD3, AD4, AD18, AD20,
AD21, AD22, AD30, AD35, AD37, AD38, AD39), §3.1–§3.10, §4.3, §4.4, §4.8, §4.9, §5.1–§5.8,
§6.1–§6.12, §7.7–§7.9, §8.1–§8.3, §9.2, §10 (intro, phase 0, A4b, the order paragraph), the red team
table, RT-1, 2, 3, 4, 6, 9, 10, 15, 17, 19, 20, 26, 27 and "How each point was answered". STORAGE
§5.14 (drafts) on the `storage` worktree (read only; never change it). Code: `crates/tc-ui` (all of
`src/app`, `src/app/web`, `app/*`, `build-app.sh`, `Cargo.toml`, `tests/app*`), `crates/tc-api/src`
(endpoint, lib tables, write, test, app, event, ids, token, align), `crates/tc-api/tests/golden.rs`,
`crates/tc-app/src` (hooks, dispatch, replies, handlers clientError, app API), `crates/tc-app/tests/app/{common,field_store}.rs`,
`crates/tc4/src/join.rs`, `crates/tc4/tests/hooks.rs`, `crates/tc-server/src/stub.rs`,
`.github/workflows/ci.yml`, `.audit/plans/A3.md` (D5–D7), `.audit/plans/A4a.md` (§9),
`docs/evidence/ui-runtime-2026-10-18.md`. tC4 at `d310bf3`: `src/data/saveScheduler.ts` (all),
`test/saveScheduler.test.ts` (all), the case lists of `test/{noteScheduler,alignSaveScheduler,checkSaveScheduler,storyScheduler}.test.ts`,
`src/state.jsx:1693-1723` (drainSchedulers), `:3680-3721` (close guard, pagehide),
`:4018-4038` (`go`), `:4373-4377` (gateway preview), `:7286-7299` (backToProjects),
`src/App.jsx:42-83` (SaveIndicator) and `:163-190` (ViewBoundary, ViewError),
`test/viewErrorBoundary.test.tsx`, `scripts/preload.cjs`.

---

## 0. Before you start

1. `git -C /home/bot/tc-rst/.claude/worktrees/p0-a4b log --oneline -1` must show `phase0`'s tip
   (`01befe2` at planning). If `phase0` moved and you have no commit yet, `git merge --ff-only
   phase0`; once you have commits, `git merge phase0` and say so in the message. Never touch the
   `storage` branch or `/home/bot/tc-rst/.claude/worktrees/storage`.
2. Every shell command that runs cargo, rustc, node, npx or Playwright starts with
   `. ~/.tc-env && `. Use `-j 6`. Use only this worktree's `target/`. If `df -h /home` shows under
   30 GB free and no rustc runs here (`pgrep -a rustc | grep p0-a4b`), run
   `python3 -I /home/bot/tc-rst/.audit/tools/prune-target.py /home/bot/tc-rst/.claude/worktrees/p0-a4b/target`.
   At planning: 252 GB free.
3. Tools: wasm-bindgen 0.2.129 in `~/.cargo/bin`; node 22.23.3; Playwright 1.56.1 is **not** in
   any `parity/*/node_modules` of this machine: set
   `PLAYWRIGHT_MODULE=/home/bot/.npm/_npx/d537ee5ee2a13f03/node_modules/playwright`
   (Chromium build 1194 is in `~/.cache/ms-playwright`; `~/.tc-env` sets
   `PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64`). `wasm-bindgen-test` is not available:
   everything with logic is a host test; the browser is covered by Playwright.
4. **The join of A2 and A3 is done** (A3 merged second; `crates/tc4/src/join.rs`,
   `tc4_serve_answers_hello_with_real_state`). A4b does nothing for it; say so in the row note.
5. Build commands you will use again and again:
   - `cargo build -p tc4 --features test-hooks -j 6` (the `tc4` the browser tests run;
     `target/debug/tc4` with `--test-hooks` support; A4a's smoke runs on it too);
   - `crates/tc-ui/build-app.sh --out target/tc-ui-app` (plain UI, A4a's smoke) and
     `crates/tc-ui/build-app.sh --test-hooks --out target/tc-ui-app-hooks` (the probe UI, step 9);
   - `CARGO_JOBS=6` for build-app.sh (it reads `CARGO_JOBS`, default 2).

## 1. What A4b is, and what it is not

A4b makes typed text survive everything a browser runtime can do to it, before any real save
exists: a per-field **save machine** (pure, host-tested), the **draft buffer** in `localStorage`
with the shell's mirror bridge, the **editor refresh rule** of AD37, a **panic hook** and
`boot.js` **recovery**, the **navigation gate** and the **close guard**. It proves them against
A3's `test/fieldStore` (in process, with real reloads and real crash points) and in Chromium
through a test-only **probe page**.

Not A4b (do not build): the view of any tC4 screen (A5 ports `App.jsx`, TopBar, the X2 save
indicator; B4a the verse editor, with §6.3's Arabic IME Playwright case; B4b the section editor); `session/open { buffered }` and the draft
offer (B1b, C1b); real save endpoints and `editor/draft` (C2, C3); the aligner and `align/flush`
(B6, C5: A4b gives the gate a hook they register); the navigation snapshot that restores a view
after a reload (A5; A4b only names its `sessionStorage` key); the shell's main script, its log,
its single-instance lock and its close dialog (G1: A4b defines the page's side of the bridge).

### Parity with tC4

The screens the row touches are tC4's **view-error callout** (`tC4:src/App.jsx:163-190`): the
in-app one after one panic and the static one `boot.js` draws after three must both have its DOM
(`data-testid="view-error"`, `role="alert"`, tone warn, `<strong>app.viewError</strong>`, the
message, the Retry-style outline button `data-testid="view-error-home"` with `app.backToHome`).
Behaviour ported from tC4: the save scheduler (`saveScheduler.ts`), the drain loop of every
scheduler (`drainSchedulers`, `state.jsx:1693-1707`), navigation holds on a standing failure
(FR-32, `:4018-4025`, `:4377`), the close guard (`:3680-3721`: `setCanClose(allSaved)` under the
shell, `beforeunload` only in a browser), the indicator's ranking (`App.jsx:57-62`). Declared
differences are listed in D13.

---

## 2. Decisions this plan makes (and the design text you change for each)

The design is normative. Each item below refines or corrects it; change the named design text in
the same commit as the code and say why in the commit message (AGENTS.md; the task). At the end,
add the row note of step 11.

**D1. The save machine is `tc_ui::app::save`, not `tc_ui::logic::save_machine`.** A4a put every
pure runtime module under `crate::app` (boot, cache, client, effects, events, load), host-tested
in `tests/app/`. §6.5's name was a proposal (§0 "Names"). Change §6.5's first sentence.

**D2. `FieldSave` gets two refinements.** `Failed(SaveError)` instead of `Failed(NotSaved)`: a
write can also end in `ApiError::Internal`, `Unavailable` (every real save before S1),
`BadRequest`, a 401/403/413 status, or an `Ack` the machine cannot read, and each must show
"Not saved … Retry", never vanish. And a `Held(Hold)` state: the field keeps its text and cannot
save now (no current open after `StaleScope`, or a read-only mode), which is not the same as
"waiting for its timer" (AGENTS.md: enums for states). `Saving` holds the whole `SentSave`. §6.5
code block.

**D3. One `localStorage` item per field, keyed by JSON, not one map.** §6.6 says
`localStorage["tc4.drafts.v2"]` maps `"<projectRef>|<projectId>|<fieldKey>"` to entries. Two
problems: (a) a folder name may hold `|` (`ProjectRef` refuses only `/ \ NUL :` and
all-dots-and-spaces, §3.2), so the key is ambiguous; (b) one map means every keystroke parses and
rewrites every buffered field's text (a section buffer included), which breaks PERF rule 1 for
the 16 ms typing budget. So each field is its own item, `"tc4.drafts.v2:" + <BufferKey as
canonical JSON>`, and the value is one `BufferEntry`. A reader skips, and never deletes, an item it
cannot parse (a newer build's, or a damaged one). Change §6.6 first bullet and §3.10's "browser
buffer key".

**D4. The entry format, in tc-api, is shared with the shell and the server.** `tc_api::buffer`
(new) holds `BufferKey`, `FieldKey`, `FieldKind`, `FieldPlace`, `BufferEntry`, `SentSave`,
`DraftCall` and `BufferLine` (the mirror's line). Beyond §6.6's `{api, base, text, sent, request,
at}`:
- `sent: Option<SentSave { request, text, scope }>` replaces the loose `sent` and `request`
  (no "request without its body" state), and keeps the save's `Scope`, so a retry rebuilds the
  **same body** (the reply cache compares body digests; a retry with another epoch would be
  `RequestReused`).
- `instance: ServerInstance`: the run the entry was written under. **An entry restores into the
  machine (and a `sent` is retried) only under the same instance.** The OS can give a restarted
  `tc4` the old port, which makes the old origin and its `localStorage` reachable again; resending
  a `sent` there would store it a second time over its old token (a self-conflict, A3 D7). Every
  other entry waits for the offer at the next open (AD4).
- `draft: Option<DraftCall { path, body }>`: how `boot.js` posts this entry as a `Draft` with a
  plain `fetch` after three panics (§6.8). `body` is the JSON text of the draft request without
  `request` and `text`; `boot.js` adds a fresh `request` and the entry's `text`. Contract for every
  later draft endpoint: its request has top-level `request: RequestId` and `text: String`.
- `FieldKey { kind: FieldKind, place: FieldPlace }`. `FieldKind` is the closed list of §6.5's
  timing table: `Verse, Layout, Section, Comment, CheckComment, Frame, StoryTitle, StoryRef`, plus
  `Test` (the probe's `test/fieldStore` fields). `FieldPlace` is the kind's own locator text (each
  later unit defines its form, e.g. `TIT 1:4`): non-empty, at most 256 bytes, no control
  character, validated in `Deserialize`.
- The mirror's line: `BufferLine::Changed { key, entry }` or `BufferLine::Removed { key }`,
  adjacently tagged like every tc-api enum, written by `serde_json` (or byte-for-byte the same by
  `boot.js`), one per line; JSON escapes newlines, so a line never holds one.
- The persisted entry **accepts unknown keys** (§3.10: "Unknown keys are kept on rewrite"): no
  `deny_unknown_fields` on it, and tc-ui's rewrite keeps the keys it does not know. (Trap for C1b:
  a request must refuse unknown keys at any depth, so `session/open { buffered }` must carry the
  entries as raw lines, not as `BufferEntry` values.)
Change §6.6 (the entry, the mirror's line format) and §3.10.

**D5. The shell bridge's buffer calls, made exact.** §5.7 lists `bufferChanged(entry)` and
`takeBuffered()`. A4b defines: `bufferChanged(line: string)` takes one `BufferLine` JSON line and
returns nothing (fire and forget; the shell appends it). `takeBuffered(): Promise<string[]>`
returns the shell's current lines, the last line per key of the log it read at launch and of every
line appended since, with `Removed` keys dropped, **on every call** (not once: a reload in the
same launch must not lose them, and a key removed meanwhile must not come back). The page merges
them with its own `localStorage` entries by newest `at` (ties: `localStorage`). And
`onCloseRequested(fn)`: `fn` returns `Promise<boolean>`; the page runs the navigation gate and
answers whether everything is saved; the shell closes at once on `true`, and asks its "Unsaved
changes — close anyway?" question on `false` (or if `fn` has not answered in 5 s). This replaces
tC4's best-effort `pagehide` drain (`state.jsx:3699-3704`) with a drain that finishes before the
window goes: the translator's last sentence is saved, not offered at the next launch (AGENTS.md
"Matching tC4": declared difference D13a). Change §5.7.

**D6. `test/fieldDraft`, a test hook for drafts.** The row's third Playwright case must show that
"the server holds the drafts" (§6.8) after `boot.js` posts them, and no draft endpoint exists in
phase 0 (`editor/draft` is C2's). Add to `test_hook_table!`: `command "test/fieldDraft":
tc_api::test::FieldDraft => ();` with `FieldDraft { request, field, shown: ShownToken, text }`.
tc-app keeps the newest draft per field in `fields.json` (`drafts` map, `#[serde(default)]`, schema
stays 1) and counts them; it verifies the token like `fieldStore` (`BadToken` for a forged one or
another field's), reaches `HoldPoint::DraftWrite`, and never changes the field's value or stores.
`FieldLogView` gains `draft: Option<StoredDraft { request, text, base: TokenBase }>` and
`drafts: u32`. `CrashPoint::DuringDraftWrite` stays refused (§5.8 says the unit that adds real
draft writes accepts it). Change §3.9's `test` row and §5.8's table.

**D7. AD37's "since then" is "since the editor last took its pair".** Read literally ("no input
since it opened"), a field that was typed into, saved and is now clean would show "Juan changed
this verse while you were typing. Your text will be kept as a second version." under a teammate's
later change, although nothing unsaved remains and no second version will be kept. The pair is
taken when the editor opens **and when an `Ack` leaves the field clean** (the field then holds the
stored value and its token). So: a refresh replaces the pair while the field has had no input or
composition since the pair was last taken (waiting for blur when focused); once it has, the field
keeps its text and base and shows the notice when the refreshed value differs in NFC. Change
§6.3's first sub-bullet and AD37 by one clause each.

**D8. Retries: same id only for a lost reply.** The reply cache keeps every answer of a write,
refusals included (`crates/tc-app/src/dispatch.rs`, module comment; `replies.rs`). So:
- a lost reply (no answer: `Transport::Unreachable`, status 503, a reload) is retried with the
  **same** `RequestId` and the body rebuilt from `sent` (A-2), after 250 ms doubling to 5 s while
  the page lives, and **only after `app/hello` answers with the instance the field was sent
  under**: a run that restarted has an empty reply cache, so the resend would store the text a
  second time over its old token. A restarted `tc4` also has a new token, so a page that still
  holds the old one gets 401 (a failure, never a resend); the `hello` check covers what the token
  does not (a test proxy that rewrites it, P9), and costs a round trip only on this rare path;
- **Retry** after a failure, the gate's one retry, the resend after `NotSaved::StaleScope`
  (A-19) and the save after `RequestReused` all mint a **new** id (the old one would get its cached
  refusal back for ever).
Say this in §6.5's retry bullet.

**D9. The navigation gate, exactly.** `gate()` is tC4's `drainSchedulers` loop: send every field
that may save now, retry each standing failure **once** (tC4's `drain()` retries,
`saveScheduler.ts:139-146`), wait for every reply, run the registered hooks (B6 registers
`align/flush`), and repeat while new work appeared; `false` as soon as a failure stands after its
retry. A field whose rule is Save-only (the section editor, RT-1) is **never** saved by the gate
and does not block it (tC4's editing lock keeps the exits off while a card is open, DELTA:909-927;
a section buffer restored after a panic stays a draft and is offered at the next open). A `Held`
field does not block it either (the person cannot save it; its entry stays for the copy or the
next open). The gate has no timeout: while a lost reply is retried, the indicator says "Saving…".
Change §6.5 "The navigation gate".

**D10. Panic recovery details `boot.js` needs.** §6.8 and AD30, refined:
- `boot.js` makes a page id (16 random bytes, hex) and exposes `window.tc4Boot = { page, track(el,
  itemKey), untrack(el), measure }` **before** it imports the wasm glue (a dynamic `import()`, so
  its listeners exist even if the glue fails to load). The panic hook writes the page id into the
  crash marker, so `boot.js` can tell this page's panic from the marker a previous load left.
- A trap is a `window` `error` whose `error` is a `WebAssembly.RuntimeError`, **or** an
  `unhandledrejection` whose reason is one (wasm-bindgen-futures runs tasks from promise
  callbacks), **or** one thrown synchronously out of `run_app` into `boot.js`'s `try`. A trap with
  no marker for this page (an out-of-memory abort skips the panic hook) still counts.
- When `sessionStorage` cannot hold the panic count, `boot.js` never reloads (a reload loop would
  follow): it draws the static callout at once (as A4a stops on a blocked marker, §3.10).
- The capture listener uses a `WeakMap` from element to item key (`track`), not a DOM attribute:
  an extra attribute on tC4's textareas would show in the DOM parity layer (A6b).
- The static callout's words come from `sessionStorage["tc4.viewErrorText.v1"] = {lang, title,
  home}`, which the wasm writes at boot and on every change of the shown language; English when it
  is missing. Every text goes in by `textContent`, never `innerHTML` (a panic message can hold a
  project name or user text).
- The hook cuts the message to 8 KiB at a character boundary before it sends `app/clientError`
  with `keepalive`: a keepalive body is capped at 64 KiB, and A3's server cuts at 8 KiB anyway.
- "Back to Home" in the static callout removes `tc4.crash.v1`, `tc4.panics.v1` and the navigation
  snapshot `tc4.nav.v1` (A5 writes it) and reloads.
Change §6.8 bullets 2–5 and AD30's "Why".

**D11. `window.__tcIdle` counts saves.** A field that is `Saving`, or `Dirty` with an armed save
timer, keeps the page busy, so a test or capture that waits for idle sees the save. A Save-only
field does not (it would keep the page busy for ever). `BusyCount` gains `saves`. §6.2 last bullet.

**D12. NFC is `unicode-normalization` 0.1.25 on both sides.** The clean rule compares the
server's NFC (tc-app and tc-journal use this crate) with the UI's. JavaScript's
`String.prototype.normalize` follows Chromium's ICU Unicode version, which can differ for newly
encoded characters, and a mismatch keeps a field dirty for ever: it saves every 2 s, the server
answers `Unchanged`, the texts still differ. tc-ui depends on the workspace crate, and a test pins
one version in `Cargo.lock`. Measure the release wasm growth (§5).

**D13. Declared differences from tC4** (list them in the row note and in `docs/evidence/…`):
- a. Close in the shell drains first (D5) where tC4 drains best effort on `pagehide`.
- b. A failure outside `NotSaved` (D2) shows Retry; tC4 had no such errors (in-process store).
- c. Recovery after a WebAssembly panic: one reload, then the static callout. tC4 cannot panic
  this way; its boundary (`ViewBoundary`) only catches render errors.
- d. A dirty field restored after a reload saves on its timer with no editor (AD22); tC4 has no
  reload survival at all.

---

## 3. Steps, in order

Each step ends with `cargo fmt --all`, the step's tests, and a commit. Steps 1–6 are host-only
and need no browser. Keep each new file under about 800 lines; split by concern as named.

### Step 1. tc-api: the buffer types and `test/fieldDraft`

Files: `crates/tc-api/src/buffer.rs` (new), `src/lib.rs` (`pub mod buffer;`, the table row),
`src/test.rs`, `tests/golden.rs`, `tests/roundtrip.rs`, golden files, `src/ids.rs` only if you put
`FieldPlace`'s `Arbitrary` beside the others (prefer `buffer.rs`'s own `#[cfg(feature =
"proptest")] mod arbitrary`).

```rust
// crates/tc-api/src/buffer.rs
//! The draft buffer's persisted format (APP-DESIGN §6.6, D3–D5 of the A4b plan): one browser
//! storage item per field and one line of the shell's `drafts-buffer.log` per change.

/// The `localStorage` item of a field is this prefix followed by its `BufferKey` as canonical JSON.
pub const BUFFER_ITEM_PREFIX: &str = "tc4.drafts.v2:";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BufferKey { pub project: ProjectRef, pub id: ProjectId, pub field: FieldKey }

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldKey { pub kind: FieldKind, pub place: FieldPlace }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind { Verse, Layout, Section, Comment, CheckComment, Frame, StoryTitle, StoryRef, Test }

/// Validated: 1..=256 bytes, no control character. `#[serde(try_from = "String", into = "String")]`.
pub struct FieldPlace(String);   // parse(&str) -> Result<Self, IdError>; as_str()

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]           // NOT deny_unknown_fields: §3.10
pub struct BufferEntry {
    pub api: u32,                 // API_VERSION when written
    pub instance: ServerInstance, // the run it was written under (D4)
    pub base: Vec<ShownToken>,    // the tokens the next save of the field names
    pub text: String,             // the field's text now
    pub sent: Option<SentSave>,   // the save in flight, if any (A-2)
    pub at: WallMs,               // the last change, Date.now()
    pub draft: Option<DraftCall>, // how boot.js posts it as a Draft (§6.8)
}
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SentSave { pub request: RequestId, pub text: String, pub scope: Option<Scope> }
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftCall { pub path: String, pub body: String }   // body: JSON object text w/o request, text

#[serde(tag = "type", content = "value", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum BufferLine { Changed { key: BufferKey, entry: BufferEntry }, Removed { key: BufferKey } }
```
All with `#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]` (FieldPlace by
hand: regex `[^\x00-\x1f\x7f]{1,40}` filtered through `parse`). `BufferEntry` and `BufferLine`
must stay serde_json-free in tc-api (tc-api has serde_json only under `test-hooks`): `DraftCall.body`
is a `String` for that reason.

`src/test.rs`: `FieldDraft { request: RequestId, field: String, shown: ShownToken, text: String }`
(`deny_unknown_fields`), `StoredDraft { request, text, base: TokenBase }`, and on `FieldLogView`
the fields `draft: Option<StoredDraft>` and `drafts: u32`. Row in `test_hook_table!` after
`fieldLog`: `command "test/fieldDraft": $crate::test::FieldDraft => ();`.

Tests:
- `tests/golden.rs`: `Example` impls for `FieldDraft` and the new `FieldLogView` fields; new
  `types/buffer.json` in `shared_types_have_golden_json` (a key of each of two kinds, an entry with
  and without `sent` and `draft`, a `Changed` and a `Removed` line) and in the expected set of
  `no_golden_file_is_left_without_its_endpoint_or_type`. Run with `UPDATE_GOLDEN=1` once, **read
  the diff** (`endpoints/test/fieldDraft.json` new, `fieldLog.json` changed, `types/buffer.json`
  new), then without.
- `tests/roundtrip.rs`: `BufferKey`, `BufferEntry`, `BufferLine` in `every_shared_type_round_trips`.
- `buffer_entry_accepts_keys_it_does_not_know` (a JSON entry with an extra `"later": 1` parses);
  `field_place_refuses_empty_long_and_control_text`; `buffer_key_json_is_canonical` (serializing a
  parsed key gives the same bytes; the item key of a folder named `a|b:c "d"` round-trips).
- `every_request_refuses_a_field_it_does_not_know` covers `FieldDraft` automatically.

Design text: §3.9 `test` row gains `fieldDraft`; §5.8 table gains `test/fieldDraft` (D6).

### Step 2. tc-app: `Handle<FieldDraft>`

File: `crates/tc-app/src/hooks.rs`. In `FieldsFile` add `#[serde(default)] drafts:
BTreeMap<String, StoredDraft>` and `#[serde(default)] draft_count: BTreeMap<String, u32>` (or one
map of a small struct). `TestFields::draft(&self, app, hooks, req) -> Result<(), ApiError>`: same
name checks as `store`; `read_token` (BadToken when it fails or names another field or a version
above the current one); `hooks.holds.reach(HoldPoint::DraftWrite).await` **before** the lane;
take the lane; dead check;
record `StoredDraft { request, text: nfc(text), base: Current|Stale }`, bump the count; write
`fields.json` atomically on `spawn_blocking` (as `store` does); `Disk` on failure. `log` fills
`draft` and `drafts`. `impl Handle<FieldDraft> for App` like `FieldStore`'s. Update the module
comment's bullet list.

Tests in `crates/tc-app/tests/app/field_store.rs` (the `multi!` style):
- `a_draft_is_kept_and_changes_no_value` (value, token and stores unchanged; `draft.text` set;
  `drafts == 1`; a second draft replaces it, `drafts == 2`);
- `a_draft_with_a_forged_or_foreign_token_is_bad_token`;
- `a_draft_survives_a_restart` (restart on the same folder; `fieldLog` shows it);
- `a_held_draft_does_not_hold_a_store`: the draft reaches `HoldPoint::DraftWrite` **before** it
  takes the lane (so a held draft never delays a save, as R22 asks of the real store), and takes the
  lane only for its file write (drafts and stores rewrite one `fields.json`, so the write itself
  must be serialized or one would overwrite the other). Hold `DraftWrite`, spawn a draft, wait
  until it is held, store a text into the same field (it answers), release, and read both in
  `fieldLog`.

### Step 3. tc-ui: the buffer codec (pure)

Files: `crates/tc-ui/Cargo.toml` (add `unicode-normalization = { workspace = true }` to
`[dependencies]`), `crates/tc-ui/src/app/mod.rs` (`pub mod buffer;`),
`crates/tc-ui/src/app/buffer.rs`, `crates/tc-ui/tests/app/buffer.rs`, `tests/app.rs` (mod).

```rust
// crates/tc-ui/src/app/buffer.rs
pub fn item_key(key: &BufferKey) -> String;             // BUFFER_ITEM_PREFIX + serde_json::to_string(key)
/// `None` for an item that is not a buffer item, does not parse, or is not in canonical form
/// (re-serializing must give the same text): such an item is left exactly as it is.
pub fn parse_item_key(item: &str) -> Option<BufferKey>;

/// The browser's storage, or a map in tests. Every call may fail: storage can be blocked or full.
pub trait Store {
    fn keys(&self) -> Vec<String>;
    fn get(&self, item: &str) -> Option<String>;
    fn set(&mut self, item: &str, value: &str) -> Result<(), StoreError>;
    fn remove(&mut self, item: &str) -> Result<(), StoreError>;
}
pub struct MemStore { /* BTreeMap; a `fail_writes: bool` switch for tests */ }

/// Writes `entry` over the item, keeping every key of the stored JSON object this build does
/// not know (§3.10). Returns the JSON text written, for the mirror's line.
pub fn write(store: &mut dyn Store, key: &BufferKey, entry: &BufferEntry) -> Result<String, StoreError>;
/// Removes the item only if its text, in NFC, is `if_text` in NFC (or unconditionally for
/// `None`). Another tab may have written newer text for the same field: that entry stays.
pub fn remove(store: &mut dyn Store, key: &BufferKey, if_text: Option<&str>) -> Result<Removal, StoreError>;
pub enum Removal { Removed, Kept, Absent }
/// Every entry this build can read, by key; items it cannot read are skipped and untouched.
pub fn entries(store: &dyn Store) -> Vec<(BufferKey, BufferEntry)>;
pub fn entries_of(store: &dyn Store, project: &ProjectRef, id: &ProjectId) -> Vec<(BufferKey, BufferEntry)>;
/// The mirror's lines.
pub fn changed_line(key: &BufferKey, entry_json: &str) -> String;   // key bytes = item key's JSON
pub fn removed_line(key: &BufferKey) -> String;
/// What `takeBuffered` gave, merged with this page's entries: newest `at` per key, ties to the
/// page's. Lines that do not parse, and `Removed` lines, add nothing.
pub fn merge(local: Vec<(BufferKey, BufferEntry)>, mirror: &[String]) -> Vec<(BufferKey, BufferEntry)>;
/// The copy of a read-only project takes the folder's entries (RT-10, §6.4): every entry of
/// `(from_project, from_id)` is written under `(to_project, to_id)` and then removed from the old
/// key (write first, so a failure leaves both, never neither).
pub fn rekey(store: &mut dyn Store, from: (&ProjectRef, &ProjectId), to: (&ProjectRef, &ProjectId)) -> Result<usize, StoreError>;
pub fn nfc(text: &str) -> String;                                  // the one NFC of tc-ui (D12)
```

Tests (`tests/app/buffer.rs`):
- `item_keys_are_unambiguous_for_any_folder_name` (proptest over `BufferKey`: `parse_item_key(&item_key(k)) == Some(k)`);
- `a_non_canonical_or_foreign_item_is_never_read_or_removed` (`tc4.drafts.v2:{ "project" … }` with spaces, a different prefix, garbage: `entries` skips them; after `write`/`remove`/`rekey` of other keys they are byte-identical);
- `rewriting_an_entry_keeps_keys_it_does_not_know`;
- `an_entry_is_removed_only_when_its_text_is_the_text_just_saved` (two-tab rule; NFC-equal counts as equal);
- `entries_of_a_folder_need_its_project_id_too` (A-9: same folder, another id: not listed);
- `merge_keeps_the_newest_entry_per_field_and_skips_torn_lines` (a cut-off last line, a `Removed` line);
- `rekey_moves_every_entry_of_the_folder_and_nothing_else`; `rekey_that_cannot_write_removes_nothing`;
- `the_lines_have_the_golden_form` (build `changed_line`/`removed_line` for the examples of
  `crates/tc-api/tests/golden/types/buffer.json` and compare as JSON values);
- `one_nfc_for_the_ui_and_the_server` (read `../../Cargo.lock`, assert exactly one
  `name = "unicode-normalization"` package; and that `nfc("e\u{301}") == "\u{e9}"`).

### Step 4. tc-ui: the save machine (pure)

Files: `crates/tc-ui/src/app/save/{mod.rs, rules.rs, classify.rs, indicator.rs}` (or `save.rs` plus
siblings; keep `pub mod save;` in `app/mod.rs`), `crates/tc-ui/tests/app/save.rs` (split into
`save.rs`, `save_refresh.rs`, `save_props.rs` if long).

```rust
pub type Ms = f64;                                   // performance.now()

// rules.rs: tC4's timing per field (§6.5 Timing, AD20), one total match.
pub struct FieldRules { pub save: SaveRule, pub draft: DraftRule }
pub enum SaveRule {
    BlurOrIdle { idle_ms: u32 },   // verse, layout, OBS frame, title, reference, test: blur or 2 s
    OnBlur,                        // user comment (A-18): never on a pause
    OnLeave,                       // check comment (A-10): Done, and leaving the item (the gate)
    OnSaveOnly,                    // section editor (A-1, RT-1): only the Save button
}
pub enum DraftRule { Never, Throttled { gap_ms: u32 }, OnBlurAndIdle { idle_ms: u32 } }
pub fn rules(kind: FieldKind) -> FieldRules;
//  Verse | Layout | Frame | StoryTitle | StoryRef | Test => BlurOrIdle{2000}, Throttled{1000}
//  Section       => OnSaveOnly, OnBlurAndIdle{2000}
//  Comment       => OnBlur, Throttled{1000}     (STORAGE now has DraftField::UserComment, R15)
//  CheckComment  => OnLeave, Throttled{1000}

/// The value an editor shows and the token it stands for, taken together (AD37).
pub struct Pair { pub value: String, pub token: ShownToken, pub view: Option<ViewSeq> }

pub enum FieldSave {
    Clean,
    Dirty { since: Ms },
    Saving { sent: SentSave },                // one request in flight, or a lost reply being retried
    Failed(SaveError),                        // nothing saves until Retry or the gate's one retry
    Held(Hold),                               // cannot save now; the text stays
    SavedWithWarning(String),                 // clean; the disk reported an error after the rename
}
pub enum SaveError { NotSaved(NotSaved), Api(ApiError), Transport(Transport), BadAck(&'static str) }
pub enum Hold { NoOpen, ReadOnly(ReadOnlyReason) }
pub struct Notice { pub by: Option<PersonLabel> }   // "<by> changed this verse while you were typing…"
pub enum Source { Input, Program }                  // Input: boot.js already wrote the entry, if one existed
pub enum FieldScope { App, Project(Option<Scope>) } // App: test fields; None: no current open

// classify.rs: one exhaustive match, no wildcard on ApiError or Transport.
pub enum Answered { Acked(Ack), Lost, Reattach, Reissue, Failed(SaveError) }
pub fn classify(reply: Result<Ack, CallError>) -> Answered;
//  Ok(ack)                                   -> Acked
//  Api(NotSaved(StaleScope)) | Api(StaleScope{..}) -> Reattach    (A-19; a write should get the first)
//  Api(RequestReused)                        -> Reissue          (a bug; new id, current text)
//  Api(Version{..})                          -> Lost             (the page reloads, A4a; same id after)
//  Api(any other) / Api(NotSaved(other))     -> Failed
//  Dropped                                   -> Lost             (never for a write; be safe)
//  Transport(Unreachable) | Transport(Status(503)) -> Lost
//  Transport(Status(_)) | Transport(Unreadable(_)) -> Failed

pub enum Command {
    Write { key: BufferKey, entry: BufferEntry, mirror: bool },
    Remove { key: BufferKey, if_text: Option<String>, mirror: bool },
    Send { key: BufferKey, sent: SentSave, base: Vec<ShownToken> },
    Resend { key: BufferKey, sent: SentSave, base: Vec<ShownToken> },  // a lost reply; same id (rule 6)
    Draft { key: BufferKey, text: String, base: Vec<ShownToken> },
    DiscardDraft { key: BufferKey },
}

pub struct SaveMachine { /* fields: BTreeMap<BufferKey, Field>, instance, ids, clock-free */ }
impl SaveMachine {
    pub fn new(instance: ServerInstance, ids: Box<dyn FnMut() -> RequestId>) -> Self;
    /// An editor mounts. A field the machine already holds with unsaved text keeps its text and
    /// base (the pair is a refresh, D7); a clean or new one takes the pair. Returns the text to show.
    pub fn open(&mut self, key: BufferKey, scope: FieldScope, pair: Pair, now: Ms) -> (String, Vec<Command>);
    /// After a reload in the same run (AD22): the entry becomes a field with no editor. `sent`
    /// is retried at once with the same id and body; other unsaved text waits for its rule.
    pub fn restore(&mut self, key: BufferKey, entry: BufferEntry, scope: FieldScope, now: Ms) -> Vec<Command>;
    pub fn typed(&mut self, key: &BufferKey, text: &str, source: Source, now: Ms) -> Vec<Command>;
    pub fn composition(&mut self, key: &BufferKey);          // compositionstart counts as input (AD37)
    pub fn focus(&mut self, key: &BufferKey, now: Ms) -> Vec<Command>;
    pub fn blur(&mut self, key: &BufferKey, now: Ms) -> Vec<Command>;
    pub fn save(&mut self, key: &BufferKey, now: Ms) -> Vec<Command>;      // Save button, Done
    pub fn discard(&mut self, key: &BufferKey) -> Vec<Command>;            // Cancel (section)
    pub fn close(&mut self, key: &BufferKey, now: Ms) -> Vec<Command>;     // editor unmounts
    pub fn tick(&mut self, now: Ms) -> Vec<Command>;
    pub fn reply(&mut self, key: &BufferKey, request: RequestId, answered: Answered, now: Ms) -> Vec<Command>;
    pub fn retry(&mut self, now: Ms) -> Vec<Command>;                      // the Retry button
    pub fn refresh(&mut self, key: &BufferKey, pair: Pair, by: Option<PersonLabel>) -> Vec<Command>; // AD37
    pub fn local_change(&mut self, request: RequestId, view: ViewSeq);     // Changed{LocalSave}
    pub fn sessions(&mut self, open: &[OpenSession], now: Ms) -> Vec<Command>; // reattach; read-only holds
    pub fn restarted(&mut self, instance: ServerInstance);                 // drop every field; no command
    pub fn begin_gate(&mut self);
    pub fn flush_for_gate(&mut self, now: Ms) -> Vec<Command>;
    pub fn gate(&self) -> GateState;                                       // Waiting | Again | Pass | Blocked
    // reads
    pub fn state(&self, key: &BufferKey) -> Option<&FieldSave>;
    pub fn text(&self, key: &BufferKey) -> Option<&str>;
    pub fn notice(&self, key: &BufferKey) -> Option<&Notice>;
    pub fn worst(&self) -> Worst;                       // Saved < Dirty < Saving < Error (tC4 rank)
    pub fn warnings(&self) -> Vec<String>;
    pub fn next_deadline(&self) -> Option<Ms>;
    pub fn busy(&self) -> usize;                        // D11
}

// indicator.rs
pub enum Worst { Saved, Dirty, Saving, Error }          // data_state(): "saved" | "dirty" | "saving" | "error"
pub struct Indicator { pub state: Worst, pub commit_error: bool }
/// The worst of the fields and the aligners (tC4 `App.jsx:57-62`): error, saving, dirty, saved.
/// "Checkpoint not saved" shows only when no save failed.
pub fn indicator(fields: Worst, aligns: &[AlignSave], checkpoint_failed: bool) -> Indicator;
/// The close guard's value: everything saved; a failed checkpoint does not count (tC4 `allSaved`).
pub fn all_clean(fields: Worst, aligns: &[AlignSave]) -> bool;
```

Per-field state (private): `rules`, `scope`, `current`, `persisted: Option<String>` (the text the
base token stands for, when known; a restored field does not know it), `base: Vec<ShownToken>`,
`pair_view`, `state`, `editor: Option<{focused, touched, pending: Option<(Pair, Option<PersonLabel>)>}>`,
`last_input`, `buffered` (an entry this machine wrote and has not removed), `mirrored`,
`draft: {due: Option<Ms>, last_text: Option<String>}`, `own: [RequestId; ≤4]` with `own_view`,
`notice`, `gate_retried`, `retry_at: Option<Ms>` (lost-reply backoff), `retry_gap: Ms`, `warning`.

Rules the code must keep (each has a test below):
1. **One save in flight per field.** A `Send` is emitted only from `Dirty`/`Failed`(on Retry)/
   `Held`(on reattach) and moves the field to `Saving`. The next save waits for the reply and uses
   its tokens.
2. **Write the entry before the send.** Every `Send` is preceded, in the same `Vec`, by a `Write`
   of the entry with `sent = Some(..)`, `instance`, `base`, current `text`, `draft`. If that write
   fails (the glue reports it with `buffer_failed(key)`), the glue removes the item (best effort)
   and sends anyway: a stale item **without** `sent` could later be saved again over the old token.
3. **Clean only on the stored text.** On `Acked`: exactly one `saved` and one `tokens` entry, else
   `Failed(BadAck)` (never a loop). `base = [ack token]`, `persisted = saved`, `pair_view = ack.view`,
   `own` gets the request. If `ack.saved[0].text == nfc(current)`: `Clean` (or `SavedWithWarning`
   when `disk_warning`), `Remove { if_text: Some(saved), mirror: mirrored }`, the editor's pair is
   taken again (D7: `touched = false`), and a focused editor keeps showing the person's text as
   typed (the NFC form only after blur, §6.3). Else `Dirty`, a `Write` of the entry (new base,
   `sent: None`), and if the idle deadline or a blur passed meanwhile, the next `Send` at once.
4. **Failures stop automatic saves** (FR-32, `saveScheduler.ts:310-314`): in `Failed`, typing keeps
   updating `current` and the entry, the timer sends nothing, drafts may continue.
5. **Retry**: every `Failed` field sends its current text with a **new** id (D8); a field already
   `Saving` is untouched, so ten presses send one request (tC4 #356).
6. **Lost reply**: `Saving` stays, and at `retry_at` (250 ms, doubling to 5 s) the machine emits
   `Command::Resend { key, sent, base }` with the same id and `sent`. The glue reads `app/hello`
   first (D8): the same instance → it sends; another instance → the runtime's restart
   (`restarted`, no resend); no answer → `reply(.., Answered::Lost, ..)` again (next backoff). A
   reply for a request the field is no longer waiting for is ignored. (Add `Resend` to `Command`;
   the in-process driver of step 6 sends it to the same live app, whose instance never changes.)
7. **Reattach** (`NotSaved::StaleScope`, A-19): if `sessions` lists an open of the same folder
   **and the same project id**, send the current text with the field's **base** token (never a
   token fetched later), the new scope and a new id; else `Held(NoOpen)` until `sessions` lists one.
8. **Reissue** (`RequestReused`): send the current text, new id, same base.
9. **Read-only** (`SessionMode::ReadOnly` for the field's open): `Held(ReadOnly(reason))`, no
   send, entry kept (RT-10); back to `Writable`: `Dirty` again.
10. **Restart** (`restarted`): every field is dropped, no command (entries stay for the offer,
    AD4); a reply that comes later finds no field and is ignored.
11. **Restore** (same instance only; the glue checks it): `sent` → `Saving` and an immediate
    `Send` with `sent`'s id, text, scope and the entry's base; else `Dirty` with
    `last_input = now` (a `BlurOrIdle` field saves 2 s later with no editor; `OnBlur`/`OnLeave`
    wait for an editor's blur or the gate; `OnSaveOnly` never sends).
12. **Save-only fields** (Section): no `Send` from typing, blur, ticks, restore or the gate; only
    `save()`. `discard()`: `Remove { if_text: None }` and `DiscardDraft`.
13. **Drafts**: `Throttled{gap}`: due `gap` after the field became dirty or after the last draft;
    sent only while `Dirty`/`Failed`/`Held` (never while `Saving`: a draft that overtakes its save
    would leave older text in `drafts.json`, trap 19), and only if `current` differs from the last
    draft's text. `OnBlurAndIdle{idle}`: on blur, and `idle` after the last input.
14. **Typing writes the entry only when `boot.js` could not**: `Source::Input` with
    `buffered == true` emits nothing for the buffer (boot.js's capture listener wrote it);
    `buffered == false` (removed after a clean `Ack` while still focused, trap 5) or
    `Source::Program` emits a `Write` with `mirror: true`. `focus` writes an entry with the current
    text if none (`mirror: false`), so the listener has one to update; `blur` of an untouched clean
    field removes it (`mirror: false`).
15. **Dirty means "differs from the stored text, in NFC"** when `persisted` is known (typing back
    to the stored text in a clean-able state → `Clean` and remove, as tC4); when unknown
    (restored), any text is dirty until an `Ack` settles it.
16. **AD37** (`refresh`): ignore a pair whose `view` is not newer than `max(pair_view, own_view)`.
    No input since the pair was taken: not focused → take it (`current = persisted = value`,
    `base = [token]`; the glue shows it); focused → keep it as `pending`, apply on blur; input
    before that blur drops it and, when its value differs in NFC from `persisted` (or the token
    differs from the base when `persisted` is unknown), sets the notice. Input since the pair was
    taken: keep `current` and `base`; set `notice { by }` when the refreshed value differs in NFC.
    `local_change(request, view)` for a request in `own` or in flight records `own_view` and never
    sets a notice.

Tests (host, `crates/tc-ui/tests/app/save*.rs`). Ports of tC4's scheduler tests
(`test/saveScheduler.test.ts`, one field instead of one book; name the tC4 case in a comment):
- `typing_resets_the_debounce_one_save_carries_the_latest_text` (type at 0 and 1500; nothing at
  3000; one `Send` at 3500 with the second text);
- `the_state_stays_saving_until_the_reply_never_optimistic` (60 s of ticks while `Saving`);
- `states_go_saved_dirty_saving_saved_in_order` (record `worst()` after each step);
- `a_failure_keeps_the_text_and_the_error`; `a_failing_store_is_not_hammered_until_retry`
  (input while `Failed`, 10 s of ticks: no `Send`);
- `retry_sends_everything_typed_since_the_failure_with_a_new_id`;
- `edits_during_a_save_keep_the_field_dirty_and_the_next_save_carries_them`;
- `two_fields_save_independently` (replaces "per-book writes");
- from `alignSaveScheduler.test.ts` #356: `ten_retry_presses_while_one_runs_send_one_request`;
- from `noteScheduler.test.ts` round 21: `retry_replays_the_newest_text_exactly_once`;
- from `drainSchedulers` round 23: `the_gate_saves_work_typed_while_it_waits`;
- `the_gate_resolves_false_while_a_failure_stands_after_one_retry` (tC4 `drain()`).

The rebuild's own rules:
- `clean_only_when_the_ack_text_equals_the_nfc_of_the_current_text` (sent `e\u{301}`, saved `é`:
  clean; typed `x` meanwhile: dirty) (A-2);
- `an_ack_for_older_text_leaves_newer_typing_dirty_and_saves_it_next` (RT-2 case 1);
- `a_lost_reply_is_retried_with_the_same_request_and_body` and `…_backs_off_to_five_seconds`;
- `a_reload_retries_the_buffered_sent_body_not_the_current_text` (restore with `sent = T1`,
  `text = T2`: `Send { request R1, text T1 }`; after its Ack, `Send` of T2 with a new id);
- `refusals_are_never_resent_under_their_id` (Retry, gate retry, reattach, reissue: new id);
- `request_reused_saves_anew_with_a_new_request`;
- `stale_scope_reattaches_with_the_base_token_and_a_new_id` and
  `stale_scope_with_no_open_holds_until_the_project_opens_again` and
  `a_session_of_another_project_id_never_takes_a_field` (A-9, A-19);
- `save_only_fields_save_only_on_save` (Section: typing, blur, 10 s, restore, the gate: no
  `Send`; drafts on blur and after 2 s; `save()` sends) (RT-1);
- `comment_fields_save_on_blur_never_on_a_pause` (A-18); `check_comment_saves_on_done_and_on_the_gate` (A-10);
- `the_gate_never_saves_a_section_buffer_and_is_not_blocked_by_it` and
  `a_held_field_does_not_block_the_gate` (D9);
- `discard_removes_the_entry_and_the_draft`;
- `drafts_go_at_most_once_a_second_never_while_saving_and_never_twice_the_same_text`;
  `ten_seconds_of_typing_send_no_save_and_at_most_ten_drafts` (RT-20);
- `read_only_holds_fields_and_keeps_their_entries` (RT-10);
- `a_restart_drops_every_field_keeps_every_entry_and_ignores_late_replies` (A-3);
- `the_entry_is_written_before_the_send` (order of commands) and
  `a_buffer_write_that_fails_still_saves_and_leaves_no_stale_sent` (rule 2);
- `a_focused_field_cleaned_by_its_ack_writes_a_new_entry_on_the_next_keystroke` (trap 5);
- `a_malformed_ack_is_a_failure_not_a_loop`;
- `classify_maps_every_call_error` (a table; plus a proptest over `ApiError` (tc-api `proptest`
  feature) that `classify` never maps a write's answer to `Acked` unless it is `Ok`);
- `indicator_shows_the_worst_state` (tC4's rank table over fields and `AlignSave`);
  `checkpoint_error_shows_only_when_no_save_failed`; `all_clean_counts_aligners_not_checkpoints`.

AD37 (the row's "AD37 refresh cases"; §6.3's names):
- `refresh_under_open_clean_editor_swaps_value_and_token_together` (the next `Send` names the new token);
- `refresh_waits_for_blur_while_the_editor_has_focus`;
- `typing_before_blur_drops_the_waiting_pair_and_shows_the_notice`;
- `refresh_under_typed_editor_keeps_text_and_token` (notice names `by`; the next `Send` names the old base);
- `own_save_event_before_ack_shows_no_notice`; `own_save_event_after_ack_shows_no_notice`;
- `a_refresh_that_differs_only_in_nfc_shows_nothing`;
- `a_refresh_older_than_the_ack_is_ignored` (ViewSeq);
- `a_clean_field_after_its_ack_takes_a_refresh_again` (D7).

Property tests (`save_props.rs`; 256 cases each; a `MemStore` and a scripted, adversarial "server"
that answers each `Send` with any of `Acked` (saved = sent text in NFC, a fresh token), `Lost`,
`Reattach`, `Reissue`, `Failed`, in any order, possibly late; steps: type a text from a list with
NFC variants, empty, Hebrew and Arabic with marks; focus; blur; tick by 0–3 s; deliver or lose
the oldest reply; Retry; refresh with a teammate pair; a session list change; reload (rebuild the
machine from the `MemStore` and `restore` every entry of the same instance); restart):
- **`no_current_is_dropped`** (the row's proptest): after every step, for every field the machine
  holds: if `nfc(current) != nfc(stored)` (the scripted server's value from this field's last
  `Acked`, or the pair) then the state is not `Clean`/`SavedWithWarning` **and** the store holds an
  entry for the key whose `text` is `current`; and after a reload the restored field's text is the
  text before the reload. When `Clean`, the last `Acked`'s text equals `nfc(current)`.
- `one_body_per_request_id`: every `RequestId` is sent with one `(text, base, scope)` only, and an
  id answered with anything but `Lost` is never sent again (so `RequestReused` cannot come from
  the machine).
- `never_two_saves_of_one_field_in_flight`.
- `no_send_after_a_restart` and `save_only_fields_send_only_after_save`.
- **`events_reply_order_is_safe`** (§5.4): random interleavings of a save's `Ack`, its own
  `Changed{LocalSave}` (`local_change`), the refresh that event causes (a pair with the stored text
  and the `Ack`'s view), and teammate refreshes with later views: no notice unless a teammate pair
  differs from the field's unsaved text; the field's base after settling is the newest token among
  its own `Ack` and the pairs it took; never a `Send` naming a token older than the field's own last
  `Ack` (a self-conflict).

### Step 5. tc-ui: recovery and confirm models (pure)

Files: `crates/tc-ui/src/app/recovery.rs`, `crates/tc-ui/src/app/confirm.rs`, tests
`tests/app/recovery.rs`, `tests/app/confirm.rs`.

```rust
// recovery.rs
pub const CRASH_KEY: &str = "tc4.crash.v1";          // sessionStorage: the crash marker
pub const PANICS_KEY: &str = "tc4.panics.v1";        // sessionStorage: page loads that ended in a panic (boot.js owns it)
pub const VIEW_ERROR_TEXT_KEY: &str = "tc4.viewErrorText.v1";
pub const NAV_KEY: &str = "tc4.nav.v1";              // A5's navigation snapshot; Back to Home removes it
pub const MAX_CLIENT_MESSAGE: usize = 8 * 1024;
#[derive(Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
pub struct CrashMarker { pub page: String, pub view: Option<String>, pub project: Option<ProjectRef>, pub message: String, pub at: WallMs }
pub fn cut_message(message: &str) -> (String, bool);                 // at a char boundary
pub fn follows_panic(marker: Option<&str>, page: &str) -> Option<CrashMarker>; // a marker of another page load
#[derive(Serialize)] pub struct ViewErrorText { pub lang: String, pub title: String, pub home: String }

// confirm.rs (RT-27, §4.8): one RequestId per dialog, the button off while in flight.
pub struct Confirm { request: RequestId, state: ConfirmState }
pub enum ConfirmState { Idle, InFlight, Answered }
impl Confirm {
    pub fn new(request: RequestId) -> Self;                       // when the dialog opens
    pub fn press(&mut self) -> Option<RequestId>;                 // Some only when Idle
    pub fn lost(&mut self) -> RequestId;                          // a lost reply: the same id again
    pub fn answered(&mut self);
    pub fn disabled(&self) -> bool;
}
```
Tests: `the_message_is_cut_at_8_kib_on_a_char_boundary` (Hebrew at the edge);
`a_marker_of_this_page_is_not_a_previous_panic`; `a_marker_that_does_not_parse_is_ignored`;
`a_second_press_while_in_flight_sends_nothing`; `a_lost_reply_resends_the_same_id`;
`a_new_dialog_has_a_new_id`.

### Step 6. tc-app: the save machine against the real `test/fieldStore`

Files: `crates/tc-app/Cargo.toml` (`[dev-dependencies] tc-ui = { path = "../tc-ui" }`; tc-ui's host
build is the pure modules only), `crates/tc-app/tests/app/save_machine.rs` (new; register it in
`tests/app.rs`). Reuse `common::{TestApp, field_log, field_token, …}` and the `multi!` macro.

A **driver** (test code) plays the page:
- `Page { machine: SaveMachine, store: tc_ui::app::buffer::MemStore, inflight: VecDeque<(BufferKey,
  RequestId, JoinHandle<Answer>)>, now: Ms, mirror: Rc<RefCell<Vec<String>>> }`.
- It executes `Command`s exactly as the web glue will: `Write`/`Remove` on the store (+ a line in
  `mirror` when `mirror`), `Send` → `tokio::spawn(app.call("test/fieldStore", body))` with
  `FieldStore { request: sent.request, field: key.field.place, shown: base[0], text: sent.text }`,
  `Draft` → `test/fieldDraft` (awaited), `DiscardDraft` → nothing.
- Steps: `Type(i)`, `Focus`, `Blur`, `Advance(ms)` (then `tick`), `Deliver` (await the oldest
  in-flight call, `classify` its reply, `machine.reply`), `Cut` (the oldest call's reply is lost:
  `Answered::Lost`; its server task runs on), `Reload` (drop the machine and every in-flight
  handle without aborting their server work, build a new machine on the same `MemStore` and
  `restore` every entry whose `instance` is the app's), `Hold`/`Release` (`test/hold Save`), and in
  the second property `Crash(point)` (arm `test/crash` for `test/fieldStore`).
- **A crash** shows as `Answer::Ended` on delivery, or as `Then::EndProcess` for `AfterReply`
  (the app stays alive in process: treat the answer as the process's last, deliver or drop it at
  random, then stop using that app). The driver then starts `TestApp::start_in(dir)` again (new
  instance), and the page comes back **at a new origin**: an empty `MemStore`, the `mirror` kept.
  It then plays the **offer** of C1b, as the design defines it (§6.6, STORAGE §5.14): for each
  mirrored entry of the field (last line per key, `Removed` dropped), if its text in NFC equals the
  stored value it is dropped; otherwise *Keep* stores it with the **entry's base token** and a new
  id (`KeepDraft` saves with the draft's tokens). Then the page opens the field from `fieldLog`
  and goes on.
- `TEXTS = ["", "a", "e\u{301}", "\u{e9}", "Paul, a servant", "בְּרֵאשִׁ֖ית", "كَتَبَ"]`.

Assertions, after a final settle (release holds, deliver everything, tick 3 s at a time until the
field is `Clean`, at most 20 rounds; a `Failed` field fails the test):
1. `fieldLog.current.value == nfc(last text typed)`;
2. every `RequestId` appears at most once in `stores`;
3. no store has `base == Stale` (a self-conflict, A3 D7);
4. the last store's text is the last text, and the store before it (if any) differs from it
   (stored **once**, not twice in a row);
5. after each `Deliver` that made the field clean: `nfc(machine text) == fieldLog.current.value`
   at that moment (no earlier text was marked clean in its place);
6. the mirror's lines all parse as `BufferLine`.

Tests:
- deterministic: `a_reload_while_the_save_is_held_stores_once`;
  `a_reload_after_newer_typing_stores_both_in_order` (RT-2 case 1: T1 then T2, both `Current`);
  `a_retry_of_a_save_still_running_waits_and_stores_once` (RT-2 case 2);
  `crash_before_store_then_restart_offers_the_text_and_keep_stores_it_once`;
  `crash_before_reply_then_restart_offers_nothing_and_resends_nothing`;
  `crash_after_reply_without_the_reply_resends_nothing`;
  `an_entry_of_another_run_is_never_resent` (keep the `MemStore` across a restart, as a reused
  port would: the new page restores nothing, `stores` stays at one);
  `drafts_reach_the_server_and_change_no_value`.
- properties (`proptest!`, a fresh tempdir per case, multi-thread runtime as in
  `field_store.rs`): **`reload_anywhere_stores_the_last_text_once`** (steps without `Crash`;
  48 cases, 1–24 steps) and **`crash_anywhere_stores_the_last_text_once`** (every step kind, the
  three crash points; 32 cases). If a case takes over ~1 s, lower the cases and say so.

### Step 7. `boot.js`, `recovery.js` and their node test

Files: `crates/tc-ui/app/recovery.js` (new, an ES module of pure functions over objects passed
in), `crates/tc-ui/app/boot.js` (rewrite), `crates/tc-ui/app/recovery.test.mjs` (new; node
built-ins only, `node --test`), `crates/tc-ui/build-app.sh` (copy `recovery.js`; a `--test-hooks`
flag that builds `--features test-hooks`), `.github/workflows/ci.yml` (step 11).

`recovery.js` exports (no globals touched at import):
```js
export const CRASH_KEY = 'tc4.crash.v1', PANICS_KEY = 'tc4.panics.v1', TEXT_KEY = 'tc4.viewErrorText.v1',
  NAV_KEY = 'tc4.nav.v1', TOKEN_KEY = 'tc4.token', BUFFER_PREFIX = 'tc4.drafts.v2:',
  PANIC_WINDOW_MS = 10000, PANICS_TO_STOP = 3;
export function isTrap(value)                      // value instanceof WebAssembly.RuntimeError (by name if realms differ)
export function panicDecision(keptText, now)       // -> { panics: number[], action: 'reload' | 'stop' } (drops > 10 s old)
export function inputToEntry(store, itemKey, text, now) // -> mirror line string | null; updates text and at only
export function changedLine(keyText, entryText)    // '{"type":"changed","value":{"key":'+keyText+',"entry":'+entryText+'}}'
export function draftRequests(store, project, freshId) // -> [{ path, body }] for entries of `project` with a `draft`
export function calloutText(sessionStore)          // -> { lang, title, home }, English fallback
export function buildStaticCallout(doc, text, message) // -> Element: the ViewError DOM, every text by textContent
export function randomId(crypto)                    // 32 hex
```
`boot.js` (keep A4a's header comment, updated):
1. `import * as R from './recovery.js'` (static: tiny, no side effects).
2. `const page = R.randomId(crypto)`; `const tracked = new WeakMap()`;
   `window.tc4Boot = { page, track: (el, key) => tracked.set(el, key), untrack: (el) => tracked.delete(el), measure: null }`.
3. Capture listener: `window.addEventListener('input', onInput, true)`: look up
   `tracked.get(event.target)`; if found, `R.inputToEntry(localStorage, key, target.value ??
   target.textContent, Date.now())` inside `try`; if it returns a line, `window.tc4Shell?.bufferChanged?.(line)`
   inside `try`. When `tc4Boot.measure` is an array, push `performance.now()` deltas (for §5).
4. Trap handlers: `window.addEventListener('error', e => e instanceof ErrorEvent && R.isTrap(e.error) && onTrap(e.error))`
   and `'unhandledrejection'` with `e.reason`. `onTrap` runs once per page (`handled` flag):
   read the marker for this page (message), read `PANICS_KEY`, `R.panicDecision`, try to write it
   back; if the write fails → stop; `reload` → `location.reload()`; `stop` → `showStatic()`.
5. `showStatic()`: `root.replaceChildren(R.buildStaticCallout(document, R.calloutText(sessionStorage), message))`;
   post `R.draftRequests(localStorage, marker?.project, …)` each with
   `fetch(path, { method: 'POST', headers: { 'X-Tc-Token': token, 'X-Tc-Api': '1', 'Content-Type': 'application/json' }, body })`
   (token from `sessionStorage[TOKEN_KEY]`, absent: post nothing); "Back to Home" clears
   `CRASH_KEY`, `PANICS_KEY`, `NAV_KEY` and reloads.
6. `const { default: init, run_app } = await import('./tc_ui.js')`; `await init()`; then
   `try { run_app(base, page); } catch (e) { if (R.isTrap(e)) onTrap(e); else throw e; }`. An
   `init()` or `import()` failure draws the static callout with its message (no reload).

The static callout's DOM must be **exactly** the in-app `ViewError`'s (step 8) with the message
replaced. Build it with `createElement`/`setAttribute`/`textContent` from the Leptos output you
read in the browser (render `ViewError` in the probe, copy `outerHTML`); P3 compares the two.

`recovery.test.mjs` cases: `three_page_loads_in_10_s_stop_the_reloads`;
`an_old_panic_does_not_count` (> 10 s); `a_blocked_store_stops_at_once`;
`input_updates_only_text_and_at_and_keeps_unknown_keys`; `input_without_an_entry_writes_nothing`;
`the_line_matches_tc_api_golden` (read `../../tc-api/tests/golden/types/buffer.json`, build the
line for its example key and entry text, compare as parsed JSON); `drafts_only_for_the_current_folder_and_with_a_template`;
`a_draft_request_adds_request_and_text_and_keeps_the_rest`; `the_callout_sets_every_text_by_textContent`
(a message `<img src=x onerror=alert(1)>` ends up as one text node; use a minimal fake `document`);
`english_when_no_text_was_kept`.

### Step 8. tc-ui web glue: hook, saves, view error, close guard

Files: `crates/tc-ui/src/app/web/{panic.rs, saves.rs, bridge.rs, view_error.rs}` (new),
`web/mod.rs`, `web/root.rs`, `web/js.rs`, `web/api.rs`, `src/app/effects.rs` (BusyCount),
`Cargo.toml` (web-sys features `Crypto`, `BeforeUnloadEvent`). wasm32 + `app` only.

- `panic.rs`:
  - `pub(super) fn install()`: `std::panic::set_hook` that (1) calls
    `console_error_panic_hook::hook(info)` (keep the console output), (2) writes
    `CrashMarker { page, view, project, message, at: Date.now() }` to `sessionStorage[CRASH_KEY]`,
    (3) sends `app/clientError { request: fresh id, view, project, message: cut_message(..).0, at }`
    with `fetch` and `RequestInit::set_keepalive(true)` and the token and API headers, ignoring every
    error. It **schedules nothing** and **never borrows a `RefCell` with `borrow()`**: context lives
    in `Cell`s or is read with `try_borrow` (a panic while the machine is borrowed would otherwise
    panic again in the hook and abort before the marker is written).
  - `pub fn set_context(view: Option<&str>, project: Option<ProjectRef>)` (A5 and the probe call it).
  - `pub(super) fn take_previous(page: &str) -> Option<CrashMarker>`: reads and removes the marker
    when it belongs to another page load (`follows_panic`).
  - `pub(super) fn keep_view_error_text()`: an `Effect` on `shown_locale()` that writes
    `ViewErrorText { lang, title: t(keys::app::VIEW_ERROR), home: t(keys::app::BACK_TO_HOME) }`.
- `bridge.rs`: the shell bridge, every call through `js_sys::Reflect` on `window.tc4Shell`, each
  returning `Result`/`Option` and never panicking: `buffer_changed(line: &str)`,
  `async take_buffered() -> Vec<String>`, `set_can_close(bool)`, `on_close_requested(f)`
  (a `Closure` returning a `Promise<boolean>` from `future_to_promise`, kept for the page's life),
  `has_bridge() -> bool`.
- `saves.rs`: `pub struct Saves(Rc<Inner>)` holding `RefCell<SaveMachine>` (never borrowed across
  an `.await` or a call out), `RefCell<HashMap<FieldKind, Rc<dyn FieldWriter>>>`, the one
  `setTimeout` handle to `next_deadline()`, the `Busy` handle, `ArcRwSignal<Indicator>`,
  `Cell<Option<bool>>` (last `setCanClose`), gate hooks, and waiters for "no field saving".
  ```rust
  // tc-ui has no `futures` dependency: LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T>>>.
  pub trait FieldWriter {
      fn save(&self, api: &Api, key: &BufferKey, sent: &SentSave, base: &[ShownToken])
          -> LocalBoxFuture<Result<Ack, CallError>>;
      fn draft(&self, api: &Api, key: &BufferKey, text: &str, base: &[ShownToken])
          -> Option<LocalBoxFuture<Result<(), CallError>>>;
      fn draft_call(&self, key: &BufferKey, base: &[ShownToken]) -> Option<DraftCall>;
  }
  impl Saves {
      pub fn register(&self, kind: FieldKind, writer: Rc<dyn FieldWriter>);
      pub fn open/typed/composition/focus/blur/save/discard/close/refresh(..);  // call the machine, run commands
      pub fn track(&self, element: &web_sys::HtmlElement, key: &BufferKey);   // tc4Boot.track(el, item_key)
      pub async fn gate(&self) -> bool;                                       // D9
      pub fn add_gate_hook(&self, hook: Rc<dyn Fn() -> LocalBoxFuture<'static, bool>>) -> GateHookId;
      pub fn indicator(&self) -> ArcRwSignal<Indicator>;
      pub fn retry(&self);
      pub async fn buffered_for(&self, project: &ProjectRef, id: &ProjectId) -> Vec<(BufferKey, BufferEntry)>;
      pub fn rekey(&self, from: (&ProjectRef, &ProjectId), to: (&ProjectRef, &ProjectId)) -> Result<usize, StoreError>;
      pub fn write_cap(&self, at: &Scope) -> Option<WriteCap>;               // §6.4: Some only while Writable
      pub(super) fn restore(&self, hello: &HelloView);
      pub(super) fn restarted(&self, instance: ServerInstance);
  }
  ```
  `restore` reads every buffer item and restores an entry only when its `instance` is
  `hello.instance` (D4) **and** it is a `Test` field (`FieldScope::App`) or `hello.sessions` lists
  an open of the same folder with the same project id (`FieldScope::Project(Some(at))`); every
  other entry stays untouched for the offer at its project's next open.
  `Resend` → `app/hello` first (rule 6), then the writer's `save` with the same `sent`.
  `run(commands)`: `Write` → `buffer::write(LocalStore, …)`, then `bridge::buffer_changed(line)`
  when `mirror`; on a failed write the field's `Send` still goes and the item is removed (rule 2);
  report a buffer failure once per page through `app/clientError`. `Remove` → `buffer::remove`
  and a `removed_line` when `mirror`. `Send` → `spawn_local` the writer's future, then
  `machine.reply(key, request, classify(result), now)`. `Draft` → spawn, ignore the answer (a
  draft is best effort). After every change: `Busy::set_saves(machine.busy())`, re-arm the timer,
  update the indicator signal with `try_set`, and `set_can_close(all_clean)` when it changed.
  Close guard: with the bridge, `setCanClose` on every change and `onCloseRequested` → `gate()`;
  without it, one `beforeunload` listener that calls `prevent_default()` and sets `returnValue`
  while not all clean (tC4 #356: never under the shell). `all_clean` reads the fields and
  `ServerState.align_saves`.
- `view_error.rs`: `#[component] pub fn ViewError(message: String, on_home: Callback<()>)`: the
  port of `tC4:src/App.jsx:177-190` with the DS `Callout` and `Button` as A4a's `BootError` does,
  `data-testid="view-error"` and `"view-error-home"`, texts `keys::app::VIEW_ERROR`,
  `keys::app::BACK_TO_HOME`.
- `mod.rs`: `run_app(base: String, page: String)` (boot.js passes the page id): `panic::install()`
  first (instead of `console_error_panic_hook::set_once()`); in `test-hooks` builds the boot-panic
  trigger (step 9); `let after = panic::take_previous(&page)`; the rest of `boot()` as now; after
  `Runtime::new` make `Saves`, `saves.restore(&hello)`, `panic::keep_view_error_text()`, register
  the close guard; mount the probe (step 9) or `StandIn` with `after` (a `ViewError` with the
  marker's message in place of the stand-in until "Back to Home", which runs `saves.gate()` and
  then shows the stand-in). `Runtime` gains `saves()`; `apply(Effect::Restarted(i))` calls
  `saves.restarted(i)`; `Runtime::listen` delivers `Changed { cause: LocalSave { request }, view }`
  to `saves.local_change`; an `Effect` on `state.sessions` calls `machine.sessions(..)` (reattach
  and read-only holds follow `hello` re-reads too).
- `effects.rs`: `BusyCount { fetches, jobs, saves }`, `is_idle` needs all three 0; update
  `tests/app/effects.rs`'s literals; `web/api.rs`'s `Busy` gains `set_saves(n)`.
- `js.rs`: `local_get/set/remove/keys` (each a `Result`), `random_request_id()`
  (`window.crypto().get_random_values_with_u8_array`), `now_ms()` (`performance.now()`),
  `wall_ms()` (`Date.now()`).
- Async completions write signals with `try_set`/`try_update` and read with `try_get_untracked`
  (A-15); no arena signal (`RwSignal` made in a component) is touched from a spawned future or a
  timer: async code reaches only `Arc*` signals and `Rc` state. `app/mod.rs`'s
  `#![deny(clippy::indexing_slicing)]` covers the new modules (keep them under `app`).

Run `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings`
with no feature, `--features app`, `--features gallery` (and `test-hooks` after step 9), and A4a's
smoke (`node crates/tc-ui/app/smoke.mjs …`): it must still pass (93 checks at A4a's last run).

### Step 9. The probe page (feature `test-hooks`)

Files: `crates/tc-ui/Cargo.toml` (`test-hooks = ["app", "tc-api/test-hooks"]`, with a comment:
never in a release build), `crates/tc-ui/src/app/web/probe.rs` (`#[cfg(feature = "test-hooks")]`),
`web/mod.rs` (mount `Probe` when built with `test-hooks` **and** the page's query has
`probe=fields`; otherwise the normal root), `build-app.sh` (`--test-hooks`).

- Constants: `PROBE_PROJECT = "test-fields"`, `PROBE_ID = "00000000-0000-0000-0000-000000000000"`
  (test fields have no project; the key needs one; test-only).
- A `TestWriter: FieldWriter` for `FieldKind::Test`: `save` → `test/fieldStore { request:
  sent.request, field: key.field.place, shown: base[0], text: sent.text }`; `draft` →
  `test/fieldDraft`; `draft_call` → `DraftCall { path: "test/fieldDraft", body: {"field":…,"shown":…} }`.
- The page: two textareas `data-testid="probe-field-a"`/`"probe-field-b"` (places `probe-a`,
  `probe-b`), each opened from `test/fieldLog`'s `current` pair; `on:focus` → `saves.focus` +
  `saves.track(el, key)`; `on:input` → (the panic trigger below) then `saves.typed(.., Source::Input, ..)`;
  `on:blur`; `on:compositionstart`. The value is set with `prop:value` only when the machine says
  the editor shows a new text, **never while focused** (AD37). `data-testid="probe-indicator"` with
  `data-state` = `Worst::data_state()`, `probe-retry` while `Error`, `probe-leave` (runs
  `saves.gate()`; on `true` shows `data-testid="probe-left"`), `probe-restarted` after a restart.
  `panic::set_context(Some("probe"), Some(PROBE_PROJECT))`. After a panic (`after` is `Some`) the
  page shows `ViewError` in place of the fields; the machine still saves the restored fields.
  Never render a token or an entry into the DOM.
- Test triggers, read only in `test-hooks` builds: `sessionStorage["tc4.test.panicOnInput"] == "1"`
  → the input handler removes the key and panics **before** calling the machine;
  `sessionStorage["tc4.test.panicAtBoot"] = n > 0` → `run_app` decrements it and panics at once;
  a 20 ms `setInterval` ticker closure that reads the machine (closures that trap after an abort).
  Mark each `panic!` with `#[expect(clippy::panic, reason = "a test-hooks panic the recovery tests need")]`.
- `window.__tc4Probe = { buffered: (project, id) => Promise<string /* JSON array of [key, entry] */>, state: (place) => string }`
  for the browser tests (test-hooks only).

### Step 10. The browser tests and the measurements

File: `crates/tc-ui/app/no-loss.mjs` (new; reuse A4a's patterns from `smoke.mjs`: `Server` with
`--test-hooks` added to the spawn args, `Proxy` with **a token rewrite** to the current server's
token and a settable `server`, `openContext` adding the token outside `/api/` only, `watch`,
`check`). Run:

```
. ~/.tc-env && cargo build -p tc4 --features test-hooks -j 6 \
  && CARGO_JOBS=6 crates/tc-ui/build-app.sh --out target/tc-ui-app \
  && CARGO_JOBS=6 crates/tc-ui/build-app.sh --test-hooks --out target/tc-ui-app-hooks \
  && PLAYWRIGHT_MODULE=/home/bot/.npm/_npx/d537ee5ee2a13f03/node_modules/playwright \
     node crates/tc-ui/app/no-loss.mjs --tc4 target/debug/tc4 --ui target/tc-ui-app-hooks --ui-plain target/tc-ui-app
```

The page is `/?probe=fields#t=<token>`. A stub bridge, where a case needs one, comes from
`context.addInitScript` plus `context.exposeBinding` so its log lives in node across pages:
`window.tc4Shell = { bufferChanged: line => __tc4Append(line), takeBuffered: () => __tc4Take(),
setCanClose: v => __tc4CanClose(v), onCloseRequested: fn => { window.__tc4Close = fn } }`, where
`__tc4Take` returns the last `Changed` line per key with `Removed` keys dropped (a model of G1's
reduction; say so in a comment). Wait for server state by polling `test/fieldLog` (10 s cap), never
by sleeping a fixed time. After every case: `page.content()` holds no `st1.` and no `rt1.`
(`no_token_in_dom`), and no unexpected console error.

Cases (each a `check` with the name below):
- **P0 `the_plain_ui_has_no_test_hooks`**: `--ui-plain`'s `tc_ui_bg.wasm` holds neither
  `test/fieldStore`, `test/fieldDraft` nor `tc4.test.panic` (bytes search).
- **P1 `a_panic_in_an_input_handler_keeps_the_keystroke`** (row): type `draft` in field A, set
  `panicOnInput`, type `X` (the panic). After the reload: `view-error` shows with the panic message,
  the fields do not; within 4 s `fieldLog(probe-a).stores` ends with `draftX` exactly once.
  **P1b `without_the_capture_listener_the_keystroke_is_lost`**: the same with an init script that
  drops `boot.js`'s capture `input` listener (wrap `addEventListener` to ignore `('input', fn,
  true)` from boot.js): the stored text is `draft` (proves P1 tests the listener).
- **P2 `one_panic_with_trapping_closures_reloads_once`** (row): panic on input, then keep typing and
  let the ticker run for 2 s. Main-frame navigations == 2 (the first load and one reload);
  `sessionStorage[tc4.panics.v1]` has one entry; the in-app `view-error` shows; no static callout.
  Keep its normalized `outerHTML` (message replaced) for P3.
- **P3 `three_panics_show_the_static_callout_and_post_the_drafts`** (row): type `draft text` in A
  (under 1 s, so the machine sends no draft), set `panicAtBoot = "2"` and `panicOnInput`, type
  `!`. Navigations == 3 then none for 3 s; the static `view-error` shows and its normalized
  `outerHTML` equals P2's; `fieldLog(probe-a)`: `draft.text == "draft text!"`, `drafts == 1`,
  `stores` empty. Click `view-error-home`: one reload, the probe boots, and within 4 s `stores ==
  ["draft text!"]` once.
- **P4 `a_reload_with_a_save_in_flight_stores_once`**: `test/hold Save`; type T1, blur (state
  `saving`); reload; `test/release`; `stores == [T1]` (base `current`), indicator `saved`.
- **P5 `a_reload_after_newer_typing_saves_the_newer_text`** (RT-2): hold; T1, blur; focus, type T2;
  reload; release; `stores == [T1, T2]`, both `current`; the indicator is not `saved` before T2 is
  stored.
- **P6 `a_crash_before_the_reply_then_a_restart_resends_nothing_and_the_mirror_keeps_the_entry`**
  (A-3): stub bridge; arm `test/crash beforeReply` for `test/fieldStore`; type, blur; `tc4` exits
  86; start a new `tc4` on the same app data; open a new page at its address (new origin);
  `__tc4Probe.buffered(PROBE_PROJECT, PROBE_ID)` returns field A's entry with the text, a `sent` and
  the old instance; `stores` has the text once, and still once after 3 s idle. Same with
  `beforeStore`: `stores` empty, the entry offered with its text.
- **P7 `a_failed_save_holds_the_navigation_gate`** (FR-32, A-26): replace
  `<app-data>/test-hooks/fields.json` by a non-empty folder (A3's trick); type, blur → indicator
  `error`, `probe-retry` shows; `probe-leave` → no `probe-left`; put the file back; `probe-retry` →
  `saved`; type T2 and click `probe-leave` at once → `probe-left` shows only after T2 is stored.
- **P8 `the_close_guard_follows_every_field`**: stub bridge: `setCanClose` gets `true` at boot,
  `false` after typing, `true` after the save, `false` while failed; `__tc4Close()` while dirty
  resolves `true` after the text is stored. Without a bridge: `page.close({ runBeforeUnload: true })`
  while dirty raises a `beforeunload` dialog (Chromium needs a user gesture first: typing gives it);
  while clean, no dialog.
- **P9 `a_restart_behind_the_same_origin_resends_nothing`** (the port-reuse trap; D4, D8): through
  the `Proxy` (which rewrites `X-Tc-Token` to the current server's, so the page can reach the new
  run as a reused port and a matching token would let it); arm `test/crash beforeReply` for
  `test/fieldStore`; type, blur: the text is stored, `tc4` exits 86, the page's save is a lost
  reply and is retried; start a new `tc4` on the same app data and point the proxy at it. The page's
  `hello` check sees another instance before any resend, the event stream reads the new run (A4a's
  restart), the field leaves the machine; `stores` stays at 1 for 3 s; the entry stays in
  `localStorage`; reload: the entry is of another run and is not restored; still 1. Then repeat
  with the `hello` check disabled in a throwaway build (or by asserting on the request log that
  `app/hello` came between the failure and any `test/fieldStore` resend): without it the resend
  stores a stale second text, which shows the check is what P9 tests.
- **P10 `with_session_storage_blocked_a_panic_stops_at_once`**: an init script makes
  `sessionStorage.setItem` throw; panic on input: no reload, the static callout shows.

Measurements (`--measure N`; write `docs/evidence/ui-no-loss-<date>.md`, PERF rule 5: machine,
commit, build, Chromium version, load average, median and p95 of at least 20 runs):
1. **Typing, input to frame** on the probe field: per keystroke, from the `keydown` event's
   `timeStamp` to the next `requestAnimationFrame`; 200 keystrokes per run, 20 runs; with a 2 KB and
   a 20 KB starting text; release build (`build-app.sh --release --test-hooks`,
   `cargo build --release -p tc4 --features test-hooks`) and dev. Budget: 16 ms p95 (PERF "Typing
   in a verse"). Also the capture listener's own time (`tc4Boot.measure`), and the same run with the
   listener dropped (P1b's init script) to isolate A4b's cost.
2. **Counts per keystroke**: `localStorage.setItem` calls (wrap `Storage.prototype.setItem` in an
   init script) == 1, `bufferChanged` == 1; drafts over 10 s of typing ≤ 10; saves while typing
   continuously for 10 s == 0.
3. **Wasm size**: release `tc_ui_bg.wasm` before (A4a: 1,166,565 bytes) and after A4b, plain build;
   name the share of `unicode-normalization` (build once without it to see).
4. Panic to callout: from the panic to the in-app `view-error` after the reload (median of 20), as
   information (no budget).

### Step 11. Design text, CI, row note and gates

- `docs/APP-DESIGN.md`: the changes of D1–D12 (sections named there), each marked *(A4b)* in the
  style of A4a's and A9's notes; the A4b row's scope gets a closing note *(A4b: built … D1 … D13 …;
  the join was A3's; `test/fieldDraft`; evidence `docs/evidence/ui-no-loss-<date>.md`; the probe
  page and `no-loss.mjs`)*, as the A4a and A9 rows have.
- `.github/workflows/ci.yml`: after the `--features app` line add
  `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features test-hooks -- -D warnings`;
  in the `parity-baseline` job add `crates/tc-ui/app/recovery.test.mjs` to the `node --test` list.
- Gates before the last commit, all with `. ~/.tc-env && ` and `-j 6`:
  `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -j 6 -- -D warnings`;
  `cargo test --workspace -j 6`; `cargo clippy -p tc-ui --target wasm32-unknown-unknown
  --all-targets -j 6 -- -D warnings` with no feature, `--features gallery`, `--features app`,
  `--features test-hooks`; `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings`;
  `cargo clippy -p tc-server -p tc4 --lib --bins -j 6 -- -D warnings` and
  `cargo clippy -p tc-app --lib -j 6 -- -D warnings` (CI runs them: no test hook may leak into a
  release library); `node --test crates/tc-ui/app/recovery.test.mjs`; A4a's `smoke.mjs`; `no-loss.mjs`.

---

## 4. The tests that prove "done when"

| Row item | Test (file) | Asserts |
|---|---|---|
| port of tC4's scheduler tests | the 12 port cases of step 4 (`crates/tc-ui/tests/app/save.rs`) | tC4's debounce, never-optimistic state, transitions, failure retention, no hammering, retry with everything since, edits during flight, single-flight retry, newest text once, drain loop and drain refusal |
| proptest: no `current` dropped | `no_current_is_dropped` (`tests/app/save_props.rs`) | unsaved text is never `Clean` and is always in the buffer; a reload restores it |
| `reload_anywhere_stores_the_last_text_once` against `test/fieldStore`, with reloads and `test/crash` at every point | `reload_anywhere_stores_the_last_text_once`, `crash_anywhere_stores_the_last_text_once` and the 8 deterministic cases (`crates/tc-app/tests/app/save_machine.rs`) | the last text stored, once; no id twice; no stale store; no earlier text marked clean; mirror lines valid |
| the AD37 refresh cases | the 9 refresh tests and `events_reply_order_is_safe` | value and token move together; nothing written into a focused field; own saves show no notice; NFC compare; ViewSeq order |
| Playwright: a panic in an input handler keeps the keystroke | P1 (+P1b control) | the panicking keystroke reaches the server after the reload |
| one panic with trapping closures reloads once | P2 | one reload, one counted panic, the in-app callout |
| three panics show the static callout and post the drafts | P3 | no fourth load; tC4's DOM; the server holds the draft; Back to Home saves it |
| A-2, RT-2 | P4, P5, the RT-2 deterministic cases | no double store; newer typing saved after the retry |
| A-3, RT-3 | P6, P9, `a_restart_drops_every_field…`, `an_entry_of_another_run_is_never_resent` | nothing resent after a restart; entries kept; the mirror holds them |
| A-19, RT-19 | the three StaleScope tests | reattach with the base token and a new id, or hold |
| A-9, RT-9 | `entries_of_a_folder_need_its_project_id_too`, `a_session_of_another_project_id_never_takes_a_field`, buffer never drops an unreadable item | entries keyed by folder and id; nothing dropped as a bug |
| A-1, RT-1 | `save_only_fields_save_only_on_save`, `the_gate_never_saves…` (in D9's test) | a section buffer is never saved without Save |
| A-10, RT-10 | `check_comment_saves_on_done_and_on_the_gate`, `read_only_holds_fields…`, `rekey_…` | check comments survive leaving; read-only keeps drafts; the copy can take them |
| A-15, RT-15 | P1–P3, P10, `recovery.test.mjs` | boot.js reloads, counts loads, draws the static callout, posts drafts; capture listener first |
| A-20, RT-20 | `ten_seconds_of_typing_send_no_save_and_at_most_ten_drafts`, measurement 2 | no per-keystroke server write |
| A-26, RT-26 | P7, the gate tests | a standing failure holds every exit; the gate drains first |
| RT-27 | `confirm.rs` tests | one id per dialog; no second send in flight |

## 5. Measurements

See step 10. Record them in `docs/evidence/ui-no-loss-<date>.md` with the commit, the machine
(the owner's Linux PC: i7-12700, 20 CPUs, 61 GB, Ubuntu 26.04.1, shared with up to three builds;
give the load average before and after), Chromium 141.0.7390.37 (build 1194) headless, Playwright
1.56.1, node 22.23.3, and whether each number is release or dev. A typing p95 over 16 ms in the
release build is a finding: profile first (PERF rule 6), and report it; do not change the budget.

## 6. Traps (each found while planning)

1. **A folder name may hold `|`** (§3.2): §6.6's `"<projectRef>|<projectId>|<fieldKey>"` is
   ambiguous. D3's JSON key with a canonical check.
2. **One map item rewrites every entry per keystroke** (section buffers included). D3.
3. **The reply cache keeps refusals** (`dispatch.rs`): a Retry, a reattach or a reissue under the
   old id gets the old refusal for ever. Same id only after a lost reply (D8).
4. **A restarted `tc4` can get the old port**: the old origin's `localStorage` is back, with
   `sent`s of a dead run. Never restore or resend across instances (D4). A live page's lost-reply
   retry can also reach the new run before the event stream tells it about the restart: check
   `hello`'s instance before every resend (D8, rule 6). The new run's new token already refuses
   the old page (401); the check is the second guard, and P9 (with a proxy that rewrites the token)
   is the test that needs it.
5. **A focused field cleaned by its `Ack` has no entry**: the next keystroke's capture listener
   finds none and writes nothing. The machine writes the entry when it knows none exists (rule 14).
6. **A panic hook that `borrow()`s a `RefCell`** borrowed at the panic panics again; the double panic
   aborts before the marker exists. `Cell`s and `try_borrow` only.
7. **`keepalive` bodies are capped at 64 KiB**: cut the message to 8 KiB in the hook.
8. **Traps in futures arrive as `unhandledrejection`**, not `error` (wasm-bindgen-futures runs tasks
   from promise callbacks); a boot panic is thrown synchronously out of `run_app` into `boot.js`.
9. **A static `import` is hoisted**: `boot.js` must register its listeners, then `import()` the glue.
10. **`sessionStorage` blocked**: counting is impossible, so a reload could loop. Stop at once (D10).
11. **NFC must be the server's crate** or a field can stay dirty for ever (D12).
12. **The panic message is text**: `textContent` only in `boot.js` (a project name or user text can
    be in it).
13. **`fieldStore`'s unchanged text answers with the token it came with**, not the newest
    (A3 D7): the machine's base after `Unchanged` is that token.
14. **`AfterReply` in process**: the app stays alive and answers `then: EndProcess`; the driver must
    treat that answer as the process's last.
15. **Dropping a call's future does not stop the store** (`run_write` spawns the work): that is
    what a reload is; do not `abort()` and expect nothing stored.
16. **An `Ack` with more or fewer than one saved value** must be a failure, not a dirty loop.
17. **A persisted entry must accept unknown keys, but a request must refuse them at any depth**
    (tc-api's test): C1b carries entries as raw lines (D4).
18. **A draft can overtake its save** (STORAGE §5.14 writes drafts beside the actor; a save removes
    the field's entry, a later-processed older draft adds it back, and the offer then shows old
    text). The machine sends no draft while saving; tell C2 the server needs an order rule too.
19. **Leptos delegates `on:input`**: a `window` capture listener still runs first, which is what the
    design needs; never move boot.js's listener to the bubble phase.
20. **`beforeunload` under the shell refuses the close silently** (tC4 #356): register it only
    without a bridge.
21. **A4a's `Effect::Changed` drops the cause**: own-save detection uses `Runtime::listen`, which
    gets whole frames. **`Incoming::Restart` reaches no listener**: forward `Effect::Restarted` to
    `Saves` in `Runtime::apply`.
22. **`__tcIdle`** must count armed save timers (or tests settle before the save) but not Save-only
    fields (or the page is never idle) (D11).
23. **The capture listener must be what P1 tests**: the probe panics **before** calling the machine;
    P1b removes the listener and must lose the keystroke.
24. **Two pages on one origin share `localStorage`**: per-field items plus compare-and-remove keep
    one tab from deleting another's newer text. (The owner removed the one-tab check, A-19; say in
    the evidence that two tabs can still both save, and both texts are kept as a conflict.)
25. **The ViewError DOM is written twice** (Leptos and `boot.js`): P3 compares them, so a design
    system change that moves one fails a test.
26. **`tc-ui`'s `test-hooks` feature must never reach a release UI**: P0 checks the plain wasm, and
    `build-app.sh --release` is what the installer runs (G1).
27. **Each Chromium launch is a new origin** (a new port): old origins' `localStorage` stays in the
    profile, unreachable. Not A4b's to clean; write it in the row note for G1 (the shell can clear
    other origins' storage after it has read its mirror).
28. **tC4 drains on `pagehide`**; the rebuild drains on `onCloseRequested` (D5, D13a); do not add a
    `pagehide` save (keepalive bodies are capped and a reload keeps the buffer anyway).
29. **The golden files must be regenerated** when `FieldLogView` grows; read the diff before
    committing it.
30. **A restored `OnBlur`/`OnLeave` field has no timer**: it saves on the gate or an editor's blur,
    not on its own (the design's "usual timer" exists only for `BlurOrIdle`).

## 7. What later units rely on (keep these names)

- **A5**: `ViewError` (`web/view_error.rs`) for `ViewBoundary`; `Saves::indicator()` and
  `indicator::{Indicator, Worst, all_clean}` for X2; `Saves::retry()`; `Saves::gate()` for
  `go`, `backToProjects`, export, print; `panic::set_context`; `recovery::NAV_KEY` (A5 writes the
  navigation snapshot there and does not reopen an editor after a panic: `after` is `Some`); keep
  the `?probe=fields` mount when it replaces `root.rs`.
- **B4a, B4b, B5b, B7, C2–C4**: `Saves::{register, open, typed, focus, blur, save, discard, close,
  refresh, track}`, `FieldWriter`, `FieldKind`, `rules()`; B4b adds a passage shape for section
  `Ack`s (today one saved value per field); C2 the server's draft ordering (trap 18).
- **B6, C5**: `Saves::add_gate_hook` for `align/flush`; `ServerState.align_saves` already counts in
  `all_clean`.
- **B1b, C1b**: `Saves::buffered_for`, `buffer::merge`, `BufferLine` raw lines in
  `session/open { buffered }`; `Saves::rekey` for the read-only copy; `write_cap`.
- **G1**: the bridge contract of D5 (`bufferChanged(line)`, `takeBuffered(): Promise<string[]>`
  every call, `setCanClose`, `onCloseRequested(fn => Promise<boolean>)`, 5 s cap), the line format
  (`crates/tc-api/tests/golden/types/buffer.json`), trap 27.
- **B11, C6, C7a**: `confirm::Confirm`.
