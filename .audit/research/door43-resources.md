# Door43 (DCS) + unfoldingWord resource formats — research for a Rust client

Date: 2026-10-14. Scope: what a Rust rebuild of translationCore 4 (tC4) needs in order to
discover, download, pin and read ULT, UST, UHB, UGNT, tN, tW, TWL, tA, tQ, OBS and the
OBS helps from Door43 Content Service (DCS, `git.door43.org`, a Gitea fork), and to push
projects back.

## How to read this report

git.door43.org is blocked from this container, so nothing here was fetched live. Every
claim is tagged with where it comes from:

| Tag | Meaning |
|---|---|
| **[SW]** | `swagger.v1.json` in uW-Tools-Collab (DCS API spec, version `1.23.7+dcs+12-ge61c163c9c`, SW:1-10). Request and response *schemas* are authoritative. Values are not. |
| **[DOC]** | uW-Tools-Collab prose docs. They are useful for orientation, but several examples are wrong (§1.13 and §2.15 list the errors). |
| **[FIX]** | Real bytes in tC4 `test/fixtures/` (pinned copies of real DCS exports). These are the strongest evidence. |
| **[TC4]** | tC4 source code or docs that cite their own live verification (`[VERIFIED …]` notes). |
| **[MEM]** | My prior knowledge. Not checkable against these sources. Verify it before you rely on it. |

Path prefixes used in the citations:

- `UTC:` = `/home/user/ref/uW-Tools-Collab/`
- `SW:N` = `UTC:swagger.v1.json` line N
- `TC4:` = `/home/user/unfoldingword/translationcore4/`
- `FX:` = `TC4:test/fixtures/`

---

## 0. Key findings (read this first)

1. **Two different packagings exist for every resource.**
   - **(a) The RC git repo.** You get it with `git clone`, `/archive/<ref>.zip` or the
     contents/raw API. It has `manifest.yaml`, `57-TIT.usfm`, `tn_TIT.tsv`, `twl_TIT.tsv`,
     `bible/kt/god.md`, and `rc://` links.
   - **(b) The DCS Scripture Burrito export, `/<owner>/<repo>/sb/<tag>.zip`.** It has
     `metadata.json` and `ingredients/TIT.usfm`, `ingredients/TIT.tsv`,
     `ingredients/payload/kt/god.md`, and links are mostly rewritten to `./payload/kt/god.md`.
   - tC4 pins and downloads **only (b)**: TC4:src/data/resourceFetch.ts:1-27,
     FX:resources/README.md:1-13. The Rust client must decide which packaging it reads.
     Supporting both behind one trait is cheap: the TSV and USFM bytes are identical, and
     only paths and link forms differ.
2. **A pin is `(repoPath, tag, 40-hex commit sha)`.** The sha is the identity and the tag is
   only a label.
   - On every download, verify that the export's declared revision
     (`identification.primary.dcs["owner/repo"].revision`) equals a sha you got
     independently. That sha comes from the pin, or from the DCS tags API on a first install
     (TC4:src/data/resourceFetch.ts:316-329, 372-404).
   - For a commit with no tag, download `/archive/<sha>.zip`. Gitea writes the packed commit
     sha into the **zip archive comment** (TC4:src/data/resourceFetch.ts:46-68, 480-483).
3. **The Bible tN `Quote` and TWL `OrigWords` are original-language (Greek/Hebrew) text, not
   English.** The docs' English examples are wrong for Bible helps (UTC:docs/4-repository-formats/
   resource-container/translation-notes-repositories-guide.mdx:128-129).
   - A discontinuous quote uses ` & ` between spans (FX:resources/en_tn@v86/TIT.tsv, 18 of
     206 rows).
   - OBS helps quote **English** story text, and their `Occurrence` is always 1.
4. **The current tN format is TSV7:**
   `Reference ID Tags SupportReference Quote Occurrence Note`.
   - It has no GLQuote column, so the gateway-language rendering must be *derived from the
     ULT/UST alignment* (TC4:src/data/sourceHighlight.ts:263-289).
   - tC4 refuses any other header (TC4:src/data/derive.ts:130-148).
5. **The token-creation response puts the secret in `sha1`, not `token`.** The docs are wrong
   (UTC:docs/3-door43-api-developer-guide.mdx:624-631 vs SW:20205-20236;
   TC4:src/data/share/door43Api.ts:286-295). Creating a repo needs the `write:user` scope
   (user repo) or the `write:organization` scope (org repo), plus `write:repository` to push
   (TC4:src/data/share/door43Api.ts:49-60).
6. **Push is plain git over HTTPS** to the `clone_url` from the create-repo response.
   - Use Basic credentials `username:token`, never embedded in the URL
     (TC4:src/data/serverApi.ts:518-530, TC4:docs/PLATFORM-NOTES.md:155).
   - Create the repo with `auto_init: false` so the first push fast-forwards
     (TC4:src/data/share/door43Api.ts:244-249).
7. **Catalog search is `GET /api/v1/catalog/search`.** In this DCS version it is the
   successor of the legacy "catalog v5" API. The legacy `/api/catalog/v5/...` path is
   **not** in this swagger [MEM].
   - Each entry has `zipball_url`, `tarbar_url` (sic, a typo in the schema), `commit_sha`,
     `branch_or_tag_name`, `metadata_url`, `ingredients[]` and an embedded `release` and
     `repo` (SW:20823-20975).
   - The tN subject is spelled **`TSV Translation Notes`** in the catalog
     (TC4:src/data/gateways.ts:17). The docs' "Translation Notes" probably matches nothing
     unless you pass `partialMatch=true`.
8. **`GET /api/v1/repos/{owner}/{repo}` already carries `catalog.{prod,preprod,latest}`.**
   Each stage has `commit_sha`, `branch_or_tag_name` and `zipball_url` (SW:26416, SW:20999+).
   That is a one-call "latest release + sha" lookup. tC4 instead calls `releases/latest` and
   then walks the tags pages.
9. **Gateway languages are `(language, owner)` pairs, not languages.** `fr` exists under both
   `Xenizo` and `MVHS`. The Rust client needs an 18-entry registry like
   TC4:src/data/gateways.ts:56-75 and the per-owner quirks it records (`hasTwl`, `literal`,
   `_glt`/`_gst` naming).

---

## 1. DCS API endpoints

### 1.1 Hosts, base paths, headers

- **Production** is `https://git.door43.org`. The API base is `/api/v1`
  (SW: `basePath: /api/v1`; UTC:docs/2-unfoldingword-developer-guide.mdx:504-509).
- **QA** is `https://qa.door43.org`. tC4 sends every *write or account* call there in dev
  builds. QA is reset weekly. Reads (catalog, downloads) always go to production
  (TC4:src/data/dcsServer.ts:1-20).
- **Auth headers** (SW `securityDefinitions`):
  - `Authorization: token <TOKEN>` (`AuthorizationHeaderToken`: "API tokens must be prepended
    with 'token' followed by a space").
  - `Authorization: Basic base64(user:pass)` (`BasicAuth`).
  - `X-GITEA-OTP: <code>`: use it with Basic auth when the account has 2FA.
  - The `?token=` and `?access_token=` query parameters are marked "deprecated for removal in
    Gitea 1.23". Do not use them, and never put the token in a URL
    (TC4:src/data/share/door43Api.ts:8-11).
- **Error body:** `{"message": "...", "url": "..."}`. A 401 says "invalid username, password
  or token" (TC4:src/data/share/door43Api.ts:23-24; UTC:docs/3-door43-api-developer-guide.mdx:981-988).
- **CORS:** DCS serves the API and the `/sb/` export cross-origin. tC4 does browser GETs
  (TC4:src/data/resourceFetch.ts:34-38). This does not matter for Rust.
- **`X-Total-Count`:** returned on list and search endpoints. tC4 uses it on
  `repos/search?limit=1` (TC4:src/data/share/door43Api.ts:16-18, 210-230).

### 1.2 Catalog (resource discovery)

All catalog paths are `GET`, need no auth, and live under `/api/v1/catalog/…`.

| Endpoint | Purpose | Spec |
|---|---|---|
| `/catalog/search` | Search released resources | SW:1871 |
| `/catalog/entry/{owner}/{repo}/{ref}` | One entry; `ref` = release tag or default branch | SW:1061 |
| `/catalog/metadata/{owner}/{repo}/{ref}` | The entry's metadata (RC manifest or SB metadata) as free-form JSON (`CatalogMetadata` = object with `additionalProperties`) | SW:1828 |
| `/catalog/list/languages` | `{ok, data:[Language]}` (Language is an untyped object) | SW:1104 |
| `/catalog/list/subjects` | `{ok, data:[string]}` | SW:1650 |
| `/catalog/list/owners` | `{ok, data:[User]}` | SW:1466 |
| `/catalog/list/metadata-types` | `{ok, data:[string]}` | — |
| `/catalog/validation/{owner}/{repo}/{ref}` | Validation report (not needed for reading) | — |

**`/catalog/search` query parameters [SW:1871+]:**

- **Text:** `q` (keywords, comma list or repeated).
- **Exact-match filters, case-insensitive:** `owner`, `repo` and `tag` (repeat or comma-list
  each), and `lang` (array). Set `partialMatch=true` to get LIKE matching.
- **Gateway languages:** `is_gl` (bool).
- **`stage`** (array, enum `prod|preprod|latest|other`):
  - `prod` (the default) returns production releases only.
  - `preprod` returns production and pre-production releases.
  - `latest` returns all releases plus the default branch.
  - The description also mentions `branch` (all branches).
  - "Low to high" ordering applies: a stage includes the stages above it. The docs say the
    same (UTC:docs/2-unfoldingword-developer-guide.mdx:522).
- **Resource classifiers** (array, values ORed): `subject`, `flavorType`, `flavor`,
  `abbreviation`, `format`.
- **`checkingLevel`:** enum `1|2|3`, means ≥.
- **`book`:** ingredient identifier, ORed.
- **Metadata kind:** `metadataType` (enum `rc|sb|tc|ts`, ORed) and `metadataVersion`.
- **Topics:** `topic` (ORed) and `withoutTopic` (ANDed).
- **Result shape:**
  - `includeHistory` (bool): when true, all releases are returned, not only the latest.
  - `showIngredients` (bool, default true).
  - `sort` (`subject|title|reponame|tag|released|lang|releases|stars|forks`) and
    `order` (`asc|desc`).
  - `page` (1-based) and `limit` (no limit by default).

**Response `CatalogSearchResults`** [SW:20976-20998]:
`{ "ok": bool, "last_updated": date-time, "data": [CatalogEntry] }`.

**`CatalogEntry`** [SW:20823-20975]. These are the types only. I did not invent any values.

```text
id:int64  url:string(self)  name  owner  full_name  title  subject  abbreviation
language  language_title  language_direction  language_is_gl:bool
stage:string  ref_type:string  branch_or_tag_name:string (SW:20838)  commit_sha:string (SW:20842)
released:date-time (SW:20937)
zipball_url:string (SW:20969)  tarbar_url:string (SW:20953 — sic, typo in the JSON name)
contents_url  git_trees_url
metadata_url (SW:20914)  metadata_json_url (SW:20906)  metadata_api_contents_url
metadata_type  metadata_version  content_format  flavor_type  flavor
books:[string]  ingredients:[Ingredient] (SW:20875)
is_valid:bool  validation_errors_url
release:Release   repo:Repository
```

**`Ingredient`** [SW:24327]:
`identifier, path, title, sort:int64, versification, categories:[string], size:int64,
alignment_count:int64, exists:bool, is_dir:bool`. This is the RC `projects[]` entry
enriched by DCS.

#### Use-case URLs

These are built from the schema above. The parameter values are real repo, owner and
language names.

- **All released English resources:**
  `GET /api/v1/catalog/search?lang=en&stage=prod`
- **By subject, then owner:**
  `GET /api/v1/catalog/search?subject=Aligned%20Bible&lang=en&owner=unfoldingWord&stage=prod`
- **A gateway language's helps suite** (tC4's own regeneration recipe,
  TC4:src/data/gateways.ts:16-19): `GET /api/v1/catalog/search?subject=TSV%20Translation%20Notes&stage=prod`.
  Repeat it for "Translation Words" and "Translation Academy", then intersect on
  `(language, owner)`.
- **All gateway languages:** `GET /api/v1/catalog/list/languages?is_gl=true`
- **Releases of one repo, with history:**
  `GET /api/v1/catalog/search?owner=unfoldingWord&repo=en_tn&includeHistory=true&showIngredients=false`
- **Which entries contain Titus:** add `&book=tit` (lower or upper case is not stated [SW];
  test both).

