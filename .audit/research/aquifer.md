# BibleAquifer (Aquifer, ETEN / Mission Mutual) as a helps source for tC4 (Rust)

Research for unfoldingWord/translationCore4#654 (R1-R6). Research date: 2026-10-14. All work was read-only.

## Evidence tags

| Tag | Meaning |
|---|---|
| **[FILE]** | Verified by reading a file. The path or URL is given. |
| **[WEB]** | Verified by fetching a web page with WebFetch. The URL is given. |
| **[SNIPPET]** | Taken only from a WebSearch result summary. The page could not be fetched: `aquifer.bible`, `www.aquifer.bible`, `api.aquifer.bible`, `bibleaquifer.github.io` and `git.door43.org` all fail DNS from WebFetch and are blocked from the shell. Treat these as weak evidence. |
| **[INFERRED]** | My conclusion from the evidence. It was not checked directly. |

## Local material used

These clones are all shallow, and some have blobs only partly fetched.

- `/home/user/ref/AquiferOpenStudyNotes`: tag `v2026-09-18`, commit `bbb3f86016cd20913f410a95af63d9f7ff9a71d6`.
- `/home/user/ref/AquiferOpenBibleDictionary`: tag `v2026-10-12`, commit `4d92a9ac29f41544cf5e70584f6f9fbee877a7df`.
- `/home/user/ref/aquifer-extra/{docs,.github,aquifer-api-client,aquifer-api-client-demo,bibleaquifer.github.io,ACAI}`: full shallow clones, 86 MB in total.
- `/home/user/ref/aquifer-probe/{UWTranslationNotes,UWTranslationWords,UWTranslationManual,UWTranslationQuestions,FIATranslationGuide,FIAKeyTerms,SILOpenTranslatorsNotes,AFBRTTranslationNotes,BiblicaStudyNotesKeyTerms,AquiferPortugueseBibleReferenceText,unfoldingWordLiteral}`: blobless `--no-checkout` clones. Single files were read with `git show HEAD:<path>`.
- Portuguese JSON release assets (`Portuguese-json.zip`, tag `v2026-10-12`) for the 4 UW mirrors, downloaded to `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/dl/<Repo>-por/`.
- Extracted samples in `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/aq/`.

---

## TL;DR

1. **Aquifer mirrors all four unfoldingWord helps.** The `UWTranslationNotes`, `UWTranslationWords`, `UWTranslationManual` (= tA) and `UWTranslationQuestions` repos are built from **en_* release v91**. They are converted to HTML-in-JSON and machine/editorially adapted into up to 9 other languages (arb, fra, guj, hin, ind, nep, por, spa, swh, zht; the language set varies by resource). [FILE] See the READMEs and `metadata.json` in `/home/user/ref/aquifer-probe/UW*`.
2. **Licensing allows bundling.** Everything relevant is **CC BY-SA 4.0**, and some images/Bibles are CC0. The required attribution text is in each `metadata.json`. [FILE]
3. **Prefer the public GitHub repositories over the API.** Each repo has per-language `*-json.zip` release assets on date tags (`vYYYY-MM-DD`). These can be pinned by tag, commit sha and asset sha256. The REST API needs a key, serves an upstream database that is *not* the GitHub data, and has no pinning. [FILE] [WEB]
4. **Aquifer tN keeps the original-language Quote, but not Occurrence.** It also has no structured SupportReference column (that information survives only as a link to the tA article), and only 22% of notes keep the uW note ID. Non-English GL quotes are *translated* quotes. They do not match any aligned GL Bible. Aquifer's own alignments are LLM-generated SBLGNT/WLCM→text v0.4, not UGNT/UHB. **Aquifer tN can drive a tN check only if tC4 still uses UGNT/UHB (from DCS) for tokens and either defaults Occurrence or recovers it by joining back to en_tn v91.** Aquifer tW **cannot** drive a tW check: there is no TWL OrigWords and no Occurrence. [FILE] [INFERRED]
5. **Recommended minimal integration:** at build time, take a pinned per-language JSON release zip and convert it into the same internal `Helps` model that the DCS loader fills. Store a provenance record (repo, tag, commit, sha256). Start with view-only Notes, Words, Academy and Questions tabs. Treat checking from Aquifer tN as a later, separate step.

---

## Org inventory: github.com/BibleAquifer (71 repos) [WEB]

Source: https://github.com/orgs/BibleAquifer/repositories (pages 1-3, fetched 2026-10-14).

