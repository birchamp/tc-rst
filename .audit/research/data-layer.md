# tC4 data layer — inventory for a Rust rebuild

Source: `/home/user/unfoldingword/translationcore4` @ `d494a9d` (main). Scope: `src/data/**`,
`src/state.jsx`, the reference journal package `journal/*.mjs` (imported verbatim by
`src/data/journal/runtime.ts`, so it IS production code), and the unit tests in `test/`.
Paths below are repo-relative. "ipath" = path relative to a project's `ingredients/` directory.
The normative format spec is `docs/BURRITO-SPEC.md` (cited as §n); this report describes what
the code does and cites the spec only where the code defers to it.

## 0. Architecture in one page

```
React UI (src/views, src/state.jsx)
   │  every mutation
   ▼
JournalingStore  (src/data/journal/journalingStore.ts)      ← the ONE canonical write boundary
   │  1. append intent-ledger record (IndexedDB kv)
   │  2. seal + stage action in outbox (IndexedDB kv)
   │  3. publish sealed segment file (HTTP write, write-once)
   │  4. fold all events (journal/fold.mjs, in a Web Worker)
   │  5. regenerate derived files from the fold + byte-verify readback
   │  6. prune converged ledger records
   ├── JournalStore (journal/journalStore.ts): segment files, actor.json, HLC, outbox
   ├── HttpStore (src/data/httpStore.ts): raw read/write of ingredients, md5 CAS, path locks
   └── ServerApi (src/data/serverApi.ts): typed fetch over the Pankosmia HTTP server
                     │
                     ▼
        pankosmia-web (Rust server, localhost) — owns the git repos on disk:
        <root>/_local_/_local_/<abbr>/  (projects)    <root>/_local_/_sideloaded_/<name>/ (resources)
```

Key facts a Rust implementer must internalize:

1. **The journal is canonical; project files are derived.** Every edit is first an immutable
   journal "action segment" under `ingredients/checking/journal/<actorId>/segments/`; the USFM
   and the JSON sidecars are regenerated from a deterministic fold of all journal events
   (`journalingStore.ts:1-18`, spec §8). The fold implementation is `journal/fold.mjs` (1063 lines)
   and must be ported byte-exactly (determinism R-8.6.1; sync R-8.7.11 requires byte-identical
   derived files across machines).
2. **The client never touches the filesystem directly.** All IO is HTTP to the local Pankosmia
   server (`serverApi.ts`). A Rust rebuild that embeds storage can drop the HTTP hop, but must
   preserve the server's semantics that tC4 depends on (`.bak` on write, delete = rename to
   `.bak`, `add-and-commit` sweeping everything and rewriting `metadata.json`, rescan wiping
   `x-` roles, etc. — §1.2 below).
3. **Installation-local state lives in IndexedDB** (`tc4-installation` DB, object store `kv`):
   the installation secret, the outbox of staged sealed segments, and the intent ledger
   (`identity.ts:41-138`). The Rust equivalent needs an atomic, durable KV with
   `setIfAbsent` semantics (decide-and-write in one transaction; resolve only after commit).
4. **Per-machine UI/ops state** lives in the server's client-settings store
   (`GET/POST /client-settings/<storage_id>`), e.g. the ops log and last-used ordering.
5. **External libraries that carry semantics** (must be replicated or wrapped): `usfm-js`
   (USFM ⇄ JSON, plain-text extraction for I-3 hashing), `word-aligner-lib`
   (`addAlignmentsToTargetVerseUsingMerge`, unmerge; zaln weave), `wordmap`/`wordmap-lexer`
   (alignment suggestions, tokenization), `proskomma-core` (versification mapping, lazily
   loaded), `fflate` (zip), `ajv` (Burrito schema validation in burritoCheck). Details in §12.

---

## 1. Server transport — `src/data/serverApi.ts` (758 lines)

Typed fetch wrappers over pankosmia-web (verified against 0.18.5/0.18.15). Base URL `/api`
(dev proxy / same-origin) — `serverApi.ts:17-19,287-291`.

### 1.1 Endpoints the client calls (complete list)