#### Subjects and flavors

- **Docs:** "Bible", "Aligned Bible", "Translation Notes", "Translation Words", "Translation
  Words Links", "Translation Questions", "Translation Academy", "OBS Translation Notes"
  (UTC:docs/3-door43-api-developer-guide.mdx:546-553;
  UTC:docs/2-unfoldingword-developer-guide.mdx:776-784).
- **tC4 live evidence:** the TSV notes subject is **`TSV Translation Notes`**
  (TC4:src/data/gateways.ts:17).
- [MEM] DCS subjects also include `TSV Translation Questions`, `TSV Translation Words Links`,
  `TSV OBS Translation Notes` and similar, plus `Hebrew Old Testament` (UHB) and
  `Greek New Testament` (UGNT). The docs say UHB/UGNT are "Bible"
  (UTC:docs/2-unfoldingword-developer-guide.mdx:1117-1151).
- **Recommendation:** call `/catalog/list/subjects` once and build the subject map from the
  real list. Do not hardcode the doc strings.
- **Catalog flavors differ from export flavors** for OBS helps:
  - The catalog reports `parascriptural/x-notes` and `parascriptural/x-links`.
  - The `/sb/` export declares `peripheral/x-obsnotes` and `parascriptural/x-bcvarticles`.
  - Source: TC4:docs/PLATFORM-NOTES.md:123.
- **The `tc-ready` topic** marks repos that translationCore-family tools may use. Pankosmia
  filters on `tc-ready` plus `pushing2sb` (TC4:src/data/serverApi.ts:66-70). Use
  `topic=tc-ready` in catalog search.

### 1.3 The repository object (DCS-extended)

`GET /api/v1/repos/{owner}/{repo}` (SW:5210) returns `Repository` (SW:26370+). On top of
the Gitea fields (`full_name`, `default_branch`, `clone_url` SW:26425, `html_url`, `size`,
`updated_at`, `permissions`, `topics` SW:26696), DCS adds these:

- `catalog: CatalogStages` (SW:26416) with `{prod, preprod, latest}`. Each is a
  `CatalogStage` with `branch_or_tag_name`, `commit_sha`, `release_url`, `released`,
  `zipball_url`, `tarball_url`, `contents_url` and `git_trees_url` (SW:20999+).
  **This gives the newest prod tag and its commit sha in one call.**
- `subject` (SW:26682), `title`, `abbreviation`, `language`, `language_title`,
  `language_direction` and `language_is_gl` (SW:26568).
- `metadata_type` (SW:26593) and `metadata_version`, `flavor`, `flavor_type` (SW:26474),
  `content_format`, `checking_level`, `ingredients:[Ingredient]` (SW:26543).

Repo paths are case-insensitive addresses:

- `GET /api/v1/repos/Es-419_gl/es-419_tn` answers 200 with
  `full_name: es-419_gl/es-419_tn` (TC4:docs/PLATFORM-NOTES.md:112).
- Store the casing DCS reports and compare case-insensitively.

`GET /api/v1/repos/search` (SW:4936) takes these parameters:

- `q`, `owner`, `repo`, and the DCS filters `lang`, `is_gl`, `subject`, `flavorType`,
  `flavor`, `abbreviation`, `format`, `book`, `topic`, `withoutTopic`, `metadataType`,
  `metadataVersion` and `partialMatch`.
- The usual `sort`, `order`, `page` and `limit`.
- The response is `SearchResults` with an `X-Total-Count` header.

To list an org's repos, use `GET /api/v1/orgs/{org}/repos`
(UTC:docs/automation/door43-api-reference-for-mcp.mdx:53-58).

### 1.4 Releases and tags

| Endpoint | Notes | Spec |
|---|---|---|
| `GET /repos/{o}/{r}/releases?draft=&pre-release=&in-catalog=&page=&limit=` | `[Release]`. `in-catalog=true` returns only releases that are valid catalog entries | SW:14659 |
| `GET /repos/{o}/{r}/releases/latest?pre-release=&in-catalog=` | The newest non-draft, non-prerelease release by `created_at` | SW:14775 |
| `GET /repos/{o}/{r}/releases/tags/{tag}` | Release by tag | SW:14823 |
| `GET /repos/{o}/{r}/releases/{id}` | Release by id | SW:14907 |
| `GET /repos/{o}/{r}/tags?page=&limit=` | `[Tag]`. "default maximum page size is 50" | SW:15943 |
| `GET /repos/{o}/{r}/tags/{tag}` | One `Tag` | SW:16044 |
| `GET /repos/{o}/{r}/git/refs/{ref}` | `[Reference]` (prefix match). `object.sha` is the **tag object** sha for annotated tags | SW:7955 |
| `GET /repos/{o}/{r}/git/tags/{sha}` | Dereferences an annotated tag (`object.sha` = commit). Lightweight tags are not supported here | SW:8099 |
| `GET /repos/{o}/{r}/git/commits/{sha}` | `sha` may be "a git ref or commit sha" | SW:7692 |
| `GET /repos/{o}/{r}/commits?sha=<ref>&limit=1&stat=false&verification=false&files=false` | The head commit of any ref | SW:6806 |
| `GET /repos/{o}/{r}/branches` | `[Branch]` with `commit.id` | SW:6323 |

**`Release`** [SW:26195-26270]:

```text
id  tag_name(SW:26247)  target_commitish(SW:26255)  name  body  draft  prerelease(SW:26238)
created_at(SW:26213)  published_at(SW:26242)  html_url  url  upload_url
zipball_url(SW:26267)  tarball_url  author:User  assets:[Attachment]
door43_metadata: CatalogEntry   (SW:26218 — DCS extension)
```

- Releases carry **no commit sha** except `target_commitish`, which is often a branch name
  [MEM].
- tC4 reads only `tag_name`, `published_at` and `created_at`
  (TC4:src/data/upgrade.ts:75-89; TC4:src/data/resourceFetch.ts:277-294).
- `door43_metadata.commit_sha` should hold the sha [SW]. tC4 never relied on it.

**`Tag`** [SW:26856-26890]:
`{name, id, message, commit: {sha, url, created}, zipball_url, tarball_url}`.
`commit.sha` is the dereferenced commit, and that is the field tC4 uses
(TC4:src/data/resourceFetch.ts:214-218, 323-329).

#### Pagination

- tC4 walks `tags` and `releases` with `?limit=50&page=N`.
- It treats the first page's length as the server's effective page size, because the
  server may clamp `limit`. A short or empty page ends the walk. There is a 40-page safety
  bound. A 404 is conclusive absence, and other errors propagate
  (TC4:src/data/resourceFetch.ts:220-258).
- The uW helps repos carry about 90 or more release tags, so a single page misses old ones
  (TC4:src/data/resourceFetch.ts:221-225).

### 1.5 Getting the commit sha for a tag (ranked)

1. `GET /api/v1/repos/{o}/{r}/tags/{tag}` → `commit.sha` [SW:16044]. This is one call with
   no paging. tC4 does not use it; it pages through the list instead. Prefer this.
2. `GET /api/v1/repos/{o}/{r}` → `catalog.prod.commit_sha` together with
   `catalog.prod.branch_or_tag_name`. Use it for "latest prod release".
3. `GET /api/v1/catalog/entry/{o}/{r}/{tag}` → `commit_sha`.
4. `GET /api/v1/repos/{o}/{r}/git/refs/tags/{tag}`. If `object.type == "tag"`, follow with
   `git/tags/{object.sha}`.
5. `git ls-remote https://git.door43.org/{o}/{r}.git refs/tags/{tag}` (with the `^{}` peel).
   tC4's evidence used this (TC4:docs/evidence/sb-zip-zaln-2026-07-25.md:24-28).

Always cross-check the sha against the downloaded artifact (§1.6). Tags are not
immutable upstream: "anyone can name a tag anything" (TC4:docs/BURRITO-SPEC.md:349).

### 1.6 Downloading content

#### A. DCS Scripture Burrito export (what tC4 pins)

`GET https://git.door43.org/{owner}/{repo}/sb/{tag}.zip`. This is a **web route, not under
`/api/v1`, and not in the swagger** (TC4:src/data/resourceFetch.ts:39-44).

- **Made on demand.** `go-rc2sb` (v0.4.0/v0.5.0) converts the RC repo, and
  `meta.dateCreated` is the download time (TC4:docs/evidence/sb-zip-zaln-2026-07-25.md:20-23).
- **Byte-stable content.** It was verified on en_ult v89: all 55 of 55 USFM books are
  byte-identical to the tag, including every `\zaln` (TC4:docs/evidence/sb-zip-zaln-2026-07-25.md:1-19).
  The metadata layout may change across converter versions, so verify on every import.
- **Wrapped.** All entries sit under one `<repo>/` top directory. The zip also ships `.git/`
  and `.DS_Store` entries, which an install must drop (TC4:src/data/resourceFetch.ts:140-177).
- **Revision.** `metadata.json` → `identification.primary.dcs["<owner>/<repo>"].revision`
  equals the tag's commit sha (TC4:src/data/resourceFetch.ts:179-191; every FIX metadata).
  - The `<owner>` key is the org name **at export time**. A renamed org stays stale there:
    the es-419 exports say `Idiomas-Puentes/es-419_tn`, and that org now 404s
    (TC4:docs/PLATFORM-NOTES.md:109; FX:resources/es-419_tn@v66/metadata.json).
  - Treat the key as provenance, not as an address.
- **Size.** en_ult v89 is 7,709,900 bytes and en_twl v86 is 1,841,386 bytes
  (TC4:docs/evidence/sb-zip-zaln-2026-07-25.md:8; TC4:src/data/resourceFetch.ts:35).
- **Releases only.** It fetches by tag, never by branch (tC4 ruling D23b).
- **tW and TWL quirk.** DCS builds the TWL TSVs inside `en_tw/sb/<tag>.zip` from `en_twl`
  **master**, not from a release, so two builds of "the same" pin can differ.
  - Read TWL only from the `<lang>_twl` export.
  - Read articles only from the `<lang>_tw` export.
  - Source: TC4:docs/BURRITO-SPEC.md:349 ("Fetch path", D96); TC4:src/data/installedSuite.js:13-21.

#### B. Gitea archive (RC repo at any ref)

- API form: `GET /api/v1/repos/{owner}/{repo}/archive/{ref}.zip` or `.tar.gz` (SW:5888).
  The path parameter `archive` is "the git reference … with attached archive format (e.g.
  master.zip)".
- Web form: `https://git.door43.org/{owner}/{repo}/archive/{ref}.zip`. tC4 uses it for
  sha-only pins (TC4:src/data/resourceFetch.ts:46-53), and tC3 used it with a tag
  (TC4:src/data/import/tc3.ts:94-98).
- `{ref}` may be a tag, a branch or a **full commit sha**.
- The **zip archive comment** (the end-of-central-directory comment) holds the packed
  commit sha [TC4 VERIFIED with `unzip -z`, TC4:src/data/resourceFetch.ts:55-68].
  - Most Rust zip crates expose it (`zip::ZipArchive::comment()`).
  - Verify it against the pin. Never trust the archive's own metadata for this
    (TC4:src/data/resourceFetch.ts:480-483).
- The zip is wrapped in one top directory [MEM: `<repo>/`]. Strip whatever single top
  directory exists.
- The `zipball_url` fields in Release, Tag, CatalogEntry and CatalogStage point at this kind
  of archive [MEM: the `/archive/<tag>.zip` web URL].

#### C. Single files

- **Raw, web form:** `GET https://git.door43.org/{owner}/{repo}/raw/branch/{branch}/{path}`
  (UTC:docs/3-door43-api-developer-guide.mdx:407-410). The same form exists with
  `raw/tag/{tag}/` and `raw/commit/{sha}/` [MEM].
- **Raw, API form:** `GET /api/v1/repos/{o}/{r}/raw/{ref}/{filepath}` or `?ref=` (SW:14607).
  Use `media/{filepath}` (SW:12470) for LFS.
- **Contents:** `GET /api/v1/repos/{o}/{r}/contents/{filepath}?ref=` returns
  `ContentsResponse` (SW:21495): `{type, encoding, content(base64), sha, size, path, name,
  last_commit_sha, download_url, html_url, git_url, _links}`. For a directory it returns an
  array. `GET .../contents?ref=` lists the root (SW:7121).
- **Tree:** `GET /api/v1/repos/{o}/{r}/git/trees/{sha}?recursive=true&page=&per_page=`
  returns `{sha, url, tree:[{path, mode, type, size, sha, url}], truncated, page,
  total_count}` (SW:8145, SW:24208, SW:24138). Keep paging while `truncated` is true.

