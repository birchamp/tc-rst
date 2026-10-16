# What makes a Scripture Burrito valid, and how the Rust rebuild can check it

Date: 2026-10-14. Sources (read-only):

- The spec repo `/home/user/ref/scripture-burrito`: branch `develop`, shallow clone at `714f86a` (2026-09-17). It has no tags and is marked "Status: Working Draft".
- tC4 `conformance/`: `sb-schema/`, `validate.mjs`, `validate-roundtrip.mjs`, and the two samples.
- tC4 `src/data/import/burritoCheck.mjs`.
- tC4 `docs/BURRITO-SPEC.md` v1.23, §1–3, §5, §6, §7, §8.7 and §10.1.
- pankosmia-web `src/utils/burrito.rs`, which holds the server's ingredient rescan.

The probe project is `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/burrito-probe`. Its output is in `probe-output.txt` in that folder. The ajv cross-check is in `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/ajv-check`.

---

## 0. Findings

1. **There are two schema lines, and they disagree on OBS.**
   - **The v1.0.0 line.** tC4's `conformance/sb-schema/` is a copy of pankosmia `resource-core`. Apart from `$id`s and descriptions, it is semantically identical to the spec repo's committed `out/` build. The only extra is a duplicate `scripture/common.schema.json` (checked with a script, by `$id`). This line contains `gloss/textStories`, `typesetScripture`, `embossedBrailleScripture`, `signLanguageVideoTranslation` and `parascriptural/wordAlignment`.
   - **The spec repo `develop` line** (`schema/`). This is a post-1.0 working draft. It removes `gloss/textStories` (a `gloss` burrito now needs an `x-` flavor), adds an `alignment` flavor type, wrapper burritos and ingredient `properties`, and makes `languages` actually required.
   - Both lines pin `meta.version` to `"1.0.0"`.
2. **Probe result.** Rust `jsonschema` 0.58.6 works offline, with network features compiled out.
   - Against tC4's bundle, **both tC4 samples are VALID**: `sample-burrito` and `sample-burrito-obs`. Ingredient integrity also passes: md5, size and exact path set, with 8 and 56 ingredients.
   - Against `develop`, the Bible sample is VALID and the OBS sample is **INVALID**, because `gloss/textStories` does not match the `oneOf` at `/type/flavorType`.
   - Cost: compile is 10–100 ms, and one validation takes about 0.1–0.4 ms.