| Group | Repos | Notes |
|---|---|---|
| **unfoldingWord mirrors** | `UWTranslationNotes`, `UWTranslationWords`, `UWTranslationManual`, `UWTranslationQuestions`, `UWOpenBibleStories`, `unfoldingWordLiteral`, `unfoldingWordSimplified` | Built from en_* **v91** [FILE: README "Sources" sections] |
| Other translation guides | `FIATranslationGuide` (Fia, pericope-based steps + audio links), `SILOpenTranslatorsNotes` (eng only, 36 books), `AFBRTTranslationNotes` (fra only, notes on Aquifer French reference text) | |
| Glossaries / key terms | `FIAKeyTerms`, `BiblicaStudyNotesKeyTerms` | |
| Study notes | `AquiferOpenStudyNotes` (Tyndale adaptation) + `...BookIntros`, `...BookIntroSummaries`, `...Profiles`, `...Themes`; `BiblicaStudyNotes`, `BiblicaStudyNotesBookIntros` | |
| Dictionaries / lexicons | `AquiferOpenBibleDictionary`, `DictionaryBibleThemes`, `UBSGreekNTDictionary`, `UBSHebrewDictionary`, `BDBHebrewLexicon`, `BDBAramaicLexicon`, `UBSRealiaLexicon`, `UBSFloraLexicon`, `UBSFaunaLexicon`, `VideoBibleDictionary` | |
| Images / maps | `FIAImages`, `FIAMaps`, `UBSImages`, `BiblicaOpenBibleMaps` | Media are links to external URLs |
| Bibles | BereanStandardBible, WorldEnglishBible(+Updated), SBLGNT, WestminsterLeningradCodex, unfoldingWordLiteral/Simplified, LouisSegond1910, ReinaValera1909, ArabicVanDyckBible, BiblicaOpenNewArabicVersion2012, ChineseUnionVersionTraditional, BiblicaOpenChineseContemporary2023 (Trad./Simp.), IndianRevisedVersion (+Gujarati), OpenHindiContemporaryVersion, OpenKiswahiliContemporaryVersion, BiblicaOpenNepaliContemporaryVersion, NepaliUnlockedLiteralBible, BiblicaOpenIndonesianContemporaryNewTestament2025, TerjemahanBaruIndonesia, Gateway{Literal,Simplified}Text{Hindi,Indonesian}, PSLE, RussianSynodalBible, OpenBislamaBibleRevised, TokPisinBible2008, Aquifer{French,Spanish,Portuguese}BibleReferenceText | USFM + USX, plus separate `alignments/json` |
| Tooling / docs | `docs` (schemas), `.github`, `aquifer-api-client` (Python, MIT), `aquifer-api-client-demo`, `bibleaquifer.github.io` (catalog generator), `text-align` (alignment generator), `ACAI` (entity annotations) | |

---

## R1: What Aquifer offers (types, languages, overlap with tN/tW/tA/tQ)

**Answer.** Aquifer offers Bibles, study notes, Bible dictionaries, translation guides, translation glossaries, comprehension questions, images/maps and videos.

- `resource_type` values are documented in `/home/user/ref/aquifer-extra/docs/aquifer_resource_metadata.md`. [FILE]
- tN, tW, tA and tQ overlap one-to-one in **content**, as UW mirrors of v91. They do not overlap in **structure** (see "Fidelity" below).

### Coverage of the UW mirrors at tag `v2026-10-12`

Article counts are taken from `article_metadata.*.localizations` in each English `metadata.json`. Book counts come from the `*/json` listing at HEAD. [FILE]

| Aquifer repo (uW source) | `resource_type` / `order` | eng | Other languages (localized articles) |
|---|---|---|---|
| `UWTranslationNotes` (en_tn v91) | Translation Guide / canonical | 87,573 notes, **59 books**: NUM, 1CH, 2CH, ECC, ISA, JER and EZK are missing | arb 66,339 (49 bks), hin 65,629 (48), **por 63,880 (47)**, fra 37,552 (27), zht 27,238 (28), spa 20,756 (25), guj 14,872 (22), ind 8,250 (16) |
| `UWTranslationWords` (en_tw + en_twl v91) | Translation Glossary / alphabetical | 953 articles | arb, hin, ind, por, spa, swh, zht 939 each; guj 905; fra 880 |
| `UWTranslationManual` (en_ta v91) | Bible Translation Manual / monograph | 242 articles | arb, hin, por 237; zht 235; nep 211; guj 170; ind 144 |
| `UWTranslationQuestions` (en_tq v91) | Comprehension Testing / canonical | 15,601 questions, 66 books | zht 13,044; hin 10,662; arb 9,440; fra 8,414; por 7,354 (35 books; 8,617 entries in the por files); guj 1,508; nep 1,350; ind 367 |

Sources: `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/aq/{utn,utw,uta,utq}/eng_metadata.json` and `git ls-tree` on `/home/user/ref/aquifer-probe/UW*`. [FILE]

### The two local clones

Counts and sizes are from `du` on each language folder. [FILE]

| Repo | Languages and article counts | eng JSON size |
|---|---|---|
| `AquiferOpenStudyNotes` (Tyndale adaptation) | eng, arb, fra, ind, por, zht 16,921-16,923 notes, 66 books each; spa 13,303 (43 books); rus 9,035 (17); swh 4,826 (10); hin 1,020 (13) | 27 MB |
| `AquiferOpenBibleDictionary` | eng 6,130 articles; spa 5,886; hin 5,764; arb 5,739; por 5,661; fra 4,528; zht 1,161; swh 718; ind 119; rus 55; nep 15; apd 3; nld 1 | 22 MB |

A fuller per-resource inventory with language completion percentages, dated 2026-01-06, is in `/home/user/ref/aquifer-extra/docs/aquifer_full_inventory.md`. [FILE] Its counts are older than those at HEAD.

**Overlap with Door43 GL sets: not verified.** git.door43.org is unreachable from here. The comment on #654 claims that pt-BR and zh have no Door43 set. I could neither confirm nor refute that.