**Recommendation for Rust:** fetch the whole release as one zip (A or B), verify the sha,
then unpack. Reject zip-slip entries:

- an absolute path, a drive prefix (`C:`), or any all-dots segment such as `..`, `.. ` or
  `...`;
- treat `\` as a separator as well;
- see TC4:src/data/resourceFetch.ts:104-138.

### 1.7 Authentication (tokens)

Sign-in flow, as tC4 implements it (TC4:src/data/share/door43Api.ts:25-33, 258-296):

1. `GET /api/v1/user` with `Authorization: Basic base64(login:password)`. `login` may be the
   username or the email. The response gives `{login, …}` (`User`, SW:27383). Use `login`
   from here on, because the email cannot be used in routes.
2. `GET /api/v1/users/{login}/tokens?limit=50` with Basic auth returns
   `[AccessToken{id, name, scopes, token_last_eight}]` (SW:20048). **The list does not
   include the secret.**
3. For each token whose name equals your app's token name, call
   `DELETE /api/v1/users/{login}/tokens/{id}` with Basic auth → 204 (SW:20129).
   - The `{token}` path segment is "identified by ID and if not available by name".
   - The create call returns 400 when the name is already in use, so step 3 is required
     before step 4.
4. `POST /api/v1/users/{login}/tokens` with Basic auth and the body
   `{"name": "<AppTokenName>", "scopes": ["write:repository","write:organization","write:user"]}`
   (`CreateAccessTokenOption`, SW:21567).
   - The response is **201** `AccessToken {id, name, scopes, sha1, token_last_eight}`
     (SW:20205-20236).
   - **`sha1` is the secret, and it is shown only once.**
5. Later calls send `Authorization: token <sha1>`. To check a stored token, call
   `GET /api/v1/user`; a 401 means the token was revoked or expired
   (TC4:src/data/share/door43Api.ts:170-183).

More details:

- **Scopes:**
  - `write:repository` is needed for push and for `repos/search`.
  - `write:user` is needed for `POST /user/repos`.
  - `write:organization` is needed for `POST /orgs/{org}/repos`.
  - A write scope includes the read of its group.
  - DCS refused repo creation with the `read:*` scopes that the docs suggest (tC4 issue #467,
    TC4:src/data/share/door43Api.ts:49-60).
  - When a token lacks a scope, the answer is 403 with "required scope" in the message
    (TC4:src/data/share/shareOperation.ts:188-191).
- **tC3's token name was `translation-core`, and tC4's is `translationCore`**
  (TC4:src/data/share/door43Api.ts:45-47). Pick a distinct name for the Rust app, or reuse
  `translationCore` if the app replaces tC4.
- **Encode Basic credentials from UTF-8 bytes.** A non-Latin-1 password breaks naive
  encoders (TC4:src/data/share/door43Api.ts:62-67).
- **Storage:** tC4 keeps only the token. It never stores the password, name or email, and it
  keeps the token in the OS keychain when the user asks to stay signed in
  (TC4:src/data/share/session.ts:1-12).
- **Status codes** (docs): 401 = bad credentials, 403 = token invalid or insufficient,
  422 = bad request (UTC:docs/3-door43-api-developer-guide.mdx:651-657).

### 1.8 Users and organizations

- `GET /api/v1/user/orgs?limit=50&page=N` returns `[Organization{username, full_name, …}]`
  (SW:18997, SW:25378). Page until a short page arrives (TC4:src/data/share/door43Api.ts:185-196).
- `GET /api/v1/users/{username}/orgs/{org}/permissions` returns
  `{can_create_repository, can_read, can_write, is_admin, is_owner}` (SW:19886,
  `OrganizationPermissions`).
- `GET /api/v1/orgs/{org}` (SW `/orgs/{org}`) returns 404 for a renamed org; there is no
  redirect (TC4:docs/PLATFORM-NOTES.md:109).

### 1.9 Creating a repository

- **In the user's account:** `POST /api/v1/user/repos` (SW:19031).
- **In an org:** `POST /api/v1/orgs/{org}/repos` (SW:4297).
- **Body `CreateRepoOption`** (SW:22383):
  - `name` is required.
  - Optional fields: `description`, `private`, `auto_init`, `default_branch`, `gitignores`,
    `license`, `readme`, `issue_labels`, `template`, `trust_model`, `object_format_name`
    (`sha1|sha256`).
- **tC4's body:** `{"name": <name>, "auto_init": false, "default_branch": "main",
  "private": false}`.
  - With `auto_init: false` the first push is a fast-forward. An initial commit on the server
    would make the push non-fast-forward (TC4:src/data/share/door43Api.ts:244-249).
  - The docs' example uses `auto_init: true`, which is wrong for a push-existing-history
    flow (UTC:docs/3-door43-api-developer-guide.mdx:465-477).
- **Responses:**
  - `201 Repository` with `full_name`, `html_url` and `clone_url`.
  - `409` "The repository with the same name already exists" on `/user/repos`.
  - `422` validation error, `400` error.
  - The org route lists 403 and 404 but no 409 in the swagger.
- **Validate names client-side** against `^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$`
  (TC4:src/data/share/door43Api.ts:112-122).
- **Check that `clone_url` and `html_url` are on the configured server** (same origin, no
  userinfo) before you send credentials (TC4:src/data/share/shareOperation.ts:87-101,
  158-162).
- **Naming conventions:**
  - RC: `{lang}_{resource}` (UTC:docs/2-unfoldingword-developer-guide.mdx:703-726).
  - tC3 projects: `{lang}_{resource}_{book}_book`
    (UTC:docs/4-repository-formats/tool-generated/translationcore-format-guide.mdx:173-177).
  - tC4 defaults to the project folder name (TC4:src/data/share/shareOperation.ts:59-60).

### 1.10 Pushing via git over HTTPS

- **Remote:** `clone_url` from the create response (e.g. `https://git.door43.org/<owner>/<repo>.git`).
  tC4 adds it as `origin` and never edits it later. The existence of `origin` is the record
  that a share happened (TC4:src/data/share/shareOperation.ts:17-19, 141-165).
- **Credentials:** HTTPS Basic, username = Door43 login and password = the token (`sha1`).
  - tC4 sends them to its platform push route as
    `{remote, cred_type:"https", username, pass_key}`, which runs libgit2
    (TC4:src/data/serverApi.ts:518-530; TC4:docs/PLATFORM-NOTES.md:155).
  - "The credentials never reach the git config: after a push, `.git/config` holds the remote
    url only" (TC4:docs/PLATFORM-NOTES.md:155).
  - In Rust, use a `git2` `RemoteCallbacks::credentials` returning
    `Cred::userpass_plaintext(login, token)`, or a gix credential helper. Never put the token
    in the URL.
  - [MEM] Gitea accepts a personal access token as the HTTP password.
- **Non-fast-forward:** libgit2 reports `code=NotFastForward (-11)` and the remote is
  unchanged. tC4 maps this to "another device has pushed this project"
  (TC4:src/data/share/shareOperation.ts:199-214). Never force-push (shareOperation.ts:4-5).
- **Order of operations:** commit the pending work (checkpoint) → create the repo if there is
  no `origin` → add the remote → push `main` (TC4:src/data/share/shareOperation.ts:7-16).

### 1.11 API file writes (an alternative to git; optional)

- **Create one file:** `POST /repos/{o}/{r}/contents/{filepath}` with `CreateFileOptions{content(b64)
  required, message, branch, new_branch, author, committer, dates, signoff}` → 201.
- **Update one file:** `PUT` the same path with `UpdateFileOptions{sha, content}` (both
  required), plus `from_path` for a rename → 200.
- **Delete one file:** `DELETE` the same path with `DeleteFileOptions{sha}` (SW:7217+).
- **Multi-file commit:** `POST /repos/{o}/{r}/contents` with
  `ChangeFilesOptions{files:[ChangeFileOperation{operation, path, content, sha, from_path}]}`
  → 201 (SW:7121).
- **Diffpatch:** `POST /repos/{o}/{r}/diffpatch` (SW:7449). The swagger lists the body as
  `UpdateFileOptions`, but the docs say `{message, patch}`
  (UTC:docs/3-door43-api-developer-guide.mdx:493-503). Unclear; test it on QA.
- **Docs error:** the docs use `PUT` for create (UTC:docs/3-door43-api-developer-guide.mdx:479-491).
  In this Gitea, `PUT` is update-only and needs `sha`.

### 1.12 Rate limits, caching, errors

- The docs contradict each other:
  - "DCS does not enforce documented API rate limits … does not send X-RateLimit headers"
    (UTC:docs/2-unfoldingword-developer-guide.mdx:616-618;
    UTC:docs/3-door43-api-developer-guide.mdx:115-117).
  - "60/hour (anonymous), 1000+/hour (authenticated)"
    (UTC:docs/automation/door43-api-reference-for-mcp.mdx:18, 327-337).
- Back off exponentially on 429 and 5xx. Cache catalog answers.
- Content at a sha is immutable, so cache it forever by `(repo, sha)`.

### 1.13 Doc versus swagger mismatches (API)

| Docs say | Reality | Evidence |
|---|---|---|
| Token response has `"token": "abc…"` | The secret is `sha1` | UTC:docs/3-door43-api-developer-guide.mdx:624-631; SW:20225; TC4:src/data/share/door43Api.ts:286-295 |
| Scopes `read:repository`, `write:repository`, `read:organization` | Repo creation needs `write:user` or `write:organization` | TC4:src/data/share/door43Api.ts:49-60 |
| `PUT /contents/{path}` creates or updates (sha optional) | `POST` creates; `PUT` updates and requires `sha` | UTC:docs/3-…:479-491; SW:7217 |
| `auto_init: true` when creating | Use `false` for a push of existing history | UTC:docs/3-…:471-476; TC4:door43Api.ts:244-249 |
| Catalog subject "Translation Notes" | "TSV Translation Notes" | TC4:src/data/gateways.ts:17 |
| `GET /api/v1/repos?org=X` (automation docs) | Not a swagger path; use `/orgs/{org}/repos` or `/repos/search?owner=X` | UTC:docs/automation/door43-repository-analyzer-mcp.mdx:37-38 |
| The token in the docs' logout uses `{token}` | The DELETE `{token}` is the token **id** (or name) | SW:20129 |

---

## 2. Repository layouts and file formats

### 2.0 Detecting the metadata type

The order follows UTC:docs/3-door43-api-developer-guide.mdx:269-302 and
UTC:docs/automation/door43-api-reference-for-mcp.mdx:263-291, with corrections:

| Format | File | Detect by |
|---|---|---|
| Resource Container | `manifest.yaml` | Has a `dublin_core` key (`conformsto: rc0.2`) |
| Scripture Burrito | `metadata.json` | **Top-level `"format": "scripture burrito"`** in SB 1.0 (all FIX exports; `meta.version: "1.0.0"`). The docs check `meta.format` (the SB 0.x shape, UTC:docs/3-…:287), which misses real exports. Check both. |
| translationCore 3 project | `manifest.json` | `tc_version`, `generator.name == "tc-desktop"`, or `project` + `resource` + `target_language` with no `dublin_core`/`meta` |
| translationStudio | `manifest.json` | `package_version` and a `generator.name` that contains `ts-` |
| "TCore RC" | any | Has `resource_container` (UTC:docs/3-…:233-242) |

DCS reports the type as `metadata_type ∈ {rc, sb, tc, ts}` (SW catalog `metadataType` enum).

### 2.1 RC `manifest.yaml`

The shape below comes from the docs; there is no RC fixture in tC4. Sources:
UTC:docs/2-unfoldingword-developer-guide.mdx:283-316, 436-460;
UTC:docs/5-migration/tc-to-rc/new-rc-creation.mdx:100-145.

```yaml
dublin_core:
  conformsto: 'rc0.2'
  identifier: 'ult'            # ult|ust|glt|gst|uhb|ugnt|tn|tw|twl|ta|tq|obs|obs-tn|...
  title: 'unfoldingWord® Literal Text'
  language: {identifier: 'en', title: 'English', direction: 'ltr'}
  subject: 'Aligned Bible'
  type: 'bundle'               # bundle | help | dict | man | book
  format: 'text/usfm3'
  version: '86'
  relation: ['en/tw', 'en/tn', 'hbo/uhb?v=2.1.30', 'el-x-koine/ugnt?v=0.34']
  publisher, rights: 'CC BY-SA 4.0', issued, modified, contributor:[...], creator, description, source:[...]
checking: {checking_entity: [...], checking_level: '3'}
projects:
  - {identifier: 'gen', title: 'Genesis', path: './01-GEN.usfm', sort: 1,
     versification: 'ufw', categories: ['bible-ot']}
```