| Method | Route | Wrapper (line) | Purpose / contract |
|---|---|---|---|
| GET | `/version` | `getVersion` 327 | `{pkg_version}`; liveness probe |
| GET | `/git/list-local-repos` | `listLocalRepos` 332 | `string[]` of repo paths `_local_/_local_/<abbr>` |
| POST | `/git/delete/<repo>` | `deleteRepo` 341 | only to clean up debris of OUR failed create (server git-inits before validating, PLATFORM-NOTES #28) |
| GET | `/burrito/metadata/summaries[?org=<o/o>]` | `getSummaries` 348 | `{<repoPath>: RepoSummary}`; `org=_local_/_local_` filters server-side |
| GET | `/burrito/metadata/summary/<repo>` | `getSummary` 359 | one `RepoSummary` |
| GET | `/burrito/zipped/<repo>` | `getZippedRepo` 367 | whole repo dir as an UNWRAPPED zip incl. `.git/`, `*.bak`, `.DS_Store` |
| GET | `/burrito/metadata/raw/<repo>` | `getMetadataRaw` 375 | full `metadata.json` |
| GET | `/burrito/ingredient/raw/<repo>?ipath=<p>` | `readIngredient` 382 | exact file text; missing → HTTP 400 with ENOENT text (see `isNotFound`) |
| GET | `/burrito/ingredient/zipped/<repo>?ipath=<dir>` | `readIngredientZipped` 395 | one ingredient DIRECTORY as zip (no metadata.json); on Windows entry names use `\` (#425) |
| (URL) | `/burrito/ingredient/bytes/<repo>?ipath=<p>` | `ingredientBytesUrl` 405 | binary ingredient URL (OBS images) |
| GET | `/burrito/paths/<repo>` | `listPaths` 417 | every FILE under `ingredients/` (walks disk, not the metadata table); skips hidden files/dirs and `.bak` |
| GET | `/git/status/<repo>` | `gitStatus` 426 | `[{path, change_type}]`; empty = clean tree |
| POST | `/burrito/ingredient/raw/<repo>?ipath=…[&update_ingredients][&no_bak]` body `{"payload": "<string>"}` | `writeIngredient` 434 | the only write; flags are PRESENCE-ONLY (never `=true`); `no_bak` presence SKIPS the `.bak` backup (inverse of the server's doc comment) |
| POST | `/burrito/ingredient/delete/<repo>?ipath=…` | `deleteIngredient` 464 | server RENAMES to `<name>.bak` (the undo) |
| POST | `/burrito/metadata/remake-ingredients/<repo>` | `remakeIngredients` 472 | full rescan; wipes every `x-` role |
| POST | `/git/add-and-commit/<repo>` body `{commit_message}` | `addAndCommit` 482 | sweeps ALL pending changes; rewrites `metadata.json` (`meta.dateCreated`, revision, timestamp, ingredient remake); commits even when clean → caller checks `gitStatus` first |
| GET | `/git/remotes/<repo>` | `listRemotes` 494 | `{payload:{remotes:[{name,url}]}}`; an `origin` is THE record that a project was shared |
| POST | `/git/remote/add/<repo>?remote_name&remote_url` | `addRemote` 506 | name `[A-Za-z0-9_-]+`; url must carry a transport |
| POST | `/git/push/<repo>` body `{remote, cred_type:"https", username, pass_key}` | `push` 525 | creds in body; server panics if missing → client refuses empty; 401 "offline mode" when net disabled |
| POST | `/git/new-text-translation` | `newTextTranslation` 535 | create Bible project (see payload below); writes `vrs.json`, scaffolds book, initial commit |
| POST | `/git/new-obs-resource` | `newObsResource` 580 | create OBS project from `text_stories` template; initial commit |
| POST | `/git/new-scripture-book/<repo>` | `newScriptureBook` 601 | scaffold book + regenerate metadata; does NOT commit |
| GET | `/content-utils/versifications` | `getVersifications` 606 | six scheme names (`eng,lxx,org,rsc,rso,vul`) |
| GET | `/content-utils/versification/<name>` | `getVersification` 611 | `{maxVerses, mappedVerses, excludedVerses, partialVerses}` |
| POST | `/app-state/current-project/<repo>` | `setCurrentProject` 618 | tells the shell which project is open |
| GET/POST | `/client-settings/<storage_id>` body `{settings:{...}}` | `get/setClientSettings` 627-637 | per-machine JSON object store |
| GET | `/net/status` → `{is_enabled}` | `getNetEnabled` 646 | platform net gate (boots disabled) |
| POST | `/net/enable`, `/net/disable` | 654-661 | user-initiated only |
| POST | `/burrito/zipped/<_local_/_sideloaded_/name>` multipart `file` | `postZippedBurrito` 671 | general burrito import: target must be `_sideloaded_`, must not exist, zip must be UNWRAPPED |
| POST | `/temp/bytes` multipart `file` → `{uuid}` | `uploadTempBytes` 708 | stage an upload; uuid validated `/^[0-9a-f-]{36}$/` |
| POST | `/burrito/remake_burrito_from_zip/<uuid>/<repo>` | `remakeBurritoFromZip` 734 | replace an EXISTING repo's contents keeping `.git`; zip MUST be WRAPPED (one top folder); strips `.git*`; does NOT commit |
| GET | `/gitea/remote-repos/<server>/<org>` | `remoteRepos` 743 | DCS catalog per org (`RemoteRepo[]` with `book_codes`, `topics`, `latest_zip`…); `{is_good:false}` when offline |

POST success shape is `{"is_good": true, "reason": "ok"}`; `post()` throws `ServerApiError` on
`!ok` or `is_good:false` (`serverApi.ts:308-322`). `ServerApiError{route,status,reason}`;
status 0 = refused client-side, never sent (`174-201`).

**Not-found detection** (`isNotFound`, `195-200`): status 400|404 AND reason matches
`/No such file or directory|cannot find the (?:file|path) specified|\(os error [23]\)/i`.
`isNotFoundError` (`208`) is THE predicate every "absent → designed state" catch must use (D30).

**Path sanitization before any request** (`218-264`): a segment may not be empty, start with
`.`, or contain `[\s&?:#%\\*"'<>|]`; repo paths have exactly 3 segments; `content_name` for
creation must not contain `"`, `\` or control chars (server splices it into JSON unescaped,
`225-230`). Segments are `encodeURIComponent`-joined.

**Creation payloads** (`535-597`): `new-text-translation` sends
`{content_name, content_abbr, content_language_code, content_language_name|null, add_book,
book_code|'', book_title|'', book_abbr|'', add_cv (MUST be a real bool when add_book), versification,
branch_name:null}`; `x-` language codes REQUIRE `content_language_name`. `new-obs-resource` sends
`{content_name, content_abbr, content_language_code, content_language_name|null, branch_name:null}`.

### 1.2 Server behaviors the data layer relies on (replicate if Rust owns storage)

- Write creates missing dirs; whole-file only, unconditional (no CAS) — W-1/W-5.
- `.bak`: a write without `no_bak` keeps a single-level `<file>.bak`; `.bak` is git-ignored and hidden
  from `/burrito/paths`. Delete renames to `.bak`. These are the only "undo".
- `update_ingredients` registers the file and refreshes md5 via a FULL rescan which drops every
  `x-` role (accepted, D28). Rescan also rebuilds `currentScope` from USFM files — which EMPTIES an
  OBS project's scope table (PLATFORM-NOTES #37), so the store never registers/rescans on OBS
  (`journalingStore.ts:443-450,1442`).
- `add-and-commit` commits even with nothing pending and rewrites `metadata.json` every time.
- No HTTP route can write `metadata.json` (D28) → `project.meta.set` overlays refuse at checkpoint.

## 2. `src/data/dcsServer.ts` (35 lines)

`DCS_SERVER` = `https://qa.door43.org` when `import.meta.env.DEV === true`, else
`https://git.door43.org` (`dcsServer.ts:14-20`). Used ONLY for account/write calls (sign-in,
repo create, push). All reads (pins, catalog, downloads, image packs) use production
(`DCS_HOST` in `gateways.ts`). `locationOf` (`28-35`) prefixes the host label to
`owner/repo` on a non-production server.

## 3. `src/data/burritoStore.ts` (414 lines) — the interface of record and core types

`BurritoStore` (`262-414`) is the persistence interface; the only production implementation is
`JournalingStore`. `test/noBypass.test.ts` forbids app code from calling `HttpStore` mutations.

Operations: `listProjects, open, createProject, addBook, createObsProject, readBook, writeBook,
applyStructuralEdit, listStories, readStory, writeFrame, writeTitle, writeRef, readAlignments,
writeAlignments(book, data, expectMd5?), readDecisions, upsertDecision(tool, book, decision,
resource?), writeDecisions(tool, book, file, expectMd5?) → md5, readResources, writeResources(r,
expectMd5?), readSettings, readSettingsWithMd5, writeSettings(s, expectMd5?), readVersification,
writeProjectMeta, applyGatewayChange(plan), commit(message), commitPending(messageFor), readZipped`.

### 3.1 Types (exact field names)

```ts
ProjectSummary { id /*repoPath*/, name, languageTag, scriptDirection /*may be "?"*/,
  flavor: 'textTranslation'|'textStories', bookCodes /*canon order; [] for OBS*/, timestamp? }

DecisionContextId { checkId /*TSV ID*/, occurrenceNote /*tN note text*/,
  reference: {bookId /*lowercase*/, chapter:number, verse:number|"9-10"} | {story, frame},
  tool, groupId, quote: string | [{word, occurrence}], quoteString, glQuote, occurrence:number }

Decision { contextId, category?, selections: [{text, occurrence, occurrences}] | false,
  comments: string|false, reminders: boolean /*bookmark*/, nothingToSelect, verseEdits,
  invalidated, modifiedTimestamp? /*ISO, REQUIRED on write*/, status?: 'valid'|'invalid'|'todo',
  userInvalid? }

DecisionFile { schemaVersion:1, tool, book /*UPPER or "OBS"*/,
  resource?: {repoPath, version?, sha?, languageSet?: 'primary'|'fallback'}, decisions: Decision[] }

ResourcePin { repoPath /*"git.door43.org/<owner>/<repo>"*/, version? /*display only*/, flavor,
  sha /*40 hex, IDENTITY*/, books? /*UPPER codes or ["BIBLE"]*/ }

LanguageSet { gatewayLanguage:{languageId, owner}, translationNotes, translationWordsLinks,
  translationWords, translationAcademy, translationQuestions?, simplifiedText?,
  obs?, 'obs-tn'?, 'obs-twl'?, 'obs-tq'?, 'obs-images'? }

ResourcesFile { schemaVersion:2, languageSets:{primary, fallback}, resources: {originalLanguage:{nt,ot},
  lexicon:{nt,ot}, ...}, extraScripture?: [{id, ...ResourcePin}] }

SettingsFile { schemaVersion:1, textDirection?: 'ltr'|'rtl', textFont?: string|null,
  languageName?: string|null, [open keys] }   // checkingLanguage obsolete, preserved

Story { number, title, frames: [{image, text}] /*frames[0] = frame 1*/, ref: string|null }

GatewayChangePlan { resources, resourcesMd5, decisions: [{tool, book, file, expectMd5}],
  alignments?: [{book, file, expectMd5}] }
```

Completeness predicates (`125-129`): a Bible set needs tn+twl+tw+ta; an OBS set needs
obs+obs-tn+obs-twl+tw+ta. `LADDER = ['primary','fallback']` (`143`).

Full on-disk JSON examples: spec §5.1–§5.4 (alignment, decisions, resources, settings) and
§10.2–§10.5 (OBS). The Rust types must preserve unknown keys in settings (open) and must
round-trip decision records exactly (full tC3 shape — never simplify).

## 4. `src/data/httpStore.ts` (756 lines) — raw store over the server

Not a BurritoStore since #62; JournalingStore drives it. Important pieces:

- **md5** (`51-114`): self-contained RFC 1321 over UTF-8 bytes, lowercase hex. Used for CAS,
  I-3 (`targetVerseMd5`), divergence reporting. Rust: any md5 crate over UTF-8 bytes.
- **`withPathLock(key, fn)`** (`134-141`): a process-wide promise chain per key
  `"<repo> <ipath>"`; every read-check-write (sidecar CAS, book CAS, journal segment publish,
  actor.json provisioning) runs under it so check→write can't interleave in-process (B7/B13).
  Rust: a `Mutex` per (repo, ipath) or a single-writer actor. Cross-process safety relies on
  "one app instance per machine" (D39).
- **`StaleWriteError{book|ipath, expectedMd5, actualMd5}`** (`145-159`): CAS failure.
  `expectMd5 === null` means "file must still be absent".
- ipaths (`180-187`): `<BOOK>.usfm`, `checking/alignments/<BOOK>.json`,
  `checking/<tool>/<BOOK>.json`, `checking/resources.json`, `checking/settings.json`, `vrs.json`.
- `listProjects` (`348-354`): summaries for org `_local_/_local_`, flavor ∈
  {textTranslation, textStories}, sorted by `Date.parse(generated_date)` desc
  (`generated_date` = `meta.dateCreated` = last commit time; `timestamp` is scan time — useless).
- `open` (`356-362`): getSummary + POST current-project.
- `readBook` (`386-389`): text + md5 of exact bytes read (metadata md5 may lag).
- `writeBook` (`394-411`): under path lock; optional md5 precheck; write with
  `update_ingredients` + keep `.bak`.
- **Occurrence normalization I-2** (`213-243, 264-309`): `normalizeAlignmentFile` coerces every
  `occurrence/occurrences` to integers; `normalizeDecision`: integer-coerce
  `contextId.occurrence`, each tN quote word occurrence, each selection's
  occurrence/occurrences (strings `/^-?\d+$/` accepted, anything else throws `RangeError`);
  `selections: []` → `false`; `invalidated && status==='valid'` → `status:'invalid'`;
  missing `modifiedTimestamp` → now ISO.
- **`identityKey`** (`249-257`): `[checkId, bookId.lower | "obs", String(chapter|story),
  String(verse|frame), String(occurrence)].join('\u0000')` — never `Number()` a verse (spans).
- Sidecar write (`502-522`): `JSON.stringify(data, null, 2)` (NO trailing newline here; the
  journaling store's derived writes use `serialize` = stringify + `\n`), under lock, optional CAS,
  returns md5 of exactly the bytes written (B15).
- `upsertDecision` (raw, pre-journal semantics, `619-668`): match = identity key AND equal
  `quoteString`; a key match with different quoteString is co-present (appended), never
  overwritten; resolution record stamped only if absent or agreeing (repoPath+sha).
- `readVersification` (`723-731`): returns `{name: UNRECORDED_SCHEME, bytes}` (the server never
  records the scheme name).
- `commitPending` (`742-755`): serialize status-read + commit on one chain; returns null when
  `messageFor` returns null (clean tree).

