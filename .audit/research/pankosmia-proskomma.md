# Pankosmia server and Proskomma: reuse assessment for a Rust tC4

**Question.** Should a Rust rebuild of translationCore 4 reuse Pankosmia's Rust server (`pankosmia-web`) and/or Proskomma, or build its own storage and sync core?

**Short answer.** Build our own storage and sync core as a Rust library crate. Do not depend on `pankosmia-web`, and do not fork it whole. Copy a few small, MIT-licensed pieces and the data it serves (templates, the language lookup, versification JSON). Port Proskomma's versification mapper (about 380 lines of JS) to Rust, and use `proskomma-core` only as a test oracle. Keep the on-disk format the same as today, so projects created by today's tC4 still open. Section 4 gives the reasons.

## Sources and versions read

| Source | Version read | Location |
|---|---|---|
| pankosmia-web | 0.19.2, `bf0a5d3`, 2026-10-07 (shallow clone, one commit of history) | `/home/user/ref/pankosmia-web` |
| pankosmia-web (tC4's pin) | 0.18.15, `a83725b` (crates.io `.crate`, `.cargo_vcs_info.json` confirms the sha), diffed against 0.19.2 | downloaded to the scratchpad `crate01815/` |
| proskomma-core | 0.11.3, `0204ad3`, 2025-12-01 | `/home/user/ref/proskomma-core` |
| translationCore4 client | `d494a9d` | `/home/user/unfoldingword/translationcore4` |

Citation style: `file:line`. Server paths are relative to `/home/user/ref/pankosmia-web/`. tC4 paths are relative to `/home/user/unfoldingword/translationcore4/`. "PN #n" means entry n of `docs/PLATFORM-NOTES.md`.

---

## 1. pankosmia-web architecture

### 1.1 Shape and crates

- **A Rocket 0.5.1 HTTP server.** It has about 110 routes under `/api` (`src/utils/launch.rs:12-197`) and 12,786 lines of Rust in `src/`. It uses no database. State is held in three places:
  - files on disk;
  - one `AppSettings` struct of `Mutex`es in memory (`src/structs.rs:72-88`);
  - global atomics such as `NET_IS_ENABLED` (`src/static_vars.rs`).
- **Dependencies** (`Cargo.toml:23-48`):
  - `git2 = 0.20.2`: libgit2 bindings, not gitoxide.
  - `rocket 0.5.1` (json), `boon` (JSON Schema), `zip 7`, `walkdir`, `ureq` (calls to Gitea), `chksum-md5`/`md5`, `mime-infer`, `whoami`, `uuid`, `chrono`/`time`, `copy_dir`, `tempfile`, `json_value_merge`.
  - Feature-specific crates: `ffmpeg-sidecar` (audio and video endpoints) and `rten`, `rten-generate`, `rten-text` and `pankosmia_rag_chat` (a local LLM/RAG endpoint).
  - The tC4 rig's lockfile resolves to **348 crates** (`dev-env/server/Cargo.lock`).
  - `Cargo.lock` is gitignored upstream (`.gitignore:9`).
  - On macOS, libgit2 needs OpenSSL. tC4 vendors both (`dev-env/server/Cargo.toml`, "#243").
- **Blocking I/O in async handlers.** All `std::fs` and libgit2 calls run inside async Rocket handlers. Nothing uses `spawn_blocking` (grep finds no use).

### 1.2 Library or binary?

The crate is a library in name only. The only usable public API is "build the whole server":

- `pub fn rocket(launch_config: Value) -> Rocket<Build>` (`src/lib.rs:34`), plus `pub mod endpoints` (`src/lib.rs:24`).
- `structs` and `utils` are private modules (`src/lib.rs:14-15`). An outside crate cannot name or build `AppSettings`, so it cannot mount the handlers by itself or call `ingredients_metadata_from_files` (which is `pub`, but inside the private `utils`).
- `rocket()` does all bootstrap, and it panics on any bad input:
  - It finds `product.json` relative to `current_exe()` (`src/lib.rs:38-69`).
  - It creates and wipes `temp/` (`src/lib.rs:141-153`).
  - It copies templates (`src/utils/bootstrap.rs:15-86`).
  - It panics when no client is registered at `/clients/<homepage>` (`src/utils/bootstrap.rs:505-508`, PN #25).
- `src/main.rs` is a shim of about 25 lines. tC4's `dev-env/server/src/main.rs` is a verbatim copy of it, plus the crate pin.

In short, you can embed the whole HTTP server. You cannot use it as a storage library.

### 1.3 API routes (mounts in `src/utils/launch.rs`)

| Mount | Routes | What they do |
|---|---|---|
| `/` | `GET /`, `GET /clients/main` | Redirect to `/clients/<product.homepage>` (`src/endpoints/clients.rs:124-132`) |
| `/api` | `list-clients`, `client-interfaces`, `client-config`, `version`, `favicon.ico`, `GET`/`POST client-settings/<storage_id>` | Client registry; flavor-to-endpoint map built as a `BTreeMap` keyed by client id (`clients.rs:47-99`); per-client JSON settings in `client_settings/<client_id>.json` (`clients.rs:154-224`) |
| `/api/notifications` | `GET /` | SSE: `status` (net/debug) every 500 ms, `bcv`, `current_project`, `typography`, `auth`, `languages`, `alignment` (`src/endpoints/sse.rs:15-170`) |
| `/api/settings` | `languages`, `typography`, `typography-feature`, `dev-setting`, `auth-token/<key>/<code>/<client_code>` | User settings; auth-proxy callback |
| `/api/net`, `/api/debug` | `status`, `enable`, `disable` | Global atomics. Net-gated routes answer 401 "offline mode" (`src/utils/response.rs:44-49`) |
| `/api/temp` | `POST bytes`, `GET bytes/<id>`, `GET html/<id>` | Scratch uploads in `<working>/temp` |
| `/api/llm` | `model`, `rag-prompt` | Local model inference (`rten`) |
| `/api/i18n` | `POST /`, `raw`, `negotiated/<filter..>`, `flat/<filter..>`, `untranslated/<lang>`, `used-languages` | One `i18n.json` per working dir |
| `/api/navigation` | `GET`/`POST bcv…` | Shared book/chapter/verse state |
| `/api/app-state` | `current-project`, `alignment`, `snippet`, `word`, `empty-alignment` | Cross-client UI state, persisted in `app_state.json` |
| `/api/gitea` | `remote-repos/<server>/<org>`, `user-remote-repos/<server>/<user>`, `endpoints`, `login/<key>/<redir..>`, `logout/<key>`, `my-collaborators/<proxy>/<org>/<project>` | Door43/Gitea catalog (one `GET /api/v1/orgs/{org}/repos`, no paging parameters, `gitea_remote_repos.rs:47-48`); login through a configured auth proxy (`gitea_proxy_login.rs:45-49`) |
| `/api/git` | `new-text-translation`, `new-audio-translation`, `new-bcv-resource`, `new-obs-resource`, `new-tcore-resource`, `new-translation-plan-resource`, `new-print-spec-resource`, `new-scripture-book/<repo>`, `new-bcv-resource-book/<repo>`, `clone-repo/<host>/<org>/<repo>[?branch]`, `list-local-repos`, `delete/<repo>`, `add-and-commit/<repo>`, `status/<repo>`, `log/<repo>`, `copy/<repo>?target_path&delete_src&add_ignore`, `remotes/<repo>`, `remote/add/<repo>?remote_name&remote_url`, `remote/delete/<repo>?remote_name`, `push/<repo>`, `pull-repo/<remote>/<repo>`, `branches/<repo>`, `branch/<b>/<repo>`, `new-branch/<b>/<repo>` | Project creation from templates, plus a git porcelain over libgit2 |
| `/api/content-utils` | `templates`, `metadata-template/<t>`, `template-filenames/<t>`, `template/<t>/<f>`, `versifications`, `versification/<name>`, `product?resource_path` | Serves files from the app-resources tree (vrs JSON is read straight from `templates/content_templates/vrs/<name>.json`, `content_utils2/versification.rs:14-60`) |
| `/api/burrito` | `ingredient/raw` (GET/POST), `ingredients/raw` (GET), `ingredient/bytes` (GET/POST), `ingredient/delete`, `ingredients/delete`, `ingredient/copy`, `ingredient/revert`, `ingredient/zipped` (GET/POST), `metadata/raw`, `metadata/summary/<repo>`, `metadata/summaries?org`, `metadata/remake-ingredients/<repo>`, `paths/<repo>`, `paths-info/<repo>`, `audit/<repo>`, `zipped/<repo>` (GET/POST), `remake_burrito_from_zip/<temp_id>/<repo>` | File access confined to `<repo>/ingredients/`, metadata upkeep, zip import and export |
| `/api/video`, `/api/audio`, `/api/html`, `/api/system` | OBS video, audio compile, `ffmpeg-path`, framed client, `shutdown` | Product features |
| Static | `/api/webfonts`, `/api/app-resources`, one `FileServer` per client at its `url`, serving `<client>/build` | `src/utils/launch.rs:305-322` |

### 1.4 How burritos and state are stored on disk

- **Working dir.** The default is `~/pankosmia/<product short_name>/` (`src/lib.rs:112-119`); `argv[1]` overrides it (`src/lib.rs:122-126`). The README's "`pankosmia_working`" (`README.md:6`) is out of date. The working dir holds:
  - `user_settings.json`, `app_state.json` and `i18n.json`;
  - `client_settings/<client_id>.json`;
  - `temp/` (wiped at each start), `blobs/` (LLM models) and `webfonts/`;
  - see `src/utils/bootstrap.rs:20-85` and `src/utils/paths.rs:147-169`.
  - tC4's packaged working dir is `$HOME/pankosmia/tc4` (`docs/PACKAGING.md:86-93`).
- **Repo dir** is `repo_dir` in `user_settings.json` (`src/lib.rs:160-161`). The platform template's default is `%%HOMEDIR%%/pankosmia_repos`, which every Pankosmia app shares. tC4 patches it to `$HOME/pankosmia/tc4-projects` (`docs/PACKAGING.md:79-108`). The layout under it is always three levels, `<source>/<org>/<repo>`; the sanitizer requires at least three components (`src/utils/paths.rs:48-56`):
  - `git.door43.org/<org>/<repo>`: clones. The path is fixed, and the URL is always `https://<path>` (`git2/clone_repo.rs:60-70`).
  - `_local_/_local_/<abbr>`: app-created projects (`src/utils/files.rs:119-140`).
  - `_local_/_sideloaded_/<name>`: zip imports (`burrito2/post_zipped_repo.rs:83-96`).
  - `_local_/_quarantine_`, `_archive_`, `_updates_`: hidden from listings (`git2/list_local_repos.rs`, `burrito2/summary_metadatas.rs:53-57`).
- **Each project** is a git working tree:
  - `metadata.json`, `.gitignore` (from `content_templates/gitignore.txt`, `src/utils/burrito.rs:52-81`) and `ingredients/…`;
  - `ingredients/vrs.json`, copied from the chosen scheme (`src/utils/burrito.rs:16-50`);
  - `*.bak` siblings that act as a one-level undo.
  - Every read and write route works under `<repo>/ingredients/` only (`burrito2/raw_text_ingredient.rs:26-30`, `post_raw_ingredient.rs:43`).
- **App resources dir** (`APP_RESOURCES_DIR`, the packaged `lib/`) holds:
  - `templates/` (settings templates, `content_templates/<flavor>/metadata.json`, `vrs/*.json`);
  - `app_resources/lookups/bcp47-language_codes.json`;
  - `app_resources/schema/…`;
  - `product/product.json`;
  - `clients/<id>/{build/, pankosmia_metadata.json, package.json, storage_id.json}`.

### 1.5 The write path and metadata upkeep (data-safety detail)

- **Ingredient write** (`burrito2/post_raw_ingredient.rs`):
  - It holds the global `repo_dir` mutex for the whole handler (`:36`).
  - It renames the current file to `.bak` unless `no_bak` is present (`:61-74`). The doc-comment at `:19` says the opposite (PN #8).
  - It then calls `std::fs::write` (`:76`). There is no temp file, no fsync and no atomic rename. A crash between the rename and the write leaves only the `.bak`. A crash during the write leaves a truncated file.
  - grep finds no `sync_all`, `sync_data` or temp-and-rename publish anywhere in `src/`.
- **`update_ingredients`** (`:88-149`):
  - It parses `metadata.json` into a fixed struct.
  - It replaces the whole `ingredients` map with a fresh rescan (`:119-122`) and replaces `currentScope` (`:123-129`).
  - It writes the file back non-atomically (`:141`).
  - `remake-ingredients` does the same (`post_remake_ingredients_metadata.rs:43-90`).
- **Fidelity losses on every metadata rewrite:**
  - `BurritoMetadata` (`src/structs.rs:190-203`) has no `agencies`, `targetAreas` or `progress`, and no `#[serde(flatten)]`. The Scripture Burrito source schema allows all three (`/home/user/tc-rst/scripture-burrito/schema/source_metadata.schema.json`). They are dropped silently.
  - The rescan sets `role: None` (`src/utils/burrito.rs:311`), which wipes `x-` roles (PN #5).
  - The rescan rebuilds scope from the file name only, as `{BOOK: []}` (`src/utils/burrito.rs:281-287`). Chapter-level scope is lost. An OBS project's scope is emptied (PN #37).
  - Required fields that Scripture Burrito does not require (`languages`, and `mimeType` and `size` on each ingredient) make the parse fail on valid burritos. Until 0.19.2, `localizedNames` was one of them (PN #36; the 0.19.2 commit message is "Make localizedNames optional").
- **The rescan walks the whole repo, including `.git`.** `WalkDir::new(&repo_path)` has no `filter_entry` (`src/utils/burrito.rs:259`). It md5s every file that is not hidden, and it re-reads `eng.json` for each file named like a book (`:284`, `src/utils/bcv_ref.rs:4-29`).
  - `entry.unwrap()` (`:260`), `fs::metadata(..).unwrap()` (`:289`) and `File::open(..).unwrap()` (`:291`) panic when a file vanishes during the walk.
  - The panic happens under the `repo_dir` mutex, which poisons it. After that, every handler that locks it answers 500 until a restart (PN #38).
  - The crate has 570 `unwrap`/`expect` calls, 127 of them on `lock()`, and 40 `panic!` calls.
- **`add-and-commit`** (`git2/add_and_commit.rs`):
  - It holds no repo lock across the operation (`:41` clones the path and releases the lock), so it can race a concurrent write.
  - It rewrites `metadata.json` before every commit (`:69-148`): `meta.dateCreated`, a new random `revision`, `timestamp`, a full rescan, one compact line (PN #48).
  - Then it stages everything (`add_all(["."])`, `src/utils/git.rs:37`) and commits.
- **Regression at upstream HEAD (0.19.2) — verified.**
  - 0.19.2 moved the commit into `add_and_commit_repo`, which calls `repo.commit(Some("HEAD"), …, &tree, &[])` with **no parents** (`src/utils/git.rs:46-48`). 0.18.15 passed `&[&parent_commit]` (diff of the published 0.18.15 crate against 0.19.2).
  - libgit2 refuses a parentless commit onto an existing HEAD: `current tip is not the first parent` (`libgit2-sys-0.18.8+1.9.7/libgit2/src/libgit2/commit.c:113`).
  - A probe in the scratchpad (`commitprobe/`, git2 0.21.0) reproduced it: the first commit succeeds, and the second gives `Err(code -15, "failed to create commit: current tip is not the first parent")`.
  - So at 0.19.2, every checkpoint on an existing project hits `.expect("repo commit")`. That is a panic and an HTTP 500, after `metadata.json` has already been rewritten.
  - No test caught it (see §1.10). tC4 is not affected yet, because it pins 0.18.15.
- **Deletes:**
  - `POST /git/delete/<repo>` is `remove_dir_all` (`git2/delete_repo.rs:25`). It is permanent: there is no trash.
  - Deleting an ingredient renames it to `.bak` (`burrito2/post_delete_ingredient.rs:39`). A second delete or write replaces that `.bak`.
- **`remake_burrito_from_zip`** clears the working tree (keeping `.git`) and then copies (`burrito2/remake_burrito_from_zip.rs:192-207`). It is not atomic: a failure between the two steps leaves a half-emptied tree. The zip unpacker panics on several errors (`src/utils/zip.rs:50-86`).

### 1.6 Git sync with Door43

- **Clone.** Anonymous HTTPS only. The URL is `https://<repo_path>`, and the target path is fixed (`git2/clone_repo.rs:60-70`). The clone is shallow only when `?branch` is given (`:73-80`). The route is net-gated (`:30-32`).
- **Fetch and merge** (`git2/pull_repo.rs:77-184`):
  - It fetches from a named remote, or from an anonymous URL, with an **empty refspec** (`:97-102`). Fetch carries no credentials, so a private repo cannot be pulled.
  - It merges whatever `FETCH_HEAD` resolves to (`:122-130`), so the merged branch depends on ref ordering (PN #17).
  - Fast-forward does a forced checkout (`:32`).
  - A normal merge that has **conflicts** writes the conflicted index into the worktree with `checkout_index`, makes no merge commit and sets no `MERGE_HEAD`, and returns `has_conflicts: true` (`:47-50`). The next `add-and-commit` would commit the conflict markers as a single-parent commit.
  - A clean normal merge commits and then does a non-forced `checkout_head(None)` (`:68`), which leaves the worktree stale (PN #21).
  - The route has about 8 `expect`s (`:100-130`).
- **Push** (`git2/push.rs`) pushes the current HEAD branch to a named remote (`:121`):
  - With `cred_type: "https"` it uses `Cred::userpass_plaintext` with `expect` on the username and the key (`:83-86`). A missing field panics.
  - Any other `cred_type` value reads `~/.ssh/id_<cred_type>` (`:44-81`).
  - An unborn HEAD panics (`:110`). There is no `push_update_reference` callback, so a rejection by the remote server is not reported reliably (PN #46).
  - A non-fast-forward push answers 500 with the libgit2 code (PN #46, #49).
- **Other routes:**
  - `remote/add` accepts `file://` URLs (PN #33).
  - The branch switch refuses a dirty tree (`git2/set_branch.rs:48-80`).
  - `copy` copies and then deletes, so it is not one rename (PN #49).
  - No route merges two local branches, and no route creates a Door43 repository (PN #33, #40).
- **Author.** The commit author is `whoami::username()` with `@localhost`, set at creation and changed by no route (`src/utils/git.rs:19-28`, PN #47).
- **Door43 auth.** `gitea/login` redirects to a configured auth-proxy service (`gitea_endpoints` in `user_settings.json`), not to Gitea OAuth. The token returned through `settings/auth-token` lives in memory and in a cookie (`settings2/get_new_auth_token.rs:42-58`). `write_user_settings` does not persist it (`src/utils/files.rs:37-49`).

### 1.7 Validation

- **Create routes.**
  - They stamp `metadata.json` by string replacement of `%%CONTENT_NAME%%` and similar placeholders, with no escaping (`git2/new_text_translation.rs:180`, `new_obs_resource.rs:172`). A quote in a project name yields invalid JSON; tC4 refuses such names on the client side (`src/data/serverApi.ts:220-230`).
  - The language check is a BCP 47 regex, `x-` handling and a first-subtag lookup (`src/utils/burrito.rs:83-147`, PN #43).
  - At 0.19.2 the text route checks the language before `git init` (`new_text_translation.rs:72` before `:116`). Other refusals can still leave a half-made repo (PN #28).
- **Zip import.** The only checks are a file entry `metadata.json` and a directory entry `ingredients/`. They compare `format!("{:?}", path)` strings (`burrito2/post_zipped_repo.rs:24-50`) and use no JSON parse, schema or checksum (PN #29, #41).
- **Audit.** `GET /burrito/audit/<repo>` is the only schema validation (`burrito2/audit.rs:16-32`):
  - It runs basic shape checks, then `boon` against a schema path relative to the **process working directory** (`burrito_api/checks/metadata_validation.rs:7-18`), with `expect` on compile and read.
  - It fails any top-level entry other than `metadata.json` and `ingredients`.
  - It never runs on write, create or import. tC4 measured 500s from it on its own projects (PN #44).

### 1.8 Settings, i18n and client registration

- **Settings.**
  - `user_settings.json` holds languages, `repo_dir`, `app_resources_dir`, typography, `my_clients` and `gitea_endpoints` (`src/structs.rs:112-119`).
  - `app_state.json` holds bcv, current project, snippet and word.
  - Writes use `File::create` and then write, so they are not atomic (`src/utils/files.rs:37-61`).
  - The resource tree is selected twice: once by the launch variable and once by the saved `app_resources_dir`. This pinned old templates on pilot machines (PN #39).
- **i18n.**
  - The template `i18n.json` is merged with each client's `pankosmia_metadata.json` `i18n` (under `pages`) and with the product's `i18n-overrides.json`.
  - It is written once to `<working>/i18n.json` and not overwritten later (`src/utils/bootstrap.rs:400-504`).
  - The `negotiated`, `flat` and `untranslated` routes resolve keys against the user's language list. A `POST` replaces the file and bumps `I18N_UPDATE_COUNT` for the SSE stream (`i18n2/post_i18n.rs`).
- **Client registration.**
  - `app_setup.json` `clients[]` (and the user's `my_clients`) point to client directories that hold `pankosmia_metadata.json` (`id`, `require.net`, `i18n`, flavor `endpoints`, `minServerVersion`/`maxServerVersion`), `package.json` (`homepage` gives the URL `/clients/<id>`), `build/` and an optional `storage_id.json` (`src/utils/bootstrap.rs:291-398`).
  - A version mismatch panics (`:323-362`).
  - Each client is mounted as a `FileServer` (`src/utils/launch.rs:315-320`).

### 1.9 Desktop packaging

- The current `desktop-app-template` ships **Electronite** v37.1.0-graphite, unfoldingWord's Graphite-enabled Electron fork (PN #32, `docs/PACKAGING.md:318-327`). The upstream README still points to Tauri via Pithekos (`README.md:27-28`).
- **Process model:** Electronite runs the template's `electronStartup.js`, which spawns `bin/server.bin` (the shim) with `APP_RESOURCES_DIR=<pkg>/lib/` on the first free port from 19119. The window loads `/`, which redirects 303 to `/clients/<homepage>` (`docs/PACKAGING.md:241-311`).
- `Rocket.toml` must sit in the server's working dir to raise the upload limits to 128 MiB (`Rocket.toml:1-7`, PN #26). The server binds 127.0.0.1.
- The template has **no single-instance lock**: a second launch starts a second server over the same store. tC4 adds its own lock in `scripts/desktop-main.cjs:108` (`docs/PACKAGING.md:110-140`). tC4 also re-binds `APP_RESOURCES_DIR` and patches `user_settings.json` before launch (PN #39).
- **Security note.** The localhost API has no authentication and no Origin/CSRF check; grep finds no CORS or origin handling in `src/`. Several state-changing routes are body-less `POST`s on a predictable port, for example `git/delete` (`remove_dir_all`), `ingredient/delete` and `system/shutdown`. Protection relies on the browser's private-network rules.

### 1.10 License, code quality and tests

- **License:** MIT (`LICENSE`, `Cargo.toml:6`). Copying pieces is allowed with attribution.
- **Tests are effectively zero.**
  - `src/tests.rs` holds 5 tests copied from Rocket's static-files example. They call `rocket()` with no arguments, but the function takes `launch_config: Value` (`src/tests.rs:282` against `src/lib.rs:34`), so the test module does not compile. They also assert against `static/index.html` and `static/hidden/*`, which do not exist in the repo.
  - `test/curl/*.txt` holds manual curl recipes.
  - There is no CI config (no `.github/`), and no lockfile is committed.
  - The 0.19.2 commit regression above is what this produces.
- **Code quality:**
  - One handler per file, with duplicated walk logic (`list_local_repos.rs` and `summary_metadatas.rs`).
  - Paths are built by string concatenation with `os_slash_str()`. On Windows, zip entries are named with `\` (`src/utils/zip.rs:25-32`, PN #41).
  - Small bugs: `lemma` is read from `target` (`src/utils/launch.rs:285`); one shared SSE message queue is drained by whichever stream pops first (`sse.rs:35`); the `no_bak` doc-comment is inverted.
  - Large panic surface: 570 `unwrap`/`expect`, 40 `panic!` (§1.5).
- **Change rate.** 0.18.3 (2026-07-30) to 0.19.2 (2026-10-07), with behavior changes inside patch versions. One example: the `metadata.json` rewrite on commit arrived at 0.18.10 (PN #48). tC4 needed 49 PLATFORM-NOTES entries and a rig CI job to keep up (`docs/RISKS.md:9`, risk #1).

---

## 2. What tC4 actually uses (endpoint by endpoint)

tC4 reaches the server only through `src/data/serverApi.ts`; `test/noBypass.test.ts` enforces this. Route prefix: `/api`. The pin is 0.18.15 (`dev-env/README.md:7-11`).

| Route | `ServerApi` method (`src/data/serverApi.ts`) | tC4 use | Known gotcha (PN) |
|---|---|---|---|
| `GET /version` | `getVersion` :327 | Rig liveness probe, tests | — |
| `GET /git/list-local-repos` | `listLocalRepos` :332 | Name pre-check before create | Lists debris without metadata (#28) |
| `POST /git/delete/<repo>` | `deleteRepo` :341 | Only to remove its own failed create | Permanent `remove_dir_all` |
| `GET /burrito/metadata/summaries?org=_local_/_local_` | `getSummaries` :348 | Project list | Poisoned-mutex 500s (#38) |
| `GET /burrito/metadata/summary/<repo>` | `getSummary` :359 | Open a project | `generated_date` = last commit (#48) |
| `GET /burrito/metadata/raw/<repo>` | `getMetadataRaw` :375 | Flavor, scope, identification (14 call sites) | — |
| `GET /burrito/ingredient/raw/<repo>?ipath` | `readIngredient` :382 | USFM, sidecars, journal, `vrs.json` (22 call sites) | 400 with OS-specific ENOENT text for not-found (:187-200) |
| `GET /burrito/ingredient/zipped/<repo>?ipath=checking/journal` | `readIngredientZipped` :395 | Bulk journal read | `\` entry names on Windows (#41, `src/data/serverZip.ts`) |
| `GET /burrito/ingredient/bytes/…` (URL) | `ingredientBytesUrl` :405 | OBS images | — |
| `GET /burrito/paths/<repo>` | `listPaths` :417 | Existence and listing; hides `.bak` and dotfiles | — |
| `GET /git/status/<repo>` | `gitStatus` :426 | Checkpoint commits only when the tree is dirty | — |
| `POST /burrito/ingredient/raw/<repo>?ipath[&update_ingredients][&no_bak]` | `writeIngredient` :434 | Every write: USFM, sidecars, journal segments | Inverted `no_bak` (#8); role wipe (#5); OBS scope wipe (#37); panic and poison (#38) |
| `POST /burrito/ingredient/delete/<repo>?ipath` | `deleteIngredient` :464 | Delete (rename to `.bak`) | — |
| `POST /burrito/metadata/remake-ingredients/<repo>` | `remakeIngredients` :472 | Re-register after imports | Wipes roles (#5) |
| `POST /git/add-and-commit/<repo>` | `addAndCommit` :482 | Checkpoints | Rewrites `metadata.json` (#48); panics on zero commits (#20); **broken at 0.19.2** (§1.5) |
| `GET /git/remotes/<repo>` | `listRemotes` :494 | "Is this project shared?" (an `origin` remote) | — |
| `POST /git/remote/add/<repo>?remote_name&remote_url` | `addRemote` :506 | Share | A token in the URL would persist (#40) |
| `POST /git/push/<repo>` (body `{remote, cred_type:"https", username, pass_key}`) | `push` :525 | Share to Door43 | Panic on a missing credential (#40, #46); non-fast-forward is a 500 (#46, #49) |
| `POST /git/new-text-translation` | `newTextTranslation` :535 | Create a Bible project | String-templated metadata; `add_cv` null crashes |
| `POST /git/new-obs-resource` | `newObsResource` :580 | Create an OBS project | Template without `localizedNames` (#36) |
| `POST /git/new-scripture-book/<repo>` | `newScriptureBook` :601 | Add a book (does not commit) | No `\ts` markers (#19) |
| `GET /content-utils/versifications`, `/versification/<name>` | :606, :611 | Scheme list and data | — |
| `POST /app-state/current-project/<repo>` | `setCurrentProject` :618 | Tells the shell which project is open | — |
| `GET`/`POST /client-settings/<storage_id>` | :627, :634 | Machine-local UI state, install records | A `POST` replaces the whole document |
| `GET /net/status`, `POST /net/enable`, `POST /net/disable` | :646-661 | The user's offline switch | Starts offline (#45) |
| `POST /burrito/zipped/<_local_/_sideloaded_/x>` (multipart) | `postZippedBurrito` :671 | Resource install from DCS `/sb/<tag>.zip`, which tC4 downloads and verifies itself (`src/data/resourceFetch.ts:1-30`) | Must be unwrapped and need a directory entry (#26b, #29) |
| `POST /temp/bytes` + `POST /burrito/remake_burrito_from_zip/<uuid>/<repo>` | :708, :734 | Project import | Wrapped zips only; does not commit; non-atomic (#41) |
| `GET /gitea/remote-repos/<server>/<org>` | `remoteRepos` :743 | Resource catalog per gateway org | — |

**Also implicit:** static hosting of `dist/` at `/clients/uw-tc4`, the `/` redirect, and `GET /api/list-clients` (packaging smoke check, `docs/PACKAGING.md:300-304`).

**Not used:** SSE notifications, platform i18n (tC4 ships its own catalogs, `src/i18n/index.js:1-12`), `pankosmia-rcl` (D29), typography and webfonts, navigation and bcv, `pull-repo`, `clone-repo`, branches, `audit`, gitea login, LLM, audio and video. Door43 account calls and repository creation go **directly** to the Door43 API (`src/data/share/door43Api.ts`), and so do resource downloads.

**What tC4 builds on its own because the server lacks it.** A Rust core would have to own all of these:
- Compare-and-swap writes: an md5 recheck under a per-path in-process lock (`src/data/httpStore.ts:135`, `:394-411`, `:497-522`). The server has no conditional write.
- A crash-recovery **outbox in browser IndexedDB** (`src/data/journal/journalStore.ts:194-195`, `src/data/journal/identity.ts:65`). The server's writes are neither atomic nor durable.
- Immutable sealed journal segments (BURRITO-SPEC §8.1). The server cannot keep `x-` roles, so tC4 reads paths and never trusts the ingredients table (D28).
- The sync rule "`metadata.json` belongs to the server; a sync never writes it" (BURRITO-SPEC R-8.7.10), forced by PN #48.
- Its own import validator (D80 point 6, PN #44).
- Zip `\` normalization and traversal guards (`src/data/serverZip.ts:12-41`).
- A path sanitizer that mirrors the server's (`src/data/serverApi.ts:218-258`).
- Cleanup of debris from failed creates (PN #28).
- A single-instance lock and resource re-binding in the shell (`docs/PACKAGING.md:110-140`, `:241-260`).

**Platform defects and limits recorded in PLATFORM-NOTES** (`docs/PLATFORM-NOTES.md`, the `[PLATFORM]` entries):

| PN | Defect or limit |
|---|---|
| #5 | Rescans wipe `x-` roles, permanently |
| #6 | The path sanitizer forbids dot-prefixed segments and many characters |
| #7 | The write needs a string `payload` |
| #8 | `no_bak` is inverted; `.bak` is the only undo |
| #9 | Nothing auto-commits; commit sweeps the whole tree; branch operations refuse a dirty tree; a clean-tree commit still commits |
| #10 | Flavor-to-client routing is alphabetical |
| #11 | Schema bundle defects can panic the audit |
| #17 | `pull-repo` has no branch parameter; `FETCH_HEAD` ordering decides what merges |
| #19 | New books get no `\ts` markers |
| #20 | `add-and-commit` panics on a repo with zero commits |
| #21 | The worktree is stale after a normal merge; the next commit deletes merge-added files |
| #22, #26b, #29, #41 | Zip import and export shape rules; `.git`, `.bak` and `.DS_Store` in exports; Windows `\` entry names |
| #25 | Startup panics without a homepage client |
| #27, #28, #43 | Language-code handling; init before validate |
| #36 | The OBS template's metadata does not parse |
| #37 | Rescans empty an OBS project's `currentScope` |
| #38 | A vanished file or directory panics and poisons the server until restart |
| #39 | Two resource selectors pin stale templates |
| #40, #46, #49 | Push credential panics; anonymous-only fetch and clone; no repository creation; fixed clone path; the error contract |
| #44 | The audit is not a validator |
| #45 | Net gate and SSE semantics |
| #47 | The commit author is the OS user, unchangeable |
| #48 | Every commit rewrites `metadata.json`, so two clones always conflict on it |

---

## 3. Proskomma

**What it is.** `proskomma-core` 0.11.3 is "a Scripture Runtime Engine". It is about 19.6k lines of JavaScript, MIT licensed (`LICENSE`, © 2020 Mark Howe), with 68 tape test files under `test/code/main`. The last commit is 2025-12-01.
- **Input.** It parses USFM, USX, USJ and TSV (`src/parser/lexers/`, `src/parser/index.js`) into **docSets**. A docSet is a group of documents keyed by selectors, by default `lang` and `abbr` (`src/index.js:26-58`).
- **Succinct form.** Each docSet stores its documents in a compact binary form: per-docSet enums of token strings plus `ByteArray`-encoded blocks, items, scopes and grafts (`src/util/byteArray.js`, `src/util/succinct.js`, `src/model/doc_set.js`). The form serializes to and loads from "succinct JSON" for fast start-up (`src/index.js:502`, `:686`).
- **Queries and edits.** Everything is exposed through a **GraphQL** schema built on `graphql-js` (`src/index.js:21-24`, `src/graph/queries/*`, `src/graph/mutations/*`): chapter/verse navigation, blocks, items, tokens, key-value tables and versification. Mutations add, delete and update documents and tags and set verse mappings. Transforms to PERF and USFM live in `src/transforms`.
- **Versification** (`src/util/versification.js`, 381 lines):
  - `vrs2json` parses Paratext `.vrs` (`:128-152`).
  - `reverseVersification` (`:154-168`) "assumes each verse is only mapped from once".
  - `preSuccinctVerseMapping` and `succinctifyVerseMappings` build per-book, per-chapter byte tables (`:170-279`).
  - `mapVerse` (`:327-370`) maps one chapter:verse through a succinct chapter table, with `org` as the pivot.
  - Six schemes ship in `src/model/versifications/` (eng, lxx, org, rsc, rso, vul).

**Can Rust use it?** Not natively. There is no Rust port in the sources we have, and `pankosmia-web` itself has no USFM parser and no Proskomma integration (PN #41). The options:
1. **Keep it in the webview**, if the Rust app keeps a web UI (Tauri or similar). This is exactly how tC4 uses it today: a lazy dynamic import of ~917 kB minified (233 kB gzipped) (`src/data/mapReference.ts:6-18`, `:59-74`).
2. **Embed a JS engine** in the Rust core (QuickJS/rquickjs, Boa, deno_core). This is heavy and adds a second runtime for a few functions. Not recommended.
3. **Port the versification module.** It is small and pure: string parsing, map inversion, a byte-table encoder and a lookup. A direct Rust port is a few hundred lines. The succinct byte encoding is unnecessary in Rust: plain `HashMap` or `BTreeMap` tables do the same job.

**Do its concepts matter for us?**
- **Succinct docSets and GraphQL: no.** They optimize querying many loaded texts in memory inside a JS runtime. tC4's data model is per-book USFM bytes plus a journal of verse-slot events. A whole book is decomposed into a skeleton and verse slots (BURRITO-SPEC §8.4), and checking derivations run per book. A Rust core can parse one USFM book and index it in microseconds; it needs no compressed multi-docSet store and no query language. tC4 already uses `usfm-js`, not Proskomma, for USFM (PN #2, #4).
- **Versification mapping: yes, normative.** BURRITO-SPEC §5.2 makes frame mapping part of the identity of checking records. The mapping must be written once, and it must be pass-through for same-frame projects, because `eng → org → eng` loses 3 verses (`docs/BURRITO-SPEC.md:240-262`). The mechanism is pinned to Proskomma's `mapVerse` and succinct mapping semantics. A Rust port must reproduce these exactly, including the quirks tC4 already guards:
  - the string-form `mappedVerses` corruption in `reverseVersification` (`src/data/versification.ts:114-126`);
  - many-to-many results;
  - letter-suffixed target verses, which `parseInt` truncates (`versification.js:138` regex against `:189-209`).
- **Recommended approach:** port the mapper, generate golden vectors by running `proskomma-core` 0.11.3 over all six schemes (every chapter and verse, both directions), and make those vectors a Rust conformance test. That is one more case of tC4's existing "executable proof" discipline.

---

## 4. Recommendation

### Options compared

| Criterion | A. Depend on `pankosmia-web` (crate or sidecar binary) | B. Fork pieces of `pankosmia-web` | C. Own storage and sync core (Rust library) |
|---|---|---|---|
| **Data safety** (never lose work) | Non-atomic, un-fsynced writes; one-level `.bak`; permanent deletes; panics poison the process (PN #38); upstream HEAD cannot commit (§1.5). tC4 compensates in the client with md5 CAS, path locks and an IndexedDB outbox. | The fork inherits the same code. Fixing it means rewriting the write path, the rescan and the commit, which is most of what we would use. | Temp file, fsync, rename on every publish; immutable journal segments; commit under a repo lock; typed errors instead of panics; crash recovery inside the core, not in browser storage. |
| **Multi-user offline merge** | The only merge is `pull-repo`: `FETCH_HEAD` ordering, conflict markers written to the worktree with no `MERGE_HEAD`, a stale worktree, anonymous fetch, and `metadata.json` conflicts on every commit (PN #17, #21, #48). tC4's D98 design deliberately avoids all of it. | Same. | Implements BURRITO-SPEC §8.7 directly: fetch Door43 `main`, read trees **from the commit** (never the worktree), union the actor-disjoint journal files, fold, write derived files, commit with explicit parents, push fast-forward only, retry from the new tip on a non-fast-forward. Needs fetch with credentials, which the server lacks. |
| **Burrito validity** | Drops `agencies`, `targetAreas` and `progress`; wipes roles; coarse or empty scope; string-templated metadata; the audit is not usable (PN #44). tC4 had to rule "metadata.json belongs to the server" (R-8.7.10). | Fixable, but this is the core of the fork. | A serde model that keeps unknown fields (`flatten`); roles and scope owned by the fold (R-8.7.1-2); validation against the SB 1.0 schemas (`/home/user/tc-rst/scripture-burrito/schema/`) with `boon` or `jsonschema`, on write and on import. |
| **Maintenance** | 348-crate tree (LLM `rten`, ffmpeg) for ~30 routes; no tests or CI upstream; behavior changes in patch releases; tC4 already maintains 49 platform notes plus a rig. A pin means we never receive fixes; tracking HEAD means we receive regressions. | We own 12.8k lines with 570 `unwrap`s, built for a multi-client product shape we do not need (SSE, i18n, typography, navigation state, LLM, video). | We own maybe 2-4k lines for exactly our surface (§2), designed around the journal. It can be unit- and property-tested in-process with no HTTP rig. The Door43, language and template data stay shared. |
| **Ecosystem compatibility** | Highest: tC4 stays a Pankosmia client. | High. | Keep **format** compatibility instead of code compatibility: same repo layout and same Scripture Burrito metadata, so Pankosmia apps and Door43 still read our repos (RISKS #8). |

### Verdict: option C, and borrow pieces deliberately

1. **Write a `tc4-store` library crate** with a typed API, not HTTP. Its surface mirrors `BurritoStore` (`src/data/burritoStore.ts`, ARCHITECTURE §3.2): list and open projects, read and write ingredients with CAS, publish journal segments, checkpoint, import and export burritos, share and sync. If the UI stays a webview, expose the API through in-process IPC (for example Tauri commands), not a localhost port. That removes the unauthenticated-localhost exposure (§1.9) and the free-port and second-server hazard (`docs/PACKAGING.md:110-125`).
2. **Choose a git library.** `git2` matches today's server behavior and has mature HTTPS push. The cost is libgit2 plus OpenSSL builds, which tC4 already vendors on macOS (`dev-env/server/Cargo.toml`). `gix` (gitoxide) is pure Rust and strong for clone, fetch and object reads, but **check its push support before choosing it**, because push has historically lagged. The core needs only:
   - clone and fetch, with credentials;
   - reading trees and blobs at a commit;
   - writing a commit with explicit parents;
   - fast-forward-only push with libgit2 error codes mapped to typed refusals (as tC4 already does, PN #49);
   - remote management;
   - status.
   It needs no merge engine, because merging is the journal fold.
3. **Copy from `pankosmia-web`** (MIT; keep attribution):
   - the BCP 47 regex and the language-name derivation (`src/utils/burrito.rs:83-147`), with `bcp47-language_codes.json` from `resource-core`;
   - the forbidden path-segment list (`src/utils/paths.rs:19-46`), to stay compatible with burritos that Pankosmia writes;
   - the field mapping of the Door43 catalog record (`gitea2/gitea_remote_repos.rs:47-140`), adding paging and non-panicking parsing;
   - the template **data** (`content_templates/*/metadata.json`, `vrs/*.json`, `gitignore.txt` from `resource-core`). Fill the templates through serde, not string replacement.
   - Do not copy the `.bak` model, the rescan, the commit, the merge or the zip helpers.
4. **Keep on-disk compatibility for migration.** A tC4 4.x project today is a git repo at `$HOME/pankosmia/tc4-projects/_local_/_local_/<abbr>` (`docs/PACKAGING.md:79-93`), laid out as in BURRITO-SPEC §2. The Rust core should open these in place, or import them, and keep producing that layout. Then a user's existing work, journals and Door43 shares carry over. Treat the `metadata.json` changes the server makes at commit time (dateCreated, revision, sorted compact JSON, PN #48) as **legacy input**, not behavior to imitate. Decide our own commit-time metadata rule once (open question for D98's successor).
5. **Proskomma:** port `mapVerse` and the mapping-table construction to Rust, with golden vectors generated from `proskomma-core` 0.11.3. Do not adopt succinct docSets or GraphQL.

### What we give up with option C, and how to bound it

- **We stop being a Pankosmia "client".** No shared dashboard or settings clients, and no shared store. tC4 already isolates its store, ships one client, and bypasses platform i18n and `pankosmia-rcl` (PACKAGING #70; D29; §2 "Not used"). In practice it uses Pankosmia as a file and git service.
- **We must own Door43 transport and catalog code.** Most of it already lives outside the server (`src/data/share/door43Api.ts`, `src/data/resourceFetch.ts`); only git push and fetch move into Rust.
- **Upstream fixes stop reaching us.** Given §1.10, this is a smaller loss than upstream regressions reaching us. Re-check the five borrowed items at each review, using the PLATFORM-NOTES freshness discipline.

---

## Evidence produced in this research (scratchpad only)

- The published `pankosmia_web-0.18.15.crate`, unpacked; its `.cargo_vcs_info.json` sha is `a83725b…`. `diff -rq` against 0.19.2 shows that `add_and_commit.rs`, `structs.rs`, `utils/burrito.rs`, the six create routes and the summaries changed, and that `utils/git.rs` was added.
- `commitprobe/`: a 20-line Rust program, built offline with git2 0.21.0. It mirrors `src/utils/git.rs:32-48` and proves that the second parentless commit onto HEAD fails with libgit2 error -15, "current tip is not the first parent".
