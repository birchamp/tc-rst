# Plan for design unit B1b: tc-app core and MemoryBackend

Planner's revision 1, 2026-10-19. Commit this file with your first commit; it is the only file the
planner wrote.

Worktree `/home/bot/tc-rst/.claude/worktrees/app-b1b`, branch `app-b1b`, cut from the integration
branch `app` at `1f49cda` (the B1a merge). Units C and S1 (storage), A1, A2, A3, A4a, A6a, A6b, A9,
A10, A11, G0 and B1a are merged there. Before you start: `git -C /home/bot/tc-rst log --oneline -3
app`; if `app` moved, merge it into `app-b1b` first and re-read anything below that it touched.

The unit's row (docs/APP-DESIGN.md §10, phase 1):

> B1b tc-app core and MemoryBackend | the seam with `EditToken` and `ReviewToken` *(A-7)*;
> `MemoryBackend` with scripted outcomes over B1a's seed; scenario loader with resources from
> `.state`; `SessionRegistry` with one open per folder and the close at Home *(A-8, A-25)*, epochs;
> the reply cache wired to sessions; `wire` for tokens, scopes and errors (each later unit adds its
> views); the key probes of §4.3 *(A-21)*; `AppCounters`, `CountingSnapshot`; tc-corpus loading;
> contract read cases | C, B1a, A1, A3 | — | contract read cases, including the token-kind cases;
> epoch refusal tests; `opens_of_one_folder_share_one_session` in both orders;
> `probe_finds_every_key_a_scan_finds`; `open_lists_no_key` | the count-test harness runs on Psalms
> and ten times Psalms

No owner question (Q-n) gates this row ("Waits for: —"). No storage request it needs is missing:
R1 and R19 are in tc-journal (§2 below), so no fallback is used. **Never edit `crates/tc-journal`,
`crates/tc-store`, `docs/STORAGE.md`, branch `storage` or `.claude/worktrees/storage`.** A storage
bug is written up for the storage team, not fixed here.