**Language codes.** Aquifer uses ISO 639-3 codes (`eng`, `por`, `zht`, `arb`, `hin`, `ind`, `spa`, `fra`, `guj`, `nep`, `swh`), while DCS uses BCP-47 (`en`, `pt-br`, `zh`, `ar`, …), so the app needs a mapping table. [FILE] Watch `zht`: the inventory calls it Traditional Chinese, but the README text says Simplified Chinese. [FILE]

---

## R2: Licenses, redistribution and attribution

**Answer.** All relevant resources are **CC BY-SA 4.0**. tC4 may bundle and redistribute them with attribution and share-alike.

**Per-resource license** [FILE]

- **UW mirrors:** `license_info` reads © 2022 unfoldingWord, CC BY-SA 4.0. `showAdaptationNoticeForNonEnglish: true`. Non-English editions carry the notice "…adapted in the following languages … Released under CC BY-SA 4.0 license by Mission Mutual". Source: `resource_metadata.license_info` and `adaptation_notice` in each `metadata.json`.
- **AquiferOpenStudyNotes / AquiferOpenBibleDictionary:** © 2026 Mission Mutual, CC BY-SA 4.0, an adaptation of Tyndale Open Study Notes / Tyndale Open Bible Dictionary © 2023 Tyndale House. Source: `/home/user/ref/AquiferOpenStudyNotes/README.md` and `eng/metadata.json`.
- **Other resources** (from `aquifer_full_inventory.md`):
  - FIA, SIL OTN, Biblica, Video Bible Dictionary, ULT/UST: CC BY-SA 4.0.
  - UBS Images, BSB, WEB, LSG, RV1909, AVD, CUV, TB, RSB: CC0.
  - FIA copyright holder: the FIA README says "Word Collective 2025", but the inventory says "SRV Partners 2023". Use what `metadata.json` says.
- **ACAI:** CC BY-SA 4.0 © 2025 Mission Mutual, **but** its README lists "UBS Key Term data from Paratext. This is used by permission". Get legal review before bundling ACAI-derived key-term data. Source: `/home/user/ref/aquifer-extra/ACAI/README.md`. [FILE]

**Repos have no `LICENSE` file.** Neither `AquiferOpenStudyNotes` nor `AquiferOpenBibleDictionary` has one. The license is stated in the README and in `metadata.json`. [FILE]