3. **The Rust verdicts match ajv.** The probe dumps 48 positive and negative control documents. ajv, running tC4's own `compileSbValidator`, gives the same verdict on all 48.
4. **The 1.0.0 schema has gaps that need code checks.**
   - It never requires `languages` (bug #343, fixed on develop).
   - It never checks the `identification.primary` `revision` and `timestamp` values.
   - It does not check the `idAuthorities` label.
   - Its path pattern allows `..`, `\` and spaces.
   - It allows a missing `checksum`.
   - It cannot see whether a checksum matches the file.
5. **The develop repo is broken at this commit.** Its own `code/validate.js` cannot compile the metadata schema. `schema/index.js` omits `alignment/alignment.schema.json`, which `type.schema.json` references. The error is `can't resolve reference alignment/alignment.schema.json`. With the missing schema added, 4 of its own 8 example artifacts still fail. **Do not use develop as the hard gate.**

---

## 1. What "valid burrito" means

### 1.1 Which schema to target

**Hard gate: the SB v1.0.0 schema line.** In practice that is tC4's `conformance/sb-schema/`, the same as the spec's `out/`. Validate from the root `metadata.schema.json`, which dispatches on `meta.category`, using draft-07 with `format` asserted. Reasons:

- All existing tools (Pankosmia and tC4) validate against this line.
- It is the only line that accepts the OBS flavor `gloss/textStories`.
- `meta.version` `"1.0.0"` names this line.

**Advisory: develop.** For a Bible project, also validate against develop. tC4's Bible sample passes both lines. An OBS project cannot pass develop while it keeps `gloss/textStories`. `gloss/x-textStories` would pass both lines, but it breaks Pankosmia/tC4 interop and tC4's rule R-10.1.1. Keep `textStories` and track develop.

tC4's harness compiles `source_metadata.schema.json` directly instead of `metadata.schema.json`. That is equivalent for `category: "source"`, which is the only category tC4 writes.

### 1.2 Required fields: `source` category, v1.0.0

Top-level `required` is `format`, `meta`, `idAuthorities`, `identification`, `confidential`, `type`, `copyright` and `ingredients`. `additionalProperties: false` applies, so unknown top-level keys such as `x-tc4` are rejected. Optional keys are `languages`, `relationships`, `targetAreas`, `agencies`, `localizedNames` and `progress`.

| Field | Rule (schema unless noted) |
|---|---|
| `format` | `"scripture burrito"` |
| `meta.version` | enum `["1.0.0"]` |
| `meta.category` | `"source"` |
| `meta.dateCreated` | `format: date-time`, RFC 3339 **with a timezone**. `2026-07-02T12:00:00` (no zone) fails in both Rust and ajv-formats. `...Z` and `+01:00` pass. |
| `meta.defaultLocale` | required, BCP-47 pattern |
| `meta.normalization` | optional, enum NFC/NFD/NFKC/NFKD (case-sensitive). tC4 writes `NFC`. Declaring NFC does not make the content NFC; the writer must normalize. |
| `meta.generator` | optional: `{softwareName, softwareVersion}` required, `userId` and `userName` optional, nothing else |
| `idAuthorities` | at least 1 entry, each `{id: url, name?: localizedText}`. The label pattern `^[a-z][a-z0-9\-]*[a-z0-9]$` is mis-nested and **not enforced** on keys (`"Local"` passes). Develop prose says custom labels MUST start with `x-`, but tC4 and Pankosmia use `local` and `dcs`. |
| `identification.name` | required localizedText: at least 1 entry, keys are BCP-47 tags, values **trimmed** (`" padded "` fails) |
| `identification.primary` | required, exactly 1 authority key → `{ <id>: {revision, timestamp} }`. `revision` and `timestamp` are required, but their patterns are **not enforced** (schema bug: they sit beside `properties`, not inside it). The intended `timestamp` pattern is date-only (`YYYY[-MM[-DD]]`). tC4 and the spec's own examples write full ISO datetimes. Write RFC 3339 and check it in code. |
| `confidential` | boolean |
| `languages` | array of at least 1 `{tag, name, scriptDirection?, rod?, numberingSystem?}`. **Not required by the v1.0.0 schema** (bug #343). Develop and the prose require it for `scripture`. Always write it. |
| `copyright` | **`oneOf`: exactly one of** `{publicDomain: true}`, `shortStatements[]` or `licenses[]`. Both statements and licenses together fail, and so do `publicDomain: true` plus statements. The develop prose says "at least one", which is wrong. Statements: `{statement ≤500 chars, lang?, mimetype?}` (lowercase `mimetype`). Licenses: each `{url}` XOR `{ingredient}`. |
| `agencies` | if present and `copyright.publicDomain` is absent, at least one agency needs role `rightsHolder` (`copyright_constraints`). The `id` must be a prefixedId (`dcs::…`); a bare URL fails. |
| `localizedNames` | object; `{}` is fine |
| `relationships[]` | `{relationType, flavor, id: prefixedId, revision?, variant?}`. See the `oneOf` traps in §4. |
| `type.flavorType` | see §1.3 |
| `ingredients` | at least 1 entry, keys match the `path` pattern, values follow §1.4 |

### 1.3 Flavor choices

- **Bible text translation:**

  ```json
  "type": {"flavorType": {"name": "scripture",
    "flavor": {"name": "textTranslation", "projectType": "standard",
               "translationType": "firstTranslation", "audience": "common", "usfmVersion": "3.0"},
    "currentScope": {"TIT": [], "JON": []}}}
  ```

  All five flavor fields are required. The flavor object has `additionalProperties: false`, and `conventions` is the only optional field. The enums:
  - `projectType`: standard, daughter, studyBible, studyBibleAdditions, backTranslation, auxiliary, transliterationManual, transliterationWithEncoder.
  - `translationType`: firstTranslation, newTranslation, revision, studyOrHelpMaterial.
  - `audience`: basic, common, common-literary, literary, liturgical, children.
  - `usfmVersion` matches `^\d+\.\d+(\..+)?$`.
- **OBS:** `{"name": "gloss", "flavor": {"name": "textStories"}, "currentScope": {...}}`.
  - This is a v1.0.0 "beta" flavor. Its flavor object allows **only** `name`.
  - `currentScope` is required for `gloss`, and tC4 copies the template's 33-book table (R-10.2.3).
  - Develop rejects this flavor (see §0).
- **x- flavors.** A pattern like `^x-[a-z][a-zA-Z0-9]*$` names the **whole burrito's** flavor (for example `scripture/x-tc4Project`, which validates). The flavor object then allows extra properties. tC4 does not use one, because the project is a real `textTranslation`.
- `currentScope` with no books (`{}`) is **invalid** because of `minProperties: 1`. A project with zero books cannot be written as a valid burrito.

### 1.4 Ingredients, checksums and non-scripture data

Allowed keys on an ingredient: `size`, `mimeType`, `checksum`, `scope`, `role` and `lang`. Develop also allows `properties`. `additionalProperties: false` applies, so no custom fields are allowed.

- `size`: required, integer ≥ 0. It MUST equal the byte length on disk (prose, plus tC4).
- `mimeType`: required, pattern `^[\-a-z0-9]+/[\-a-z0-9+]+$`. That excludes dots, uppercase letters and parameters.
- `checksum`: **optional in the schema**; the prose says SHOULD and tC4 says MUST. If present it is `{md5 (required, ^[a-f0-9]{32}$), sha3-256?, sha3-512?}`, with at most 2 keys. The md5 is lowercase hex over the **raw bytes on disk**; uppercase fails.
- `scope`: the same grammar as `currentScope`.
  - Keys come from the `bookId` enum: 3 uppercase characters, canonical books plus deuterocanon. `FRT`, `BAK`, `GLO`, `INT` and `tit` all fail.
  - Values are `[]` (the whole book) or a non-empty array of unique strings matching `C | C-C | C:V | C:V-V | C:V-C:V` with no zeros (`[1-9][0-9]*`).
  - `{}` fails. Ranges are **not** checked against versification. The pankosmia OBS template itself carries `MAT "44-46"` and `LUK "38-44"`, which tC4 must keep byte-for-byte.
- `role`: one of the USX peripheral labels, the story roles (`timing`, `introduction`, …), `body`/`thumbnail`, `versification`, `localedata`, `copyrightStatement`, `unit …`, `(name|sign|word|concept|place)…`, or `^x-\S.*\S$`.
  - `x-a` is too short and fails.
  - The develop prose lists roles `text` (for USJ), `styleSheet`, `font` and `license`. **None of these is in the schema enum, so all fail** (checked with ajv).
- **Non-scripture project data:** alignments, check decisions, pins, settings and journals.
  - Store them as ordinary ingredients **inside the same burrito**, tagged with **`x-` roles**, not `x-` flavors. Use `mimeType: application/json`, and add `scope` where the file is per-book.
  - tC4's roles are `x-alignment`, `x-check-decisions`, `x-resource-links`, `x-check-settings` and `x-journal`. `vrs.json` carries no role. The standard role would be `versification`, since the file is in Copenhagen Alliance format.
  - Roles are **decorative**. A Pankosmia rescan drops them (D28), so readers MUST key on paths (tC4 rule S-2).
  - The prose says tools MUST NOT reject a burrito over an unknown `x-` value.
  - Develop's alternative is a separate `alignment`-flavor burrito in alignment-spec v0.4 format. That does not exist in 1.0.0, and it conflicts with tC4's "one repository, no companion repos" design.
- **Resource dependencies** go in `relationships`, using `x-` flavors (`x-bcvnotes`, `x-bcvarticles`, `x-bcvquestions`, `x-peripheralArticles`, `x-lexicon`). A source text uses `relationType: "source"` with `textTranslation`.

### 1.5 Path rules

- **Schema:** the key matches `^[^\/:?*"><|]+(/[^\/:?*"><|]+)*$`. A leading `/` and `:` are rejected. **`..`, `\`, spaces and leading `./` are allowed**, so the schema is not a safety check.
- **Prose:** paths are relative to the burrito root, use `/`, and "must match the actual layout exactly".
- **tC4/Pankosmia:**
  - Every file lives under `ingredients/`, and the key includes that prefix.
  - No segment may be empty or start with `.`.
  - No segment may contain any of `.. ~ \ & * + | space ? # % { } < > $ ! '`.
  - Book files are named `<BOOK>.usfm` with an uppercase code from the eng canon.
  - On Windows, the Pankosmia zip export names entries with `\`. Normalize them on import.

### 1.6 Rules beyond the schema (prose MUSTs plus tC4)

1. Each listed ingredient exists, and its md5 and size match the bytes on disk. tC4 adds that the listed set **equals** the set of files under `ingredients/`.
2. `checksum.md5` is present on every ingredient (tC4 §3 rule 5).
3. `languages` is present for `scripture`. tC4 writes it for OBS as well.
4. Every book in `currentScope` is covered by at least one text ingredient whose `scope` covers it, and every text ingredient has a `scope` (develop prose MUST, tC4 rule 5).
5. The `identification.primary` authority, and the prefix of every `relationships[].id` and `agencies[].id`, is declared in `idAuthorities`. tC4: `local` is always present, and `dcs` is present when there are relationships.
6. The flavor is one of tC4's two kinds: `scripture/textTranslation` or `gloss/textStories` (`burritoCheck.flavorKind`).
7. The text mimeType follows one policy. See pitfall 3.
8. OBS story files are `text/markdown` (R-10.1.1).

---

## 2. How tC4's conformance harness validates

- **Loader** (`src/data/import/burritoCheck.mjs` `compileSbValidator`, shared by the app import and the harness):
  1. `new Ajv({ strict: false, allErrors: true })`, ajv 8.20, plus `addFormats(ajv)` from ajv-formats 3 (full mode, so `date-time` requires a timezone). The default dialect is draft-07.
  2. Every `.json` under `sb-schema/` is read, and **trailing commas are stripped by regex** (`/,(\s*[}\]])/g`), because `scripture_flavor_type.schema.json` has one.
  3. Each schema's `$id` is **rewritten to `https://sb.local/<relpath>`**. The bundle ships relative ids such as `"./x.schema.json"`, `"."` and a subfolder-relative `"scripture/text_translation.schema.json"`.
  4. `addSchema` runs on each file. On a duplicate, the first one wins.
  5. The validator is `getSchema(BASE + 'source_metadata.schema.json')`.