- **`relation`** entries are `lang/resource` with an optional `?v=<version>`. The version is
  without the `v` prefix; tC4 maps `hbo/uhb?v=3.0.0` to tag `v3.0.0`
  (TC4:src/data/import/tc3.ts:194-196; TC4:src/data/installedSuite.js:69-72: "en_tn v91 and
  en_ult v91 declare `hbo/uhb?v=3.0.0`").
- **Container types and file patterns** (UTC:docs/2-…:424-434):

  | Type | Content pattern |
  |---|---|
  | `bundle` | `NN-BOOK.usfm` |
  | `help` | `{prefix}_{BOOK}.tsv` |
  | `dict` | `bible/{kt,names,other}/{slug}.md` |
  | `man` | `{manual}/{slug}/{01,title,sub-title}.md` |
  | `book` | `{project}/{chapter}/{chunk}.md` |

- **Book file numbering.** OT is `01-GEN` … `39-MAL`. **NT starts at 41**: `41-MAT` …
  `67-REV`. Front matter is `A0-FRT.usfm`.
  - Evidence: `57-TIT.usfm` (TC4:docs/evidence/sb-zip-zaln-2026-07-25.md:12-13) and
    `32-JON.usfm` (FX:README.md:9).
  - The docs disagree with each other: "40-MAT (note: some use 41-MAT)"
    (UTC:docs/4-repository-formats/resource-container/bible-text-repositories-guide.mdx:126-132, 215-216)
    and "Revelation=66" (UTC:docs/5-migration/tc-to-rc/book-integration.mdx:98-112).
  - Do not compute file names from a formula. Read them from `projects[].path`.
- **Help TSV names** in RC repos are `tn_TIT.tsv` and `twl_TIT.tsv`
  (TC4:docs/evidence/tw-twl-sbzip-combined-2026-08-03.md:39), and `tq_TIT.tsv` [DOC].
- **dict/man repos.** The docs say there is no `projects[]` (UTC:docs/4-…/translation-words-repositories-guide.mdx:57-64).
  [MEM] en_tw and en_ta do list projects (`bible`, and `intro|process|translate|checking`).
  Do not depend on either; walk the tree.

### 2.2 Scripture Burrito `metadata.json` (the DCS export, `go-rc2sb`)

This is from a real file (FX:resources/es-419_tn@v66/metadata.json, abridged):

```json
{
  "format": "scripture burrito",
  "meta": {"version": "1.0.0", "category": "source",
           "generator": {"softwareName": "go-rc2sb", "softwareVersion": "v0.4.0", "userName": ""},
           "defaultLocale": "en", "dateCreated": "2026-08-03T15:45:55.871Z", "normalization": "NFC"},
  "idAuthorities": {"dcs": {"id": "https://git.door43.org", "name": {"en": "Door43 Content Service"}}},
  "identification": {
    "primary": {"dcs": {"Idiomas-Puentes/es-419_tn": {"revision": "22f3d0c61e2ab4701cb869547de9c3c43da07208",
                                                      "timestamp": "2026-08-03T15:45:55.871Z"}}},
    "name": {"en": "Notas de Traducción"}, "description": {"en": "…"}, "abbreviation": {"en": "TN"}},
  "languages": [{"tag": "es-419", "name": {"en": "Español (Latin American Spanish)"}, "scriptDirection": "ltr"}],
  "type": {"flavorType": {"name": "parascriptural", "flavor": {"name": "x-bcvnotes"},
                          "currentScope": {"3JN": [], "JON": [], "RUT": [], "TIT": []}}},
  "confidential": false,
  "localizedNames": {"book-tit": {"abbr": {"en": "Tit"}, "short": {"en": "Titus", "es-419": "Tito"},
                                  "long": {"en": "The Letter of Paul to Titus", "es-419": "Tito"}}},
  "ingredients": {
    "ingredients/TIT.tsv": {"checksum": {"md5": "de3fe1e17dcaae7fef8399f3ce546ab7"},
                            "mimeType": "text/tab-separated-values", "size": 73425, "scope": {"TIT": []}},
    "ingredients/LICENSE.md": {"checksum": {"md5": "…"}, "mimeType": "text/markdown", "size": 2192}},
  "copyright": {"shortStatements": [{"statement": "© Idiomas Puentes 2024, CC BY-SA 4.0",
                                     "mimetype": "text/plain", "lang": "en"}]}
}
```

- **Ingredient keys** include the `ingredients/` prefix. Each value has `checksum.md5`,
  `mimeType`, `size` and an optional `scope: {BOOK: []}`. `[]` means the whole book.
  - **Verify the md5 and size on install.**
  - USFM ingredients are `mimeType: "text/plain"` in the en_ult export
    (FX:resources/en_ult@v89/metadata.json), not `text/usfm`.
- **The ULT flavor** carries extra fields: `{"name": "textTranslation", "usfmVersion": "3.0",
  "translationType": "revision", "audience": "common", "projectType": "standard"}`
  (FX:resources/en_ult@v89/metadata.json).
- **Quirks:**
  - The en_twl export says `abbreviation: {"en": "TW"}`, not TWL
    (FX:resources/en_twl@v86/metadata.json).
  - The `uW`-org lexicon burritos use the `uWBurritos` id authority with revision `"1"` (not a
    sha), and an ingredient table that omits the payload
    (FX:resources/en_ugl@d9d29e2d5892/metadata.json; .../PROVENANCE.md:7-10).
- **The docs' SB examples** (`meta.format`, `identification.name` as a string, `role`
  fields, `relationships[]`) are the SB 0.3 shape or proposals. Real DCS exports carry none of
  `role` or `relationships`
  (UTC:docs/4-repository-formats/scripture-burrito/scripture-burrito-repositories-guide.mdx:43-89;
  UTC:docs/5-migration/rc-to-sb/general-conversion-guide.mdx:239-275).

**Flavor and path map for the DCS export** (from FIX metadata, TC4:src/data/installedSuite.js
and TC4:docs/PLATFORM-NOTES.md:123):

| Resource | Export flavor (`flavorType/flavor`) | RC path | Export ingredient path |
|---|---|---|---|
| ULT, UST, UHB, UGNT | `scripture/textTranslation` | `57-TIT.usfm`, `A0-FRT.usfm` | `ingredients/TIT.usfm`, `ingredients/FRT.usfm` |
| tN | `parascriptural/x-bcvnotes` | `tn_TIT.tsv` | `ingredients/TIT.tsv` |
| TWL | `parascriptural/x-bcvarticles` | `twl_TIT.tsv` | `ingredients/TIT.tsv` (also lists 953 `payload/**.md` articles in the metadata) |
| tW | `parascriptural/x-bcvarticles` (the catalog says `peripheral/x-peripheralArticles`) | `bible/kt/god.md` | `ingredients/payload/kt/god.md`, plus `ingredients/payload/config.yaml`, **plus 66 TWL TSVs built from `en_twl` master** |
| tA | `peripheral/x-peripheralArticles` | `translate/figs-abstractnouns/01.md` | `ingredients/translate/figs-abstractnouns/01.md`, plus `ingredients/translate/{toc,config}.yaml` |
| tQ | `parascriptural/x-bcvquestions` | `tq_TIT.tsv` | `ingredients/TIT.tsv` (TC4:docs/BURRITO-SPEC.md:350) |
| OBS | `gloss/textStories` | `content/01.md` … | [TC4] `content/01.md` etc. (TC4:docs/BURRITO-SPEC.md:583-602) |
| OBS-tN | `peripheral/x-obsnotes` | [MEM] `tn_OBS.tsv` | `ingredients/OBS.tsv` |
| OBS-tQ | `peripheral/x-obsquestions` | [MEM] `tq_OBS.tsv` | `ingredients/OBS.tsv` |
| OBS-TWL | `parascriptural/x-bcvarticles` | [MEM] `twl_OBS.tsv` | `ingredients/OBS.tsv` |
| Lexicon (`uW/en_ugl`, `uW/en_uhl`) | `peripheral/x-lexicon` | — | `ingredients/content/<n>.json` (TC4:src/data/installedSuite.js:77-86) |
| OBS images (`uW/obs_images_360`) | `peripheral/x-obsimages` | — | — |

Counts from FIX metadata:

- en_tw v91 has 1021 ingredients: 66 TSV, 190 kt, 355 names, 408 other, 1 config.yaml and
  1 LICENSE.
- en_twl v86 has 1003 (48 TSV + 953 md); en_twl v91 has 1014 (59 TSV + 953 md).
- en_ta v86 has 732 ingredients.
- en_tn v86 has 49 ingredients (48 TSV); en_tn v91 has 60.
- en_ult v89 has 56 ingredients (55 books + LICENSE).

Note: TC4's evidence says the en_twl export is "links-only"
(TC4:docs/evidence/tw-twl-sbzip-combined-2026-08-03.md:30-31). The en_twl v86 and v91
metadata nevertheless lists the 953 `payload/*.md` articles. Trust the metadata, and ignore
articles that come from a `_twl` pin (TC4:docs/BURRITO-SPEC.md:349).

### 2.3 UHB and UGNT (original languages)

**UHB** (`hbo_uhb`, `rtl`), from a real file (FX:hbo_uhb/JON.usfm, v3.0.0, tag commit
`74022f0f…`):

```usfm
\id JON unfoldingWord® Hebrew Bible
\usfm 3.0
\ide UTF-8
\h Jonah
\toc1 The Book of Jonah
\toc2 Jonah
\toc3 Jon
\mt Jonah

\c 1
\p

\v 1
\w וַֽ⁠יְהִי֙|lemma="הָיָה" strong="c:H1961" x-morph="He,C:Vqw3ms"\w*
\w דְּבַר|lemma="דָּבָר" strong="H1697" x-morph="He,Ncmsc"\w*־\w יְהוָ֔ה|lemma="יְהֹוָה" strong="H3068" x-morph="He,Np"\w*
```

- **One `\w` per original word.** The attributes are `lemma`, `strong` and `x-morph` (only
  the last has the `x-` prefix).
- **`strong`** carries Hebrew prefix morphemes before the colon: `c:` (conjunction), `d:`
  (article), `l:`, `b:`, `m:`, `k:`, and stacked forms such as `m:l:H6440` or `c:d:`.
  - The number may carry a letter suffix (`H1121a`, `H5892b`) and is zero-padded to 4 digits
    (`H0413`).
  - Prefix-only words occur: `strong="l"` 17 times and `strong="b"` 3 times in JON.
  - Counts in JON: `c:` 127, `d:` 68, `l:` 29, `m:` 23, `b:` 16.
- **`x-morph`:** `He,` (or `Ar,` for Aramaic [MEM]), then colon-separated morph per
  morpheme: `He,C:Vqw3ms`, `He,R:Ncbpc:Sp1cs`.
- **Unicode** (FX:README.md:9; counted in JON):
  - **U+2060 WORD JOINER inside words** separates prefix morphemes (393×). Tokenizers must
    keep it inside the word (TC4:src/data/align/tokenize.ts:12-16).
  - **Maqaf U+05BE** joins words between `\w` markers, outside the markup (104×).
  - Sof pasuq U+05C3 appears 48× and paseq U+05C0 3×.
- **Versification:** UHB uses the English (`ufw`) frame, with `\ca N\ca*` and `\va N\va*`
  giving the Hebrew numbering. JON 1:17 is `\ca 2\ca* \v 17 \va 1\va*` (the Hebrew 2:1), and
  2:1 has `\va 2\va*` (JON has 11 `\va` and 1 `\ca`).
- **Qere/Ketiv:** in v3.0.0 "its Qere readings are footnotes and its main text is the Ketiv"
  (TC4:src/data/installedSuite.js:69-72). JON has no `\f`.
- **The docs' sample is wrong.** UTC:docs/4-…/bible-text-repositories-guide.mdx:139-153 shows
  plain untokenized Hebrew; real UHB is fully `\w`-tokenized.

**UGNT** (`el-x-koine_ugnt`): there is **no fixture** in tC4. The pinned version is
`v0.34`, sha `fc95b2b8…` (TC4:src/data/installedSuite.js:74).

- [MEM] It has the same `\w word|lemma="…" strong="G…" x-morph="Gr,…"\w*` shape.
  `strong` is `G` plus 5 digits (`G39720`), matching the `x-strong` values that ULT
  alignments copy (FX:en_ult/TIT.usfm).
- [MEM] It may carry `\k-s`/`\k-e` milestones and `x-tw="rc://*/tw/dict/…"` on some words in
  older versions.
- **Gap:** get a UGNT book (TIT) for the Rust fixtures.

### 2.4 ULT and UST (aligned gateway Bibles)