**The FAQ restricts Aquifer to open licenses.** It says all resources are CC BY, CC BY-SA or CC0, and "only the three license types selected for the Aquifer provide free usage without restrictions". [SNIPPET: https://www.aquifer.bible/faqs]

**What the About page must show** [INFERRED from CC BY-SA 4.0 §3 and the metadata fields]

For each bundled resource and language, show:

- `license_info.title`
- the copyright statement "© {dates} {holder.name}", linked to `holder.url`
- the license name and URL
- `adaptation_notice` when `showAdaptationNoticeFor{English|NonEnglish}` is true
- the Aquifer source repo, tag and commit

`scripture_burrito.copyright.shortStatements[].statement` is a ready-made HTML one-liner.

Keep the "unfoldingWord®" name unchanged. Any reformatting tC4 does is an adaptation and stays CC BY-SA 4.0.

---

## R3: Access (API vs GitHub)

**Answer.** Use the **public GitHub repos and release assets**. No key is needed, and the data is pinnable, mirrors the schema-validated export, and works offline.

### GitHub [FILE] [WEB]

- Each `<Repo>/<lang>/` folder holds:
  - `metadata.json`
  - `json/NN.content.json`: canonical, one file per book
  - `json/NNN.content.json`: alphabetical, one file per letter group
  - `json/NNNNNN.content.json`: monograph (the UW tA mirror is a single `001.content.json`)
  - `md/` (generated), `docx/` and `pdf/`
- Bibles also have `usfm/`, `usx/` and `alignments/json/`.
- **Releases.** Each date tag has a release with per-language assets:
  - `<Language>-json.zip`: `metadata.json` + `json/`
  - `<Language>-md.zip`, `-docx.zip`, `-pdf.zip`
  - `<Language>.zip`: all formats; big languages are split into `-partNNN.zip`
  - Sources: https://github.com/BibleAquifer/UWTranslationNotes/releases/expanded_assets/v2026-10-12 [WEB]; the release notes for AquiferOpenStudyNotes v2026-09-18 [WEB].
- **Download from this container works:**
  - `curl -L https://github.com/BibleAquifer/<Repo>/releases/download/<tag>/<Language>-json.zip` returned 200.
  - `https://raw.githubusercontent.com/BibleAquifer/<Repo>/<sha>/<lang>/json/NN.content.json` returned 200.
  - The GitHub REST API is blocked by this session's policy. Use the HTML pages or git instead. [FILE: curl results]
- The org profile says to "grab the languages you'd like from the latest resource release". Source: `/home/user/ref/aquifer-extra/.github/profile/README.md` [FILE]

### REST API

Source: `/home/user/ref/aquifer-extra/aquifer-api-client/README.md`, `src/aquifer/client.py` and `models.py` (MIT, Mission Mutual). [FILE]

- **Base URL:** `https://api.aquifer.bible`.
- **Swagger:** `https://api.aquifer.bible/docs/index.html?url=/swagger/v1/swagger.json`. I could not reach it (DNS).
- **Auth:** HTTP header `api-key: <key>`.
- **Getting a key:** request one at https://www.aquifer.bible/apiaccess. [FILE: client README] A product page describes "watch the demo video … complete the form to generate your Aquifer API key". [SNIPPET]
- **Rate limits and terms:** none documented anywhere I could reach. The client retries on 429/5xx with 7 attempts and exponential backoff. [FILE: client.py:61-67]
- **Caveat:** "the API used does not directly access resources available on GitHub here, but an upstream database (AquiferDB) … used by the Bible Well app". The README also says: "Our longer-term intention is to replace the AquiferDB API with an API and MCP server that provide access to the resources in the BibleAquifer GitHub repositories." [FILE]
- **Endpoints** [FILE]:

| Purpose | Endpoint | Key params |
|---|---|---|
| Languages | `GET /languages` | (none) |
| Resource counts at a passage | `GET /languages/available-resources` | `bookCode, startChapter, endChapter, startVerse, endVerse, languageCodes` |
| Resource types → collections | `GET /resources/types` | (none) |
| Collections | `GET /resources/collections`, `GET /resources/collections/{code}` | `resourceType, limit, offset`; `languageCodes` |
| **Resources for a passage** | `GET /resources/search` | `bookCode, startChapter, endChapter, startVerse, endVerse, languageCode/languageId, resourceType, resourceCollectionCode, query, limit (≤100), offset` |
| **Article content** | `GET /resources/{contentId}`, `GET /resources/{contentId}/by-language/{lang}` | `contentTextType` = `Html` \| `Markdown` \| `Json` (**TipTap**) \| `None` |
| Translations of an article | `GET /resources/{contentId}/available-languages` | (none) |
| Associations | `GET /resources/{contentId}/associations` | Returns passage associations as `{startBookCode,startChapter,startVerse,endBookCode,…}` |
| Change feed | `GET /resources/updates` | `startTimestamp, endTimestamp, languageCode, resourceCollectionCode, limit (≤1000), offset` |
| Bibles | `GET /bibles`, `/bibles/books`, `/bibles/{id}/books`, `/bibles/{id}/texts`, `/bibles/{id}/alignments/greek` | `bookCode`, chapter/verse range, `shouldReturnAudioData`, `shouldReturnSenseData` |

The API `ReviewLevel` enum also includes `Ai`. The GitHub schema allows only `None` / `Community` / `Professional`. [FILE: models.py, aquifer_article.schema.json]

---

## R4: Formats and versioning (can we pin?)

**Answer.** Yes.

- **Tags.** Every repo has lightweight tags named `vYYYY-MM-DD`. `git ls-remote` shows 0 peeled refs. Releases come often:
  - UWTranslationNotes: 34 tags since `v2025-10-20`.
  - AquiferOpenStudyNotes: 46.
  - AquiferOpenBibleDictionary: 48.
  - Source: `git ls-remote --tags`. [FILE]
- **Example pin:** `UWTranslationNotes@v2026-10-12` = commit `dbfb46e027ee0807563918f4b95c80879cc32c60`. The release page shows the commit sha (for example, `bbb3f860…` for AquiferOpenStudyNotes v2026-09-18). [WEB]
- **Release assets can change.** GitHub lets a maintainer re-upload assets and move lightweight tags, so pin **commit sha + asset sha256**. Example: `UWTranslationNotes/v2026-10-12/Portuguese-json.zip` has sha256 `646074262e54f0aaebbeb5e416f411a6c7df56204dcb560bc24d339c409888f3` and is 13,943,042 bytes. [FILE]
- **Fallback for strict sha pinning:** `raw.githubusercontent.com/BibleAquifer/<Repo>/<sha>/<lang>/…`, which cannot change.
- **Schema versions.** The article JSON carries `"version"`, the article schema version (`1.0.4` in current data). `resource_metadata.version` is the resource schema version (`1.1.2`). Neither is a content version; **the content version is the tag**. [FILE]
- **Upstream provenance** is only in README prose ("built on release `v91`"). It is not a machine-readable field. [FILE]

---

## R5: Mapping to helps tabs, and check vs view-only

| Aquifer resource | tC4 tab | Check? | Why |
|---|---|---|---|
| `UWTranslationNotes` | **Notes** | **Possible, with work**, for the tN check (eng and GLs) | Has the OL quote and the per-verse ordinal. Missing Occurrence, structured SupportReference and the full uW ID (see below). |
| `UWTranslationWords` | **Words** | **View only** | Article per term. Verse links come from TWL, but only as bare verse refs: no OrigWords, no Occurrence, no tW category/slug. |
| `UWTranslationManual` | **Academy** | View only (it is the target of tN links) | Monograph; title only, with no tA slug except for 5 new articles (`en_ta_translate_<slug>`). |
| `UWTranslationQuestions` | **Questions** | View only (tQ is not a tC check) | Question in `<strong>`, then the answer paragraph. |
| SILOpenTranslatorsNotes, AFBRTTranslationNotes, FIATranslationGuide | Notes (secondary source) | View only | No OL quote spans; prose and section-based content. |
| AquiferOpenStudyNotes (+ Profiles/Themes/BookIntros), BiblicaStudyNotes | Notes or a new "Study" tab | View only | English lemma in `<em>` from a reader's Bible. Zero Greek; Hebrew appears only in 22 Psalm 119 acrostic letters. [FILE] |
| AquiferOpenBibleDictionary, BiblicaStudyNotesKeyTerms, FIAKeyTerms, UBS lexicons | Words (secondary, "Dictionary") | View only | Alphabetical. Passage associations are verse refs. |
| Images, maps, videos | Media panel | View only | Content is HTML pointing to external URLs. Offline use needs a separate media fetch. |
| ACAI | (none) | Possible future key-term highlight | Word-level entity annotations on **Macula SBLGNT/WLCM** (ORG versification), not UGNT/UHB. |

---

## Can Aquifer tN drive a check like DCS tN?

The DCS tN TSV has the columns `Reference, ID, Tags, SupportReference, Quote, Occurrence, Note`. Here is what survives in Aquifer's HTML-in-JSON (sample files: `scratchpad/aq/utn/eng56.json`, `57.json` and `scratchpad/dl/UWTranslationNotes-por/json/*.json`). [FILE]

| tN TSV field | Aquifer equivalent | Status |
|---|---|---|
| Reference | `index_reference` (`BBCCCVVV`) and `associations.passage[0]`. Book intro = `BB000000`, chapter intro = `BBCCC000`. | Kept |
| ID | `content_id` is `en_tn_<BOOK>_<id>` for 19,194 of 87,573 eng notes. The other 68,379 have numeric AquiferDB ids. | **Mostly lost** |
| (row order in verse) | Title suffix `(#n)`. Contiguous 1..n per verse (checked on all 50 Titus verses). The array order inside the file is **not** sorted by n. | Kept (in the title) |
| Quote (OL) | First `<span style='direction: ltr\|rtl;'>…</span>`, usually inside `<p><strong>`. Discontiguous parts are kept as `&amp;`. The U+2060 word joiner in UHB quotes is kept. Present in 63,305 of 65,726 por notes; the rest are intros. | **Kept** |
| Occurrence | (no field) | **Lost** |
| SupportReference | Only as `associations.resource[]` with `resource_code: "UWTranslationManual"` and a numeric tA `content_id`, plus a "See:" `<data class="resource-ref">` link in the body. Some notes have none (for example PHM 1:1 #2). | Partial, by numeric id |
| GL quote | Second paragraph: `<p><strong>&quot;…&quot;</strong></p>` (sometimes `&quot;.</strong>`). **eng**: 202 of 202 Titus GL quotes appear verbatim in Aquifer's ULT. **por**: only 41 of 202 appear verbatim in the Aquifer Portuguese Bible Reference Text (APBRT). They are translated quotes, not taken from an aligned text. | Kept, as display text |
| Note | The remaining HTML paragraphs. | Kept (HTML) |
| Tags | (none) | Lost |

Sources for the GL quote figures: `scratchpad/glcheck.py` run on `aq/other/ult56.SFM` and `aq/other/56TIT.SFM`. [FILE]

### Consequences for a tC4 tN check [INFERRED, except where marked]

1. **OL tokenization must still use UGNT/UHB.** Aquifer's quotes are uW quotes (UGNT/UHB spelling). Aquifer's own Greek/Hebrew Bibles are SBLGNT/WLC, and its ULT/APBRT alignments are LLM-generated v0.4 `BCVWP|token-string` records against **SBLGNT**. The `meta.llm` field names the models. The Aquifer ULT USFM has **no `\zaln`** markers. [FILE: `aq/other/56-001.alignment.json`; `git show` of `unfoldingWordLiteral/eng/alignments/json/56-001.alignment.json`; `grep -c zaln` = 0]
2. **Occurrence must be defaulted or recovered.**
   - Default to 1, or to "all" when the quote occurs once in the verse.
   - Or recover it by joining to en_tn **v91** on (book, verse, `(#n)`), assuming `(#n)` is TSV row order within the verse. All non-English notes link back to the English `content_id` through `article_metadata.localizations`, so the join carries over to the GLs.
3. **The GL-quote highlight cannot come from an aligned GL Bible for non-English languages.** Show Aquifer's GL quote text as-is.
4. **Use DCS for checking.** tC4 should keep DCS (en_tn + UGNT/UHB + aligned ULT) as the default checking source. Add Aquifer tN as a check source only for GLs that have no DCS tN. Even then, the check targets are OL quote + verse + recovered Occurrence, and the GL quote is display-only.

### tW check

Not feasible from Aquifer.

- `UWTranslationWords` passage associations are verse ranges only. Schema `Passage = {start_ref, end_ref, *_usfm}`. Example: por "Eleito" ← TIT 1:1.
- The HTML keeps "Word Data: Strong's: H0887, …" but not TWL's OrigWords or Occurrence. [FILE]
- The words list for a verse still works: invert `associations.passage` to build verse → tW articles.

---

## Data formats in detail

### Resource `metadata.json` (one per language folder)

Schema: `/home/user/ref/aquifer-extra/docs/schemas/aquifer_resource.schema.json`, v1.1.2. Docs: `docs/aquifer_resource_metadata.md`. [FILE]

```jsonc
{
  "resource_metadata": {
    "aquifer_type": "Guide",                 // Bible | StudyNotes | Images | Dictionary | Guide
    "resource_type": "Translation Guide",    // Study Notes | Bible Dictionary | Translation Glossary | Comprehension Testing | Bible Translation Manual | …
    "title": "unfoldingWord® Translation Notes", "short_name": "UTN",
    "resource_code": "UWTranslationNotes",   // == GitHub repo name
    "version": "1.1.2",                      // schema version, NOT content version
    "license_info": { "title": "…", "copyright": {"dates":"2022","holder":{"name":"unfoldingWord","url":"…"}},
                      "licenses": [{"eng":{"name":"CC BY-SA 4.0 license","url":"…"}}],
                      "showAdaptationNoticeForEnglish": false, "showAdaptationNoticeForNonEnglish": true },
    "adaptation_notice": "<p>…</p>",
    "date_created": "2026-10-12", "language": "eng",
    "localizations": ["arb","fra","guj","hin","ind","por","spa","zht"],
    "order": "canonical",                    // canonical | alphabetical | monograph
    "content_type": "Html"
  },
  "scripture_burrito": { "format": "scripture burrito", "meta": {...}, "identification": {...},
    "languages": [{"tag":"en"}],             // 2-letter here, 3-letter elsewhere
    "type": {"flavorType": {"name": "parascriptural|peripheral|scripture",
             "flavor": {"name": "textAquiferCanonical|textAquiferAlphabetical|textAquiferMonograph"}}},
    "ingredients": { "json/01.content.json": {"mimeType":"text/json","size":728395,"scope":{"GEN":["Genesis 1:1", "…"]}}, … },
    "copyright": {"licenses":[{"url":"…"}], "shortStatements":[{"statement":"<p>…</p>","mimetype":"text/html","lang":"en"}]}
  },
  "article_metadata": {                       // keyed by content_id, in resource order
    "9275": { "content_id":"9275", "reference_id":8443, "index_reference":"44001001-44001003",
              "localizations": { "por": {"content_id":150609,"language":"por","title":"Atos 1.1-3"}, … } }
  }
}
```

`metadata.json` is large: eng tN is 39.5 MB, mostly `article_metadata`. A loader can skip `article_metadata` unless it needs the cross-language link. [FILE]

### Article (content) JSON: an array of these

Schema: `docs/schemas/aquifer_article.schema.json` (`$schemaVersion` 1.0.4, `additionalProperties: false`). [FILE]

```jsonc
{
  "content_id": "247752",            // string|int; sometimes "en_tn_PHM_sz2w" / "en_tw_kt_iamyahweh" / "en_ta_translate_figs-imperative3p" / "eng-gen-p1-v2"
  "reference_id": 162493,            // string|int|null; used by AquiferDB links
  "version": "1.0.4",                // article schema version
  "title": "Philemon 1:1 (#2)",
  "media_type": "Text",              // Text | Image | Video | Audio (FIA uses Audio)
  "index_reference": "57001001",     // canonical: BBCCCVVV or BBCCCVVV-BBCCCVVV; alphabetical: lower-cased title; monograph: title
  "language": "eng",
  "review_level": "None",            // None | Community | Professional
  "content": "<p>…HTML…</p>",
  "associations": {
    "passage":  [{"start_ref":"57001001","start_ref_usfm":"PHM 1:1","end_ref":"57001001","end_ref_usfm":"PHM 1:1"}],
    "resource": [{"reference_id":"26805","content_id":"33391","resource_code":"UWTranslationManual","label":"How to Translate Names","language":"eng"}],
    "acai":     [{"id":"person:Philemon","type":"person","preferred_label":"Philemon","confidence":0.8,"match_method":"canonical"}]
  }
}
```

### References

- **`BBCCCVVV`** uses **canonical Protestant book order 01-66**, not Paratext book numbers. For example, `41014036` = MRK 14:36, `45008015` = ROM 8:15 and `57…` = PHM. [FILE: Abba entry in `AquiferOpenBibleDictionary/eng/json/001.content.json`]
- Chapter 0 / verse 0 mark intros: `57000000` = PHM 0:0 is the book intro; `56001000` is the chapter 1 intro.
- Ranges use a `-` inside `index_reference`. In titles they are written with an en dash.
- Usage counts:
  - All 16,923 eng study notes have an 8-digit canonical key.
  - All 6,130 dictionary entries have alphabetical keys; 5,090 of them carry passage associations.
- **Inline references in HTML:** `<data class="bible-ref" data-start-ref="01001003" data-end-ref="01002003">1:3–2:3</data>`. These are frequent: 39,110 in eng study notes and 46,976 in the dictionary. [FILE]
- **Outlier:** SIL OTN uses a different scheme, `<div data-bnType="OpenTranslatorsNotesSection" data-startVerseId="1006001001" …>`, a 10-digit id with a leading `1`. [FILE: `scratchpad/aq/other/SILOpenTranslatorsNotes.json`]
- **Versification:** ACAI uses Copenhagen Alliance **ORG** versification. Resources themselves do not declare a versification. [FILE: ACAI README] [INFERRED: treat refs as English versification]

### Links

- **Cross-resource links:** `<data class="resource-ref" data-content-id="33391" data-resource-code="UWTranslationManual" data-language="eng">How to Translate Names</data>`. The same links are mirrored in `associations.resource`. Some targets are not numeric, for example `data-content-id="Phlm_BookIntro_Filament"`. [FILE]
- **Data quirk:** tA articles carry *reverse* links to tN notes, and those links are labeled with the wrong code. Example: por "Conhecimento presumido…" has 5,522 `resource` entries all labeled `UWTranslationManual`, but their `content_id`s are tN notes such as "Tito 1.3 (#3)". [FILE]

### Content format

- The content **is HTML** (`content_type: "Html"`). Tags seen:
  - study notes: `p, em, ul/li, span, data, strong, h3`
  - dictionary: adds `h1/h2/h3, table, a, u`
  - tN: `<span style='direction: …'>`
  - FIA: `<a href='https://s3.amazonaws.com/…mp3'>`
- No TipTap or Markdown appears in the GitHub JSON.
- **The Markdown files are lossy derivatives.** Bible refs and resource refs become plain text (no ref.ly links, despite what the docs say), and an "Associated Passages/Articles/ACAI" footer is appended. Do not ingest them. [FILE: `AquiferOpenBibleDictionary/eng/md/001.content.md:200`]
- **TipTap JSON** exists only through the API (`contentTextType=Json`). [FILE: models.py]
- **HTML is not perfectly uniform.** A GL quote can end with `&quot;.` or `&quot;`. An OL quote can sit in `<p><span>` without `<strong>`. Parse leniently: the OL quote is the first `span[style*=direction]`, and the GL quote is the first `<strong>` starting with `&quot;`. [FILE]

### Other resources

- **tW HTML** sections: `Definition`, `Translation Suggestions`, "(See also: tW links)", `Bible References` (bible-ref list) and `Word Data` (Strong's). [FILE]
- **tQ HTML:** `<p><strong>Question?</strong></p><p>Answer.</p>`. [FILE]

---

## R6: Offline size per language

### Per-language JSON release zips at tag `v2026-10-12`

[WEB: expanded_assets pages] [FILE: unzipped por]

| Resource | eng zip | por zip | por unzipped (`json/` + `metadata.json`) |
|---|---|---|---|
| UWTranslationNotes | 16.2 MB | 13.3 MB | 95 MB + 35 MB |
| UWTranslationWords | 2.37 MB | 1.35 MB | 7.5 MB + 0.7 MB |
| UWTranslationManual | 1.36 MB | 0.88 MB | 6.4 MB + 0.2 MB |
| UWTranslationQuestions | 2.26 MB | 1.30 MB | 7.2 MB + 4.4 MB |
| **Total for 4 helps, por** | (not summed) | **~16.8 MB download** | **~157 MB raw** |

- **Stripped to what tC4 needs**, por, all 4 helps: **62 MB raw, 10.2 MB gzip**. "What tC4 needs" means id, title, ref, HTML, passages and resource links, with `acai` and `article_metadata` dropped. `acai` alone is 20 MB of the por tN. [FILE: script output]
- **Other resources, eng JSON on disk:** AquiferOpenStudyNotes 27 MB (4.95 MB zip); AquiferOpenBibleDictionary 22 MB. The full repos are 844 MB and 962 MB, mostly PDF/DOCX and `.git`. [FILE: du]
- **Fit with tC4's bundled-resource install:** good, if we ship our converted form rather than the raw zips. A Titus-only por slice is tN `56.content.json` (294 KB) + tQ 56 + all of tW + all of tA. That is under ~15 MB raw and under ~3 MB compressed. [INFERRED]

---

## Recommended minimal integration (Rust)

1. **Source:** GitHub release asset `<Language>-json.zip` at a pinned tag. Record `{repo, tag, commit, asset, sha256}` in the resource pin file next to the DCS pins. Do **not** use the API: it needs a key, has no pinning, serves different upstream data (AquiferDB), and its limits are undocumented.
2. **Build-time conversion** (xtask or `ensureBundledResources` step):
   - unzip
   - validate against Aquifer's JSON schema
   - parse into the internal helps model
   - drop `acai` (keep it optional) and the md/pdf/docx files
   - write the same on-disk bundle format the DCS path uses (for example SQLite or zstd'd JSON per book)
   - store the license block and provenance
3. **Runtime:** a `HelpsSource` trait with `DcsSource` and `AquiferSource` implementations. The user picks the provider per language. Tabs stay provider-agnostic.
4. **Phase 1 (view only):** Notes (tN + intros), Words (verse → tW via inverted passage links), Academy (tA by numeric id, with links resolved through `resource-ref`) and Questions.
5. **Phase 2 (optional check):** a tN check from Aquifer, using:
   - UGNT/UHB from DCS for tokens
   - Occurrence recovered by joining (BOOK, CCC:VVV, `#n`) to en_tn v91 through the English `content_id` (`article_metadata.localizations` links GL→eng)
   - SupportReference from `associations.resource` with `resource_code=="UWTranslationManual"`, mapped to a tA slug by a title crosswalk with en_ta v91

   Validate the join on several books before shipping. [INFERRED]

### Rust data-model sketch

```rust
/// Canonical-order (01..=66) BBCCCVVV; chapter/verse 0 = intro.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AqRef(u32);
impl AqRef {
    pub fn parse(s: &str) -> Result<Self, RefError>;   // exactly 8 ASCII digits
    pub fn book(self) -> CanonBook;                      // 1..=66 -> USFM code via table
    pub fn chapter(self) -> u16;  pub fn verse(self) -> u16;
}
#[derive(Clone, Copy)] pub struct AqRange { pub start: AqRef, pub end: AqRef }

#[derive(serde::Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(untagged)] pub enum ContentId { Num(u64), Str(String) }   // "247752" also arrives as a string: normalize

pub enum Order { Canonical, Alphabetical, Monograph }
pub enum IndexKey { Passage(AqRange), Sort(String) }

pub struct AqResourceMeta {
    pub code: String,            // == repo name, e.g. "UWTranslationNotes"
    pub language: Iso639_3,      // "por"
    pub order: Order,
    pub resource_type: String,   // "Translation Guide", …
    pub license: LicenseInfo,    // title, copyright, licenses, show_adaptation_*
    pub adaptation_notice_html: Option<String>,
    pub date_created: String,
}

pub struct AqArticle {
    pub id: ContentId,
    pub title: String,
    pub index: IndexKey,
    pub media: MediaType,        // Text | Image | Video | Audio
    pub review: ReviewLevel,     // None | Community | Professional
    pub html: String,
    pub passages: Vec<AqRange>,
    pub links: Vec<AqLink>,      // {resource_code, content_id, label, language}
}

/// Parsed view of a UWTranslationNotes article.
pub struct AqNote {
    pub id: ContentId,
    pub uw_id: Option<String>,     // Some("ayy1") when content_id = "en_tn_PHM_ayy1"
    pub verse: AqRef,              // BBCCC000 / BB000000 => intro
    pub ordinal: u16,              // from "(#n)"
    pub ol_quote: Option<String>,  // first span[style*=direction]; parts split on '&'
    pub gl_quote: Option<String>,  // display only
    pub body_html: String,
    pub ta_ids: Vec<ContentId>,    // links with resource_code == "UWTranslationManual"
    pub occurrence: Option<i8>,    // None from Aquifer; filled by optional en_tn join
}

pub enum Provenance {
    Dcs { owner: String, repo: String, tag: String, commit: String },
    Aquifer { repo: String, tag: String, commit: String, asset: String, sha256: [u8; 32] },
}

pub trait HelpsSource {
    fn notes(&self, book: CanonBook, ch: u16, v: u16) -> Vec<Note>;      // provider-neutral Note
    fn words_for_verse(&self, book: CanonBook, ch: u16, v: u16) -> Vec<WordRef>;
    fn word_article(&self, id: &WordId) -> Option<Article>;
    fn academy_article(&self, id: &AcademyId) -> Option<Article>;
    fn questions(&self, book: CanonBook, ch: u16, v: u16) -> Vec<Question>;
    fn provenance(&self) -> &Provenance;
    fn attribution_html(&self) -> String;
}
```

### Example pin

```toml
[[helps.aquifer]]
repo   = "UWTranslationNotes"
lang   = "por"           # ISO 639-3; map to app locale "pt-br"
tag    = "v2026-10-12"
commit = "dbfb46e027ee0807563918f4b95c80879cc32c60"
asset  = "Portuguese-json.zip"
sha256 = "646074262e54f0aaebbeb5e416f411a6c7df56204dcb560bc24d339c409888f3"
```

---

## Risks and open questions

- **Book gaps, eng:** eng tN and ULT lack NUM, 1CH, 2CH, ECC, ISA, JER and EZK, even though both are built on v91. [FILE] The reason is unknown.
- **Book gaps, GLs:** many GLs are partial. For example por tN covers 47 books and por tQ 35. Tabs must show "not available" per book.
- **Review status:** `review_level` is `None` on every entry of the por UW mirrors (65,726 tN entries). Who made the translations, and how, is not stated. The API enum has an `Ai` level. [FILE] [INFERRED: GL quality unknown]
- **Weekly tags:** content can change under the same English `content_id`. Pin a version and diff on upgrade.
- **Link quirks:** tA reverse links are mislabeled (`resource_code`). `data-content-id` values are not always numeric. The HTML has small template variations. Use a lenient parser and add tests.
- **Unreachable pages:** I could not fetch https://www.aquifer.bible/faqs, /aquifer-resources, /apiaccess or the Swagger file (DNS failure from WebFetch). The statements about them above are [SNIPPET] level only.
- **Prior conclusion on the issue.** #654 already has a comment (birchamp, 2026-10-14) concluding GO with Portuguese/Titus as the first slice (epic #711). [WEB: https://github.com/unfoldingWord/translationCore4/issues/654] This report agrees on GitHub-at-pinned-sha, CC BY-SA and the tab mapping. It adds three points:
  - tN can drive a check only with UGNT/UHB from DCS and recovered Occurrence; tW cannot drive a check.
  - por GL quotes do not come from an aligned text.
  - The size figure is ~17 MB compressed, ~62 MB stripped raw for 4 helps in por (the comment says ~80 MB per language).