What the planner read (re-read the parts a step cites): AGENTS.md; docs/PLAN.md;
docs/ARCHITECTURE.md; docs/PERFORMANCE.md; docs/APP-DESIGN.md "Owner's answers", §0–§2 (AD2–AD8,
AD12, AD22, AD32, AD35–AD38), §3.1–§3.6 and §3.9, §4.1–§4.6, §7.1–§7.2, §8.1–§8.2, §9.1–§9.2,
§10 (introduction, phase 1, order paragraph), §12 Q-15/Q-16/Q-23, RT-7, RT-8, RT-16, RT-19, RT-21,
RT-25 and "How each point was answered"; docs/evidence/state-seed-check-2026-10-18.md; B1a's
plan; tc-app (`lib.rs`, `app.rs`, `dispatch.rs`, `handlers.rs`, `replies.rs`, `sessions.rs`,
`events.rs`, `jobs.rs`, `hooks.rs`, `config.rs`, `reports.rs`, `backend/mod.rs`,
`backend/seed.rs` and its submodules' public items, `Cargo.toml`, `tests/app.rs`,
`tests/app/{common,hello,dispatch,state_seed}.rs`); tc-api (`lib.rs`, `endpoint.rs`, `ids.rs`,
`token.rs`, `app.rs`, `jobs.rs`, `event.rs`, `query.rs`, `error.rs`, `refusal.rs` (Open),
`write.rs`, `shown.rs`, `test.rs`, `tests/golden.rs` head); tc-journal (`lib.rs`, `token.rs`,
`fold.rs` (Delta, Counters), `fold/reads.rs`, `fold/ranges.rs`, `fold/state.rs` (FoldState, how
`Delta.books` is filled), `fold/project.rs` (`chapter_usfm`, `derived_files`), `fold/people.rs`,
`fold/cow.rs` (ChunkVec), `keys.rs` (RecordKey, ToolId, CheckTarget), `command.rs`,
`tests/operation_counts.rs`, `tests/scope.rs` (R1's own differential test),
`tests/support/mod.rs`); tc-store (`lib.rs`, `handle.rs`, `reply.rs`, `open.rs` (OpenError));
tc-resources (`lib.rs` re-exports, `dcs.rs` `install_dcs_zip`/`from_export`, `pin.rs`
ResourcePin); tc-door43 `verify.rs` `verify_sb_export`, `unpack.rs` Layout; tc-corpus (`lib.rs`,
`history.rs` Journal); tc4 (`lib.rs` serve, `args.rs`, `tests/release.rs`, `tests/common`);
`parity/app/{README.md,env-rust.mjs,env-rust.test.mjs,state.mjs,state.sha256.json,units.json}`;
`parity/tc4-baseline/fixtures/build-state.mjs` (RESOURCES table); `fixtures/parity-state/README.md`;
tC4 at `d310bf3`: `src/state.jsx:190-205` (`leaveCheckpoints`), `:1918-1945`
(`startLeaveCheckpoint`), `:2160-2185` (open waits for the leave checkpoint), `:6195-6205` (sync
waits for it), `:7286-7328` (`backToProjects`). The planner also built the `sample` state with
`node parity/app/state.mjs` into the scratchpad and checked every sideloaded burrito against its
metadata (all md5s match; every metadata revision equals its install record's `sha`).

---

## 0. Ground rules for the builder

- Every shell command that runs cargo, rustc, node or npx starts with `. ~/.tc-env && `. Use
  `-j 6` on every cargo command (four agents share 20 CPUs). Use only this worktree's `target/`.
  If `df -h /home` shows under 30 GB free and no rustc runs in this worktree, run
  `python3 -I /home/bot/tc-rst/.audit/tools/prune-target.py /home/bot/tc-rst/.claude/worktrees/app-b1b/target`.
  Never delete anything outside this worktree.
- Tests never touch the network and never run node (B1a's D11). Cargo tests assemble scenarios
  from committed bytes (step 6).
- Commit each step below as soon as it builds and its tests pass. Each commit message says what
  changed and why; a commit that changes the design text says why in the message too, and ends
  with the attribution line the session gives you.
- Before the last commit run, in this order, and fix everything:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -j 6 -- -D warnings`
  3. `cargo test --workspace -j 6`
  4. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings`,
     then the same with `--features gallery`, then with `--features app` (tc-ui has no `harness`
     feature yet; check `crates/tc-ui/Cargo.toml` again, and run any feature that exists then)
  5. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings`
  6. `cargo build --release -p tc4 -j 6 && TC4_RELEASE_BIN=target/release/tc4 cargo test -p tc4 --test release -j 6 -- --ignored`
     (the release check of AD35; step 9 adds words to it)
  7. `node --test parity/app/observe.test.mjs parity/app/env-rust.test.mjs` (the pure tests run
     without a browser).

---

## 1. What B1b is, and what it is not

**Is.** The storage seam of tc-app and everything that turns it into open sessions:

1. The seam (`backend/mod.rs`): the traits `ProjectStore` and `ProjectHandle`; the token wrappers
   `EditToken` and `ReviewToken`, which only `backend` can make; `Snapshot` (an O(1) clone that
   also mints tokens); `StoreCommand`, whose constructors take the right token kind; the reply,
   error and event types; `NoProjects`, the store of a build with no backend.
2. `MemoryBackend` (`backend/memory.rs`, feature `fixture-backend`): holds folds (from B1a's seed,
   from tc-corpus, or from changes a test makes), opens them as handles, mints deterministic
   tokens, answers every command `Unavailable` unless a scripted outcome is queued, and adds
   teammates' changes through tc-journal's encoder.
3. The scenario loader (`backend/scenario.rs`, feature `fixture-backend`): reads a parity `.state`
   folder, seeds each project with B1a's `tc4_state_seed`, refuses a project whose check fails
   (listing every mismatch), reads the shared project's Door43 remote, and installs the
   sideloaded resources into a `ResourceStore`.
4. `SessionRegistry` (`session.rs`): one open per folder, shared by every caller; a screen user,
   side users and in-flight writes; epochs that are never reused; the close at Home with tC4's
   leave checkpoint; the per-session reply cache; the event pump; the request guard for queries.
5. `wire/`: tokens, `ChangeScope` from a fold `Delta`, record references, person labels, and the
   errors of the seam as `ApiError`s.
6. `counters.rs`: `AppCounters`, `AppCounts`, `CountingSnapshot`, and a clippy guard that keeps
   whole-project fold walks out of tc-app's code.
7. The tests the row names, the contract suite (its "both" cases), and the operation-count harness
   on tc-corpus Psalms and ten times Psalms.
8. `tc4 serve --scenario <state dir>` (feature `fixture-backend`), so the parity harness starts the
   Rust app on a scenario's projects, and `env-rust.mjs` passing it.

**Is not** (each comes with the unit named, which adds its own types, endpoints and golden JSON):

| Not in B1b | Who |
|---|---|
| any new endpoint row (`session/open`, `session/close`, `home/*`, `editor/*`, `test/script`, `test/writeTeammate`, `test/counters`) | `session/open` job, its `Opened` outcome and the S14 overlay: B3; `session/close` (the HTTP face of `leave_screen`): B3; `home/*`: B2; `test/script`: B4a (first save endpoint); `test/writeTeammate`: B10 |
| `ProjectStore::peek`, `ProjectStore::create`, `ProjectHandle::{offered_drafts, leave, counters}` | `peek`: B2; `create`: the unit that builds create (needs R2); `offered_drafts`: C1b; `leave`: B3/D1; `counters`: C0 |
| `CommandError::{Refused, LookAgain, PlanStale}` and the matching wire arms | C0/C2 (the memory backend never produces them) |
| `StoreCause::Command` and the pairing of a store commit with its `RequestId` (`ChangeCause::LocalSave`) | C2, the first real save |
| check-list re-assessment names in `ChangeParts.checks` after a verse edit | B5a/C4 (`CheckListCache`) |
| any screen, any capture, any `report.json` row | B2 onwards |
| tc-store as a dependency of tc-app | C0 |

The seam's sketch in §4.2 lists the full surface; B1b builds what phase 1 uses and records in the
design which unit adds the rest (step 10).

---

## 2. Facts that change the design, and the design text to fix

Fix each in docs/APP-DESIGN.md in the commit of the step that relies on it, marked *(B1b: …)* as
B1a marked its notes, and say why in the commit message.

1. **R1 is provided.** tc-journal `fold/ranges.rs` (commit `c4b8663`) has `FoldState::chapters_of`,
   `chapter_keys` (→ `ChapterKeys { front, verses, alignments, comments, checks }`),
   `decisions_of(tool, book)`, `alignments_of(book)`, `comments_of(book)` and `book_keys(book)`,
   each costing what it returns, with tc-journal's own differential test
   (`tests/scope.rs::range_reads_find_what_a_scan_finds`). §4.3 says to probe keys "until R1
   exposes the fold's per-chapter entries"; R1 now exposes them, so "the key probes of §4.3" are
   R1's range reads, wrapped and counted in `CountingSnapshot`. Fix §4.3's probe list, §4.4's
   "There is no ProjectIndex" note (still true), §4.7's `editor` row, and the R1 row of §9.2
   ("provided: tc-journal `c4b8663`"). The test keeps the row's name,
   `probe_finds_every_key_a_scan_finds`.
2. **R19 is provided.** tc-journal's token payload carries `"kind": "edit" | "review"` under the
   MAC (`token.rs`, `TokenKind`). `parse_edit` and `parse_review` refuse the other kind after the
   MAC check, as tc-store's `ProjectHandle::parse_kind` does. Fix the R19 row of §9.2 and AD3's
   "after R19" wording; the contract case `review_token_in_a_save_is_refused_by_the_actor` stays
   a store case for C0.
3. **Mint from the snapshot that was read.** §4.2's sketch puts `mint` and `mint_review` on
   `ProjectHandle`, which mints from the newest snapshot. A screen that read snapshot n and mints
   from n+1 (a teammate's change arrived between) gets a token that lists a head the person never
   saw; the save then supersedes the teammate's change silently (I4). tc-store's own
   `Snapshot::token` mints from the snapshot that was read. B1b puts `mint`/`mint_review` on
   `Snapshot` (and `CountingSnapshot`), not on the handle. Fix §4.2's sketch.
4. **Commands take the right token kind by type.** `submit(StoreCommand)`, where `StoreCommand`'s
   constructors take `EditToken` for edits and `ReviewToken` for a resolution, so the inner
   journal token never leaves `backend`; the parse functions take the wire types
   (`&tc_api::ShownToken`, `&tc_api::ReviewToken`). Fix §4.2's sketch.
5. **Teammates are not part of a scenario's load.** §4.2 says "the `shared` scenario adds two
   teammates' heads on Titus 1:1". tC4's `shared-home`, `sync-internet-consent`, `sync-failed` and
   `sync-ok` show no conflict (`parity/tc4-baseline/README.md:407-410`); only after "Door43 main:
   two teammates change Titus 1:1" (`sync-ok-teammates`) does the card show "1 conflict ·
   Review". Heads added at load would put a conflict chip on every `shared` screen. B1b builds the
   helper (`backend::memory::teammates::edit_verse`) and `MemoryBackend::teammate_edits`; B10's
   scripted sync and `test/writeTeammate` call it when a screen's steps ask. Fix §4.2's
   "Teammates" bullet.
6. **A refused project does not fail the whole scenario.** §4.2 says "A mismatch fails the
   scenario load". Every scenario holds `sample_burrito_obs`, which B1a's check refuses with 53
   mismatches the storage team owns (docs/evidence/state-seed-check-2026-10-18.md). Failing the
   scenario would load nothing at all; loading the refused project would make a screen look right
   for the wrong reason (AD7). B1b loads the projects that pass, lists each refused one with every
   mismatch (stderr of `tc4 serve`, the scenario report, a test that asserts the exact list and
   fails the day the storage team fixes them), and shows nothing of a refused project. Fix §4.2.
   **Consequence for B2, to say in your final answer:** until the storage team's fix lands, the
   Rust Home in every scenario shows only the Bible card, while the harness's `home()` waits for
   both cards (§7.1), so B2's Home captures fail until then.
7. **`Delta.books` is not `ChangeParts.books`.** tc-journal fills `Delta.books` when a key of a
   book becomes active for the first time (`fold/state.rs:1483, 1680`): a first decision on a
   check, a first alignment of a verse, a new comment. `ChangeParts.books` means "the book changed
   as a whole" (§3.6), and `QueryKey::CheckList(Book(b))` depends on it
   (`crates/tc-api/src/query.rs`). Copying `Delta.books` would refetch every rail of the book on
   a first decision, against AD17 and the count test `decision_refetches_no_rail` (C4). B1b
   derives `books` from book-level record keys only (step 4). Add one sentence to §3.6's
   `ChangeParts` comment.
8. **The write's scope is checked before the reply cache.** §4.3 checks every project-scoped
   request "before any work"; §4.4 gives each session its own reply cache. Dispatch must know a
   write's scope before it picks a cache, so tc-api's `Write` gains `fn at(&self) ->
   Option<&Scope>` (step 1). A stale-scope refusal is then never kept under the request id: the
   UI may resend the same id in the new session and it runs. Fix §3.3's `Write` sketch and §4.4's
   `ReplyCache` row ("the session's cache for project-scoped writes; the app-wide cache for writes
   with no scope").
9. **The open job and its outcome are B3's.** §3.6 says the `Opened` outcome comes with B1b. B1b
   builds no open endpoint and no job: the registry's `open` is what B3's `session/open` job will
   call (with AD8's forwarding of the store's progress). Fix §3.6's sentence to "`Opened` (B3)".
10. **`record_keys` is kept out by a lint.** §8.1 asks `CountingSnapshot` to count `record_keys` and
    fail a test when any path calls it. A count fails only on a path a test runs; B1b adds
    `crates/tc-app/clippy.toml` with `disallowed-methods` for every whole-project fold walk, so
    no path in tc-app's code can call one except inside `counters.rs` (counted) and B1a's seed
    check (with an `#[expect]` saying why). Fix §8.1.
11. **Module map.** `sessions.rs` (`SessionDirectory`, `NoSessions`) is replaced by `session.rs`;
    `AppDeps.sessions` becomes `AppDeps.store`. Fix §4.1's `sessions.rs`, `session.rs` and
    `lib.rs` rows.
12. **The scenario's resources go into the app's own store.** "A temporary `tc-resources` store" is
    the app's `<app-data>/resources`, which the harness's run folder makes temporary; B3's
    `BookResources` reads the app's `ResourceStore`, so a second store would need a second path.
    Clarify §4.2's "Resources" bullet.

Also add a short *(B1b: …)* note to the row in §10 summarising 1, 3–6, 8 and 9.

---

## 3. Shape of the result

```
crates/tc-api/src/endpoint.rs     Write::at(); the ScopeOf impls of every write
crates/tc-app/
  clippy.toml                     the root's four keys + disallowed-methods (step 2)
  src/lib.rs                      pub mod backend, counters, session, wire; sessions.rs removed
  src/backend/mod.rs              the seam (step 2); NoProjects
  src/backend/memory.rs           MemoryBackend, Scripted, teammates (step 3)
  src/backend/scenario.rs         ScenarioState, ScenarioProject, SideloadedResource (step 6)
  src/counters.rs                 AppCounters, AppCounts, CountingSnapshot (step 2)
  src/wire/{mod,token,scope,error}.rs  (step 4)
  src/session.rs                  SessionRegistry, ProjectSession, SideLease, WriteLease, ReadGuard (step 5)
  src/app.rs, dispatch.rs, handlers.rs, hooks.rs   wired to the registry (step 5)
  tests/app/{memory,wire,sessions,scenario}.rs     (steps 3–6), in the `app` binary
  tests/support/counting_store.rs  a ProjectStore wrapper that counts and holds (step 5)
  tests/store_contract.rs          store_contract! and its memory instance (step 7)
  tests/op_counts.rs (+ tests/op_counts/*.rs)      the harness and the count rows (step 8)
  examples/memory_bench.rs         the measurements (step 10)
crates/tc4/src/{args.rs,lib.rs}   --scenario (step 9); tests/scenario.rs; tests/release.rs words
fixtures/parity-state/common/     the 7 state files build-state.mjs generates (step 6)
parity/app/{env-rust.mjs,README.md,capture-rust.mjs,env-rust.test.mjs,units.json}   (step 9)
docs/APP-DESIGN.md                §2's edits; docs/evidence/memory-backend-2026-10-19.md (step 10)
```

Data flow: `tc4 serve --scenario dir` → `ScenarioState::read(dir)` → `MemoryBackend::from_scenario`
(seeds, refusals) and `install_resources(&ResourceStore)` → `App::start(config, AppDeps { store,
.. })` → `SessionRegistry::open(folder, role)` → `ProjectStore::open` once per folder →
`ProjectSession { handle, epoch, replies, .. }` → queries take `App::read(&scope)` →
`CountingSnapshot`; writes go through dispatch → `SessionRegistry::writer(&scope)` → the session's
`ReplyCache`; store events → the session's pump → `Event::Session { at, Changed { view, scope,
cause } }`.

---

## 4. Steps

### Step 1. tc-api: every write states its scope

**Files:** `crates/tc-api/src/endpoint.rs`; `crates/tc-api/tests/golden.rs`;
`crates/tc-app/src/dispatch.rs` (its unit test's `Boom`).

- Extend the trait:
  ```rust
  pub trait Write: Endpoint {
      fn request(&self) -> RequestId;
      /// The open project this write applies to; `None` for a write that is not project-scoped.
      /// tc-app checks it before the reply cache, so a stale scope is never answered from it.
      fn at(&self) -> Option<&Scope>;
  }
  ```
- In `write_impl!` generate `fn at(&self) -> Option<&Scope> { ScopeOf::scope_of(self) }`, with a
  crate-private `trait ScopeOf { fn scope_of(&self) -> Option<&Scope>; }` and two crate-private
  macros, `scoped!(prefs::SetDraftUnit, prefs::SetAlignSuggestions, prefs::RecordPlace)` (returns
  `Some(&self.at)`) and `unscoped!(app::Quit, app::ClientError, prefs::SetLocale, net::Set,
  jobs::Cancel, account::SignIn, account::CancelSignIn, account::Resume, account::SignOut)` plus,
  under `test-hooks`, `unscoped!(test::Crash, test::Hold, test::Release, test::FieldStore,
  test::Reset)`. A write row without one of the two does not compile: every later unit must say
  whether its write is project-scoped. Check the table again when you write this; the lists above
  are every `command`/`job` row at `1f49cda`.
- `Boom` in `dispatch.rs`'s unit test implements `Write` by hand: add `fn at(&self) ->
  Option<&tc_api::Scope> { None }`.
- No wire shape changes. If any golden file changes, that is a bug.

**Test** (in `golden.rs`, which holds an example request per endpoint):
`a_write_names_its_scope_exactly_when_its_json_has_at` — for every `command` and `job` row
(product and test hooks), `example.at()` is `Some(s)` exactly when the example's JSON object has a
top-level `"at"` key, and then `s` equals that key's value read as `Scope`. `endpoint_goldens!` is
generic over every row and queries are not `Write`, so write a second callback macro with a
`command`/`job` arm that checks `Example::example()` and an empty `query` arm, as tc-api's
`write_impl!` splits its arms.

Commit: "tc-api: every write says whether it is project-scoped, so the server can check its scope
before the reply cache".

### Step 2. The seam, the counters and the lint

**Files:** `crates/tc-app/src/backend/mod.rs`, `crates/tc-app/src/counters.rs`,
`crates/tc-app/src/lib.rs`, `crates/tc-app/clippy.toml`, `crates/tc-app/src/backend/seed/check.rs`
(attributes only), `crates/tc-app/tests/app.rs` (crate attribute).

The seam (`backend/mod.rs`; names mirror tc-store's `handle.rs`/`reply.rs` so C0 maps one to one):

```rust
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// The fold at one moment, with the sequence number of its publication. Clone is an Arc clone.
#[derive(Clone)]
pub struct Snapshot { inner: Arc<SnapshotInner> }   // { fold: FoldState, seq: u64, signing: Arc<TokenKey> }
impl Snapshot {
    pub(in crate::backend) fn new(fold: FoldState, seq: u64, signing: Arc<TokenKey>) -> Self;
    pub fn fold(&self) -> &FoldState;
    pub fn seq(&self) -> u64;
    /// An edit token for `key` as THIS snapshot shows it (§2.7, §2 item 3 of the plan).
    pub fn mint(&self, key: &RecordKey, displayed: Option<ChangeId>) -> Result<EditToken, TokenError>;
    pub fn mint_review(&self, key: &RecordKey) -> Result<ReviewToken, TokenError>;
}

/// tc-journal's one token type, split by kind. No public constructor outside `backend`.
pub struct EditToken(tc_journal::ShownToken);
pub struct ReviewToken(tc_journal::ShownToken);
impl EditToken {
    pub fn key(&self) -> &RecordKey;
    pub fn heads(&self) -> &[ChangeId];
    /// The wire form `st1.<body>`. The only way an edit token's text leaves the backend.
    pub fn to_wire(&self) -> Result<tc_api::ShownToken, tc_api::token::TokenTextError>;
}
impl ReviewToken { /* key, heads, to_wire -> tc_api::ReviewToken (`rt1.`) */ }
// Debug prints "EditToken(..)" / "ReviewToken(..)"; no Display, no From/Into, no text accessor.

/// One command, built only from the token kind it needs.
pub struct StoreCommand(tc_journal::command::Command);
impl StoreCommand {
    pub fn save_verse(shown: EditToken, words: String) -> Self;
    pub fn resolve(conflict: ConflictId, shown: Vec<ReviewToken>, choice: Choice) -> Self;
    // later units add one constructor per command they send
    pub(in crate::backend) fn shown(&self) -> Vec<(RecordKey, Vec<ChangeId>)>; // for scripted outcomes
    pub(in crate::backend) fn into_journal(self) -> tc_journal::command::Command; // for C0
}

pub enum Stored { Durable, WrittenNotDurable(String), Unchanged }
pub struct StoreReply { pub tokens: Vec<(RecordKey, EditToken)>, pub new_conflicts: Vec<ConflictId>,
                        pub stored: Stored, pub seq: u64, pub changes: Vec<ChangeId> }
pub enum CommandError { NotWritten(String), ReadOnly(ReadOnlyReason), BadToken(TokenError),
                        Closed, Unavailable }            // ReadOnlyReason = tc_api::write::ReadOnlyReason
pub enum StoreEvent { Committed { seq: u64, delta: Delta, cause: StoreCause }, Mode(SessionMode) }
pub enum StoreCause { Intake { devices: Vec<DeviceId> }, System }
pub struct ProjectFacts { pub id: tc_journal::ProjectId, pub kind: ProjectKind,
                          pub device: DeviceId, pub mode: SessionMode }   // SessionMode = tc_api::event::SessionMode
pub struct ProjectEntry { pub folder: ProjectRef, pub id: tc_journal::ProjectId, pub kind: ProjectKind }
#[derive(Clone)] pub enum OpenFailure { NotAProject, Unreadable { reason: String },
    LockHeld { holder: Option<(String, u32)> }, NewerFormat { major: u16 }, Unavailable }
#[derive(Debug, thiserror::Error)] pub enum StoreError { #[error("{0}")] Other(String) }

pub trait ProjectStore: Send + Sync + 'static {
    /// The projects this store holds. No lock, no fold.
    fn list(&self) -> Result<Vec<ProjectEntry>, StoreError>;
    /// Blocking: called on the blocking pool. A second open of a folder whose handle is live is
    /// `LockHeld` (STORAGE §6.1 O1), which `SessionRegistry` exists to never meet.
    fn open(&self, folder: &ProjectRef, progress: ProgressSink) -> Result<Arc<dyn ProjectHandle>, OpenFailure>;
    /// `test/reset`: every project back to its seed. Sessions are closed first.
    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError>;
}