- `strict: false` is needed because of the unknown keywords `$$target`, `revision` and `timestamp`. The spec repo's own validator instead calls `ajv.addKeyword(...)` for each.
- **Extra checks in `validate.mjs`:**
  - **Stage-1, metadata-related:**
    - schema validity;
    - "metadata lists exactly the on-disk files" (a sorted path list under `ingredients/`);
    - `checkIngredients`: every listed file is present, and `checksum.md5 === md5(bytes)` and `size === byteLength`;
    - `vrs.json` is present and shaped correctly, and covers every `currentScope` book;
    - the scope grammar check (`journal/grammar.mjs` `SCOPE_RANGE_RE`, which is *looser* than the schema because it allows `0`).
  - **Stage-2:** the count of role-tagged ingredients is 5 and `vrs.json` has no role, plus native `relationships`. This stage is "non-durable by design", because a server rescan drops roles.
  - **Phase-2:** a two-actor git merge conflicts on `metadata.json`. The resolution regenerates the ingredients table from disk: fresh md5, size and mimeType, with `role` and `scope` carried over for paths that survive.
  - **OBS:**
    - the schema is valid with `gloss/textStories`;
    - a misspelt flavor and an extra flavor field both fail;
    - every story is `text/markdown`, and a `text/plain` story fires;
    - the layout equals the template;
    - `currentScope` equals the template table (key order ignored);
    - each check also fires on a deliberately broken copy.