From a real file (FX:en_ult/TIT.usfm, v89, commit `84c73ba0…`):

```usfm
\id TIT EN_ULT en_English_ltr Wed Aug 24 2022 09:32:57 GMT-0400 (Eastern Daylight Time) tc
\usfm 3.0
\ide UTF-8
\h Titus
\toc1 The Letter of Paul to Titus
\toc2 Titus
\toc3 Tit
\mt1 Titus

\ts\*
\c 1
\p
\v 1 \zaln-s |x-strong="G39720" x-lemma="Παῦλος" x-morph="Gr,N,,,,,NMS," x-occurrence="1" x-occurrences="1" x-content="Παῦλος"\*\w Paul|x-occurrence="1" x-occurrences="1"\w*\zaln-e\*,
\zaln-s |x-strong="G14010" x-lemma="δοῦλος" x-morph="Gr,N,,,,,NMS," x-occurrence="1" x-occurrences="1" x-content="δοῦλος"\*\w a|x-occurrence="1" x-occurrences="1"\w*
\w servant|x-occurrence="1" x-occurrences="1"\w*\zaln-e\*
\zaln-s |x-strong="G23160" x-lemma="θεός" x-morph="Gr,N,,,,,GMS," x-occurrence="1" x-occurrences="2" x-content="Θεοῦ"\*\w of|x-occurrence="1" x-occurrences="5"\w*
\w God|x-occurrence="1" x-occurrences="2"\w*\zaln-e\*
```

- **`\zaln-s |…\*` (the start milestone) describes ONE original-language word:**
  - `x-strong`: copied from UGNT/UHB, including Hebrew prefixes such as `b:H7225` [DOC].
  - `x-lemma`.
  - `x-morph`.
  - `x-occurrence` / `x-occurrences`: the **verse-level** ordinal and count of that original
    surface word (`x-content`) in the original verse.
  - `x-content`: the exact original surface form.