pub trait ProjectHandle: Send + Sync {
    fn facts(&self) -> ProjectFacts;
    fn snapshot(&self) -> Snapshot;                     // the newest published one
    fn parse_edit(&self, token: &tc_api::ShownToken) -> Result<EditToken, TokenError>;   // MAC, then kind
    fn parse_review(&self, token: &tc_api::ReviewToken) -> Result<ReviewToken, TokenError>;
    fn submit(&self, command: StoreCommand) -> BoxFuture<Result<StoreReply, CommandError>>; // never blocks
    fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<StoreEvent>;
    fn checkpoint(&self, reason: CheckpointReason) -> BoxFuture<CheckpointOutcome>;      // tc_api::event::CheckpointOutcome
    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>>;
}
pub enum CheckpointReason { LeavingProject, Other(String) }   // mirror tc-store's; others come with D1

/// The store of a build with no backend (release builds before C0): it lists nothing, and
/// opening anything is `Unavailable`.
pub struct NoProjects;
```

Invariants of the seam:
- `EditToken`/`ReviewToken` are made only in `backend` (`pub(in crate::backend) fn new`), so a
  review token cannot reach an edit by a conversion slip; the MAC's kind stops a token relabelled
  through JSON (R19).
- `parse_edit` checks the MAC first, then the kind, and returns a `TokenError::Malformed` naming
  both kinds for the wrong kind (as tc-store does). Never parse the wire text with
  `parse_unverified`.
- `to_wire` returns an error for a body `tc_api` refuses (over 64 KiB, say), never panics; the
  caller turns it into `ApiError::Internal` with a report.
- Add two `compile_fail,E0308` doctests on `StoreCommand`: `save_verse` given a `ReviewToken`,
  `resolve` given `Vec<EditToken>`. They are the type split's own proof.

The counters (`counters.rs`):

```rust
pub enum AppCounter { RecordsRead, ValuesDecoded, ItemsDerived, RowsSerialized, ChaptersProjected,
    BookResourceLoads, SummariesComputed, KeysProbed, SavesWaitingBehindADraftWrite,
    ConflictWalks, BooksRendered, BookUsfm }