- **`validate-roundtrip.mjs`** needs the live Pankosmia rig, so it was not run here. It measures what survives server operations:
  - `update_ingredients` and `remake-ingredients` register every file with the correct md5;
  - `relationships` survive from 0.18.5 on;
  - `x-` roles are dropped;
  - content bytes are never rewritten;
  - the zip round-trip is byte-identical;
  - Stage-1 still holds on the server-touched copy.
- **Pankosmia rescan** (`utils/burrito.rs:255-325`, read, not run):
  - It walks the repo and skips paths that contain `/.`, files with no extension, `metadata.json` and `*.bak`.
  - md5 comes from `chksum-md5`.
  - mimeType comes from `mime_infer`. When that finds nothing, `.usfm`/`.vrs` become `text/plain` and anything else becomes `application/octet-stream`.
  - `scope` is set to `{BOOK: []}` only for files whose stem is a canonical book code.
  - `role` is set to `None`.
  - Inferred from the code: a file at the repo root would be keyed `/name.ext`, which fails the schema path pattern.

---

## 3. Rust validation plan: crate choice and probe

### 3.1 Crate choice

Use **`jsonschema = { version = "=0.58.6", default-features = false }`** with `serde_json` and `md-5`.

- **Drafts and features:** it supports draft-07 and `if`/`then`/`else`, `oneOf`, `propertyNames`, `contains` and `format`. The large BCP-47 and XHTML regexes compile.
- **Network:** the default features pull in `reqwest`, `rustls` and `aws-lc-rs` (`resolve-http`). Turning them off leaves no network code. `.offline()` refuses any `$ref` that is not in the registry, and the `Registry` builder's `DefaultRetriever` always fails.
- **Dialect detection:** the bundle's `$schema` is `"http://json-schema.org/draft-07/schema"`, without the trailing `#`. Auto-detection works (tested), but set `Draft::Draft7` explicitly anyway.
- **Formats:** the crate asserts `format` by default for draft-07, matching ajv-formats on `date-time` (tested both ways). Call `.should_validate_formats(true)` explicitly.
- **Unknown keywords:** `$$target`, `revision` and `timestamp` are silently ignored, so there is no ajv-style strict-mode issue.
- **Threading:** `Validator` is `Send + Sync` and owns everything, so the `Registry` can be dropped after `build`. A compile-time assertion is in the probe. That makes `static SB: LazyLock<Validator>` workable.
- **Errors:** read them with `iter_errors(&v)` and `.instance_path()`. A `oneOf` failure is opaque ("is not valid under any of the schemas listed in the 'oneOf' keyword"). For good flavor messages, dispatch on `(flavorType.name, flavor.name)` in code first, the way develop's generated `type.validation.schema.json` does with `if`/`then`/`else`.