- **`\zaln-e\*`** closes the milestone.
- **`\w word|x-occurrence="n" x-occurrences="m"\w*`** inside the milestone is a target word.
  Its occurrence and occurrences are the verse-level ordinal and count of that exact token in
  the **gateway** verse (`of` #1 of 5).
- **Relationship types** (UTC:docs/2-unfoldingword-developer-guide.mdx:1406-1508):
  - 1:1.
  - 1:N: one `zaln` wraps several `\w`.
  - N:1: nested `\zaln-s\zaln-s \w \zaln-e\zaln-e`.
  - N:M: nested milestones wrapping several words.
  - Measured nesting depth in TIT: ULT reaches 3 and UST reaches 5.
- **One original word can be rendered by several separate groups** with the same
  `x-content`/`x-occurrence`. In TIT 1:1, ἐκλεκτῶν#1 is "of" (one group) and also "the chosen
  people" (another group) (FX:en_ult/TIT.usfm; TC4:src/data/sourceHighlight.ts:242-249).
- **Unaligned target words** may appear as bare `\w` outside any `zaln`, and an original
  word with no rendering appears as an empty `\zaln-s…\*\zaln-e\*` [DOC example:
  UTC:docs/4-…/bible-text-repositories-guide.mdx:176, `אֵת`].
- **Punctuation and spaces** are bare text between `\w` and `\zaln-e`.
- **`\ts\*` chunk markers** occur 23 times in TIT; they are presentation-only.
- **The `\id` line** carries tool junk after the book code (`EN_ULT en_English_ltr <date> tc`).
- **Markers in TIT:** ULT has 705 `\zaln-s`/`\zaln-e`, 932 `\w`, 46 `\v` and 3 `\c`. UST has
  707 and 1822 `\w`.
  - `grep -c zaln-s` returns 656 for ULT because it counts *lines*
    (TC4:docs/evidence/sb-zip-zaln-2026-07-25.md:16).
- **Mid-line verses.** Real aligned corpora put `\v N` in the middle of a line after a
  paragraph marker (`\q1 \v 3 \zaln-s …`): there are 5,138 such lines in en_ult v89. A
  line-start parser silently drops those verses (TC4:src/data/seed.ts:9-13).
- **Integer occurrences.** Attribute parsing yields strings. Store `occurrence` and
  `occurrences` as integers, because the alignment libraries fail on strings
  (TC4:src/data/align/occurrences.ts:1-17).
- **The docs' Genesis 1:1 example** omits `x-occurrence` on `\zaln-s` and `x-occurrences`
  on `\w` (UTC:docs/4-…/bible-text-repositories-guide.mdx:176;
  UTC:docs/2-…:321). Real files carry all four attributes.
- **tC4 alignment sidecar model.** Use it if you store alignments outside USFM.
  `{topWords:[{word,strong,lemma,morph,occurrence,occurrences}],
  bottomWords:[{word,occurrence,occurrences}]}` per alignment, with a per-verse `wordBank`
  (TC4:src/data/align/zaln.ts:3-42). This is the same model as tC3's
  `alignmentData/<book>/<ch>.json` (TC4:src/data/import/tc3.ts:103-123).

### 2.5 Translation Notes (tN) — TSV7, current

The header is exact, and tC4 refuses anything else (TC4:src/data/derive.ts:134-148):

```text
Reference	ID	Tags	SupportReference	Quote	Occurrence	Note
```

From a real file (FX:resources/en_tn@v86/TIT.tsv; the Note column is truncated here):

```text
front:intro	m2jl				0	# Introduction to Titus\n\n## Part 1: General Introduction\n\n### Outline of Titus\n\n1. … ([1:1–16](../01/01.md))…
1:intro	c7me				0	# Titus 1 Chapter Introduction\n\n… (See: [[rc://*/ta/man/translate/figs-abstractnouns]]) …
1:1	rtc9		rc://*/ta/man/translate/figs-abstractnouns	κατὰ πίστιν ἐκλεκτῶν Θεοῦ καὶ ἐπίγνωσιν ἀληθείας	1	The words **faith**, **knowledge**, and **truth** are abstract nouns. … Alternate translation: [to help God’s chosen people …]
1:1	xrtm		rc://*/ta/man/translate/figs-activepassive	ἐκλεκτῶν Θεοῦ	1	If your language does not use this passive form, …
```

- **`Reference`:** `front:intro` (book intro), `N:intro` (chapter intro), or `c:v`.
  - [MEM] Ranges `c:v-v`, `c:v-c:v` and comma lists occur in other books.
  - Keep span verses as strings, because `Number("9-10")` is NaN (TC4:src/data/derive.ts:125-128,
    44-54). A verse may also be `front` or have a letter suffix (derive.ts:48-50).
- **`ID`:** a 4-character `[a-z0-9]` id. It is stable across languages: 89 of the 157 en
  checkIds also exist in es-419 (FX:resources/README.md:42-44).
- **`Tags`:** empty in every en/es row of the fixtures. The docs' "grammar/culture" tags
  (UTC:docs/4-…/translation-notes-repositories-guide.mdx:117-123) do not exist in real data.
- **`SupportReference`:** `rc://*/ta/man/translate/<slug>`, or empty for a plain note.
  - The last path segment is the tC "groupId".
  - Empty rows (49 of 206 in en TIT, 104 of 216 in es-419 TIT) are plain notes, not checks
    (TC4:src/data/derive.ts:279-291).
  - tC3 groups the slugs into categories (TC4:src/data/derive.ts:207-256).
- **`Quote`:** **original-language** text (Greek in the NT, Hebrew in the OT), copied from
  UGNT/UHB surface forms with punctuation, such as `Θεοῦ,` in v91 TIT 1:1.
  - ` & ` separates discontinuous spans (`ἐφανέρωσεν & τὸν λόγον αὐτοῦ`; 18 of 206 en TIT
    rows; 5 in JON).
  - Hebrew quotes contain U+2060 joiners and maqaf (`וַֽ⁠יְהִי֙ דְּבַר־יְהוָ֔ה`).
  - es-419 v66 has legacy rows whose Quote is an English label such as
    `Connecting Statement:` or `General Information:` with Occurrence 0
    (FX:resources/es-419_tn@v66/TIT.tsv:28).
- **`Occurrence`:** 1-based. It is the occurrence of the **whole quote** in the original
  verse.
  - `0` means "not tied to words": intro rows and label rows.
  - [MEM] `-1` historically meant "all occurrences". No fixture row has it.
- **`Note`:** Markdown on one physical line, with **literal `\n`** (backslash-n) for
  newlines.
  - It contains `[[rc://*/ta/man/translate/x]]` links, relative links `../01/01.md`,
    `**bold**`, and "Alternate translation: [ … ]".
  - Some rows carry escaped markdown (`\\*\\*And\\*\\*` in JON).
- **es-419 links** use `rc://es-419/ta/…` instead of `rc://*/…` in notes
  (FX:resources/es-419_tn@v66/TIT.tsv).
- **Counts** (FX:resources/README.md:34-44):
  - en TIT v86 has 206 data rows → 157 check items, 28 groupIds.
  - es-419 TIT has 216 rows → 112 items.
  - JON v86 has 196 rows.
- **The docs' sample TSV** has English quotes and a `grammar` tag
  (UTC:docs/4-…/translation-notes-repositories-guide.mdx:126-130;
  UTC:docs/2-…:1047-1057). Both are wrong for the real en_tn.

### 2.6 The older tN TSV9 format [MEM — not in these sources]

tC4 only mentions that "the TSV9 GLQuote column is gone" (TC4:src/data/sourceHighlight.ts:263-268;
TC4:src/views/HelpsPanel.jsx:416-419). From memory, the pre-2021 en_tn and current
legacy-GL repos use this format:

```text
Book	Chapter	Verse	ID	SupportReference	OrigQuote	Occurrence	GLQuote	OccurrenceNote
TIT	1	1	rtc9	rc://*/ta/man/translate/figs-abstractnouns	κατὰ πίστιν	1	for the faith	…
```

- The files are named `<lang>_tn_<NN>-<BOOK>.tsv`, e.g. `en_tn_57-TIT.tsv`.
- The columns map to TSV7 as follows:
  - `Book` + `Chapter` + `Verse` become `Reference`; `Chapter` may be `front` and `Verse` may
    be `intro`.
  - `OrigQuote` becomes `Quote`.
  - `OccurrenceNote` becomes `Note`. Newlines were `<br>`.
  - `GLQuote` (the gateway-language rendering) is dropped.
- Old SupportReference values could be bare slugs (`figs-metaphor`) instead of `rc://`.
- If the Rust app must read older GL tN, parse it by header-sniffing. Never guess-parse
  (TC4:src/data/derive.ts:130-133).

### 2.7 Translation Words Links (TWL) — TSV6

The header is exact (TC4:src/data/derive.ts:134):

```text
Reference	ID	Tags	OrigWords	Occurrence	TWLink
```

A real export row (FX:resources/en_twl@v86/TIT.tsv):

```text
1:1	trr8	name	Παῦλος	1	./payload/names/paul.md
1:1	zfgc		δοῦλος	1	./payload/other/servant.md
1:1	pmq8	keyterm	Θεοῦ	1	./payload/kt/god.md
1:1	nze4	keyterm; name	Ἰησοῦ	1	./payload/kt/jesus.md
1:1	f44k	keyterm	Θεοῦ	2	./payload/kt/god.md
```

- **`Tags`** values are `keyterm`, `name`, `keyterm; name` or empty.
  - In en TIT v86: keyterm 107, empty 71, name 6, `keyterm; name` 4.
  - The docs' "kt/names/other" (UTC:docs/4-…/translation-words-links-repositories-guide.mdx:102-124)
    is wrong. The **category comes from the TWLink path** (`kt|names|other`), not from Tags
    (TC4:src/data/derive.ts:150-179).
- **`OrigWords`:** an original-language word or phrase. ` & ` is possible
  (TC4:src/data/sourceHighlight.ts:116-118).
- **`Occurrence`:** verse-level, usually 1. TIT 1:1 has Θεοῦ #2.
- **`TWLink` has two forms. Readers must accept both** (TC4:docs/BURRITO-SPEC.md:349):
  - The RC source branch uses `rc://*/tw/dict/bible/names/paul`
    (TC4:docs/evidence/tw-twl-sbzip-combined-2026-08-03.md:36-41).
  - The sb-zip export uses `./payload/names/paul.md`. All 188 of 188 rows in en_twl v86 and
    182 of 182 in en_tw v87 TIT use it.
  - **But the OBS-TWL v3 export still uses `rc://*/tw/dict/bible/kt/god`** in all 2381 of
    2381 rows (FX:resources/en_obs-twl@v3/OBS.tsv). The claim "every sb-zip export rewrites"
    (TC4:docs/BURRITO-SPEC.md:349) is therefore not true for OBS.
  - Normalize either form: slug = last segment with `.md` stripped, category = the segment
    before it.
- **Article gate.** A TWL row whose article is missing from the pinned tW set must be dropped.
  Example: en_tw v91 lacks `kt/arcofthecovenant.md`, which 2SA row `vwrw` links to
  (FX:resources/README.md:25; TC4:src/data/derive.ts:181-203).
- **Counts:** en TIT v86 has 188 rows → 188 items, categories kt 111, other 71, names 6
  (FX:resources/README.md:40-41). en_tw v91 `TIT.tsv` (TWL built from master) has 182 rows,
  which differs from en_twl v86.

### 2.8 Translation Words (tW) — Markdown dictionary

- **RC layout:** `bible/kt/*.md`, `bible/names/*.md`, `bible/other/*.md`
  (UTC:docs/4-…/translation-words-repositories-guide.mdx:68-99).
- **Export layout:** `ingredients/payload/{kt,names,other}/*.md` and
  `ingredients/payload/config.yaml` (FX metadata). en_tw v91 has 190 kt, 355 names and
  408 other articles.
- **Real article format** (FX:resources/en_tw@v87/payload-kt-god.md):

  ```markdown
  # God

  ## Definition:

  In the Bible, the term “God” refers to …
  * God has always existed; …

  ## Translation Suggestions:
  …
  (Translation suggestions: [How to Translate Names](rc://en/ta/man/translate/translate-names))

  (See also: [create](../other/creation.md), [false god](../kt/falsegod.md), …)

  ## Bible References:

  * [1 John 1:7](rc://en/tn/help/1jn/01/07)
  * [Genesis 1:2](rc://en/tn/help/gen/01/02)

  ## Examples from the Bible stories:

  * __[1:1](rc://en/tn/help/obs/01/01)__ __God__ created the universe …

  ## Word Data:

  * Strong’s: H0136, H0305, H0410, … G23160, …
  ```

- **Headings** end with a colon, in this order: Definition, Translation Suggestions, Bible
  References, Examples from the Bible stories, Word Data (god.md lines 3, 15, 26, 47, 62).
  - The docs drop the colons, reorder the sections, and show a DokuWiki
    `{{topic>god&nocomments}}` placeholder (UTC:docs/4-…/translation-words-repositories-guide.mdx:130-156).
    That is outdated.
- **Link forms inside articles:**
  - `rc://en/ta/man/translate/<slug>` (tA).
  - `rc://en/tn/help/<book>/<cc>/<vv>` (zero-padded chapter and verse).
  - `rc://en/tn/help/obs/<ss>/<ff>` (OBS story/frame).
  - Relative `../kt/x.md` between articles.
- **Strong's lists** use H-numbers zero-padded to 4 digits and G-numbers in the 5-digit UGNT
  style (`G23160`). A Rust lemma → tW index can key on them.

### 2.9 Translation Academy (tA) — Markdown manual

- **RC layout:** `<manual>/<slug>/{title.md, sub-title.md, 01.md}`, with manuals `intro`,
  `process`, `translate` and `checking`. Each manual also has **`toc.yaml` and
  `config.yaml`**, which the docs omit.
  - Evidence: FX:resources/en_ta@v86/metadata.json ingredient list (732 entries);
    UTC:docs/4-…/translation-academy-repositories-guide.mdx:66-100.
- **The subtitle file is `sub-title.md`.** UTC:docs/2-…:1242, 1252 and
  UTC:docs/4-…/translation-academy-repositories-guide.mdx:37 wrongly say `subtitle.md`.
- **`title.md`** contains the bare title with no `#` and **no trailing newline**: `Abstract
  Nouns` (FX:resources/en_ta@v86/translate-figs-abstractnouns-title.md, 14 bytes).
- **`01.md` starts directly with `### Description`.** It has **no** `# Title` / `## Subtitle`
  header, contrary to the docs (UTC:docs/2-…:1255-1271;
  UTC:docs/4-…/translation-academy-repositories-guide.mdx:106-130).
  - The sections are `### Description`, `### Reason This Is a Translation Issue`,
    `### Examples From the Bible`, `### Translation Strategies` and `### Examples of
    Translation Strategies Applied` (FX:resources/en_ta@v86/translate-figs-abstractnouns-01.md:1,10,14,36,42).
  - Assemble the display as `# {title}\n## {sub-title}\n\n{01.md}`.
- **`toc.yaml`** (FX:resources/en_ta@v86/translate-toc.yaml) is a nested tree: `title`, then
  `sections: [{title, link?, sections?[…]}]`. `link` is the article slug. The translate toc
  has 177 links.
- **`config.yaml`** [MEM] maps a slug to `{dependencies: [...], recommended: [...]}`.
- **tN `SupportReference` `rc://*/ta/man/translate/<slug>`** resolves to
  `translate/<slug>/`. All 28 TIT groupIds exist in the v86 translate toc
  (FX:resources/README.md:36-39).

### 2.10 Translation Questions (tQ) — TSV7

- **Header:** `Reference	ID	Tags	Quote	Occurrence	Question	Response`
  (TC4:src/data/derive.ts:136, verified against the en_tq v89 export, derive.ts:317-322).
- **Per book:** RC `tq_TIT.tsv`; export `ingredients/TIT.tsv`, flavor
  `parascriptural/x-bcvquestions` (TC4:docs/BURRITO-SPEC.md:350).
- **Column content:** Quote, Occurrence and Tags are usually empty
  (UTC:docs/2-…:1359-1366). The docs' "comprehension/checking" tags
  (UTC:docs/4-…/translation-questions-repositories-guide.mdx:86-103) do not appear in
  en_obs-tq (FX: zero rows with tags, quote or occurrence).
- **Gap:** there is no Bible tQ fixture; only OBS tQ exists. [MEM] Older tQ was
  `<book>/<cc>/<vv>.md` markdown (`# Question?\n\nAnswer.`).

### 2.11 OBS and its helps

**OBS stories** (`<lang>_obs`, flavor `gloss/textStories`) [TC4:docs/BURRITO-SPEC.md:583-633;
UTC:docs/4-…/open-bible-stories-repositories-guide.mdx:89-139]:

```markdown
# 1. The Creation

![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg)

This is how God made everything in the beginning. …

![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-02.jpg)

Then God said, "Let there be light." …

_A Bible story from: Genesis 1-2_
```

- **Files:** `content/01.md` … `content/50.md`, plus `content/front/title.md`,
  `content/front/intro.md` and `content/back/intro.md` (en_obs and pankosmia use `front/`
  and `back/` directories; TC4:docs/BURRITO-SPEC.md:600).
- **Frame rule:** every `![…](…)` line starts a frame. A frame is that image line plus the one
  paragraph after it. The title is frame 0, and the trailing `_…_` line is the reference line
  (TC4:docs/BURRITO-SPEC.md:610-633).
- **Locator:** `story:frame` without leading zeros (`1:1`); `1:0` is the title
  (TC4:docs/BURRITO-SPEC.md:635).
- **Images:** the URL basename is `obs-en-SS-FF.jpg`. The default pack is
  `uW/obs_images_360` at sha `7146d5b5…` (TC4:src/data/installedSuite.js:47-51).
- **Older formats:**
  - [DOC] The tS export format uses `====== Title ======` and
    `{{https://cdn…jpg?direct&}}` (UTC:docs/5-migration/ts-to-rc/index.mdx:187-196).
  - [MEM] Older RC en_obs used `content/01/01.txt`-style chunks. Pinned v9 uses `NN.md`.

**OBS helps** (FX, all real exports):

- **OBS-tN** (`en_obs-tn` v13): **one** `OBS.tsv` with the TSV7 tN header. `Reference` =
  `story:frame`, frame 0 = the title.
  - 2325 rows; 58 title rows tagged `title`, all without a SupportReference.
  - **Quote is English story text** (`the beginning`, `us & our & us`).
  - Occurrence is 1 in every row (FX:resources/README.md:45-49).
  - The docs' `tn_01.tsv … tn_50.tsv` layout
    (UTC:docs/4-…/open-bible-stories-repositories-guide.mdx:108-118;
    UTC:docs/4-…/translation-notes-repositories-guide.mdx:98-108) is not what the export
    contains.
- **OBS-TWL** (`en_obs-twl` v3): one `OBS.tsv` with the TSV6 TWL header.
  - `OrigWords` is English (`God`, `Spirit`).
  - TWLink keeps `rc://*/tw/dict/bible/kt/god`.
  - Tags: keyterm 1243, name 636, `keyterm; name` 211, empty 291. 2381 rows, no frame-0 rows.
- **OBS-tQ** (`en_obs-tq` v10): one `OBS.tsv` with the TSV7 tQ header, 672 rows.
- **OBS-SN/SQ** (study notes and questions) [MEM] exist as `en_obs-sn` and `en_obs-sq`. They
  are not in scope here.

### 2.12 Lexicons (needed for tC's word details)

- `uW/en_ugl` (Greek) and `uW/en_uhl` (Hebrew) are SB burritos with flavor
  `peripheral/x-lexicon`.
- They have **no tags**, so pin them by sha and fetch them as `/archive/<sha>.zip`
  (TC4:src/data/installedSuite.js:77-86).
- The entries are tC3's Strong's JSON: `ingredients/content/<n>.json` → `{"brief": "...",
  "long": "..."}` (FX:resources/en_ugl@d9d29e2d5892/1.json).
- [MEM] File names are Strong's numbers without the letter (`1.json` = G0001).

### 2.13 `rc://` link grammar

The general form is `rc://<lang|*>/<resource>/<type>/<project>/<chapter>/<chunk>`
(UTC:docs/2-unfoldingword-developer-guide.mdx:464-473).

| Link | Meaning |
|---|---|
| `rc://*/ta/man/translate/figs-metaphor` | tA article. `*` = the resolving set's language. |
| `rc://*/tw/dict/bible/kt/god` | tW article → `bible/kt/god.md` (RC) or `payload/kt/god.md` (export) |
| `rc://en/tn/help/gen/01/02` | tN at Gen 1:2 (zero-padded) |
| `rc://en/tn/help/obs/01/01` | OBS-tN story 1, frame 1 |
| `rc://en/ult/book/gen/01/02` | ULT at Gen 1:2 |

In notes, `[[rc://…]]` (double brackets) is an inline reference that should render as the
target's title.

### 2.14 tC3 project repos (what tC3 pushed to Door43)

These matter for importing users' old work and for repo naming.

- **Root files:** `manifest.json` (`tc_version`, `generator`, `project{id,name}`,
  `resource{id,name}`, `target_language{id,name,direction,book}`, `toolsSelectedGLs`,
  `toolsSelectedOwners`, `tsv_relation`, `source_translations`), the working USFM, `LICENSE.md`
  and `.apps/translationCore/{alignmentData,checkData,selectionsData,…}`
  (UTC:docs/4-repository-formats/tool-generated/translationcore-format-guide.mdx:80-170).
- **The project folder** holds chapter JSON (`<book>/<ch>.json` = `{verse: text}`) and
  `<book>/headers.json` (TC4:src/data/import/tc3.ts:78-91).
- **The tC3 "Export project" zip** renames `.apps` to `apps`, drops `.git`, and adds
  `wordAlignments/<project>.usfm` and `manifest.externalResources` (the resource URLs
  `…/archive/<tag>.zip`) (TC4:docs/DECISIONS.md:1740-1744; TC4:src/data/import/tc3.ts:186-199).
- **Index items** live at `apps/translationCore/index/<tool>/<book>/`, and alignments at
  `alignmentData/<book>/<ch>.json` → `{verse:{alignments:[{topWords,bottomWords}],wordBank}}`
  (TC4:src/data/import/tc3.ts:10-18, 103-123).
- **The docs' claim** that tC3 makes exactly two USFM files
  (UTC:docs/4-…/translationcore-format-guide.mdx:191-226) is consistent with the tC4 import
  notes.

### 2.15 Doc versus reality mismatches (formats)

| Docs | Reality (evidence) |
|---|---|
| tN Quote English ("In the beginning"), Tags `grammar` | Greek/Hebrew Quote, empty Tags (FX en_tn) |
| TWL Tags `kt/names/other` | `keyterm`, `name`, `keyterm; name`, empty. Category is in TWLink (FX en_twl) |
| tA `subtitle.md`; 01.md starts `# Title ## Subtitle` | `sub-title.md`; 01.md starts `### Description`; title.md has no `#` and no newline (FX en_ta) |
| tW headings without colons, `{{topic>…}}` | `## Definition:` etc., with real Bible Reference link lists (FX en_tw god.md) |
| UHB plain text "no alignment data … tokenized" | Fully `\w`-wrapped with lemma/strong/x-morph (FX hbo_uhb) |
| SB detection via `meta.format` | Top-level `format` (SB 1.0) (all FIX metadata) |
| OBS-tN `tn_01.tsv … tn_50.tsv` | One `OBS.tsv` (FX en_obs-tn) |
| NT starts at 40-MAT / REV=66 | 41-MAT … 67-REV (57-TIT, TC4 evidence) |
| UHB/UGNT subject "Bible" | [MEM] "Hebrew Old Testament" / "Greek New Testament"; check `/catalog/list/subjects` |

---

## 3. Mapping a tN/TWL quote to words (highlight and selection)

There are two different tasks.

- **(A) Highlight the gateway rendering.** Light up the ULT/UST words that render a note's
  original-language quote. This uses the gateway Bible's `\zaln` alignment and is automatic.
- **(B) The checker's selection.** In tC's tN/tW check tools, the translator taps the words
  in **their own target verse** that render the quote. That selection is stored as
  `[{text, occurrence, occurrences}]` and revalidated when the verse changes. tC3 and tC4 do
  not compute (B) automatically. It is a human decision guided by (A).

### 3.1 Inputs

- **The note:** `Quote` (string; ` & ` separates spans; `…` may appear in older data), the
  integer `Occurrence` (the verse-level occurrence of the whole quote; 0 = none), and
  `Reference` (c:v).
- **The aligned gateway verse:** parsed USFM verse objects where each `\w` word sits inside
  a stack of `\zaln` milestones. Each milestone gives `(content = x-content, occurrence =
  x-occurrence)`, which is the identity of one *original-word instance* in the verse
  (TC4:src/data/sourceHighlight.ts:1-10).

### 3.2 Normalization (applied to quote words and `x-content` alike)

From TC4:src/data/sourceHighlight.ts:84-130:

- Apply Unicode **NFC**. UHB Hebrew is not NFC in the source; journaled data is NFC
  (TC4:src/data/import/tc3.ts:50-53).
- Strip leading and trailing characters that are not letters or numbers, so `Θεοῦ,` becomes
  `Θεοῦ`. Keep internal marks and U+2060.
- Split the quote on whitespace. Drop `&` (a span separator, not a word) and `…`.
- For tN, the quote is also stored as a word array `[{word, occurrence}]`, where the
  occurrence is the word's ordinal *within the quote* (TC4:src/data/derive.ts:258-277). That
  value is not used for matching.

### 3.3 Tokenize the gateway verse

From TC4:src/data/sourceHighlight.ts:31-63:

1. Walk the verse objects depth-first, carrying a stack `orig: [{content, occurrence}]`.
2. On a `zaln` milestone, push `{content, occurrence}` and recurse into its children.
3. On a `word` node, emit `{text, word: true, orig: <current stack>}`.
4. Emit other text nodes as non-words. Skip footnotes (`\f`). For a section node, skip its
   own text but walk its children.
5. **Group instances.** Consecutive word tokens that share the *same* stack object form one
   alignment group. Bare text between them does not break the group; a different stack does
   (TC4:src/data/sourceHighlight.ts:132-157).

### 3.4 The highlight algorithm (`matchQuote`)

From TC4:src/data/sourceHighlight.ts:165-261:

```text
words  = normalized quote words (spans flattened, '&' dropped)
groups = alignment groups in gateway order
groupHas(g, w, occ?) = some orig in g.orig with norm(orig.content)==w (and orig.occurrence==occ if given)

1. CANDIDATES: for each group index s where groupHas(groups[s], words[0]):
       picked=[s]; at=s
       for each next word w: find the first j >= at with groupHas(groups[j], w)
                             (j may equal at: one group can render several quote words)
       if all words were found → candidate {picked, instances = set of "content#occurrence"
                                            for every matched (group, word), span = last-first}
2. CLASSES: candidates that share ANY original instance are the same quote occurrence.
       Walk the candidates in order; join the first class that shares an instance, else start
       a new class.
3. Pick class number `Occurrence` (1-based; 0 or missing is treated as 1).
   Within the class take the TIGHTEST candidate (smallest span).
   Highlight EVERY group whose orig stack contains ANY of that candidate's instances.
   This expands to all gateway groups that render the same original word.
4. FALLBACK (fewer classes than Occurrence):
     single-word quote → highlight groups with groupHas(g, w, Occurrence) (exact x-occurrence)
     multi-word quote  → highlight every group that contains any quote word (degraded, but visible)
   A quote with no matching words highlights nothing.
```

- **Why classes:** in en_tn TIT 2:2 the quote `τῇ ἀγάπῃ` anchors at τῇ#1 ("in faith") and
  at τῇ#2 ("in love"). Both candidates share ἀγάπῃ#1, and the τῇ#2 candidate is tighter, so
  "in love" wins (sourceHighlight.ts:175-180; test TC4:test/sourceHighlight.test.ts:86-93).
- **Why the expansion step:** en_ult TIT 1:1 renders ἐκλεκτῶν#1 as "of" and also "the
  chosen people". The note must light both (sourceHighlight.ts:242-249).
- **Never return occurrence 1's words for occurrence 2.** `ἐκλεκτῶν` with occurrence 2 in
  TIT 1:1 returns nothing (test sourceHighlight.test.ts:107-111).

### 3.5 Gateway-language quote text (replaces the old GLQuote column)

`gatewayQuote` takes the highlighted word tokens in verse order and joins them with spaces.
It inserts `…` where non-highlighted words fall between them, and returns null when nothing
matches (TC4:src/data/sourceHighlight.ts:263-289). The real tests
(TC4:test/sourceHighlight.test.ts:114-134) give:

- `ἐκλεκτῶν Θεοῦ` → `of the chosen people of God`
- `πίστιν & ἀληθείας` → `the faith … of the truth`
- `Θεοῦ` with occurrence 1 → `of God`
- `λόγος` → null (fall back to the original quote)

### 3.6 OBS (plain-text source, no alignment)

From TC4:src/data/sourceHighlight.ts:68-114:

- Tokenize the story frame with `[\p{L}\p{N}\p{M}'’-]+` word runs.
- Each `&` span must match as a contiguous run of normalized words, after the previous span.
- The first match wins and Occurrence is ignored, because it is always 1.
- If the match is not complete, highlight nothing.

### 3.7 Target selections (what the checker records)

- **Tokenizer:** use `string-punctuation-tokenizer` `tokenize({text})`. It produces word
  tokens with punctuation stripped, keeping U+200D and U+2060 inside words. Use the **same
  tokenizer** for rendering and for recording (TC4:src/data/selections.ts:17-20;
  TC4:src/data/align/tokenize.ts:1-31).
- **Record:** `[{text, occurrence: nth instance of text in verse (1-based), occurrences:
  total count of text in verse}]` (TC4:src/data/selections.ts:9-37).
- **Re-locate** by `(text, occurrence)` (TC4:src/data/selections.ts:39-55).
- **Revalidation:** after a verse edit, `word-aligner-lib`
  `selectionsHelpers.validateVerseSelections(verseText, selections).selectionsChanged` marks
  the decision invalid. The decision is flagged, never deleted
  (TC4:src/data/revalidate.ts:27-43).
- **Decision identity:** `checkId|chapter|verse|quoteString|occurrence`. Cross-language
  re-attach uses `(book, c, v, quoteString, occurrence)` with a groupId tiebreak
  (TC4:src/data/derive.ts:361-370, 486-523). Quotes compare equal through the tokenizer,
  span by span on `&` (derive.ts:465-479).

### 3.8 Extension for Rust: proposing target words from the target's own alignment

If the target translation is aligned (tC's word-alignment tool), the same algorithm works
with the target alignment records in place of ULT. Each alignment's `topWords` play the role
of the zaln stack, and its `bottomWords` play the role of the gateway words. Run §3.4
against the target verse to *pre-select* a suggestion, and let the translator confirm it.
This is my proposal and is not implemented in tC4. Required properties:

- verse-level `occurrence` on topWords, which is copied from the zaln `x-occurrence`;
- NFC on both sides;
- tolerance for one original word aligned into several separate target groups.

---

## 4. The gateway language model

- **What a gateway language (GL) is:** a language of wider communication that bridges the
  originals and heart languages (UTC:docs/2-…:2090-2098). Original languages are always
  `unfoldingWord/hbo_uhb` and `unfoldingWord/el-x-koine_ugnt`, whatever the GL.
- **A GL is a (language, owner) pair.** Two owners may publish the same language: `fr` from
  both `Xenizo` and `MVHS`. Key the registry `lang::owner` (TC4:src/data/gateways.ts:81-86).
  - The docs' organizational models are single-language orgs (`es-419_gl`, `fr_gl`), the
    multi-language `unfoldingWord`, and personal accounts (UTC:docs/2-…:486-496, 645-701).
- **Repo naming in each owner:**
  - `<lang>_tn`, `_tw`, `_twl`, `_ta`, `_tq`, `_obs`, `_obs-tn`, `_obs-twl`, `_obs-tq`.
  - The literal Bible is `<lang>_ult` (English) or `<lang>_glt` (other GLs). The simplified
    Bible is `<lang>_ust` or `<lang>_gst`.
  - Exception: BSOJ's Arabic literal text is `ar_avd`.
  - Sources: TC4:src/data/gateways.ts:44-54, 112-134; TC4:src/data/installed.ts:471-487.
- **The registry tC4 ships.** There are 18 entries, built from a catalog intersection of
  released tN, tW and tA under the same owner (TC4:src/data/gateways.ts:11-19, 56-75). The
  `seen` tags are as of 2026-08-03 and are a freshness note, not pins.

  | id | owner | dir | TWL repo? | Bible? | tQ? | simplified? | seen tn/tw/ta |
  |---|---|---|---|---|---|---|---|
  | en | unfoldingWord | ltr | yes | yes | yes | yes (`en_ust`) | v89/v89/v89 |
  | es-419 | es-419_gl | ltr | yes | yes | yes | yes | v66/v37/v4 |
  | fr | Xenizo | ltr | yes | **no** | yes | no | v2.0 ×3 |
  | fr | MVHS | ltr | yes | yes | yes | yes | v0.0.2/v0.0.1/v0.0.2 |
  | ru | ru_gl | ltr | yes | yes | yes | no | v60.1/v14.2/v13.3 |
  | fa | fa_gl | **rtl** | yes | yes | yes | yes | v9/v9/v7.8 |
  | ar | BSOJ | **rtl** | yes | yes (`ar_avd`) | yes | yes | v6.1/v5/v3 |
  | id | bahtraku | ltr | yes | yes | yes | yes | v25/v26/v30 |
  | hi, bn, gu, kn, mr, ne, or, te | translationCore-Create-BCS | ltr | yes | yes | yes | yes | (see the file) |
  | ml | Door43-Catalog | ltr | **no** | yes | yes | no | v14.2/v8.2/v10.3 |
  | ur-deva | Door43-Catalog | **rtl** | **no** | yes | yes | no | v14.1/v8.3/v10.1 |

- **An owner without `_twl`** (`hasTwl: false`) pins the bundled **English `en_twl`** with
  its own `<lang>_tw`. TWL rows carry only original-language words and article slugs, so
  this works. The tW article gate then drops rows whose slug the GL's tW lacks
  (TC4:src/data/gateways.ts:34-41; TC4:docs/BURRITO-SPEC.md:349).
- **Multi-language owners** (`translationCore-Create-BCS` holds hi_*, bn_*, …) require
  matching on the full `<lang>_<suffix>` repo name. Matching on the suffix alone produced
  mixed-language sets (TC4:src/data/installed.ts:505-509).
- **Two-rung ladder:** a project pins `primary` (the chosen GL) and `fallback` (English, the
  bundled suite). Resolution is per (tool, book), by the book coverage of the pinned commit
  (TC4:docs/BURRITO-SPEC.md:296-352; TC4:src/data/installedSuite.js:53-66).
- **Pinned English suite, v91, with real shas** (TC4:src/data/installedSuite.js:22-45, 68-103):

  | Repo | Version | sha |
  |---|---|---|
  | en_tn | v91 | `e586762e…` |
  | en_twl | v91 | `7cbb3419…` |
  | en_tw | v91 | `ff5b3852…` |
  | en_ta | v91 | `ce9a1bb9…` |
  | en_tq | v91 | `8be02772…` |
  | en_ult | v91 | `35d21595…` |
  | en_ust | v91 | `85f274a7…` |
  | en_obs | v9 | `d39a1dc7…` |
  | en_obs-tn | v13 | `e86138ea…` |
  | en_obs-twl | v3 | `44ebc9fa…` |
  | en_obs-tq | v10 | `01b92fe8…` |
  | el-x-koine_ugnt | v0.34 | `fc95b2b8…` |
  | hbo_uhb | v3.0.0 | `74022f0f…` |
  | uW/en_ugl | sha-only | `d9d29e2d…` |
  | uW/en_uhl | sha-only | `72df5ac2…` |

- **Org renames:** es-419's exports name `Idiomas-Puentes`. Today the org is `es-419_gl`
  (once `Es-419_gl`), and the old name 404s. Resolve the owner from the registry, not from
  export metadata (TC4:src/data/installed.ts:113-122; TC4:docs/PLATFORM-NOTES.md:109).
- **Catalog support:** `is_gl` on search and list, and `language_is_gl` on entries and repos
  (SW:26568). This is how you would rebuild the registry dynamically.
- **Partial coverage is normal.** es-419_tn v66 covers only 3JN, JON, RUT and TIT
  (FX:resources/es-419_tn@v66/metadata.json; TC4:docs/evidence/es419-suite-pins-2026-07-31.md:17-27).
  `currentScope` keys in the export metadata give per-commit book coverage without scanning
  files.

---

## 5. tC4 fixtures a Rust test suite can reuse

All are under `FX:` = `/home/user/unfoldingword/translationcore4/test/fixtures/`. All are
read-only copies. Provenance is in FX:README.md and FX:resources/README.md. The license is
CC BY-SA 4.0, stated in each metadata.

### 5.1 Inventory

| Path | Format | What it is | Good for |
|---|---|---|---|
| `en_ult/TIT.usfm` (= `resources/en_ult@v89/TIT.usfm`, byte-identical) | USFM 3 + `\zaln` | en_ult v89 Titus, commit `84c73ba0…`, 144,781 bytes | aligned-USFM parser; zaln nesting (depth 3); `\ts\*`; occurrence attributes; highlight vectors |
| `en_ust/TIT.usfm` | USFM + `\zaln` | en_ust v89 Titus, commit `37ec2231…`, 186,101 bytes | deep nesting (depth 5), N:M alignments |
| `hbo_uhb/JON.usfm` | USFM, `\w` lemma/strong/x-morph | UHB v3.0.0 Jonah (`32-JON.usfm`), sha256 `55a36ef7…` | RTL; U+2060; maqaf; strong prefixes; `\ca`/`\va` |
| `sample-burrito/{TIT,JON}.usfm` | plain USFM drafts with `___` stubs | generated sample project; JON has span verse `\v 9-10` | target-draft parsing; spans |
| `usfm-corpus/*.usfm` (6 files) | USFM | aligned copies plus hand-made `exotic-poetry-footnotes.usfm` (`\q1/\q2`, `\b`, `\d`, `\s`, `\f…\f*` with `\fr\ft\fq\fqa\fk`, nested `\+w`, lists, span `\v 4-5`, zaln + footnote) and `exotic-partial-book.usfm` (chapter 3 only, no final newline) | **byte-for-byte round-trip/identity tests** (an edit to one verse changes no other byte) |
| `resources/en_tn@v86/{TIT,JON}.tsv` + `metadata.json` | TSV7 + SB metadata | en_tn v86 slices, rev `c354b8ae…` | tN parser; Greek and Hebrew quotes; `&`; intro rows |
| `resources/en_tn@v91/TIT.tsv` + metadata | TSV7 | en_tn v91, rev `e586762e…` (quote punctuation differs from v86: `Θεοῦ,`) | version-diff and re-attach tests |
| `resources/es-419_tn@v66/TIT.tsv` + metadata | TSV7 | Spanish tN, rev `22f3d0c6…`, stale owner `Idiomas-Puentes` | GL notes; `rc://es-419/…` links; occurrence-0 label rows; cross-language re-attach |
| `resources/en_twl@v86/TIT.tsv` + metadata | TSV6 | en_twl v86, rev `570e76d0…`, `./payload/...` links | TWL parser; Tags forms; occurrence 2 |
| `resources/en_twl@v91/metadata.json` | SB metadata only | 59 book TSVs (no 1CH/2CH/ECC/EZK/ISA/JER/NUM) | coverage from `currentScope` |
| `resources/en_tw@v87/{TIT,JON}.tsv`, `payload-kt-god.md` + metadata | TSV6 + markdown | en_tw v87 (TWL inside tW), rev `eaeb7bfe…` | tW article parser; Hebrew TWL |
| `resources/en_tw@v91/{2SA,TIT}.tsv` + metadata (1021 ingredients) | TSV6 | en_tw v91, rev `ff5b3852…`; 2SA has 3316 rows | **article gate** (2SA `vwrw` → missing `kt/arcofthecovenant.md`); large-file parse |
| `resources/en_ta@v86/translate-toc.yaml`, `translate-figs-abstractnouns-{01,title}.md` + metadata (732 ingredients) | YAML + markdown | en_ta v86, rev `c7caddfb…` | toc parser; article assembly; slug existence |
| `resources/en_obs-tn@v13/OBS.tsv` + metadata | TSV7, `story:frame` | the whole OBS-tN, 2325 rows | OBS notes; title rows `N:0` |
| `resources/en_obs-twl@v3/OBS.tsv` + metadata | TSV6 with `rc://` links | the whole OBS-TWL, 2381 rows | rc:// TWLink form; English OrigWords |
| `resources/en_obs-tq@v10/OBS.tsv` + metadata | TSV7 tQ | the whole OBS-tQ, 672 rows | tQ parser |
| `resources/en_ugl@d9d29e2d5892/{metadata.json,1.json,README.md}` | SB + JSON | Greek lexicon commit-archive pieces | lexicon entry parse; non-DCS id authority |
| `vrs/{eng,org,lxx,rsc,rso,vul}.json` | Copenhagen Alliance versification JSON `{maxVerses, mappedVerses, excludedVerses, partialVerses}` | the six standard schemes, sha-verified (FX:vrs/README.md) | versification mapping (e.g. `"GEN 31:55": "GEN 32:1"`) |
| `vrs-synthetic/syn.json` | same shape | a tiny synthetic TIT remap | mapping unit tests |

### 5.2 Ground-truth numbers to assert

These were counted at vendoring time (FX:resources/README.md:34-51) and re-checked here:

- en_tn v86 TIT:
  - 206 data rows; 49 without SupportReference → **157 check items**.
  - 28 distinct groupIds, all present in the tA v86 translate toc.
  - tC3 categories: grammar 71, figures 53, culture 26, other 6, discourse 1.
  - 4 intro rows (occurrence 0) and 18 `&` quotes.
- en_twl v86 TIT: **188 items**; kt 111, other 71, names 6. One row has occurrence 2.
- es-419_tn v66 TIT: 216 rows → **112 items**. 89 of the 157 en checkIds also exist in es.
- en_obs-tn v13: **2325 items**, 58 title rows, occurrence 1 everywhere.
- en_obs-twl v3: **2381 items**; story 1 has 35.
- en_obs-tq v10: 672 rows.
- Each `metadata.json` revision equals the pin sha in TC4:src/data/installedSuite.js.

### 5.3 Ready-made test vectors

**Quote → highlight** (TC4:test/sourceHighlight.test.ts:62-134, all against
`en_ult/TIT.usfm`):

- TIT 1:1 `κατὰ πίστιν ἐκλεκτῶν Θεοῦ, καὶ ἐπίγνωσιν ἀληθείας`, occurrence 1:
  - includes faith, chosen, people, knowledge;
  - excludes servant and Paul;
  - exactly one "God".
- TIT 1:1 `Θεοῦ` occurrence 1 → `[of, God]`. Occurrence 2 must not include "servant".
- TIT 2:2 `τῇ ἀγάπῃ` → includes love; excludes faith and patience.
- TIT 1:15 `τοῖς δὲ μεμιαμμένοις καὶ ἀπίστοις, οὐδὲν καθαρόν` → includes corrupted,
  unbelieving and pure; excludes all.
- TIT 1:1 `ἐκλεκτῶν` occurrence 2 → empty.
- The empty quote → empty. `λόγος` → empty.
- gatewayQuote:
  - `ἐκλεκτῶν Θεοῦ` → `of the chosen people of God`;
  - `πίστιν & ἀληθείας` → `the faith … of the truth`.
- Θεοῦ tokens in TIT 1:1 carry orig occurrences `of#1, God#1, of#2, God#2`.

**Zip and pin behavior** (TC4:test/resourceFetch.test.ts:44-455; synthetic zips, with cases
worth porting):

- strip the wrapper;
- drop `.git/` and `.DS_Store`;
- zip-slip rejections (`../`, `/abs`, `C:x`, `C:/x`, `..` followed by a space, `...`, `\`);
- refuse on a sha mismatch or a missing revision;
- paginated tag lookup with a server that clamps the page size;
- an archive comment that does not match the pin;
- a 404 counts as conclusive absence, while 5xx propagates.

### 5.4 Gaps (fixtures the Rust suite should add)

- No UGNT book, no Bible tQ TSV, and no RC-form repo sample: no `manifest.yaml`, no
  `tn_TIT.tsv` with rc:// TWLinks, no `bible/` tree.
- No OBS story markdown under test/fixtures. [TC4] has one at
  `conformance/sample-burrito-obs/`, and the text_stories template is at
  `conformance/fixtures/text_stories/`.
- No recorded DCS API JSON (catalog search, tags, releases, create repo, token). Record these
  from QA (`qa.door43.org`) once network access is available. Do not hand-write them.
- No legacy TSV9 tN sample and no tC3 project zip. [TC4] has import fixtures under
  `conformance/fixtures/import/`.

---

## 6. Recommendations for the Rust client (short)

1. **Pin type:** `Pin { repo: (host, owner, name), tag: Option<String>, sha: [u8;20] }`.
   Identity is `(repo, sha)`. Compare the repo case-insensitively and keep DCS casing.
2. **Fetch:**
   - If a tag is present, GET `/{o}/{r}/sb/{tag}.zip` (or `/archive/{tag}.zip` to read the
     RC form).
   - Otherwise GET `/archive/{sha}.zip`.
   - Verify the sha: the export's `identification.primary.dcs.*.revision` for `/sb/`, or the
     zip comment for `/archive/`.
   - On a first install, verify against `GET /api/v1/repos/{o}/{r}/tags/{tag}` →
     `commit.sha`.
   - Check each ingredient's md5 and size. Reject zip-slip entries.
   - Store installs content-addressed by `(owner, repo, sha)`.
3. **Discovery:**
   - Use `GET /api/v1/catalog/search?lang=…&owner=…&stage=prod&topic=tc-ready`, or
     `GET /api/v1/repos/{o}/{r}` → `catalog.prod.{branch_or_tag_name, commit_sha}` for
     "latest".
   - Page every list with an effective-page-size rule.
4. **Parsers:**
   - USFM must be marker-stream based (mid-line `\v`), byte-preserving, and zaln-aware.
   - TSV must check the header exactly, split on `\t` only (no quoting), keep the literal
     `\n` in Note, and keep span refs as strings.
   - TWLink accepts both forms. tA assembly is title, then sub-title, then 01.md.
5. **Unicode:** normalize to NFC for comparisons only, never for stored bytes. Keep U+2060
   and U+200D inside words.
6. **Auth:** Basic auth → delete the same-name token → create a token with scopes
   `write:repository`, `write:organization`, `write:user` → keep `sha1` in the OS keychain.
   Send `Authorization: token …`.
7. **Push:** create the repo with `auto_init: false` and `default_branch: "main"`. Check that
   `clone_url` has the same origin as the server. Add `origin` and push `main` with
   userpass(login, token) credentials. Map NotFastForward to a friendly refusal and never
   force-push.
8. **Dev target:** run all write paths against `https://qa.door43.org`, which is reset weekly
   (TC4:src/data/dcsServer.ts:9-18).

## 7. Open questions (not answerable from these sources)

- **The legacy `/api/catalog/v5/search` endpoint.** Is it still served, and are its field
  names identical to v1? The swagger has only `/api/v1/catalog`.
- **`zipball_url` and `metadata_url` values.** What do they look like on a real
  CatalogEntry or CatalogStage: web `/archive/<tag>.zip`, or API?
- **The exact catalog subject strings** for UHB, UGNT, TWL, tQ and the OBS helps.
- **Whether the en_twl `/sb/` zip really contains the 953 articles** that its metadata
  lists (TC4 evidence says "links-only").
- **The diffpatch body schema** (the swagger says UpdateFileOptions; the docs say `{message,
  patch}`).
- **How the `/sb/` export handles a repo that is already SB** (`metadata_type: sb`). Likely
  verbatim, but unverified.