#[derive(Default)] pub struct AppCounters { /* one AtomicU64 per AppCounter */ }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppCounts { pub records_read: u64, /* … one field per counter */ }
impl AppCounters { pub fn add(&self, counter: AppCounter, n: u64); pub fn read(&self) -> AppCounts; }
impl AppCounts {
    pub fn since(&self, earlier: &Self) -> Self;
    /// The fields of `self` larger than the same field of `other`, by name.
    pub fn exceeding(&self, other: &Self) -> Vec<(&'static str, u64, u64)>;
}

/// Every read a query makes of a project goes through one of these.
pub struct CountingSnapshot { snapshot: Snapshot, counters: Arc<AppCounters> }
impl CountingSnapshot {
    pub fn new(snapshot: Snapshot, counters: Arc<AppCounters>) -> Self;
    pub fn seq(&self) -> u64;
    pub fn fold_counters(&self) -> tc_journal::fold::Counters;
    // a record (RecordsRead += 1 each)
    pub fn key_state(&self, key: &RecordKey) -> Option<KeyState>;
    pub fn projected(&self, key: &RecordKey) -> Option<Record>;
    pub fn heads(&self, key: &RecordKey) -> Vec<ChangeId>;
    pub fn writer_of(&self, key: &RecordKey) -> Option<DeviceId>;      // device of projected_change
    pub fn mint(&self, key: &RecordKey, displayed: Option<ChangeId>) -> Result<EditToken, TokenError>;
    pub fn mint_review(&self, key: &RecordKey) -> Result<ReviewToken, TokenError>;
    // R1's range reads (KeysProbed += keys returned)
    pub fn chapters_of(&self, book: BookId) -> Vec<u16>;               // not counted: numbers only
    pub fn chapter_keys(&self, book: BookId, chapter: u16) -> ChapterKeys;
    pub fn decisions_of(&self, tool: ToolId, book: BookId) -> Vec<RecordKey>;
    pub fn alignments_of(&self, book: BookId) -> Vec<RecordKey>;
    pub fn comments_of(&self, book: BookId) -> Vec<RecordKey>;
    pub fn detached(&self) -> Vec<RecordKey>;
    // projection
    pub fn chapter_usfm(&self, book: BookId, chapter: u16) -> Option<String>;  // ChaptersProjected += 1
    pub fn book(&self, book: BookId) -> BookView;
    pub fn projected_books(&self) -> Vec<BookId>;
    pub fn conflict_counts(&self) -> &tc_journal::fold::ConflictCounts;
    pub fn person_labels(&self, me: DeviceId) -> BTreeMap<DeviceId, tc_journal::fold::PersonLabel>;
    // whole-project walks, counted; a screen that shows the whole thing may call them
    pub fn conflicts(&self) -> Vec<Conflict>;             // ConflictWalks += 1
    pub fn render_book(&self, book: BookId) -> Vec<(DerivedFile, Vec<u8>)>;   // BooksRendered += 1
    pub fn book_usfm(&self, book: BookId) -> Option<String>;                  // BookUsfm += 1
}
```

Check each wrapped tc-journal signature against the source before writing (`ConflictCounts` is
`tc_journal::fold::ConflictCounts`; `conflicts()` returns `impl Iterator<Item=&Conflict>`, so
clone into a `Vec` or return the iterator with a counted constructor). Add a wrapper only when a
read needs it; later units add theirs here, never a read of the fold elsewhere.

The lint (`crates/tc-app/clippy.toml`; clippy takes the file nearest the package, so copy the
root's four `allow-*-in-tests` keys into it):

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
allow-dbg-in-tests = true
disallowed-methods = [
  { path = "tc_journal::fold::FoldState::record_keys", reason = "lists and sorts every key of the project; no read in tc-app may cost the project (APP-DESIGN §4.3); use CountingSnapshot's range reads" },
  { path = "tc_journal::fold::FoldState::frontier", reason = "walks every key; tc-app never needs it (§4.3)" },
  { path = "tc_journal::fold::FoldState::device_stats", reason = "walks every change" },
  { path = "tc_journal::fold::FoldState::derived_files", reason = "walks every book; materialize is the store's" },
  { path = "tc_journal::fold::FoldState::conflicts", reason = "a whole-list walk: go through CountingSnapshot::conflicts, which counts it" },
  { path = "tc_journal::fold::FoldState::conflicts_of", reason = "as conflicts" },
  { path = "tc_journal::fold::FoldState::render_book", reason = "counted in CountingSnapshot::render_book" },
  { path = "tc_journal::fold::FoldState::book_usfm", reason = "counted in CountingSnapshot::book_usfm" },
]
```

- `counters.rs`'s wrappers carry `#[expect(clippy::disallowed_methods, reason = "the one counted
  call")]`. B1a's `seed/check.rs` calls `conflicts()` (line 220) and almost certainly
  `derived_files()`: give each calling function `#[expect(clippy::disallowed_methods, reason = "the
  seed check reads the whole fold once when a fixture loads; no interactive path runs it")]`.
- Integration test crates are linted with the same file: add `clippy::disallowed_methods` to the
  `#![allow(...)]` line of `tests/app.rs`, `tests/store_contract.rs` and `tests/op_counts.rs`, with
  a comment: tests read whole folds to check results; the lint guards the product's paths.
- **Prove the lint fires** before you rely on it: add `let _ = snapshot.fold().record_keys();` in
  `session.rs` temporarily, run clippy, see `disallowed_methods`, remove it. If clippy says the
  path does not resolve (it warns about an unknown path), the re-export path is the problem: try
  `tc_journal::fold::state::FoldState::record_keys` (the module is private, so it may not resolve
  either) and report what worked in the commit message. Do not ship a lint that silently matches
  nothing.

Tests (unit tests in `counters.rs` are fine for the arithmetic):
`app_counts_since_subtracts_field_by_field`, `exceeding_names_every_field_that_grew`.

Commit: "tc-app: the storage seam, with edit and review tokens apart by type, snapshots that mint,
and the counted reads".

### Step 3. MemoryBackend

**Files:** `crates/tc-app/src/backend/memory.rs` (and `backend/memory/teammates.rs` if you split),
`crates/tc-app/src/backend/mod.rs` (module, `#[cfg(feature = "fixture-backend")]`),
`crates/tc-app/tests/app/memory.rs`, `tests/app.rs` (mod line).

```rust
/// A project the memory backend holds.
#[derive(Clone)]
pub struct MemoryProject { pub id: tc_journal::ProjectId, pub kind: ProjectKind, pub fold: FoldState,
                           pub carried: BTreeMap<GitOid, Vec<u8>>, pub remote: Option<Door43Remote> }
impl MemoryProject {
    pub fn from_seed(seed: StateSeed, remote: Option<Door43Remote>) -> Self;
    pub fn empty(id: tc_journal::ProjectId, kind: ProjectKind) -> Self;
    pub fn from_changes(id: tc_journal::ProjectId, kind: ProjectKind, changes: Vec<Change>) -> Self;
}

#[derive(Clone, Default)]
pub struct MemoryBackend { inner: Arc<Mutex<BTreeMap<ProjectRef, Arc<ProjectState>>>> }
impl MemoryBackend {
    pub fn new() -> Self;
    /// Adds a project under `folder`. Its fold gets counters of its own.
    pub fn add(&self, folder: ProjectRef, project: MemoryProject) -> Result<(), MemoryError>;
    /// Queues an outcome for the next command submitted to `folder`'s open handle (FIFO).
    pub fn script(&self, folder: &ProjectRef, outcome: Scripted) -> Result<(), MemoryError>;
    /// Adds teammates' changes to `folder`'s fold, as an intake would; an open handle publishes a
    /// new snapshot and sends `Committed { cause: Intake { devices } }`.
    pub fn teammate_edits(&self, folder: &ProjectRef, changes: Vec<Change>) -> Result<Delta, MemoryError>;
    /// The fold the next open of `folder` gets (for tests).
    pub fn fold(&self, folder: &ProjectRef) -> Option<FoldState>;
}
pub enum Scripted {
    /// A fresh edit token for each displayed value; the fold does not change; `Stored::Durable`.
    Written,
    WrittenNotDurable(String),
    NotWritten(String),
    /// The next outcome, after this long (the reply waits; the caller never does).
    After(Duration, Box<Scripted>),
    /// The project becomes read-only (a `Mode` event), and this and every later command is refused.
    ReadOnly(ReadOnlyReason),
    /// These changes reach the fold (a new snapshot, a `Committed` event), then the next outcome.
    TeammateHeads(Vec<Change>, Box<Scripted>),
}
pub mod teammates {
    /// One change per text, each by its own Local device (Teammate 1, 2, … in device-id order),
    /// all with the record's current heads as parents: what "two teammates change Titus 1:1 from
    /// the same version" writes. tc-journal's encoder (`Change::new`) makes them (AD36).
    pub fn edit_verse(fold: &FoldState, key: &RecordKey, texts: &[&str]) -> Result<Vec<Change>, MemoryError>;
}
```

Behaviour:
- Per folder a `ProjectState { seeded: MemoryProject, current: Mutex<FoldState>, published:
  RwLock<Snapshot>, seq: AtomicU64, mode, scripts: VecDeque<Scripted>, open: bool, subscribers:
  Vec<UnboundedSender<StoreEvent>> }`.
- Tokens: `TokenKey::derive(&MEMORY_INSTALLATION_SECRET, project.id)` with a fixed 32-byte
  constant (AD's "fixed test installation secret, so they are deterministic").
- `facts().device`: a fixed Local device, `DeviceId::local([0xB1; 15])`, "this computer", so
  `person_labels(me)` gives `Mine` to it and `Teammate(n)` to the teammates.
- `open`: unknown folder → `NotAProject`; a folder whose handle is open → `LockHeld { holder:
  Some(("this computer".into(), std::process::id())) }` (mimics tc-store's O1, so the registry's
  sharing is tested against a backend that would refuse a second open); else a handle whose first
  snapshot is the current fold at `seq` 1. `progress` is not used.
- `submit` with no script → `Err(CommandError::Unavailable)` (AD6). With a script, the returned
  future does the outcome (`Written` mints, from the newest snapshot, an edit token for each
  `(key, heads)` of `StoreCommand::shown()` with `displayed` = the shown token's single head when
  it lists one; `TeammateHeads` folds the changes into a clone of `current`, publishes it with
  `seq + 1`, sends `Committed`, then continues). A read-only project refuses every command with
  `ReadOnly(reason)` before looking at the script queue.
- `checkpoint` → `CheckpointOutcome::Unchanged` (memory has no git); `close` → marks the folder
  closed, drops the subscribers (the session's pump ends), resolves `Ok(())`. Dropping a handle
  without `close` also marks it closed (as tc-store's `Drop` does).
- `list` → every folder with `id` and `kind`, in folder order.
- `reset` (test-hooks): `current` = `seeded.fold.clone()` with fresh own counters, scripts cleared,
  mode `Writable`. Refuse (`StoreError::Other`) while a handle is open: the registry closes every
  session first.
- `MemoryProject::from_changes`: `FoldState::fold(kind, changes.into_iter().map(FoldInput::from))`.
- Every project added gets `fold.own_counters()` (trap 7).

`teammates::edit_verse`: parents = `fold.heads(key)`; the record = `fold.projected(key)` with its
`VerseValue::Text { words, .. }` replaced (refuse a key whose projected value is not a text verse);
devices `DeviceId::local([0x7E, n, 0, …])` for n = 1, 2, … so Teammate 1 sorts first; `seq` 1;
`at` a fixed instant (2026-10-01T00:00:00Z plus n minutes), later than any seed time; batch ids
from sha256("tc4-teammate-batch\0" ‖ device bytes), one batch of one change each; cause
`Cause::Edit`. Follow `crates/tc-journal/tests/support/mod.rs` `Dev::batch` for the
`ChangeBody` fields.

Tests (`tests/app/memory.rs`; build backends from B1a's sample seed via the scenario helpers of
step 6 if step 6 is done first, else seed `fixtures/tc4/sample-burrito` directly with
`Tc4Folder::read` + `tc4_state_seed`):
- `a_second_open_of_a_folder_with_a_live_handle_is_lock_held`; and after `close`, an open works.
- `an_unknown_folder_is_not_a_project`.
- `commands_are_unavailable_without_a_script` — `save_verse` on TIT 1:1's token → `Unavailable`;
  the snapshot's seq and fold frontier-free facts (heads of the key) are unchanged.
- `scripted_written_returns_a_fresh_token_for_the_displayed_value_and_changes_nothing` — the reply
  lists one `(TIT 1:1, token)` whose heads equal the shown token's; `stored` is `Durable`; seq and
  heads unchanged.
- `scripted_not_written_and_written_not_durable_are_returned_as_scripted`.
- `scripted_read_only_switches_the_mode_and_refuses_every_later_command` — a `Mode(ReadOnly(..))`
  event; `facts().mode` is read-only; a second command (no script) is `ReadOnly`, not
  `Unavailable`.
- `scripted_teammate_heads_reach_the_fold_before_the_reply` — after the reply, the newest
  snapshot has seq 2, TIT 1:1 has two heads and is `Resolved::Conflict`, and the subscriber got
  `Committed { seq: 2, cause: Intake { devices: [t1, t2] }, delta }` with TIT 1:1 in `delta.keys`.
- `scripted_delay_holds_the_reply_not_the_caller` (`tokio::test(start_paused = true)`): `submit`
  returns at once; the future is pending until the paused clock advances past the delay.
- `teammate_edits_of_a_closed_project_reach_the_next_open`.
- `reset_restores_every_project_to_its_seed` — teammates' heads, a read-only switch and queued
  scripts are gone; the fold's heads of TIT 1:1 equal the seed's.
- `tokens_are_the_same_in_every_backend` — two backends from the same seed mint the same wire
  text for TIT 1:1.
- `a_snapshot_does_not_move_when_the_project_does` — a snapshot taken before `teammate_edits`
  still shows one head; the new one shows two (snapshots are immutable).

Commit: "tc-app: the memory backend, with scripted outcomes and teammates' changes made by
tc-journal's encoder".

### Step 4. `wire`: tokens, scopes and errors

**Files:** `crates/tc-app/src/wire/mod.rs`, `wire/token.rs`, `wire/scope.rs`, `wire/error.rs`,
`crates/tc-app/tests/app/wire.rs`. Pure functions; `pub mod wire` so later units' handlers and the
tests use the same code.

```rust
// wire/token.rs
pub fn shown(token: &EditToken) -> Result<tc_api::ShownToken, ApiError>;        // via EditToken::to_wire; error = Internal (the caller reports)
pub fn review(token: &ReviewToken) -> Result<tc_api::ReviewToken, ApiError>;
pub fn edit_token(handle: &dyn ProjectHandle, token: &tc_api::ShownToken) -> Result<EditToken, ApiError>;   // Err = NotSaved(BadToken)
pub fn review_token(handle: &dyn ProjectHandle, token: &tc_api::ReviewToken) -> Result<ReviewToken, ApiError>;
pub fn token_update(key: &RecordKey, token: &EditToken) -> Result<TokenUpdate, ApiError>;

// wire/scope.rs
pub fn record_ref(key: &RecordKey) -> Option<RecordRef>;          // canon::encode_key → to_canonical → RecordRef::parse
pub fn record_key(record: &RecordRef) -> Result<RecordKey, ApiError>;   // BadRequest on failure
pub fn view(seq: u64) -> ViewSeq;
pub fn project_id(id: tc_journal::ProjectId) -> tc_api::ids::ProjectId; // `to_string()` is the lowercase hyphenated UUID
pub fn person_label(label: &tc_journal::fold::PersonLabel) -> tc_api::shown::PersonLabel;  // Teammate(u32) → Teammate(u16) saturating; computer None
pub fn change_scope(delta: &Delta, conflicts_changed: bool,
                    by: impl Fn(&RecordKey) -> Option<tc_api::shown::PersonLabel>) -> ChangeScope;
pub fn change_cause(cause: &StoreCause, label: impl Fn(&DeviceId) -> Option<tc_api::shown::PersonLabel>) -> ChangeCause;

// wire/error.rs
pub fn command_error(error: CommandError) -> ApiError;
pub fn open_failure(failure: OpenFailure) -> ApiError;
pub fn stored(stored: &Stored) -> (tc_api::write::Stored, Option<String>);   // (Ack.stored, Ack.disk_warning)
```

`change_scope` rules (errs toward a refresh, as `QueryKey::depends_on` does):
- `delta.everything` → `ChangeScope::Everything`.
- `chapters` ← `delta.chapters`, plus `(book, chapter)` of every `ChapterFront`, `Verse` and
  `Alignment` key; sorted, no repeats.
- `books` ← only the book of a `BookPresence`, `BookHeader` or `BookLayout` key (§2 item 7). Never
  copy `delta.books`.
- `stories` ← `StoryBase(s)`, `Frame(s, _)`, `StoryRef(s)`, and a `Check` whose target is
  `Frame { story, .. }`.
- `checks` ← `CheckListRef { tool, unit }` of every `Check` key: `ToolId::TranslationNotes` →
  `Tool::TranslationNotes`, `TranslationWords` → `TranslationWords`, `TranslationQuestions` → no
  list (tc-api has no tQ tool; the record still goes in `records`); unit `Book(b)` for a verse
  target, `Stories` for a frame target.
- `aspects` ← `Pins` if `delta.pins` or any `Pins`/`ResourcesExtra` key; `Settings` for a `Setting`
  key; `Meta` for a `ProjectMeta` key; `Conflicts` if `!delta.new_conflicts.is_empty()` or
  `conflicts_changed` (the caller compares the conflict counts before and after: a resolution
  removes a conflict and `new_conflicts` lists only added ones) or a `DeviceProfile` key (names
  in the review and the "changed by" lines change).
- A `Versification` key → `Everything` (every check maps again).
- `records` ← every key of `delta.keys` as `RecordChange { record, by: by(key) }`. A key whose
  `record_ref` fails (over 4,096 bytes) → `Everything`. Then `ChangeScope::bounded(parts)` (over
  256 records → `Everything`).
- `delta.paths` (carried paths moved) adds nothing: no phase-1 screen shows carried files; say so
  in a comment.
- Write the key mapping as one `match` over every `RecordKey` variant with no `_` arm, so a new
  variant does not compile until someone decides its parts.

`command_error`: `NotWritten(r)` → `NotSaved(Disk(r))`; `ReadOnly(r)` → `NotSaved(ReadOnly(r))`;
`BadToken(_)` → `NotSaved(BadToken)`; `Closed` → `NotSaved(StaleScope)` (the session closed under
the write; the UI reattaches and resends, A-19); `Unavailable` → `Unavailable(Feature::Writes)`.
`open_failure`: `NotAProject` → `Refused(Open(NotAProject))`; `Unreadable { reason }` →
`Refused(Open(Unreadable { reason }))`; `LockHeld { holder }` → `Refused(Open(LockHeld { holder:
holder.map(|(host, pid)| LockHolder { host, pid }) }))`; `NewerFormat { major }` →
`Refused(Open(NewerFormat { format: major }))`; `Unavailable` → `Unavailable(Feature::Writes)` (add
"and opening a project, until C0" to `Feature::Writes`'s doc comment; no wire change).
`stored`: `Durable` → `(Written, None)`; `WrittenNotDurable(r)` → `(Written, Some(r))`;
`Unchanged` → `(Unchanged, None)`.

Tests (`tests/app/wire.rs`):
- `edit_and_review_tokens_cross_the_wire_with_their_prefixes` — `shown(edit)` serializes to
  `"st1.…"`, `review(rt)` to `"rt1.…"`; reading each back with `edit_token`/`review_token` gives a
  token with the same key and heads.
- `a_relabelled_review_token_is_bad_token` — a review token's body wrapped as `st1.` (through
  `tc_api::ShownToken::from_body`, the `server` feature) parses as JSON but `edit_token` answers
  `NotSaved(BadToken)`; the reverse for `review_token`.
- `change_scope_of_two_teammates_editing_titus_1_1` — on the delta of step 3's teammate edits:
  `Parts { chapters: [TIT 1], books: [], records: [TIT 1:1 by Teammate 2 or 1 as the projected
  head says], aspects: [Conflicts], .. }`.
- `every_record_kind_maps_to_its_change_part` — one synthetic `Delta` per `RecordKey` variant
  (build keys directly; `Delta` has public fields), asserting the parts listed above.
- `a_first_record_in_a_book_does_not_mark_the_book_changed` — a delta whose `books` is `[TIT]` and
  whose only key is a new `Check` key gives `books: []` and `checks: [TN TIT]`.
- `more_than_256_records_is_everything`, `a_versification_change_is_everything`,
  `everything_stays_everything`.
- `a_resolution_marks_the_conflict_list` — `conflicts_changed = true` with no new conflicts gives
  `aspects: [Conflicts]`.
- `a_translation_questions_check_lists_its_record_and_no_check_list`.
- `command_errors_map_to_not_saved` and `open_failures_map_to_open_refusals` — one assertion per
  variant (the tables above).
- `stored_maps_to_the_ack_fields`, `person_labels_keep_mine_names_and_numbers`,
  `a_record_key_survives_the_round_trip` (for one key of each variant that the memory projects
  hold).

Commit: "tc-app: wire for tokens, change scopes and the seam's errors".

### Step 5. Sessions, epochs, the request guard and the reply cache per session

**Files:** `crates/tc-app/src/session.rs` (new), delete `crates/tc-app/src/sessions.rs`;
`crates/tc-app/src/{lib.rs,app.rs,dispatch.rs,handlers.rs,hooks.rs}`;
`crates/tc-app/tests/support/counting_store.rs`; `crates/tc-app/tests/app/{common,hello,dispatch,sessions}.rs`;
`crates/tc4/src/lib.rs` (the `AppDeps` field rename only).

```rust
pub struct SessionRegistry { inner: Arc<RegistryInner> }   // Clone
impl SessionRegistry {
    pub(crate) fn new(store: Arc<dyn ProjectStore>, bus: EventBus, reports: ReportLog,
                      clock: Arc<dyn Clock>, counters: Arc<AppCounters>, runtime: tokio::runtime::Handle) -> Self;
    /// Opens `folder` for the screen, or joins the session a side user holds or is opening. At most
    /// one folder has the screen: a screen open of another folder first leaves the old one (as
    /// going Home would). Waits for a leave checkpoint of `folder` that is still running.
    pub async fn open_screen(&self, folder: &ProjectRef, progress: ProgressSink) -> Result<OpenSession, OpenFailure>;
    /// A sync, a USB export or a Settings-from-Home save: shares the session; leaves when dropped.
    pub async fn open_side(&self, folder: &ProjectRef, progress: ProgressSink) -> Result<SideLease, OpenFailure>;
    /// The close at Home: the screen user leaves at once (the caller goes Home now); the
    /// "leaving the project" checkpoint runs behind it; the session closes when its last user and
    /// its last write are gone. A scope that is not the screen's is `Err(StaleScope)`.
    pub fn leave_screen(&self, at: &Scope) -> Result<(), StaleScope>;
    /// Holds the session open for one write. `None`: the scope is closed or another open's.
    pub fn writer(&self, at: &Scope) -> Option<WriteLease>;
    /// The session of a query; `Err(current)` names the folder's open epoch, if any.
    pub fn reader(&self, at: &Scope) -> Result<Arc<ProjectSession>, Option<Epoch>>;
    pub fn resolve(&self, at: &Scope) -> Option<SessionInfo>;   // what the prefs handlers read
    pub fn open_sessions(&self) -> Vec<OpenSession>;            // hello; sorted by folder
    /// Resolves when no open, leave checkpoint or close of `folder` is running.
    pub async fn settled(&self, folder: &ProjectRef);
    /// quit and test/reset: the screen leaves, side leases are ended, every session closes.
    pub async fn close_all(&self);
    /// test/quiet: an open, checkpoint or close is running, or a session's write is.
    pub fn busy(&self) -> bool;
}
pub struct ProjectSession { /* scope, id (tc-api), handle, replies: Arc<ReplyCache>, counters, view: AtomicU64 */ }
impl ProjectSession {
    pub fn scope(&self) -> &Scope;
    pub fn handle(&self) -> &Arc<dyn ProjectHandle>;
    pub fn read(&self) -> CountingSnapshot;
    pub fn view(&self) -> ViewSeq;
    pub fn replies(&self) -> &Arc<ReplyCache>;
}
pub struct SideLease { /* Drop: the side user leaves */ }  impl SideLease { pub fn scope(&self) -> &Scope; pub fn session(&self) -> &Arc<ProjectSession>; }
pub struct WriteLease { /* Drop: the write is gone */ }   impl WriteLease { pub fn session(&self) -> &Arc<ProjectSession>; }
pub struct SessionInfo { pub project: ProjectRef, pub id: tc_api::ids::ProjectId }   // moved from sessions.rs
pub struct StaleScope;
```

Per folder the registry keeps one slot: `Opening` (a `watch` channel every caller waits on;
`ProjectStore::open` runs once, on `spawn_blocking`), `Open { session, screen: bool, side: u32,
writes: u32, leaving: Option<watch::Receiver<bool>> }`, or `Closing` (a `watch` the next open
waits on). Never hold the registry's `std::sync::Mutex` across an `.await`; take what you need,
drop the guard, await, lock again.

Invariants:
- **One open per folder** (AD38): `ProjectStore::open` is called at most once per folder at a time;
  every caller that arrives while it runs waits for it and becomes another user of the same
  session; a failure reaches every waiter (`OpenFailure: Clone`), consumes no epoch and leaves no
  slot, and adds one `ReportOp::Open` report (`Refused { code, key }` from the `OpenRefusal`'s
  `Message`).
- **Epochs**: one `AtomicU64` per registry, from 1, `fetch_add` at each successful store open,
  never reset (not by `test/reset`, not by `close_all`), so a page from an earlier open or scenario
  never matches. A screen that joins a session a side user kept open gets that session's epoch.
- **The close at Home** follows tC4's `backToProjects` + `startLeaveCheckpoint`
  (`tC4:src/state.jsx:7286-7328, 1918-1945`): `leave_screen` returns at once; the checkpoint
  (`CheckpointReason::LeavingProject`) runs in a task; `SessionEvent::Checkpoint(outcome)` goes out
  for that scope; a failed checkpoint adds a `ReportOp::Checkpoint` `Failed { reason }` report and
  the session still closes (the saves are durable in the journal; nothing is lost; tC4 retries the
  owed checkpoint at the next open, which is D1's to build). An open of the same folder, by any
  role, waits for a running leave checkpoint (`tC4:src/state.jsx:2177, 6203`).
- **A session closes** when it has no screen, no side user and no write in flight, after its leave
  checkpoint: `handle.close()`, then the slot is removed, then `Event::Session { at,
  SessionEvent::Closed }`. A close error adds a report; the slot is removed anyway.
- **Writes hold the session**: a `WriteLease` is taken in dispatch before the reply cache and moved
  into the write's work, so the session cannot close under a running write, even when the HTTP
  caller went away. `writer` answers `None` once the session is `Closing`.
- **Events**: `Opened` once per epoch when the store open completes; the session's pump (a tokio
  task over `handle.subscribe()`) turns `Committed { seq, delta, cause }` into
  `Changed { view: wire::view(seq), scope, cause }` (`conflicts_changed` = the pump's previous
  snapshot's `conflict_counts()` differ from the newest's; `by` = the label of
  `writer_of(key)`; labels from `person_labels(facts.device)`), updates `view`, and turns
  `Mode(m)` into `SessionEvent::Mode(m)`. The pump ends when the handle's senders drop.
- `open_sessions()` lists a session while it has a screen or a side user, with `role: Screen` when
  the screen is a user, else `Side`, and `mode` from `handle.facts().mode`.

Integration:
- `AppDeps.sessions` → `AppDeps.store: Arc<dyn ProjectStore>`; `AppDeps::production()` uses
  `NoProjects`. `Inner` gains `store`, `sessions: SessionRegistry`, `counters: Arc<AppCounters>`.
  `App::sessions() -> &SessionRegistry`, `App::counters() -> AppCounts`, and
  `App::read(&self, at: &Scope) -> Result<Arc<ProjectSession>, ApiError>` (the request guard of
  queries: `Err(ApiError::StaleScope { current })`). `tc4`'s `AppDeps::production()` call needs no
  change; any literal `AppDeps { .. }` does.
- `handlers.rs`: `session_of` reads `self.inner.sessions.resolve(at)`; `hello` reads
  `open_sessions()`.
- `dispatch.rs` `run_write`: after `parse`, `canonical` and `digest`, if `req.at()` is `Some(at)`:
  `let Some(lease) = sessions.writer(at) else { return self.error_answer(NotSaved(StaleScope)) }`
  (never cached; checked before the internet refusal), the cache is
  `lease.session().replies().clone()`, and the lease moves into `work`; if `None`, the app-wide
  cache as today.
- `App::quit`: stop the seed (as now), then `sessions.close_all().await`, then flush prefs.
- `hooks.rs` `Reset`: release every hold first (a held operation could block a close), then
  `sessions.close_all().await`, then `store.reset()` (report a failure as `Internal`), then the
  rest as today. `Quiet`: add `self.inner.sessions.busy()`.

`tests/support/counting_store.rs` (included with `#[path = "../support/counting_store.rs"] mod
counting_store;` from each test binary that needs it): `CountingStore<S>` wraps any
`ProjectStore`, counts `open` calls per folder, can hold the next `open` of a folder until
released (a `std::sync::Barrier` or `Mutex<bool>` + `Condvar`, since `open` blocks), and wraps each
returned handle to count `checkpoint` (with its reasons) and `close`, and to hold or fail the next
checkpoint. It is backend-agnostic, so C0's store instance of the contract suite reuses it.

Migrate the tests that used `FakeSessions` (`tests/app/common.rs`, `hello.rs`, `dispatch.rs`):
`Parts.sessions` → `Parts.store: Arc<MemoryBackend>` holding `kr_ulb` as
`MemoryProject::empty(PROJECT_ID, Bible)`; tests open it with `open_screen` and use the scope it
returns (epochs are the registry's now; "another open's scope" is `Epoch::new(scope.epoch.get() +
1)` before any reopen). Keep each test's assertion; change only how the session comes to exist.

Tests (`tests/app/sessions.rs`, all through `App`; the row's epoch refusal tests are the first
five):
- `a_write_after_going_home_is_stale_scope_and_writes_nothing` — `prefs/setDraftUnit` with the
  left scope → `NotSaved(StaleScope)`; after 50 ms `files.writes()` holds nothing new.
- `a_write_with_the_epoch_of_an_earlier_open_is_stale_after_a_reopen` — open (e1), leave, settle,
  open (e2 > e1); a write at e1 is stale, at e2 lands.
- `a_query_with_a_stale_scope_names_the_current_epoch` — `App::read` with e1 →
  `StaleScope { current: Some(e2) }`; with the folder closed → `current: None`.
- `a_stale_scope_refusal_is_never_kept_under_its_request_id` — id X at a closed scope → stale; the
  same id X at the open scope (other body) runs: no `RequestReused`.
- `epochs_are_never_reused_across_reopens_and_resets` — open, `test/reset`, open: the second epoch
  is larger.
- `a_retry_of_a_project_write_is_answered_from_its_sessions_cache` — the same id with another body
  → `RequestReused` while the session is open.
- `the_sessions_cache_goes_with_its_session` — after leave and reopen, the same id with the new
  scope runs.
- `writes_with_no_scope_still_use_the_app_wide_cache` — `net/set` retried with another body →
  `RequestReused` with no session open.
- `a_session_stays_open_while_one_of_its_writes_runs` — hold a `WriteLease` (via
  `sessions().writer`), `leave_screen`: `settled` stays pending and the store's `close` count is 0
  until the lease drops; then it closes once.
- `going_home_runs_the_leave_checkpoint_then_closes` — one checkpoint with `LeavingProject`, one
  close; events in order `Opened`, `Checkpoint(Unchanged)`, `Closed` for the scope; `hello` lists
  no session.
- `going_home_returns_before_the_leave_checkpoint_ends` — hold the checkpoint: `leave_screen`
  returns `Ok` and `settled` is pending.
- `an_open_during_the_leave_checkpoint_waits_for_it_and_gets_a_new_epoch` — hold the checkpoint,
  start `open_screen`: no second store `open` until release; then a new epoch, the old scope stale.
- `a_side_session_outlives_going_home_and_closes_when_its_holder_leaves` — screen + side; leave
  the screen: one checkpoint, no close, `hello` shows the session as `Side` with the same epoch;
  drop the side lease: one close.
- `opening_a_second_project_leaves_the_first_screen_session` — two memory projects; screen on A
  then on B: A's checkpoint and close ran, `hello` lists only B.
- `a_refused_open_reaches_every_caller_and_leaves_nothing_open` — hold the open of an unknown
  folder, two callers, release: both get `NotAProject`, one store `open`, one `Open` report, no
  session, no epoch used (the next good open gets epoch 1 if it is the registry's first).
- `a_failed_leave_checkpoint_still_closes_and_says_so` — the wrapper fails the checkpoint:
  `Checkpoint(Failed { reason })` goes out, a `Checkpoint` report is kept, the session is closed.
- `quit_closes_every_session_after_its_checkpoint`.
- `test_reset_closes_every_session_and_puts_the_seed_back` — teammates' heads added to the memory
  project are gone after `test/reset`; `hello.sessions` is empty.
- `teammates_on_an_open_project_send_one_changed_event` — the `Event::Session { at, Changed {
  view: 2, scope, cause: Intake { from: [Teammate 1, Teammate 2] } } }` frame on the bus, with
  step 4's expected scope; `session.view()` is 2.
- `hello_lists_the_open_sessions_with_their_roles_and_modes` (replaces the FakeSessions case in
  `hello.rs`).

Commit: "tc-app: one session per open folder, epochs, the close at Home with tC4's leave
checkpoint, and a reply cache per session".

### Step 6. The scenario loader and its resources

**Files:** `crates/tc-app/src/backend/scenario.rs`; `crates/tc-app/Cargo.toml` (`zip` optional
under `fixture-backend`: `fixture-backend = ["dep:zip"]`, `zip = { workspace = true, optional =
true }`); `fixtures/parity-state/common/**` (7 files) and `fixtures/parity-state/README.md`;
`crates/tc-app/tests/app/scenario.rs`; `crates/tc-app/tests/app/state_seed.rs` (make
`obs_written_up` `pub(crate)` for reuse).

```rust
/// A parity scenario's state as build-state.mjs lays it out: `repos/_local_/_local_/<project>/`,
/// `repos/_local_/_sideloaded_/<resource>/`, `client_settings/uw-tc4.json`, `vrs/*.json`,
/// `provenance.json`.
pub struct ScenarioState { files: BTreeMap<String, Vec<u8>> }
impl ScenarioState {
    pub fn read(dir: &Path) -> Result<Self, ScenarioError>;                 // plain files only; refuses a link
    pub fn from_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Result<Self, ScenarioError>;
    /// A scenario assembled from this repository's committed bytes, exactly as
    /// `parity/app/state.sha256.json` records it: each recorded file is found by its sha256 among
    /// `fixtures/tc4/**`, `fixtures/parity-state/**` and `parity/tc4-baseline/fixtures/vendor/**`.
    /// Only `provenance.json` may be absent (it names the build, not the state).
    pub fn from_repository(root: &Path, scenario: &str) -> Result<Self, ScenarioError>;
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>>;
    pub fn projects(&self) -> Result<Vec<ScenarioProject>, ScenarioError>;
    pub fn resources(&self) -> Result<Vec<SideloadedResource>, ScenarioError>;
}
pub struct ScenarioProject { pub folder: ProjectRef, pub tc4: Tc4Folder, pub remote: Option<Door43Remote> }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Door43Remote { pub owner: String, pub repo: String }
pub struct SideloadedResource { pub dir: String, pub pin: ResourcePin, pub files: BTreeMap<String, Vec<u8>> }
pub fn install_resources(resources: &[SideloadedResource], store: &ResourceStore) -> Result<Vec<Installed>, ScenarioError>;
pub struct ScenarioReport { pub loaded: Vec<(ProjectRef, SeedReport)>, pub refused: Vec<(ProjectRef, SeedRefusal)> }
impl MemoryBackend { pub fn from_scenario(state: &ScenarioState) -> Result<(MemoryBackend, ScenarioReport), ScenarioError>; }
#[derive(Debug, thiserror::Error)] pub enum ScenarioError { Io { path, problem }, Layout { path, problem },
    Records { problem }, Remote { path, problem }, Install { dir, problem }, Digest { path, problem } }
```

Rules:
- Every path of the state is one of the five kinds above; anything else is `Layout`. `vrs/*.json`
  and `provenance.json` are read and ignored (tC4's own copies; tc-versification has its own).
- A project folder name must parse as `ProjectRef`; else `Layout` naming it. The folder name is the
  project's folder in the app (`sample_burrito`, `sample_burrito_obs`, as env-rust.mjs's
  `PROJECTS` expects). Its files go to `Tc4Folder::from_files(name, …)`; its project id is
  `fixture_project_id(name)` (B1a's D10).
- `.remotes.json` beside a project (the mock server's remote list; the seed ignores it as
  `MockServerRemotes`) is read with serde into `[{name, url}]`; the `origin` URL
  `https://<host>/<owner>/<repo>.git` gives `Door43Remote { owner, repo }`; anything unreadable is
  `Remote` (a shared project must not silently look local). B2 needs it for "On Door43".
- `client_settings/uw-tc4.json`'s `installedResources` maps `"_local_/_sideloaded_/<dir>"` →
  `{repoPath, version, sha, flavor}`, which deserializes as `tc_resources::ResourcePin`. A record
  without its folder, or a folder without its record, is `Records`.
- `install_resources`: for each resource, zip its files in memory (metadata.json at the zip root;
  Stored compression is fine) and call `tc_resources::install_dcs_zip(store, &pin, &zip, 64 <<
  20)`. An install already present is returned as it is (idempotent: a restarted `tc4` with the
  same app-data does nothing). Never install anything from `bundle/en/` (§4.2; Q-11).
- `from_scenario`: seeds every project with `tc4_state_seed`; a passing one is added as
  `MemoryProject::from_seed(seed, remote)`; a refused one is listed in `report.refused` with its
  whole `SeedRefusal`, and is not added. It never adds teammates (§2 item 5).

`fixtures/parity-state/common/` holds the only state files no committed file matches by sha256:
`client_settings/uw-tc4.json` and `repos/_local_/_sideloaded_/{el-x-koine_ugnt,en_obs,en_ta,en_tw,
en_ult,en_ust}/metadata.json` (about 390 KB; identical in all three scenarios). Copy them from
`node parity/app/state.mjs --scenario sample --out target/b1b/state-sample`, never by hand, and add
a row to `fixtures/parity-state/README.md` saying so, plus that their content licence is
`fixtures/tc4/LICENSE-CONTENT.md`'s. The other 172 resource files are byte-identical to files
under `fixtures/tc4/test-fixtures` and `parity/tc4-baseline/fixtures/vendor` (the planner checked
every digest).

Tests (`tests/app/scenario.rs`):
- `every_scenario_is_assembled_from_committed_bytes` — for `sample`, `en-pins`, `shared`:
  `from_repository` succeeds, and its files with their sha256 equal `state.sha256.json`'s entries
  for that scenario minus `provenance.json`, both ways.
- `every_scenario_loads_its_bible_project_and_refuses_its_obs_project` — `report.loaded` is
  `[sample_burrito]` with B1a's declared differences; `report.refused` is `[sample_burrito_obs]`
  whose `SeedRefusal::Mismatches` tuples equal `state_seed::obs_written_up()` exactly; the
  backend's `list()` is `[sample_burrito]`. A comment says this test changes the day the storage
  team fixes those mismatches.
- `only_the_shared_scenario_names_a_door43_remote` — `shared` gives `facilitator-zq/sample_burrito`
  (and the OBS folder its own, even though that project is refused); the others `None`.
- `a_scenario_loads_no_teammate_and_no_conflict` — every loaded fold has `conflict_count() == 0`
  and TIT 1:1 has one head.
- `scenario_resources_install_exactly_the_install_records` — into a fresh store in a tempdir:
  `installed_dcs_pins()` equals the 12 records' pins; installing again changes nothing (same
  receipts, no new content file written: `content_files_written()` on the test thread does not
  move); no pin of `tc_resources::suite::bundled_pins()` that is not in the records is installed.
- `a_state_with_an_unknown_entry_or_an_unmatched_record_is_refused` — three cases, each with its
  path in the error.
- `a_project_folder_that_is_no_project_ref_is_refused` — e.g. `a:b`.
- `the_projects_folder_stays_empty` — after `from_scenario`, opening a session and closing it, the
  app's `--projects` tempdir has no entry (phase 1 writes no project, §10 introduction).

Commit: "tc-app: load a parity scenario into the memory backend, refusing a project whose seed
check fails, and install its resources".

### Step 7. The contract suite's read cases

**Files:** `crates/tc-app/tests/store_contract.rs` (its own test binary).

```rust
/// What a backend gives the contract cases.
pub struct ContractFixture {
    pub store: Arc<dyn ProjectStore>,
    pub bible: ProjectRef,                        // the sample Bible project
    pub sources: BTreeMap<BookId, Vec<u8>>,       // its USFM files' bytes (TIT, JON)
    pub conflicted: (ProjectRef, RecordKey),      // a record with two heads (two teammates on TIT 1:1)
}
macro_rules! store_contract { ($name:ident, $make:path) => { mod $name { /* one #[tokio::test] per case */ } } }
store_contract!(memory, memory_fixture);   // C0 adds store_contract!(store, store_fixture)
```

`memory_fixture`: `ScenarioState::from_repository(root, "sample")` → `from_scenario`; plus folder
`sample_burrito_teammates` = the `shared` scenario's Bible project with
`teammates::edit_verse(TIT 1:1, ["…", "…"])` applied (`MemoryProject::from_seed` then
`teammate_edits`).

Cases ("both" rows of §4.2's table, and the token-kind cases):
- `chapter_view_equals_seed_usfm` — for TIT and JON, split the source bytes at each `\c N` marker
  (find markers with tc-usfm's parser or at line starts, then check the rule on these files: every
  `\c` starts a line here): the head before the first `\c` plus every chunk equals
  `CountingSnapshot::book_usfm(book)`, and each chunk equals `chapter_usfm(book, n)` byte for byte.
- `minted_token_parses` — `snapshot.mint(TIT 1:1)` → `wire::token::shown` → JSON → back →
  `edit_token(handle, …)`: same key, same heads.
- `forged_token_is_refused` — a flipped byte in the body; a token minted under another
  installation secret (a second `MemoryBackend` cannot do this, so build a `TokenKey` from another
  secret in the test with `tc_journal::TokenKey::derive` and `ShownToken::mint`); a token of
  another project id: each → `NotSaved(BadToken)`.
- `review_token_lists_every_head` — on the conflicted record: the review token's heads equal
  `heads(key)` (two); the edit token minted with `displayed: None` lists one of them only.
- `review_token_text_is_refused_as_an_edit_token` — `"rt1.<body>"` does not deserialize as
  `tc_api::ShownToken`; `"st1.<review body>"` deserializes and `edit_token` refuses it.
- `edit_token_text_is_refused_as_a_review_token` — the mirror case.
- `snapshot_clone_costs_nothing` — 1,000 clones of a snapshot and 1,000 `handle.snapshot()` calls
  move no fold counter (`fold_counters().since(&before) == Counters::default()`) and no
  `AppCounts` field; two `snapshot()` calls with no change between have the same `seq`.
- `opens_of_one_folder_share_one_session` — through `App::sessions()` over `CountingStore(store)`,
  in both orders, each as its own test function (`…_screen_first`, `…_side_first`): hold the first
  open, start the second, release; both get the same epoch; the store's `open` count for the
  folder is 1; `hello` shows one session, role `Screen`; leaving the screen keeps it (role `Side`);
  dropping the side lease closes it once.

Commit: "tc-app: the contract suite's read cases on the memory backend".

### Step 8. The operation-count harness (the row's "done when")

**Files:** `crates/tc-app/tests/op_counts.rs` with modules under `crates/tc-app/tests/op_counts/`
(`harness.rs`, `corpus.rs`, `session.rs`, `probes.rs`); `crates/tc-app/Cargo.toml` dev-dependency
`tc-corpus = { path = "../tc-corpus" }`.

`corpus.rs`:
```rust
pub struct Corpora { pub psalms: MemoryProject, pub larger: MemoryProject, pub chapter: u16 }
/// Spec::psalms(1) and Spec::scaled(1, 10), generated and folded once per test binary.
pub fn corpora() -> &'static Corpora;
```
Fold as `crates/tc-journal/tests/operation_counts.rs` does:
`FoldState::fold(ProjectKind::Bible, corpus.journal.lines().into_iter().map(|l| FoldInput::from(l).prepare()))`,
id `corpus.journal.project`. `chapter`: the first Psalms chapter whose `chapter_keys(PSA, c)`
(front, verses, alignments) are equal in both folds, with no comment in it and no key in conflict
(`key_state` heads ≤ 1 and not `Resolved::Conflict`), found with R1 reads only.

`harness.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost { pub app: AppCounts, pub fold: tc_journal::fold::Counters }
pub struct Scale { pub app: App, pub folder: ProjectRef, pub scope: Scope, _dir: TempDir }
/// A fresh App over a MemoryBackend holding `project` (a clone with its own counters) as "psalms",
/// with the screen session open.
pub async fn scale(project: &MemoryProject) -> Scale;
/// What `action` cost: AppCounts and the project's fold counters, before and after.
pub async fn cost<F, Fut>(scale: &Scale, action: F) -> Cost where F: FnOnce(&Scale) -> Fut, Fut: Future<Output = ()>;
/// The fields that grew from Psalms to ten times Psalms, by name; empty when none did.
pub fn growth(small: &Cost, large: &Cost) -> Vec<String>;
/// Runs `action` on both corpora and fails, naming every field that grew.
pub async fn costs_what_it_touches<A>(name: &str, action: A) -> (Cost, Cost);
```
Start each `App` on tempdirs with `ProjectsFolder::Given(tmp/projects)` (never the default folder;
`AppDeps::production()` reads the real home) and `AppDeps { store: Arc::new(backend),
..AppDeps::production() }`.

Rows (`session.rs`):
- `open_lists_no_key` — the cost of `open_screen` + `hello` on both corpora: `growth` is empty;
  every `AppCounts` field is 0 (no key read, no walk); the fold's `records_listed`,
  `records_projected`, `changes_visited` and `keys_recomputed` deltas are 0.
- `snapshot_costs_nothing_at_any_size` — 1,000 `session.read()` and clones: all counters 0 on both.
- `a_chapter_read_costs_one_chapter` — on `corpora().chapter`: `chapter_keys`, `key_state` and
  `mint` per verse, `chapter_usfm`: `growth` empty and `keys_probed` > 0 (so the row measures
  something).
- `a_token_round_trip_costs_one_record` — mint a verse token of the common chapter, `shown`, back
  through `edit_token`: `growth` empty.
- `the_harness_fails_an_action_that_costs_the_project` — the planted case: an action that reads
  `chapter_keys` of every chapter of every projected book; `growth` names `keys_probed`. Proves the
  harness can fail.

`probes.rs`: `probe_finds_every_key_a_scan_finds` — for each of: the sample Bible project (Titus,
Jonah), `sample_burrito_teammates` (conflict), a project with records beyond its versification,
Psalms and ten times Psalms — compare, per projected book, with `snapshot.fold().record_keys()`
filtered (the scan, allowed in tests):
- the union over `chapters_of(book)` of `chapter_keys(book, c).verses` equals the scan's `Verse`
  keys of the book; of `.front`, its `ChapterFront` keys;
- `alignments_of(book)` equals the scan's `Alignment` keys of the book (and the union of
  `chapter_keys(..).alignments` over `chapters_of` is a subset: see trap 8);
- `decisions_of(tool, book)` for each `ToolId::ALL` equals the scan's `Check` keys with that tool
  and a verse target in the book;
- `comments_of(book)` equals the scan's `Comment` keys whose history targets the book (as
  `tc-journal/tests/scope.rs:240-258` filters);
- `detached()` equals `fold().detached()` collected.
The beyond-versification project: `MemoryProject::from_changes` with a Titus presence and header,
verses 3:16 and 3:17 and chapter 4 verses 1–3 (Titus has 3 chapters and 15 verses in chapter 3),
an alignment at 4:1, a tN decision on 4:2, a comment on 4:3. Build the changes with a small
`Dev`-style helper in the test (copy the pattern of `tc-journal/tests/support/mod.rs`); try B1a's
seed on an edited `TIT.usfm` first only if you want, but direct changes are simpler and do not
depend on the seed's scope check.

Commit: "tc-app: the operation-count harness on Psalms and ten times Psalms, with the open, read
and token rows and the key reads' differential test".

### Step 9. `tc4 serve --scenario`, and the harness passing it

**Files:** `crates/tc4/Cargo.toml` (feature `fixture-backend = ["tc-app/fixture-backend"]`;
dev-dependency features `["test-hooks", "fixture-backend"]`), `crates/tc4/src/args.rs`,
`crates/tc4/src/lib.rs`, `crates/tc4/tests/scenario.rs`, `crates/tc4/tests/release.rs`;
`parity/app/env-rust.mjs`, `parity/app/README.md`, `parity/app/capture-rust.mjs` (header comment),
`parity/app/env-rust.test.mjs` (skip message), `parity/app/units.json`.

- `Args` gains `#[cfg(feature = "fixture-backend")] pub scenario: Option<PathBuf>`; the usage line
  of a fixture build adds `[--scenario <state dir>]`; a repeated flag is `Repeated`.
- In `serve`, after taking `app.lock` and before `App::start`: with `--scenario`, `ScenarioState::
  read(dir)` (a missing or unreadable folder stops the start with its path, `start_failed`, before
  the address line), `MemoryBackend::from_scenario`, then `install_resources(&scenario.resources()?,
  &ResourceStore::open(app_data.join(RESOURCES_FOLDER))?)`; then `App::start(config, AppDeps {
  store: Arc::new(backend), ..AppDeps::production() })`. Opening the store before `App::start`
  is safe: `App::start`'s `open_keeping_leftovers` finds whole installs and no `.partial/`.
- Print one stderr line per project and a total, e.g. `tc4: scenario <dir>: loaded
  sample_burrito (3 declared differences)`, `tc4: scenario <dir>: refused sample_burrito_obs, 53
  mismatches, the first: <Display of the first three>; docs/evidence/state-seed-check-2026-10-18.md`,
  `tc4: scenario <dir>: 12 resources installed`. Never print to stdout before the address line.
- `release.rs`: add `"--scenario"` to `HOOK_TEXT` (a flag only fixture builds have) and update the
  comment on `tc4-seed-batch`: since B1b `tc4` calls the seed in fixture builds, the word now
  guards the release binary too. Run the ignored release test (§0 item 6).
- `tests/scenario.rs`: `tc4_serve_with_a_scenario_loads_its_projects_and_installs_its_resources` —
  write `ScenarioState::from_repository(root, "sample")`'s files into `<tmp>/state`; spawn `tc4
  serve --app-data … --projects <empty> --ui <a minimal ui dir as the other tc4 tests make> --test-hooks
  --scenario <tmp>/state`; read the address; `test/quiet` answers; `app/hello` has no session;
  stderr names `sample_burrito` loaded and `sample_burrito_obs` refused; quit; then
  `ResourceStore::open_read_only(app_data/resources)` lists the 12 pins; the projects folder is
  still empty. And `a_missing_scenario_folder_stops_the_start_with_its_path` (exit code and the
  error line, nothing else on stdout). Follow `tests/seed.rs` for spawning and reading the store.
- `env-rust.mjs`: `scenarioArgs(scenario, dir)` builds the state with `buildScenarioState({
  scenario, out: path.join(dir, 'state') })` (from `./state.mjs`) and returns `['--scenario',
  thatDir]`; drop the "B1b's `--backend memory`" comment; the exit-2 hint and the 404 hint name
  `--features test-hooks,fixture-backend`. `README.md`'s build line, `capture-rust.mjs`'s header
  and `env-rust.test.mjs`'s skip message say the same. If you can, run the browser tests once
  (`crates/tc-ui/build-app.sh`, `cargo build -p tc4 --features test-hooks,fixture-backend -j 6`,
  `TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/env-rust.test.mjs`); if
  you cannot, say so in the final answer.
- `units.json`: add `"B1b"` (it owes no layer; the gate's §7.6 table names no B1b screen); run
  `cargo test -p tc-ui --test screen_record -j 6` to confirm the gate accepts it.

Commit: "tc4: serve a parity scenario on the memory backend (fixture builds only), and the harness
passes it".

### Step 10. Measurements, design text, final checks

**Files:** `crates/tc-app/examples/memory_bench.rs` (`required-features = ["fixture-backend"]`;
examples may use dev-dependencies), `docs/evidence/memory-backend-2026-10-19.md`,
`docs/APP-DESIGN.md` (whatever of §2 is not yet done; the *(B1b: …)* note in the row).

Run the measurements of §6, write the evidence file (machine: CPU model and whether it has SHA and
AVX-512, `grep -o -w 'sha_ni\|avx512f' /proc/cpuinfo | sort -u`; commit; median and p95 per
PERF rule 5), run §0's final checks, commit.

---

## 5. The row's tests and "done when", mapped

| Row item | Where | Proof |
|---|---|---|
| contract read cases, including the token-kind cases | `tests/store_contract.rs` (step 7) | the eight cases on `memory`; the two `compile_fail` doctests (step 2); `a_relabelled_review_token_is_bad_token` (step 4) |
| epoch refusal tests | `tests/app/sessions.rs` (step 5) | the first five tests there |
| `opens_of_one_folder_share_one_session` in both orders | `tests/store_contract.rs` (step 7) | `…_screen_first`, `…_side_first` |
| `probe_finds_every_key_a_scan_finds` | `tests/op_counts/probes.rs` (step 8) | five projects, R1 reads against the scan |
| `open_lists_no_key` | `tests/op_counts/session.rs` (step 8) | zero key reads, no growth |
| done when: the count-test harness runs on Psalms and ten times Psalms | `tests/op_counts.rs` (step 8) | four rows pass on both corpora, and the planted row shows the harness fails a project-sized action |

---

## 6. Measurements

Release build, this PC, at least 20 runs (5 for anything over 1 s), median and p95, in
`docs/evidence/memory-backend-2026-10-19.md` with machine and commit:

1. Corpus generation and fold, Psalms and ten times Psalms (what every `op_counts` run pays; also
   the debug wall time of `cargo test -p tc-app --test op_counts -j 6`, measured with `time`).
2. `ProjectHandle::snapshot()` + clone on ten times Psalms, per call (expect well under 1 µs: an
   Arc clone; a FoldState clone copies a few thousand chunk pointers and is not what a read pays).
3. `open_screen` then `leave_screen` + `settled` on the memory backend, ten times Psalms.
4. A chapter read of the common chapter (range read, `key_state` and `mint` per verse,
   `chapter_usfm`) on Psalms and on ten times Psalms: the two medians should be close (rule 1; the
   count test is the proof, the clock only a check). Compare with PERF's 50 ms "first chapter's
   verses ready" for scale.
5. `MemoryBackend::from_scenario` for `sample` (two seeds) and `install_resources` into a fresh
   store (12 installs, about 190 files, each fsynced): this is what each `tc4 serve --scenario`
   start costs the harness.
6. `tc4 serve --scenario` from spawn to the address line, 5 runs.
7. The test-suite cost: `cargo test -p tc-app -j 6` wall time before your first commit and after
   your last.

No PERF budget applies to these; they size the harness and check rule 1. If (2) is not O(1) in
practice, or (4) grows with the corpus, stop and find out why before you go on.

---

## 7. Traps

1. **Teammates at load** would put a conflict chip on four `shared` screens tC4 shows without one
   (§2 item 5). Load none; B10 adds them when a step asks.
2. **The OBS project is refused in every scenario** until the storage team fixes 53 mismatches.
   Refuse it per project, list every mismatch, assert the exact list; never excuse a mismatch to
   make it load (AD7). B2's Home captures wait for the fix (§2 item 6).
3. **`Delta.books` is not "the book changed"** (§2 item 7). Derive `ChangeParts.books` from
   book-level keys.
4. **tQ has no wire tool.** A `TranslationQuestions` check key lists its record and no check list.
5. **`RecordRef` caps at 4,096 bytes.** A key that does not fit makes the scope `Everything`; never
   unwrap it.
6. **Mint from the snapshot that was read** (§2 item 3), never from the handle's newest snapshot.
7. **A cached corpus fold shares its counters with every clone.** Give every project the memory
   backend adds `own_counters()`, or parallel tests add into each other's counts and a count test
   passes or fails by chance.
8. **`chapters_of` lists only chapters with a verse or a front.** An alignment or comment anchored
   in a chapter with no verse record (after a renumber) is missed by a loop over `chapters_of`;
   book-wide reads use `alignments_of`/`comments_of`. The differential test catches a loop.
9. **The lint must be proved to fire** (step 2), and the crate's `clippy.toml` replaces the root's,
   so copy the four `allow-*-in-tests` keys or test code starts failing `unwrap_used`.
10. **Do not add a data variant to `tc_api::write::ReadOnlyReason`**: it is a unit-only enum written
    as a string; one data variant turns every variant into a tagged object on the wire. tc-store's
    `OpenOnAnotherComputer { host }` and `WriteFailed` have no tc-api variant yet: C0's problem,
    not B1b's (the memory backend produces only tc-api's four).
11. **Two "seeds" in tc-app.** `crate::seed` is A9's first-launch resource seeding; `backend::seed`
    is B1a's state seed. Do not mix them up in names, reports or features.
12. **The close at Home does not wait.** `leave_screen` returns at once; the checkpoint and close
    run behind it, and only a later open of that folder waits (tC4). A blocking leave would make
    every "back to projects" wait up to 500 ms (a whole-Bible checkpoint, PERF).
13. **No std `Mutex` guard across `.await`** in the registry; the compiler may not stop you
    (`MutexGuard` is `!Send`, which stops it only inside spawned futures).
14. **Epochs are never reset**, not by `test/reset`: the harness reuses one page across scenarios'
    `home()`, and a stale page must meet `StaleScope`, never another open's session.
15. **A stale-scope refusal is never cached** (§2 item 8), and the scope check comes before the
    internet refusal too.
16. **A write keeps its session open** through a `WriteLease` moved into the cached work, not held
    by the HTTP caller.
17. **Every write row needs a `ScopeOf` impl**; tc-app's `Boom` test type implements `Write` by
    hand and needs `at()`.
18. **Resources go into the app's store before `App::start`**, from the scenario only, never from
    `bundle/en/`; `install_dcs_zip` returns an existing install unchanged, so a restart is free.
19. **No node in cargo tests**: `ScenarioState::from_repository` finds every byte by its recorded
    sha256; only the 7 generated files are committed.
20. **`AppDeps::production()` reads the real home folder.** Tests always pass
    `ProjectsFolder::Given(tempdir)`, or the first launch creates `~/translationCore4/projects`.
21. **`test/reset` releases holds before it closes sessions**, or a held operation blocks
    `close_all` and the reset never answers.
22. **A failed leave checkpoint still closes the session** and says so; it never keeps the person in
    the project or drops anything (tC4's `commitErrorRepo` retry is D1's).
23. **`hello.sessions` must be sorted** (by folder), or `hello`'s golden-like tests flake.
24. **tc-api's golden files must not change.** `Write::at()` adds no field. A golden diff means a
    wire change by mistake.
25. **`FoldState::frontier()` walks every key** (it is in the lint list). The C0 fallback in §4.2 that
    compares frontiers is not needed: tc-store publishes snapshots and events (R4, R5, R16 are in
    `crates/tc-store/src/handle.rs`). Say so to C0 in your final answer; do not edit §9.2's R4/R5/R16
    rows beyond what B1b relies on.

---

## 8. Invariants that protect a translator's work (check each before your last commit)

- Nothing in B1b writes a project: the memory backend holds folds in memory, and the only files
  written are app-data (the resource store, prefs). `the_projects_folder_stays_empty` and the tc4
  test assert it.
- No work is hidden: the key reads are R1's, checked against a full scan on five projects; a
  refused scenario project is reported with every mismatch, never dropped silently; a delta that
  cannot be described exactly becomes `Everything` (a refresh), never a smaller scope.
- No work is silently rewritten: tokens are minted from the snapshot that was shown; an edit token
  cannot stand where a review token is needed (type, prefix, MAC kind); a stale scope never gets a
  cached reply; a session never closes under a running write.

---

## 9. What B1b leaves for later units (put the short form in your final answer)

- **B2**: `ProjectStore::peek` and `ProjectEntry`'s use for cards; `Door43Remote` for "On
  Door43"; the OBS card is missing until the storage team's fix (§2 item 6).
- **B3**: `session/open` (job, AD8's forwarding, `Opened` outcome) calling `open_screen`, and
  `session/close` calling `leave_screen`; a card click that joins a Home sync's in-flight open
  shares that open's progress sink.
- **B4a**: `test/script` over `MemoryBackend::script`; `StoreCommand` constructors for its saves.
- **B10**: `ScriptedSync` and `test/writeTeammate` over `teammates::edit_verse` and
  `teammate_edits`, with tC4's texts for `sync-ok-teammates`.
- **C0**: `StoreBackend` implementing the seam over `tc_store::ProjectHandle` (whose shape the seam
  mirrors); `store_contract!(store, …)` with the store-only cases; tc-store's read-only reasons
  that tc-api lacks; `CommandError::{Refused, LookAgain, PlanStale}`; `ProjectHandle::counters`.
- **C1b**: `offered_drafts`. **C2**: `StoreCause::Command` and `ChangeCause::LocalSave`.
  **D1**: the owed leave checkpoint retried at the next open.