### 3.2 Schema-loading gotchas, all hit in the probe

1. **Not every file is valid JSON.** `scripture_flavor_type.schema.json` has a trailing comma in **both** schema copies, so `serde_json` rejects it. Strip trailing commas (the probe has a string-aware stripper) or skip the file. It is unreferenced in both copies.
2. **`Registry::prepare()` crawls every `$ref` in every registered resource up front** (ajv compiles lazily). tC4's dead `scripture_flavor_type.schema.json` refers to `scripture/print_publication`, `braille_scripture_publication` and `book_scope`, none of which exist. With it registered, `prepare()` fails with `Resource '…/scripture/print_publication.schema.json' is not present in a registry`. **Register only the closure reachable from the root.** For tC4's bundle that is 46 schemas from `metadata.schema.json`, or 39 from `source_metadata`.
3. **The relative `$id`s must be rewritten.** Kept as-is, `$id: "scripture/text_translation.schema.json"` registered at `…/scripture/text_translation.schema.json` resolves to `…/scripture/scripture/…`. Its `../common.schema.json` then lands on `…/scripture/common.schema.json`. That is why Pankosmia ships a duplicate `scripture/common.schema.json`; without it, `prepare()` fails (tested with `RAWID=1`). `source_metadata.schema.json` has `$id: "."`. The fix is the same as tC4's: set every `$id` to `BASE + relpath`. The probe uses `BASE = https://burrito.bible/schema/`, the spec's own namespace, so the develop files (which already carry absolute ids) load unchanged.
4. **Develop has two files with the same `$id`.** Both `type.schema.json` (a `oneOf` "documentation" schema) and `type.validation.schema.json` (generated `if`/`then`/`else`) claim `https://burrito.bible/schema/type.schema.json`. The spec's `index.js` registers the validation variant. Pick one explicitly if you ever load develop. Keying by path, as the probe does, sidesteps the clash, because only the `oneOf` file is reachable.
5. **Vendor the schemas.** `include_str!` the reachable set and do no runtime file IO. The Pankosmia copy of the 1.0.0 line is already re-keyable. The spec's `out/` is flat (no subfolders) with absolute `$id`s, so it could be registered by `$id` directly, but that variant was not probed.

### 3.3 Probe result (verbatim, trimmed)

```
== schema dir …/translationcore4/conformance/sb-schema (root metadata.schema.json)
  GOTCHA strict parse failed: scripture_flavor_type.schema.json: trailing comma at line 40 column 13 -> retrying with trailing commas stripped
  reachable from root: 46 schemas; not loaded: ["parascriptural/word_alignment…", "recipe_element…", "recipe_section…", "reference_format…", "scripture/common…", "scripture_flavor_type…"]
  registry+compile: 32ms
-- …/conformance/sample-burrito
  schema: VALID              validate: 0.37ms
  ingredients: OK (8 entries; md5+size+exact set)
-- …/conformance/sample-burrito-obs
  schema: VALID              validate: 0.15ms
  ingredients: OK (56 entries; md5+size+exact set)
== schema dir /home/user/ref/scripture-burrito/schema (develop, root metadata.schema.json)
-- sample-burrito      schema: VALID
-- sample-burrito-obs  schema: INVALID (1 errors)
    - at /type/flavorType : {"name":"gloss","flavor":{"name":"textStories"},…} is not valid under any of the schemas listed in the 'oneOf' keyword
```

**Controls** (`CONTROLS=1`). There are 48 mutations of the Bible sample. All behave as expected against the 1.0.0 bundle, apart from the known `languages` gap. Rust and ajv (tC4's own loader) **agree on 48/48**. They rejected or accepted the following.

- **Rejected:**
  - `mimeType` `application/vnd.foo+json`, or with `; charset=utf-8`;
  - an uppercase md5, a checksum without md5, a missing `size`, or an extra ingredient field;
  - role `alignment` without `x-`;
  - scope `{}`, scope key `FRT` or `tit`, a range `"0"`, or a duplicate range;
  - a path with `:` or a leading `/`;
  - `meta.version` `1.1.0`;
  - `dateCreated` as a bare date, without a timezone, or as `yesterday`;
  - a missing `defaultLocale`, or lowercase `nfc`;
  - `copyright` with statements plus licenses, or `publicDomain` plus statements;
  - `agencies` with no `rightsHolder`;
  - two authorities in `primary`;
  - a missing `usfmVersion`;
  - an x-flavor relationship with `relationType` `expression` or `target`;
  - an unknown top-level field;
  - a padded `localizedText` value;
  - an empty `currentScope`;
  - `gloss/textStories` with an extra field.
- **Accepted:**
  - `text/x-usfm`;
  - no checksum;
  - role `x-alignment`;
  - scope `["1:1-2:5"]`;
  - **a path with `\`, `../escape.usfm` or a space**;
  - **`identification.primary` timestamp `"not a date"`**;
  - **idAuthority label `"Local"`**;
  - **no `languages`**;
  - `gloss/x-textStories`;
  - `scripture/x-tc4Project`.

  The items in bold need code checks.

### 3.4 Key probe code

`Cargo.toml`:

```toml
[dependencies]
# default features pull in reqwest + rustls (resolve-http). We never fetch, so turn them off.
jsonschema = { version = "=0.58.6", default-features = false }
serde_json = { version = "1", features = ["preserve_order"] }
md-5 = "0.11.0"
```

Load the bundle, re-key `$id`s, keep only the reachable closure, and build an offline draft-07 validator:

```rust
use jsonschema::{Draft, Registry, Validator};
const BASE: &str = "https://burrito.bible/schema/";

fn load_bundle(schema_dir: &Path) -> Vec<(String, Value)> {
    let mut files = Vec::new();
    walk(schema_dir, &mut files);                     // every *.json, sorted
    files.into_iter().map(|f| {
        let rel = f.strip_prefix(schema_dir).unwrap().to_string_lossy().replace('\\', "/");
        let text = fs::read_to_string(&f).unwrap();
        let mut v: Value = serde_json::from_str(&text)  // scripture_flavor_type has a trailing comma
            .or_else(|_| serde_json::from_str(&strip_trailing_commas(&text))).expect("schema JSON");
        let uri = format!("{BASE}{rel}");
        v["$id"] = Value::String(uri.clone());           // bundle ships "./x", ".", "scripture/x" ids
        (uri, v)
    }).collect()
}

/// Registry::prepare() resolves EVERY registered resource's $refs eagerly, so a dead file with
/// a dangling ref fails the whole load. Keep only what the root reaches.
fn reachable(bundle: Vec<(String, Value)>, root_rel: &str) -> Vec<(String, Value)> {
    // BFS over "$ref" strings; resolve "x.json", "../x.json", "sub/x.json" against the file's
    // URI with an RFC 3986 merge; ignore "#…" local refs. (Full code in src/main.rs.)
}

fn build_validator(registry: &Registry, root_rel: &str) -> Validator {
    jsonschema::options()
        .with_draft(Draft::Draft7)
        .should_validate_formats(true)   // meta.dateCreated: date-time, like ajv-formats
        .offline()                       // a ref outside the registry is a build error, never a fetch
        .with_registry(registry)
        .build(&json!({ "$ref": format!("{BASE}{root_rel}") }))
        .expect("validator build")
}

const _: () = { const fn assert_send_sync<T: Send + Sync>() {} assert_send_sync::<Validator>(); };

fn sb_validator(schema_dir: &Path, root_rel: &str) -> Validator {   // registry is a local
    let bundle = reachable(load_bundle(schema_dir), root_rel);
    let registry = Registry::new().draft(Draft::Draft7).extend(bundle).unwrap().prepare().unwrap();
    build_validator(&registry, root_rel)
}
```

Ingredient integrity (the non-schema half):

```rust
fn check_ingredients(burrito: &Path, meta: &Value) -> Vec<String> {
    let mut fails = Vec::new();
    let ing = meta["ingredients"].as_object().expect("ingredients object");
    for (rel, entry) in ing {
        if rel.split('/').any(|s| s.is_empty() || s == "." || s == "..") || rel.starts_with('/') || rel.contains('\\') {
            fails.push(format!("unsafe path {rel}")); continue;
        }
        let Ok(bytes) = fs::read(burrito.join(rel)) else { fails.push(format!("missing {rel}")); continue };
        // md-5 0.11's output (hybrid_array::Array) has no LowerHex impl; hex it by hand.
        let md5: String = Md5::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if entry["checksum"]["md5"].as_str() != Some(md5.as_str()) { fails.push(format!("md5 mismatch {rel}")); }
        if entry["size"].as_u64() != Some(bytes.len() as u64) { fails.push(format!("size mismatch {rel}")); }
    }
    // tC4: listed set == files on disk under ingredients/
    let on_disk: BTreeSet<String> = /* walk ingredients/, relativize, '/'-join */;
    let listed: BTreeSet<String> = ing.keys().cloned().collect();
    for x in on_disk.difference(&listed) { fails.push(format!("unlisted file {x}")); }
    fails
}
```

To run it:

```
burrito-probe <schema-dir> metadata.schema.json <burrito-dir>...
```

Environment flags:

- `CONTROLS=1` runs the 48 mutations.
- `DUMP=dir` writes them out for ajv.
- `NODRAFT=1`, `NOFMT=1` and `RAWID=1` test the gotchas.
- `ALL=1` loads every file.
- `OWNED=<dir>` builds on another thread and drops the registry.

### 3.5 Proposed design for the rebuild

1. **`burrito::schema`.** Vendor the reachable v1.0.0 set from tC4's `sb-schema` with `include_str!`, keyed `BASE + relpath`, with `$id` rewritten at load. Hold it in a `static SB_1_0_0: LazyLock<Validator>`. Add a test that the vendored set equals tC4's `sb-schema`, so drift is caught. Optionally, keep a `develop` validator in tests only, as forward-compat telemetry (Bible kind only).
2. **`burrito::check(dir) -> Vec<Finding>`** runs in layers, and the first failing layer stops the rest, as `checkBurrito` does:
   - (a) the JSON parses;
   - (b) `SB_1_0_0`;
   - (c) the §1.6 rules plus the schema gaps:
     - `languages` is present;
     - `primary` revision matches `revisionString` and timestamp is RFC 3339;
     - authority labels are declared;
     - strict paths;
     - `checksum.md5` is required;
     - `currentScope` coverage;
     - the flavor kind;
     - the mimeType policy;
   - (d) disk integrity: md5, size and the exact set.

   Return typed findings, mapped onto tC4's refusal codes (`import.damaged.*`) for parity.
3. **One writer owns `metadata.json`.** `regenerate(dir)` works like this:
   - It rescans `ingredients/` and hashes the **exact bytes it wrote**, after NFC and LF normalization.
   - It computes md5, size and mimeType from one fixed extension table.
   - It carries `role` and `scope` forward for paths that survive, and rebuilds `currentScope`.
   - It runs `check` in memory **before** an atomic temp-file-plus-rename write.

   Nothing else edits one ingredient entry at a time.
4. **Tests:**
   - tC4's two samples serve as golden fixtures.
   - The 48 controls become a table test. They were cross-checked against ajv once, here, 48/48.
   - A property test: random edits, then `regenerate`, then `check` comes back empty.

---

## 4. Pitfalls

1. **md5 and size go stale on every write.** Any ingredient write without a metadata regeneration leaves an invalid burrito. Always hash the buffer you are about to write, after normalization. Never trust a cached hash. `metadata.json` itself is not an ingredient.
2. **Line-ending conversion breaks checksums.** Git `core.autocrlf` (Windows) or a `* text=auto` attribute can rewrite USFM, MD or JSON bytes on checkout, and the md5 and size then no longer match. tC4 protects its own fixture with `conformance/sample-burrito/** -text`. Projects the rebuild creates need the same, either `.gitattributes` `* -text` or repo `core.autocrlf=false`. Note that a Pankosmia remake strips `.gitattributes` from imported zips.
3. **USFM mimeType: policy conflict.**
   - tC4 BURRITO-SPEC §3 rule 5 says `text/plain`, and Pankosmia's rescan writes `text/plain` because `mime_infer` does not know `.usfm`.
   - The develop prose says USFM MUST be `text/x-usfm`.
   - The schema accepts both.

   Recommendation: write one value consistently. Use `text/plain` if Pankosmia or tC4 interop matters; otherwise use `text/x-usfm`. Accept both on read. Other types: `.md` is `text/markdown` (required for OBS stories), `.json` is `application/json` and `.tsv` is `text/tab-separated-values`.
4. **mimeType pattern.** It forbids dots, uppercase and parameters. Any vendor type (`application/vnd.*`, for example `.docx`/`.xlsx` from `mime_infer`) makes the whole burrito invalid. Map unknown extensions to `application/octet-stream`.
5. **Scope keys.** Keys come only from the schema's `bookId` enum, uppercase. Peripheral books such as `FRT`, `BAK`, `GLO` and `XXA` cannot be scope keys. Values are `[]` or ranges without zeros. `{}` is invalid, and so is a project with no books. Ranges are not checked against versification. Do not add such a check to the OBS gate, because the upstream template carries `MAT "44-46"`. tC4's own scope regex is looser than the schema (it allows `"0"`), so the schema is the stricter gate.
6. **`copyright` is exactly-one-of.** Statements plus licenses fails, even though the prose says "at least one". Adding `agencies` without a `rightsHolder` fails, unless a `publicDomain` key is present (even `false` skips that check).
7. **`relationships` `oneOf` traps.**
   - An `x-` flavor with `relationType` `target` or `expression` matches two branches and fails.
   - In 1.0.0, `source` works only with `textTranslation` or `audioTranslation`, so OBS source pins get no row (tC4 §3 rule 6).
   - `id` must be `prefix::rest`.
   - `revision` matches `^[0-9A-Za-z]([0-9A-Za-z_.:\-]{0,62}[0-9A-Za-z])?$`.
8. **Trim user-entered names.** `localizedText` values must not have leading or trailing whitespace and must not be empty. That covers `identification.name`, language names and agency names.
9. **Ingredient objects are closed.** Do not add fields. Custom roles need at least 2 characters after `x-`. Do not copy the develop prose's `text`, `styleSheet`, `font` or `license` roles, which fail the schema.
10. **Roles do not survive Pankosmia.** A Pankosmia rescan drops `x-` roles, re-infers mimeTypes, skips extensionless files (so `LICENSE` without `.md` falls out of the table) and would key root-level files with a leading `/`. Keep every file under `ingredients/` with an extension. Never make behavior depend on a role.
11. **`meta.dateCreated` needs a timezone** (`…Z` or an offset), and `meta.version` must stay exactly `"1.0.0"`.
12. **Things the 1.0.0 schema never checks:**
    - that `languages` is present;
    - the identification `revision` and `timestamp` values;
    - the idAuthority label;
    - path safety (`..`, `\`);
    - that a checksum is present;
    - that checksums are correct;
    - that `currentScope` books are covered by ingredients.

    Each needs code (§3.5, step 2c).
13. **Schema-loading gotchas in Rust:** the trailing comma, the eager `prepare()` crawl over dead files, the relative and duplicate `$id`s, `default-features = false`, and `md-5` 0.11 having no `{:x}`. See §3.2.
14. **OBS against develop.** `gloss/textStories` fails the develop schema. If the spec ever releases develop as 1.1, OBS projects will need a migration, either to an `x-` flavor or to whatever replaces the beta flavor.
