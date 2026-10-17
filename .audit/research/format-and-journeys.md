# tC4 normative project format and user journeys — research for a Rust rebuild

Source repository: `/home/user/unfoldingword/translationcore4` at commit `d494a9d` (2026-10-14).
Normative spec: `docs/BURRITO-SPEC.md` version 1.23 (2026-10-13), Appendix A version 2.4.

Citations use `SPEC §x (Lnnn)` for `docs/BURRITO-SPEC.md` line numbers, `ARCH`, `JOURNEYS`,
`PN #n` (`docs/PLATFORM-NOTES.md` note number), `CONTEXT`, and file paths for code. Where
the reference implementation (`journal/*.mjs`) pins down a detail that the prose leaves
open, this report quotes the code, because the spec says "If this document and the
harness disagree, the harness is wrong", while byte-identical output in practice depends
on the code (SPEC L7).

Read in full: BURRITO-SPEC.md, ARCHITECTURE.md, JOURNEYS.md, PLATFORM-NOTES.md, CONTEXT.md,
AGENTS.md, conformance/README.md, conformance/validate.mjs, conformance/normative/check.mjs,
both sample burritos, the sb-schema files that bind tC4. Read in part: validate-journal.mjs
(group index), validate-roundtrip.mjs, validate-transport.mjs, journal/{files,grammar,
schema,fold,checkpoint,skeleton,hlc,relationships,reconcile,story}.mjs,
src/data/journal/{identity,journalingStore}.ts, the team-sync fixture.

---

## 0. Executive summary

- **One project = one git repository = one valid Scripture Burrito (SB) at every commit.**
  There are two kinds: a Bible project (`scripture/textTranslation`) and an OBS project
  (`gloss/textStories`). No companion repos, no export/import loop (SPEC §1, L11-19).
- **Everything tC4-specific lives under `ingredients/checking/`.** `.tc4/` is impossible
  because the server's ingredient-path sanitizer forbids dot-prefixed segments (SPEC §2 L45; PN #6).
- **Phase 1 sidecars are JSON files** for alignments, check decisions, resource pins and settings.
  **The journal (Phase 2 write side, which ships in 4.0.0, D47)** is a per-actor directory of
  immutable, sha256-sealed "action segment" files. When the journal is active it is canonical,
  and `<BOOK>.usfm`, the sidecars, and `metadata.json` are derived, committed projections of
  the fold (SPEC §8 L427).
- **The fold is a deterministic, pure function of the event set:** de-duplicate by `ts`, sort by
  `ts` (plain string compare of a hybrid logical clock), apply per-key LWW registers with fork
  detection, and use structural chains for books. It must account for every record
  (projected, retained, invalid, pending, forked, auto-merged, or superseded); a record in none
  of these states is "silent data loss" (R-8.6.2).
- **The conformance harness is executable.** `validate.mjs` has 52 checks, `validate-journal.mjs`
  368, and the normative gate requires every one of the 105 `[R-…]` rule ids in §8 and §10 to be
  claimed by a live check. A Rust implementation can be validated by pointing
  `BURRITO=<dir>` / `OBS_BURRITO=<dir>` at its output, and by porting or bridging the journal suite.

---

## 1. Project kinds and overall model

| Kind | Flavor | Content | Spec |
|---|---|---|---|
| Bible project | `type.flavorType = {name:"scripture", flavor:{name:"textTranslation", usfmVersion:"3.0", …}}` | one plain USFM file per book | §1–§9 |
| OBS project | `type.flavorType = {name:"gloss", flavor:{name:"textStories"}}` | 50 Markdown story files | §10 (differences only) |

SPEC §1 (L13-17): a Bible project contains:
- the translation text, one plain-USFM file per book. This file is **canonical in Phase 1**.
- checking sidecars under `ingredients/checking/`: word alignments, check decisions, resource
  pins, and settings.
- per-actor event journals. When they are active they are canonical, and the USFM becomes a derived
  artifact that is still committed.

Phase framing (ARCH §11 A-2): "Draft USFM canonical in Phase 1; journals canonical in Phase 2;
USFM derived thereafter". In practice, the 4.0.0 app already journals every mutation (D47(c),
ARCH §9 L274-279), and the Phase-1 sidecars become regenerated mirrors at each checkpoint.

---

## 2. Exact on-disk layout

### 2.1 Bible project (SPEC §2, L21-45)

```
<project>/                              git repository
  metadata.json                         SB metadata (§3)
  .gitignore                            MUST contain: **/*.bak
  ingredients/
    <BOOK>.usfm                         one per book in scope (§4.1)
    vrs.json                            the versification scheme (§4.3) — NO role (see §6 below)
    checking/
      alignments/<BOOK>.json            role x-alignment        (§5.1)
      translationWords/<BOOK>.json      role x-check-decisions  (§5.2)
      translationNotes/<BOOK>.json      role x-check-decisions  (§5.2)
      translationQuestions/<BOOK>.json  reserved, post-Phase-1 only; same schema as §5.2
      resources.json                    role x-resource-links   (§5.3)
      settings.json                     role x-check-settings   (§5.4)
      journal/<actorId>/segments/<encoded-ts>.action.json   role x-journal (§8.1)
      journal/<actorId>/actor.json                           role x-journal (§8.1)
    audio/…                             reserved slot; no behavior; readers MUST NOT fail on it
```

- `<BOOK>` is the UPPERCASE 3-character USFM code, and it MUST be in the eng canon (66 books). Non-canonical
  books are not allowed (D26) (SPEC §2 L41). The closed list is in `journal/grammar.mjs` `BOOK_CODES`
  (GEN…REV in canon order).
- Reserved `ingredients/audio/` (SPEC §2 L43): files there register as ordinary ingredients. A
  checkpoint MUST NOT touch, regenerate or delete them (R-8.7.3), and they are never divergence.
- Note the stale comment: the §2 tree says `vrs.json  role x-versification`, but §4.3 (L105-112,
  corrected 2026-08-24, issue #15) withdrew that role. **vrs.json carries no role.**
- **Ingredient path ("ipath") constraints** (SPEC §2 L45, from pankosmia-web `utils/paths.rs`):
  no segment may be empty, start with `.`, or contain any of
  `..  ~  \  &  *  +  |  space  ?  #  %  {  }  <  >  $  !  '`. The reference implementation adds
  more refusals (`journal/grammar.mjs` `ipathError`): C0/C1 control characters, bidi controls, Windows reserved
  device names (`con|prn|aux|nul|com[1-9]|lpt[1-9]`), and a segment ending in `.`.
- `.bak` files are the server's single-level undo (delete = rename to `.bak`). They are git-ignored
  and excluded from ingredient scans (SPEC §6 W-3; PN #8).

### 2.2 OBS project (SPEC §10.2, L583-608)

```
<project>/
  metadata.json                         SB metadata (§3 + §10.1)
  .gitignore                            MUST contain: **/*.bak
  ingredients/
    LICENSE.md                          copied from the source, byte-identical
    content/01.md … content/50.md       one story file per story (§10.3)
    content/front/title.md              copied from the gateway source; not drafted
    content/front/intro.md              copied from the gateway source; not drafted
    content/back/intro.md               copied from the gateway source; not drafted
    checking/
      translationNotes/OBS.json         role x-check-decisions  (§10.5)
      translationWords/OBS.json         role x-check-decisions  (§10.5)
      resources.json                    role x-resource-links   (§5.3 + §10.6)
      settings.json                     role x-check-settings   (§5.4)
      journal/<actorId>/…               role x-journal          (§8.1, §10.7)
```

- There is no `vrs.json` and no alignment sidecar (R-10.2.1). Story path = `content/NN.md`, with two digits,
  for stories 1–50. The scope is always all fifty stories (R-10.2.2).
- The layout is pankosmia's `text_stories` content template, byte for byte, vendored at
  `conformance/fixtures/text_stories/` (SPEC §10.2 L585).

### 2.3 What the reference samples actually contain

`conformance/sample-burrito/` (10 files; a Phase-1, **unjournaled** project — no `journal/`
directory):

```
.gitignore                                       9 bytes   "**/*.bak\n"
metadata.json                                 5959
ingredients/JON.usfm                           744   (stub book + the 2:9-10 span fixture)
ingredients/TIT.usfm                          1166   (1:1-1:5 drafted, rest `___`)
ingredients/vrs.json                         27681   (eng scheme)
ingredients/checking/alignments/TIT.json      7301   (one record: 1:1)
ingredients/checking/resources.json           6361
ingredients/checking/settings.json             467
ingredients/checking/translationNotes/TIT.json 2732  (2 decisions)
ingredients/checking/translationWords/TIT.json 1913  (2 decisions)
```

`conformance/sample-burrito-obs/` (58 files): `.gitignore`, `metadata.json`,
`ingredients/LICENSE.md`, `content/01.md`–`50.md`, `content/front/{title,intro}.md`,
`content/back/intro.md`, `checking/resources.json`, `checking/settings.json`. Story 1 is
drafted (title, frames 1–2, reference line). Stories 2–50 are in seed form. There are no decision files.

Real journals, written by the reference writers, are in `conformance/fixtures/team-sync/journals/translator-{a,b,c}/`
(see §8.10 below).

---

## 3. `metadata.json` (SPEC §3, L47-74)

Base it on Pankosmia's textTranslation template
(`resource-core/templates/content_templates/text_translation/metadata.json`), then apply these rules:

1. `meta.category` = `"source"`; `meta.normalization` = `"NFC"`;
   `meta.generator.softwareName` identifies tC4.
2. `idAuthorities` MUST include `local`. It MUST include `dcs`
   (`{"id": "https://git.door43.org", "name": {"en": "Door43 Content Service"}}`) when
   `relationships` are present.
3. `type.flavorType` = `{name:"scripture", flavor:{name:"textTranslation", usfmVersion:"3.0", …}}`.
   The bundled schema `sb-schema/scripture/text_translation.schema.json` **requires** `name`,
   `projectType`, `translationType`, `audience`, `usfmVersion`, with `additionalProperties:false`.
   Enums: projectType ∈ {standard, daughter, studyBible, studyBibleAdditions, backTranslation,
   auxiliary, transliterationManual, transliterationWithEncoder}; translationType ∈
   {firstTranslation, newTranslation, revision, studyOrHelpMaterial}; audience ∈ {basic, common,
   common-literary, literary, liturgical, children}.
4. `type.flavorType.currentScope`: one key per book. The value `[]` means the whole book (the default). The value can also be
   an array of range strings (a "passage set", D26). The range grammar is `C`, `C-C`, `C:V`, `C:V-V`, `C:V-C:V`.
   A project holds 1–66 books.
   - Reference regex (`journal/grammar.mjs` `SCOPE_RANGE_RE`): `^(\d+(-\d+)?|\d+:\d+(-\d+(:\d+)?)?)$`.
   - The SB schema (`sb-schema/scope.schema.json`) is stricter. It uses no leading zeros (`[1-9][0-9]*`), `minItems:1`, and
     `uniqueItems:true` for non-empty arrays, with `minProperties:1` on the scope object, and property names are SB book ids.
5. `ingredients` MUST list **every** file under `ingredients/`. The key includes the `ingredients/` prefix.
   Each entry MUST have `checksum.md5`, `size`, `mimeType` (`text/plain` for `.usfm`,
   `application/json` for `.json`; OBS stories `text/markdown`), and `role` per §2. A file whose
   basename is a book code MUST also have `scope` (`{"<BOOK>": []}` or ranges). **No other fields**
   are allowed (`ingredient.schema.json` `additionalProperties:false`; `checksum` allows `md5`
   (required, `^[a-f0-9]{32}$`) plus at most one of `sha3-256`/`sha3-512`).
6. `relationships` (Stage 2): a **derived mirror** of `checking/resources.json`, made by
   `relationshipsFromPins` (`journal/relationships.mjs`) (SPEC L70):
   - one row per distinct repository; the first pin that names the repo wins.
   - order: original languages, extraScripture, primary set, fallback set, then the other `resources`
     groups (the lexicons).
   - row = `{relationType, flavor, id: "dcs::<owner>/<repo>", revision: <pin.version>}`. `revision` is
     omitted when the pin has no `version` label.
   - relationType from the flavor type: `scripture`→`source`, `parascriptural`→`parascriptural`,
     `peripheral`→`peripheral`. Any other type (e.g. OBS source `gloss/textStories`) gets **no row**.
   - Schema constraints: `id` matches `^[0-9a-zA-Z][0-9a-zA-Z\-]{1,31}::\S+$` with the prefix declared
     in `idAuthorities`. Original-language repos MUST use `source`+`textTranslation`. An `x-` flavor
     MUST NOT use `relationType:"target"` (it would match two `oneOf` branches) (SPEC L72).

### 3.1 Full example: `conformance/sample-burrito/metadata.json`

```json
{
  "format": "scripture burrito",
  "meta": {
    "version": "1.0.0", "category": "source",
    "generator": {"softwareName": "translationCore 4 (sample)", "softwareVersion": "0.1.0", "userName": "Equipo Ejemplo"},
    "defaultLocale": "en", "dateCreated": "2026-07-02T12:00:00.000Z", "normalization": "NFC"
  },
  "idAuthorities": {
    "local": {"id": "http://_local_", "name": {"en": "Local Project"}},
    "dcs":   {"id": "https://git.door43.org", "name": {"en": "Door43 Content Service"}}
  },
  "identification": {
    "primary": {"local": {"ejemplo_tj": {"revision": "1", "timestamp": "2026-07-02T12:00:00.000Z"}}},
    "name": {"en": "Equipo Ejemplo — Tito y Jonás", "es-419": "Equipo Ejemplo — Tito y Jonás"},
    "description": {"en": "Sample multi-book tC4 drafting+checking project (…)"},
    "abbreviation": {"en": "ejemplo_tj"}
  },
  "languages": [{"tag": "es-419", "name": {"en": "Spanish (Latin America)", "es-419": "Español (Latinoamérica)"}, "scriptDirection": "ltr"}],
  "type": {"flavorType": {
    "name": "scripture",
    "flavor": {"name": "textTranslation", "usfmVersion": "3.0", "translationType": "firstTranslation", "audience": "common", "projectType": "standard"},
    "currentScope": {"TIT": [], "JON": []}
  }},
  "confidential": false,
  "localizedNames": {},
  "ingredients": {
    "ingredients/JON.usfm": {"checksum": {"md5": "33a071cb6e88abeb4354727aa22e0fe8"}, "mimeType": "text/plain", "size": 744, "scope": {"JON": []}},
    "ingredients/TIT.usfm": {"checksum": {"md5": "1ce94f44dd0f1c4e87fa88a7a80f9216"}, "mimeType": "text/plain", "size": 1166, "scope": {"TIT": []}},
    "ingredients/checking/alignments/TIT.json": {"checksum": {"md5": "a85b64a6…"}, "mimeType": "application/json", "size": 7301, "scope": {"TIT": []}, "role": "x-alignment"},
    "ingredients/checking/resources.json": {"checksum": {"md5": "1566bca9…"}, "mimeType": "application/json", "size": 6361, "role": "x-resource-links"},
    "ingredients/checking/settings.json": {"checksum": {"md5": "f92a6a03…"}, "mimeType": "application/json", "size": 467, "role": "x-check-settings"},
    "ingredients/checking/translationNotes/TIT.json": {"checksum": {"md5": "490c0191…"}, "mimeType": "application/json", "size": 2732, "scope": {"TIT": []}, "role": "x-check-decisions"},
    "ingredients/checking/translationWords/TIT.json": {"checksum": {"md5": "6835ebf6…"}, "mimeType": "application/json", "size": 1913, "scope": {"TIT": []}, "role": "x-check-decisions"},
    "ingredients/vrs.json": {"checksum": {"md5": "dd0169fc…"}, "mimeType": "application/json", "size": 27681}
  },
  "copyright": {"shortStatements": [{"statement": "CC BY-SA 4.0, Equipo Ejemplo"}]},
  "relationships": [
    {"relationType": "source", "flavor": "textTranslation", "id": "dcs::unfoldingWord/el-x-koine_ugnt", "revision": "v0.34"},
    {"relationType": "source", "flavor": "textTranslation", "id": "dcs::unfoldingWord/hbo_uhb", "revision": "v3.0.0"},
    {"relationType": "source", "flavor": "textTranslation", "id": "dcs::unfoldingWord/en_ult", "revision": "v91"},
    {"relationType": "source", "flavor": "textTranslation", "id": "dcs::unfoldingWord/en_ust", "revision": "v91"},
    {"relationType": "parascriptural", "flavor": "x-bcvnotes", "id": "dcs::es-419_gl/es-419_tn", "revision": "v66"},
    {"relationType": "parascriptural", "flavor": "x-bcvarticles", "id": "dcs::es-419_gl/es-419_twl", "revision": "v18"},
    {"relationType": "parascriptural", "flavor": "x-bcvarticles", "id": "dcs::es-419_gl/es-419_tw", "revision": "v37"},
    {"relationType": "peripheral", "flavor": "x-peripheralArticles", "id": "dcs::es-419_gl/es-419_ta", "revision": "v4"},
    {"relationType": "parascriptural", "flavor": "x-bcvnotes", "id": "dcs::unfoldingWord/en_tn", "revision": "v91"},
    {"relationType": "parascriptural", "flavor": "x-bcvarticles", "id": "dcs::unfoldingWord/en_twl", "revision": "v91"},
    {"relationType": "parascriptural", "flavor": "x-bcvarticles", "id": "dcs::unfoldingWord/en_tw", "revision": "v91"},
    {"relationType": "peripheral", "flavor": "x-peripheralArticles", "id": "dcs::unfoldingWord/en_ta", "revision": "v91"},
    {"relationType": "parascriptural", "flavor": "x-bcvquestions", "id": "dcs::unfoldingWord/en_tq", "revision": "v91"},
    {"relationType": "peripheral", "flavor": "x-lexicon", "id": "dcs::unfoldingWord/en_ugl", "revision": "v0.5"},
    {"relationType": "peripheral", "flavor": "x-lexicon", "id": "dcs::unfoldingWord/en_uhl"}
  ]
}
```

Observations:
- `en_ust` appears in both `extraScripture` and the fallback's `simplifiedText`. It is de-duplicated to one row.
- `en_uhl` has no `version` label (sha-only pin), so it has no `revision`.
- `localizedNames: {}` is present. The server's `BurritoMetadata` struct **requires** this key. The OBS
  template lacks it, which breaks every server rescan on OBS projects (PN #36).

### 3.2 Scripture Burrito flavors and ingredient roles used

| Where | Value |
|---|---|
| Bible project flavor | `scripture` / `textTranslation` (usfmVersion `"3.0"`) |
| OBS project flavor | `gloss` / `textStories` (flavor object allows only `name`) |
| Ingredient roles (all `x-` custom; schema pattern `^x-\S.*\S$`) | `x-alignment`, `x-check-decisions`, `x-resource-links`, `x-check-settings`, `x-journal`; OBS images override: `x-obsimages` |
| `vrs.json` | no role |
| Pin flavors (in resources.json, `type/flavor`) | `scripture/textTranslation` (UGNT, UHB, ULT, UST, GST), `parascriptural/x-bcvnotes` (tn), `parascriptural/x-bcvarticles` (twl, tw, obs-twl), `parascriptural/x-bcvquestions` (tq), `peripheral/x-peripheralArticles` (tA), `peripheral/x-lexicon` (UGL/UHL), `gloss/textStories` (obs), `peripheral/x-obsnotes` (obs-tn), `peripheral/x-obsquestions` (obs-tq), `peripheral/x-obsimages` (obs_images_360) |

**Roles are decorative (stage rule S-2, permanent; SPEC §6 W-2 L403):** every server rescan
rebuilds the ingredients table from disk and drops `x-` roles. Readers MUST NOT depend on roles.
Paths are authoritative. A pure-HTTP client cannot re-assert roles, because no HTTP route writes `metadata.json`.
So app-created projects carry no roles after their first regeneration, and this is the accepted condition.

### 3.3 OBS `metadata.json` differences (R-10.1.1, R-10.2.3)

- `type.flavorType = {"name":"gloss","flavor":{"name":"textStories"},"currentScope":{…}}`. The schema allows nothing else
  in the flavor object.
- `currentScope` is the template's table **verbatim** (33 books that the stories retell, e.g.
  `"GEN": ["1-2","3","6-8","11-15","16-22","24:1-25:26","25:27-35:29","37-50","3:15","12:1-3","1-3","6","14","22"]`;
  note the overlapping, unsorted ranges). A writer MUST NOT reconstruct or filter it. The keys
  and ranges are checked, but key order is not, because each server commit from pankosmia-web 0.18.10 sorts keys (PN #48).
- Story ingredient `mimeType` is `text/markdown`. Story ingredients carry no `scope`.
- The sample OBS metadata has `idAuthorities.local` only (no `relationships`, no `dcs`).

---

## 4. Text ingredients (Bible)

### 4.1 `ingredients/<BOOK>.usfm` (SPEC §4.1, L78-84)

- USFM 3.0, one file per book. Header: `\id`, `\usfm 3.0`, `\h`, `\toc1-3`, `\mt`, then
  `\c`/`\p`/`\v`. **Untranslated verse stub: `\v N ___`.**
- **I-1: no alignment markup at rest.** No `\zaln-s`/`\zaln-e`, and no `\w` word-attribute tokens.
  Alignments are woven in only on export (aligned-USFM producer).
- **Verse keys are strings and MAY be spans.** `\v 9-10` parses to the single key `"9-10"`. NEVER
  `Number()`-coerce a verse key (harness checks "spans").
- **A partial book file is legal** when scope is a passage set (D26).
- **No `\ts\*` written by tC4.** Sections are derived from the *source* text at load. An imported file
  that carries `\ts\*` is preserved byte-exactly (D14; SPEC §8.4a).
- Sample `TIT.usfm` (excerpt):
  ```
  \id TIT ejemplo_tj
  \usfm 3.0
  \h Tito
  \toc1 La Epístola de Pablo a Tito
  \toc2 Tito
  \toc3 Tit
  \mt Tito
  \c 1
  \p
  \v 1 Pablo, siervo de Dios y apóstol de Jesucristo, según la fe … con la piedad,
  \v 2 con la esperanza de la vida eterna, …
  \v 6 ___
  ```
  JON 2 ends with `\v 9-10 Mas yo te ofreceré sacrificios …` (the span fixture).

### 4.2 Derivations, never stored (SPEC §4.2, L86-93)

- **targetBible** = `usfm-js.toJSON(usfm).chapters` → `{ "<ch>": { "<v>": {verseObjects} } }` + `headers`.
- **Check item lists**: derived at load from pinned tN/tW TSVs plus the original-language book, then merged with
  stored decisions by identity key. They MUST filter to scope. Progress = decided ÷ in-scope derived total.
- **The tW article gate** (issue #643): a TWL row whose linked article is not carried by the
  resolving set's tw pin derives no item (not shown, not counted). The gate reads the tw pin's
  `metadata.json` ingredient list (`payload/<category>/<slug>.md`). Fallback happens per book, never per
  article. A stored decision on a hidden item is invalidated and kept (D36).
- **Aligned USFM** is produced only on export (`wordaligner.merge` → `\zaln`).
- Derived caches MAY be written (e.g. decision `summary`). They are disposable.

TSV column layouts (ARCH §3.4): TWL 6 columns `Reference, ID, Tags, OrigWords, Occurrence, TWLink`;
tN 7 columns `Reference, ID, Tags, SupportReference, Quote, Occurrence, Note`. tW articles are
`payload/{kt,names,other}/<id>.md`. tA is `<category>/<article>/01.md` + `translate/toc.yaml`. Lexicons are
`content/<entry>.json`. TWLink forms: `./payload/kt/son.md` (sb-zip) or
`rc://*/tw/dict/bible/kt/god` (RC/tC3). The slug is the last segment without `.md`, and the category is the segment before it.

---

## 5. Versification (consolidated rules)

### 5.1 `ingredients/vrs.json` (SPEC §4.3, L95-138)

- Written by the platform at creation (`new_text_translation.rs`). It holds the **full chosen scheme**
  in the platform's vrs-to-JSON shape:
  ```jsonc
  {
    "maxVerses":     { "GEN": ["31","25","24", …], …92 books… },  // per-chapter verse counts AS STRINGS
    "mappedVerses":  { "GEN 31:55": "GEN 32:1", "GEN 32:1-32": "GEN 32:2-33", … },  // 265 entries in eng
    "excludedVerses": [],
    "partialVerses": {}
  }
  ```
- **No `role`.** The platform registers it with `checksum`, `mimeType`, `size` only.
- Platform ships six schemes: `eng, lxx, org, rsc, rso, vul` (`GET /content-utils/versifications`,
  `GET /content-utils/versification/{scheme}`). The default is **`eng`**, and the user may choose another at creation.
  **The scheme is fixed for the life of the project.** The client MUST NOT edit the file after creation.
- `maxVerses` MUST cover every book in `currentScope`.
- **The scheme NAME is not recorded in the burrito.** Resolve the name by a ladder: (1) the journal's
  `project.vrs.set` register (`name` field), (2) a **normalized** (canonical-form, not byte) fingerprint
  against the served schemes, (3) `unknown`. `unknown` MUST NOT be treated as `eng`. An absent file
  carries no information.
- `mappedVerses` maps THIS scheme's refs INTO `org`. `org` is the pivot for scheme-to-scheme conversion
  (forward through source table, backward through target). Values MAY be a string or an **array of
  strings** (one-to-many). Readers MUST accept both. Key grammar `^[A-Z0-9]{3} \d+:\d+(-\d+)?$`;
  target grammar additionally allows a sub-verse letter `^[A-Z0-9]{3} \d+:\d+[a-z]?(-\d+)?$`
  (validate.mjs L143-144; 103 eng targets use a letter).
- The server never maps a reference. `maxVerses` is the only key any platform code reads.

### 5.2 The frame rule (SPEC §5.2, L224-284; D24(c), D60)

- `reference.chapter`/`reference.verse` in decisions, in check items, and in every stored ref are in the
  **project's** versification. Resource refs map into the project frame **at derive time**, before
  scope filtering, merge and write.
- **Every unfoldingWord resource (helps TSVs and original-language texts) is in the `eng` frame**
  (measured on 194,080 tN refs, 58,834 TWL refs, and 1,189 OL chapters).
- **Same frame ⇒ no conversion and no validation.** An eng project MUST NOT be mapped (eng→org→eng loses
  3 verses). Pass-through keeps forms such as `front`, comma lists and letter verses. Only a real cross-frame
  conversion rejects them as `malformed-reference`.
- Mechanism: Proskomma's versification toolkit (`mapVerse`), loaded lazily.
- **Write once.** A mapped reference becomes the §5.2 identity key and the §8.5 register key. It is never
  re-derived from a stored reference. Runtime cross-frame needs (the aligner reading the OL verse) are
  lookups, never written into keys. §5.1 alignment records stay keyed in the project frame.
- **Unplaceable references are dropped and never guessed.** Validate the mapping result against the target
  scheme's `maxVerses`. Below 1, past the last verse, in a missing chapter, split across chapters, or ambiguous
  ⇒ omit the check with a recorded reason and never journal it. It leaves the progress **denominator**. A reader
  MUST surface the count.
- **Fan-out becomes a span when contiguous** (`"67-68"`). A gapped fan-out is unplaceable.
- Known losses vs en_tn@v90: rsc 8 rows, rso 13, lxx 81, vul 1,067; eng 0.
- Scheme names are open: never reject an unfamiliar scheme name.

### 5.3 Renumber interaction

The project frame never moves on a renumber. So a renumbered alignment is re-keyed and marked
`invalid:true`. A decision never re-keys: it is invalidated and kept on its key (R-8.5.20–24; §8.5 below).

---

## 6. Checking sidecars — schemas and examples

Common rules (SPEC §5, L140-145):
- Every sidecar has top-level `schemaVersion` (integer). Decisions, alignments and settings use `1`. resources.json uses `2`.
- **I-2: every `occurrence`/`occurrences` is an integer.** USFM attribute parsing yields strings, so
  normalize before persisting. The alignment libraries fail wholesale on strings.
- `contextId.reference.bookId` is **lowercase** (`"tit"`), the tC3 convention. Filenames and scope use uppercase.
- Spec evolution (§9 L569-571): bump `schemaVersion` only for a breaking change. Readers MUST reject an unknown
  major version. Additive optional fields do not bump.
- **Byte form at checkpoint** (`journal/checkpoint.mjs` `serialize`): `JSON.stringify(doc, null, 2) + "\n"`.
  That is 2-space indentation, a trailing LF, and JS key-order semantics (see §8.7.3 below).

### 6.1 Alignments: `checking/alignments/<BOOK>.json` (role `x-alignment`; SPEC §5.1 L147-183)

```jsonc
{
  "schemaVersion": 1,
  "book": "TIT",
  "chapters": {
    "1": {
      "1": {
        "alignments": [            // one per original-language word/word-group, INCLUDING empty ones
          { "topWords":    [{"word":"Παῦλος","strong":"G39720","lemma":"Παῦλος","morph":"Gr,N,,,,,NMS,","occurrence":1,"occurrences":1}],
            "bottomWords": [{"word":"Pablo","occurrence":1,"occurrences":1}] }   // [] if unaligned
        ],
        "wordBank": [{"word":"de","occurrence":1,"occurrences":5}],              // target words not yet aligned
        "invalid": false,                   // set true when re-review is needed
        "targetVerseMd5": "<md5 lowercase hex>",
        "sourceVersion": "dcs::unfoldingWord/el-x-koine_ugnt@v0.34",
        "done": true                        // OPTIONAL (D73); no `false` form
      }
    }
  }
}
```

The sample record TIT 1:1 has 10 `alignments` (one has two `topWords`: Ἰησοῦ+Χριστοῦ → Jesucristo),
21 `wordBank` entries, `invalid:false`, `targetVerseMd5:"f8aee622daff1496bccc7c2e3b358b43"`. Key
order inside the sample's word objects is `strong, lemma, morph, occurrence, occurrences, word`, which is the
`unmerge` output order.

Rules:
- The payload is **exactly** what `word-aligner`'s `unmerge` produces and `merge` consumes. The library key
  `alignment` is persisted as `alignments`.
- **`targetVerseMd5`** = md5 (lowercase hex) of the UTF-8 bytes of the verse's plain text. The plain text is the
  concatenation of the `text` fields of the verse's usfm-js verseObjects, then `trim()`. **I-3:
  an entry is valid only if this matches the current verse text**, whatever the flag says. "Plain
  text" is operationally **usfm-js 3.4.3**. A non-JS implementation MUST reproduce it. The executable contract is
  the stored sample hashes and harness fixtures (SPEC L180). The journal's version (`journal/fold.mjs`
  `verseTextMd5`) parses `\v 1 ${content}` with `{chunk:true}`, concatenates every verseObject's
  `text`, trims, and md5s. It refuses content that contains `\v `/`\c ` or U+0001.
- A verse with no alignment work MAY be absent.
- **`done`** (D73): set when every target word is placed and no source group lacks a target word, or by
  "Mark valid" with words still in `wordBank` (Mark valid also re-stamps `targetVerseMd5` and clears
  `invalid`). Recompute on every later alignment edit, including a reflow after a text edit. Absent means not done.
  `done:true` never outranks I-3 or `invalid`.
- **Renumber:** re-key to the new verse key, keep `alignments`/`wordBank`/`targetVerseMd5` (and every other
  field), and set `invalid:true` (R-8.5.21; the fold sets it).
- An original-language text upgrade marks every alignment of the covered books `invalid:true` and keeps them
  (J12).

### 6.2 Decisions: `checking/<toolId>/<BOOK>.json` (role `x-check-decisions`; SPEC §5.2 L185-294)

`toolId` ∈ `translationWords` | `translationNotes` (`translationQuestions` reserved). The journal grammar
closes the set to the first two (`TOOL_IDS`).

Full sample (`sample-burrito/ingredients/checking/translationWords/TIT.json`):

```json
{
  "schemaVersion": 1,
  "tool": "translationWords",
  "book": "TIT",
  "resource": {
    "repoPath": "git.door43.org/es-419_gl/es-419_twl",
    "version": "v18",
    "sha": "c2d9547ec95e40c2ba085c597b127c94733feb8a",
    "languageSet": "primary"
  },
  "decisions": [
    {
      "contextId": {
        "checkId": "a9p2",
        "occurrenceNote": "",
        "reference": {"bookId": "tit", "chapter": 1, "verse": 1},
        "tool": "translationWords",
        "groupId": "apostle",
        "quote": "ἀπόστολος",
        "quoteString": "ἀπόστολος",
        "glQuote": "",
        "occurrence": 1
      },
      "category": "kt",
      "selections": [{"text": "apóstol", "occurrence": 1, "occurrences": 1}],
      "comments": false,
      "reminders": true,
      "nothingToSelect": false,
      "verseEdits": false,
      "invalidated": false,
      "modifiedTimestamp": "2026-07-02T14:24:39.000Z"
    },
    {
      "contextId": {"checkId": "t1g7", "occurrenceNote": "", "reference": {"bookId": "tit", "chapter": 1, "verse": 1},
                    "tool": "translationWords", "groupId": "god", "quote": "Θεοῦ", "quoteString": "Θεοῦ", "glQuote": "", "occurrence": 1},
      "category": "kt",
      "selections": [{"text": "Dios", "occurrence": 1, "occurrences": 2}],
      "comments": false, "reminders": false, "nothingToSelect": false, "verseEdits": false, "invalidated": false,
      "status": "valid",
      "modifiedTimestamp": "2026-07-02T14:21:07.000Z"
    }
  ],
  "summary": {"note": "derived cache, regenerable — decided counts by category", "decided": {"kt": 2}}
}
```

tN differences (from `translationNotes/TIT.json`): `quote` is a **word-occurrence array**
`[{"word":"χάρις","occurrence":1},{"word":"καὶ","occurrence":1},{"word":"εἰρήνη","occurrence":1}]`, and it
MUST NOT be flattened. `quoteString` is `"χάρις καὶ εἰρήνη"`. `occurrenceNote` is the note text. `glQuote` is the gateway quote.
`groupId` is the tA module (`"translate-blessing"`). `category` is the tA category (`"translate"`). The `gr8c` decision
shows the re-review state `verseEdits:true, invalidated:true` with no `status`, so it derives as `invalid`.
`comments` can be a string (a **check comment**).

Field semantics:
- The record is the **full tC3 check-item shape**. Do not simplify it, because it must round-trip with `tc-checking-tool-rcl`.
  The harness requires `contextId.{checkId, occurrenceNote, reference, tool, groupId, quote, quoteString,
  glQuote, occurrence}`, `reference.{bookId, chapter, verse}`, and top-level `{selections, comments,
  reminders, nothingToSelect, verseEdits, invalidated}` (validate.mjs L240-254).
- `modifiedTimestamp` is REQUIRED (Phase-2 forward-compat; the seeding source of `ts`).
- **Only touched checks are stored.** A missing decision means "unchecked".
- **Identity key** (normative): `(contextId.checkId, reference.bookId, reference.chapter,
  reference.verse, contextId.occurrence)`, with `quoteString` as a verification field. A key match whose
  quoteString differs SHOULD be treated as unmatched. **Chapter and verse compare as strings.**
  `reference.verse` is stored as a JSON number for single verses and as the span string for spans.
  - Serialized form (`journal/grammar.mjs` `identityKeyOf`): `checkId|bookId|chapter|verse|occurrence`,
    e.g. `t1g7|tit|1|1|1`. Each part must be non-empty and contain neither `|` nor `:`.
  - **Decision key** (toolId-prefixed), used by note targets, dispositions and the fold's `dec|` registers:
    `translationWords|t1g7|tit|1|1|1`.
  - OBS story key: `checkId|obs|story|frame|occurrence` (R-10.5.1).
- **`selections`**: `false` = none. `[]` is never used (it coerces to `false`). Element = `{text, occurrence,
  occurrences}`. **Done** = `selections !== false || nothingToSelect === true`.
- **`status`** (OPTIONAL, D2): `"valid" | "invalid" | "todo"`. When absent, derive it: `invalidated` ⇒
  invalid; done ⇒ valid; else todo. A writer that sets `invalidated:true` MUST NOT leave `status:"valid"`.
- **`userInvalid`** (OPTIONAL, D94): marks a user's own Invalid on an invalidated decision. Rules:
  invalidating a not-yet-invalidated decision whose `status` is `"invalid"` ⇒ write `userInvalid:true`.
  On a decision that is already invalidated, a user status write sets `userInvalid:true` iff status is `"invalid"`, else
  removes it. On re-attach by identity key after a re-pin: `invalidated:false`, remove `userInvalid`, and
  `status` = `"invalid"` if `userInvalid` was true, else **`"todo"`** (also when `status` was absent). The field only appears
  together with `invalidated:true` and `status:"invalid"`.
- `verseEdits`/`invalidated` replace tC3's marker files. `reminders` is the **bookmark** flag. `comments` is the
  **check comment** (`false` or a string; clearing it writes `false`).
- **Resolution record `resource`** (D17/D30/D58): `{repoPath, version?, sha, languageSet?}`. It names the repo
  the book's check list derived from: tN → the set's `translationNotes` pin, tW → the set's
  `translationWordsLinks` pin. It MUST equal one rung's pin by **(repoPath + sha)**. `languageSet`
  (`"primary"|"fallback"`), when present, must agree with the match. If the pin equals both rungs (the D97 ruling 7
  form), it resolves to `primary`. A change of resolution is always a warned update.
- **Cross-language re-attach** (D17): when `checkId` finds no match, re-attach on
  `(reference + original-language quote + occurrence)`, tie-break on `groupId`. A match that stays ambiguous is
  **unplaced** and never auto-attached.
- **Carry-over** (D36): when a resolution changes, recompute each affected book's file. Re-attached
  decisions are re-keyed to the NEW `contextId`. Unplaceable ones get `invalidated:true,
  status:"invalid"` (+`userInvalid` per rule) and are **kept in full**. They are never deleted and never counted as
  progress. There is no review queue.
- **Renumber**: a decision never moves. It is invalidated and retained on its own key (R-8.5.22).
- `summary` is OPTIONAL and disposable. **Checkpoint projections do not emit it**
  (`projectDecisions` writes `schemaVersion, tool, book, resource, decisions` only). The app strips it
  for the seed comparison (`journalingStore.ts` L1766-1773). So it disappears after first open.
- **Decision array order at checkpoint**: sorted by the canonical JSON of `contextId` (keys sorted,
  `canonKeys`). In effect this sorts by `checkId` first (fold.mjs L880-885). The sample is stored in this
  order (a9p2, t1g7 / gr8c, swi9) so it can be seeded.

### 6.3 Resource pins: `checking/resources.json` (role `x-resource-links`; SPEC §5.3 L296-357)

```jsonc
{
  "schemaVersion": 2,
  "languageSets": {                     // EXACTLY the keys primary and fallback
    "primary": {
      "gatewayLanguage": {"languageId": "es-419", "owner": "es-419_gl"},
      "translationNotes":      {"repoPath": "git.door43.org/es-419_gl/es-419_tn",  "version": "v66", "sha": "22f3d0c6…", "flavor": "parascriptural/x-bcvnotes", "books": ["TIT","JON","RUT","3JN"]},
      "translationWordsLinks": {"repoPath": "git.door43.org/es-419_gl/es-419_twl", "version": "v18", "sha": "c2d9547e…", "flavor": "parascriptural/x-bcvarticles", "books": ["TIT","JON","RUT","3JN"]},
      "translationWords":      {"repoPath": "git.door43.org/es-419_gl/es-419_tw",  "version": "v37", "sha": "7586f4ff…", "flavor": "parascriptural/x-bcvarticles", "books": ["TIT","JON","RUT","3JN"]},
      "translationAcademy":    {"repoPath": "git.door43.org/es-419_gl/es-419_ta",  "version": "v4",  "sha": "26606b57…", "flavor": "peripheral/x-peripheralArticles"}
    },
    "fallback": {                       // English suite that ships with the install; languageId MUST be "en"
      "gatewayLanguage": {"languageId": "en", "owner": "unfoldingWord"},
      "translationNotes":      {"repoPath": "git.door43.org/unfoldingWord/en_tn",  "version": "v91", "sha": "e586762e…", "flavor": "parascriptural/x-bcvnotes", "books": [/* 59 codes */]},
      "translationWordsLinks": {"repoPath": "git.door43.org/unfoldingWord/en_twl", "version": "v91", "sha": "7cbb3419…", "flavor": "parascriptural/x-bcvarticles", "books": [/* 59 codes */]},
      "translationWords":      {"repoPath": "git.door43.org/unfoldingWord/en_tw",  "version": "v91", "sha": "ff5b3852…", "flavor": "parascriptural/x-bcvarticles", "books": ["BIBLE"]},
      "translationAcademy":    {"repoPath": "git.door43.org/unfoldingWord/en_ta",  "version": "v91", "sha": "ce9a1bb9…", "flavor": "peripheral/x-peripheralArticles"},
      "translationQuestions":  {"repoPath": "git.door43.org/unfoldingWord/en_tq",  "version": "v91", "sha": "8be02772…", "flavor": "parascriptural/x-bcvquestions"},   // OPTIONAL
      "simplifiedText":        {"repoPath": "git.door43.org/unfoldingWord/en_ust", "version": "v91", "sha": "85f274a7…", "flavor": "scripture/textTranslation"}       // OPTIONAL
    }
  },
  "resources": {                        // language-set-independent
    "originalLanguage": {
      "nt": {"repoPath": "git.door43.org/unfoldingWord/el-x-koine_ugnt", "version": "v0.34", "sha": "fc95b2b8…", "flavor": "scripture/textTranslation"},
      "ot": {"repoPath": "git.door43.org/unfoldingWord/hbo_uhb", "version": "v3.0.0", "sha": "74022f0f…", "flavor": "scripture/textTranslation"}
    },
    "lexicon": {
      "nt": {"repoPath": "git.door43.org/unfoldingWord/en_ugl", "version": "v0.5", "sha": "8fa6eb60…", "flavor": "peripheral/x-lexicon"},
      "ot": {"repoPath": "git.door43.org/unfoldingWord/en_uhl", "sha": "db0098f3…", "flavor": "peripheral/x-lexicon"}
    }
  },
  "extraScripture": [
    {"id": "ult", "repoPath": "git.door43.org/unfoldingWord/en_ult", "version": "v91", "sha": "35d21595…", "flavor": "scripture/textTranslation"},
    {"id": "ust", "repoPath": "git.door43.org/unfoldingWord/en_ust", "version": "v91", "sha": "85f274a7…", "flavor": "scripture/textTranslation"}
  ]
}
```

Rules:
- **`sha` is the pin's identity** (D58): REQUIRED, 40 lowercase hex, the pinned commit as read from the export's own
  `metadata.json` `identification.primary.<authority>[...].revision`. `version` is an OPTIONAL display
  label. It must be non-empty when present, and it is never compared as identity. `flavor` is REQUIRED. Pin-entry grammar:
  `journal/grammar.mjs` `pinEntryError`. `gatewayLanguage` is `{languageId, owner}` (both required).
- **Fetch path:** the DCS sb-zip export `/sb/<tag>.zip`. The import verifies the export's commit SHA against
  the pin. The export preserves `\zaln` byte-identically (PN #16).
- **Two language sets** (D17/D30): exactly `primary` and `fallback`. Each set is complete with
  `translationNotes` + `translationWordsLinks` + `translationWords` + `translationAcademy`.
  `fallback.gatewayLanguage.languageId` MUST be `"en"`.
- **`translationWordsLinks` MUST name a repo ending in `_twl`** (D97 ruling 6). The one-repo form is illegal. Opening
  a project with a non-`_twl` twl pin ⇒ do not read TWL from it, and tell the user to recreate the project.
  **Ruling 7:** a gateway org with no `_twl` repo (catalog `hasTwl:false`) pins `en_twl` + its own `<lang>_tw`.
  This is the only set that mixes two languages.
- **Read TWL TSVs only through `translationWordsLinks`, and articles only through `translationWords`.** Ignore
  (and never strip) the TSVs inside a `_tw` export and the articles inside a `_twl` export (D96, D32).
- **Optional per-set slots** (D64): `translationQuestions`, `simplifiedText`. An absent slot means it covers no book and is never an error.
  Completeness is unchanged.
- **Resolution** (D30): the unit is **(tool, book)**. Each book's check list comes from ONE resource at ONE version.
  The ladder is exactly `primary` → `fallback`, by book coverage. Any other language is a whole-project gateway change
  (warned re-derive). The pins bind every opener. A personal preference never changes resources.
- **Missing pinned version:** always offer **Download** (ask "Turn on the internet?" if offline). The user MAY
  re-pin to a local version, warned, with carry-over. The app never forces a re-pin.
- **Per-pin coverage `books`** (D41/D61/D62/D82): OPTIONAL list of uppercase book codes the exact commit
  contains, or exactly `["BIBLE"]` (the whole-collection form, e.g. tw articles). `BIBLE` mixed with codes is invalid.
  - The list is a fact about the commit. Never shrink it. You MAY **widen** it from a sha-exact local read.
  - A recorded empty array `[]` is NOT a record and falls through to a local scan.
  - Three states per (tool, book): covered → use the pin (Download if absent, never substitute); not covered →
    unwarned fallback; no record and not local → warned fallback.
  - **Backfill** on open for pins that lack `books` and whose resource is local. It must be idempotent and run under CAS.
  - The pin's own `books` outranks locally-derived coverage. tA pins record nothing (tA has no coverage form).
- **`extraScripture`** (normative): entries `{id, repoPath, sha, flavor, version?}`, with unique `id` slugs (`ult`,
  `ust`). They fill the source panes. Absence is legal.
- **OBS members** (§10.6): a set MAY also pin `obs` (`gloss/textStories`), `obs-tn`
  (`peripheral/x-obsnotes`), `obs-twl` (`parascriptural/x-bcvarticles`), `obs-tq` (OPTIONAL,
  `peripheral/x-obsquestions`), `obs-images` (OPTIONAL override, `peripheral/x-obsimages`). A complete OBS set is
  `obs + obs-tn + obs-twl + translationWords + translationAcademy`. The OBS sample's primary set has none of
  tn/twl. Its fallback has the Bible suite plus `obs, obs-tn, obs-twl, obs-tq, obs-images`. The OBS sample has no
  `resources` or `extraScripture` key.
- **Projection key order** (checkpoint.mjs L118): within a set the order is `gatewayLanguage, translationNotes,
  translationWordsLinks, translationWords, translationAcademy, translationQuestions, simplifiedText,
  obs, obs-tn, obs-twl, obs-tq, obs-images`. `resources` is `originalLanguage` then `lexicon`, each with
  `nt` then `ot`. `extraScripture` keeps the order of each id's first pin event.
- **schemaVersion 1 → 2 migration** (L353): v1 `gatewayLanguage` + `resources.{translationWords,
  translationNotes, translationAcademy}` become `fallback`. `primary` is initialized equal to `fallback`. A twl pin is
  added, and `originalLanguage`/`lexicon`/`extraScripture` carry over. A new project created by the app starts
  with `primary == fallback` (English) (J1 e2e L97-106).
- **Stage rule S-1 (permanent):** `resources.json` is authoritative, and `metadata.json.relationships` is a mirror.
- Watch item: Pankosmia roadmap #160 might add a native per-project resources mechanism [PROPOSED].

### 6.4 Settings: `checking/settings.json` (role `x-check-settings`; SPEC §5.4 L359-396)

A **generated mirror**. The journal owns settings through `settings.set` events (LWW per dotted path). The writer and
every checkpoint regenerate the whole file from the fold.

```jsonc
{
  "schemaVersion": 1,
  "textDirection": "ltr",        // "ltr" | "rtl"; missing/null → ltr
  "textFont": "Charis SIL",      // script-font label or null → app default
  "languageName": "Español"      // display name saved at creation, or null; presentation only
}
```

- Missing values and a missing file are legal. A missing file is regenerated from the fold.
- **Keys are open, with no allowlist.** Preserve unknown keys per path. Reserved spec fields:
  `checkCategories`, `ui.paneSettings`, `ui.toolsSettings`.
- `checkingLanguage` is **obsolete** (D91). Do not write it, preserve it if present, and ignore it.
- The Bible sample still carries the pre-D91 shape:
  ```json
  {"schemaVersion":1,
   "checkCategories":{"translationWords":["kt","names","other"],"translationNotes":["translate"]},
   "ui":{"note":"home for the RCL saveSettings payload …",
         "paneSettings":[{"bibleId":"targetBible","languageId":"es-419"},{"bibleId":"ugnt","languageId":"grc","owner":"unfoldingWord"}],
         "toolsSettings":{}}}
  ```
  The OBS sample has `{"schemaVersion":1,"checkCategories":{…}}`. The app's J1 asserts `textDirection:"ltr"`, a
  truthy `textFont`, and no `checkingLanguage`.
- Projection (`projectSettings`): the doc starts `{schemaVersion:1}`, then `setDeep` for each folded path, in
  register first-seen order. Seeding splits the settings file by **top-level key only**: one `settings.set` per
  top-level key, so `ui` is one register whose value is the whole object (reconcile.mjs L172-177).

### 6.5 Comments, bookmarks, notes: where each lives

| User-facing term (CONTEXT) | Storage |
|---|---|
| **Check comment** (on one check item) | `decision.comments` (string or `false`) in the §5.2 record; journal `check.decision.set` |
| **Bookmark** | `decision.reminders` (boolean) in the §5.2 record |
| **User comment** (free text on a passage, in Understand) | **journal only**: `note.add` segment, `{target, text, generation}`. Notes are grow-only, with no edit or delete in v1. **They project into fold output only. No sidecar file is written for notes** (the checkpoint set in §8.7 has no notes file). |
| Translation note | the pinned tN resource (read-only) |

### 6.6 Selections

They are part of the decision record (`selections`). Validity is revalidated at load:
`selectionsHelpers.validateVerseSelections(currentVerseText, selections)`. A change flags `invalidated`
(ARCH §3.3). Check-level "done" = `selections !== false || nothingToSelect`.

---

## 7. OBS story files (SPEC §10.3–10.5, L610-639)

Frame model:
```
# 1. La Creación

![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg)

Así fue como Dios hizo todo en el principio. Creó el universo …
Después de que Dios creó la tierra, estaba oscura y vacía, …

![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-02.jpg)

Entonces Dios dijo: «Que haya luz». …

![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-03.jpg)



![OBS Image](…obs-en-01-04.jpg)
…
![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-16.jpg)



_Una historia bíblica de: Génesis 1-2_
```

- LF line ends only (a CR is refused). Line 1 is the title, `# N.` or `# N. Title` (regex `^# (\d+)\.( (.*))?$`).
- **The image line splits frames** (`^!\[[^\]]*\]\([^)]*\)$`). A frame is its image line plus exactly ONE paragraph
  (contiguous non-blank lines; single newlines kept). Text before the first image (other than the
  title) is malformed. A second paragraph in a frame is malformed.
- **Reference line**: the last non-blank line, of the form `_…_`, not an image line. It must be non-empty, it is drafted, it has no
  locator, and it is never a check target. A reference line with no frame is malformed.
- **Seed form** (R-10.2.4): the title reduced to `# N.`. Each frame is the image line followed by an empty paragraph, in the
  template's exact bytes: **three blank lines between image lines, five after the last**, and no reference line.
  The image lines keep the source cdn URL. Pictures resolve at render time (project `x-obsimages` ingredient →
  primary `obs-images` pin → fallback `obs-images` pin → bundled default `uW/obs_images_360`). Never rewrite an image line.
- **Byte-strict writes** (R-10.3.4): a write replaces only its region. Frame F's region runs from after its image line to the next
  image line, the reference line, or EOF. The title's region is line 1. The reference region is that line, or the trailing blank
  lines. Clearing a frame returns it to seed form. Writing a frame the story does not have is refused.
- Frame text rules (`journal/story.mjs` `frameTextError`): no `\r`, no blank or whitespace-only line, no leading or trailing
  newline, and no line in image, reference or title form. Title and reference text are single-line and trimmed. A reference text
  must be non-empty.
- **Frame locator** `story:frame` with no leading zeros (`1:1`). Frame 0 is the title. In JSON, `story`/`frame` are integers.
- **OBS check decisions** (R-10.5.1): a §5.2 record with `contextId.reference = {"story":1,"frame":1}` (never
  mixed with bookId/chapter/verse). File `checking/<toolId>/OBS.json` with `"book":"OBS"`. The `resource` record is
  required for `(toolId, OBS)`. There is no alignment layer. `occurrence` is `1` in every OBS help row and is never counted.

---

## 8. The event journal (SPEC §8 + Appendix A) — full detail

Status: §8.1–§8.6, the checkpoint and derived-file half of §8.7, §8.8, and team-sync round 1 (R-8.7.7–12) are
**NORMATIVE**. The long-term sync protocol (per-actor publication branches) is [PROPOSED] reference.
The write side ships in 4.0.0 (D47).

Purpose (L423): "so that a translator's work is never lost, and never lost silently". Every
change is recorded permanently with who, what and when. It must allow offline and team combination without a server. Genuine
conflicts go to a person.

### 8.1 Files and actors (SPEC §8.1, L429-439)

- **Location:** `ingredients/checking/journal/<actorId>/actor.json` and
  `ingredients/checking/journal/<actorId>/segments/<encoded-ts>.action.json`.
- **Segment file = one JSON document** [R-8.1.1]:
  ```json
  {"container":1,"body":"<the action JSON serialized exactly once, as a string>","sha256":"<64 lowercase hex>"}
  ```
  `sha256` is computed over the UTF-8 bytes of the exact `body` string. `body` parses to `{"events":[event, …]}`:
  ONE action (one user mutation), events in strictly ascending `ts`, all carrying the directory's
  actor. The reference writer (`journal/files.mjs` `sealAction`) does
  `body = JSON.stringify({events})` and `seg = JSON.stringify({container:1, body, sha256})`. That is compact JSON,
  key order `container, body, sha256`, and **no trailing newline**.
- **Filename** [R-8.1.2] = the first event's `ts` with `:`→`_` and `|`→`,`, plus `.action.json`.
  Example: `2026-10-13T10_00_00.000Z,0000,translator-a.action.json`. It is reversible, and it sorts in `ts` order
  within the actor directory. The encoder refuses a non-ts input.
- **Multi-scope actions** are allowed [R-8.1.3]. One segment may carry `_project`-scope and book events.
- **Write model** (D50) [R-8.1.4]: every store mutation, even a single event, publishes as ONE NEW
  immutable sealed segment. Never append to or rewrite a journal file. [R-8.1.5] A byte-identical rewrite of an
  accepted segment is an idempotent accept. A *different* action at an accepted path is rejected, and the old bytes are kept.
- **Validity is the commit marker** [R-8.1.6]: a segment failing parse → `container:1` → sha256 →
  body parse → events array → schema is invisible **as a whole**. [R-8.1.7] Readers MUST surface it (never drop it
  silently). [R-8.1.8] Recovery = republish the same bytes from the writer's durable **outbox** (staged intent),
  never an in-place repair. Process-crash atomicity is guaranteed. Power-loss durability is detected but not guaranteed.
- [R-8.1.9] **4 MiB** per segment file. Writers refuse to seal an oversize action. Readers treat an oversize segment as
  invalid. The app splits large seeds recursively in halves (`journalingStore.ts` `chunkForSealing`).
- [R-8.1.10] Sealed segments are the ONLY stream form. `.jsonl` or any other file is rejected at intake.
- [R-8.1.11] `<actorId>` = `[a-z0-9-]{4,32}` (no `|`). [R-8.1.12] A device writes only under its own
  actorId, and every event in a segment carries the directory's actor (the writer checks this too).
- [R-8.1.13] `actor.json`:
  ```json
  {"schemaVersion": 1, "actorId": "translator-a", "createdAt": "2026-10-13T08:00:00.000Z", "device": "translation device"}
  ```
  `actorId` MUST equal the directory name. `createdAt` is REQUIRED, a fixed-width ISO-8601 UTC ms instant.
  `displayName`/`device` are OPTIONAL strings (a pseudonym is fine). The validator is `validateActorDoc`.
- **Actor identity is per project (D53)**. App rule: actorId = `'a' + first 15 hex chars of
  HMAC-SHA-256(installationSecret, repoPath)`. The secret is 32 random bytes, hex, kept in installation-local
  storage (IndexedDB), and never in a project (`src/data/journal/identity.ts` L1-19). Same path ⇒ same id. A copied or
  re-imported project at a different path ⇒ a different id. This is deliberate: merging two of your own projects then reads as two actors and gives a
  visible fork, not a silent linear overwrite.
- [R-8.1.15] Journals are **permanent**. There is no compaction, pruning or rewriting in v1.
- Journal files register as ordinary ingredients on rescan.
- [R-8.1.16] **Disjoint writers are the merge guarantee**: no two devices write the same journal file.
- Containment (reference writer): lstat + realpath + O_NOFOLLOW. A symlinked `segments` dir or segment path
  is refused (JC-23f).
- **Outbox** (app): staged intents in the installation KV under key `outbox:<repoPath>:<actorId>:<ts>`.
  They are replayed at open. A sync keeps each outbox entry (G1).

### 8.2 Time and event identity: HLC (SPEC §8.2, L441-445)

- [R-8.2.1] `ts` = `<ISO-8601 UTC, fixed-width ms, 'Z'>|<4-hex lowercase counter>|<actorId>`, e.g.
  `2026-07-07T14:03:22.113Z|0007|maria-x1`. Regex (`grammar.mjs` L41-43):
  `^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)\|([0-9a-f]{4})\|([a-z0-9-]{4,32})$`. The physical part MUST be a real
  calendar instant. **Plain string comparison is the total order.**
- [R-8.2.2] Issue: if physical now > last physical, reset the counter to `0000`, else increment. [R-8.2.3] Past `ffff`, bump
  physical by 1 ms and reset the counter.
- [R-8.2.4] On receiving any event (import, merge, open), ratchet the local clock to at least the max `ts`
  seen. This includes outbox entries.
- [R-8.2.5] **Event identity is `ts`.** The union de-duplicates by `ts`. There is no content-hash id: identical payloads are
  still distinct events. The same `ts` with different content is corruption, and the fold refuses it.
- The actor segment of `ts` MUST equal `actor` (schema "actor binding").

### 8.3 Event envelope (SPEC §8.3, L447-464)

```jsonc
{"v": 1, "op": "text.verse.set", "actor": "maria-x1",
 "ts": "2026-07-07T14:03:22.113Z|0007|maria-x1",
 "base": "<ts of the event whose state this op observed, or null>",
 "generation": "<ts of the book.add generation>",   // REQUIRED on align.verse.set, check.decision.set, note.add (book targets)
 "supersedes": ["<ts>", "..."],                       // OPTIONAL — fork resolution only
 "batch": "<ts>",                                     // OPTIONAL — first event of the same user action
 "seed": {"source": "creation|sidecar-migration|out-of-band-usfm|tc3-import", "batch": "<ts>"},  // OPTIONAL
 /* op-specific fields */ }
```

- [R-8.3.1] `base` is per target key: the `ts` of the previous **live** event on that key that this op observed.
  The schema requires `base < ts` (strictly earlier). [R-8.3.2] Two live events on one key with the same `base` from
  **different actors** form a **fork**. [R-8.3.3] Same-actor events are totally ordered and never fork.
  [R-8.3.4] `base: null` means "no prior state I know of", not "no prior state exists" (D53). How the fold handles it is in §8.6.
- [R-8.3.5] `supersedes` names the head(s) a resolution replaces, never its own `ts`. [R-8.3.6] An erased head is
  reported (`retained[]`, reason `superseded`). A `supersedes` MUST NOT erase a head outside its own structural
  ancestry. The fold refuses that, keeps the head live, and reports the attempt (`supersedeRefused`).
- [R-8.3.7] `batch` groups events of one user action. It never affects fold state.
- [R-8.3.8] `seed` marks migrated or imported data. Otherwise the event is ordinary: it is not exempt from any rule.
- [R-8.3.9] Version policy: an unknown `v` or op ⇒ the fold refuses with a clear message. Additive optional fields do not bump
  `v`, MUST be tolerated, and never make a fork. Versions in use: `v:1` (Bible vocabulary), `v:2`
  (+ OBS story ops and story targets), `v:3` (+ `actor.name.set`). Each is a superset of the one before.
- Other schema rules (`journal/schema.mjs` `validateEvent`/`validateAction`): an action has ≥1 event, strictly ascending
  `ts`, and a single actor. Every value must survive a JSON round trip (no NaN, Infinity or -0). Every object KEY must be NFC. Max JSON depth is 64.
  At most one `actor.name.set` per action.

### 8.4 Book decomposition: skeleton + verse slots (SPEC §8.4, L466-473)

- [R-8.4.1] **Verse content** = for each verse key, every byte after the `\v <key> ` marker up to the next `\v ` or
  `\c ` marker or EOF. This includes inline markup and the inter-verse paragraph markers and newline that follow. **Skeleton** =
  everything else, with each verse's content replaced by a slot.
- Reference codec (`journal/skeleton.mjs`): the verse marker regex is `\\v[ \t]+(\d+(?:-\d+)?)[ \t]`. The marker, including
  its single trailing space or tab, belongs to the skeleton. A slot is encoded in the skeleton as `U+0001 + "<chapter>:<verseKey>" +
  U+0001`. Front matter before any `\c` is pseudo-chapter `0`. A duplicate verse key is refused.
  Example skeleton start: `\id TIT ejemplo_tj\n\usfm 3.0\n…\c 1\n\p\n\v 1 \x011:1\x01\v 2 \x011:2\x01…`,
  and slot `1:1` content = `"Pablo, … con la piedad,\n"` (with trailing LF).
- [R-8.4.2] `recompose(decompose(usfm)) === usfm` byte-identically for any input (property-tested).
- [R-8.4.3] `U+0001` is reserved. Source files containing it are rejected. Journaled verse content containing `\v `,
  `\c ` (regex `\\[vc][ \t]`) or U+0001 is refused everywhere.
- [R-8.4.4] Slot keys MUST match `<chapter>:<verse>` or `<chapter>:<v>-<v>` (`^\d+:\d+(-\d+)?$`). Duplicates are
  refused. Consequence: adding or removing a paragraph break between verses edits the *preceding* verse's content.
  Renumbering, adding or removing verses or chapters is a structural change.
- §8.4a: sections are presentation only (from the source's `\ts\*`, then `\p`, then per-verse). [R-8.4.5] No section
  object or op exists. [R-8.4.6] A section save = one `text.verse.set` per *changed* verse, sharing a `batch`. The conflict
  unit stays the verse.

### 8.5 Operation vocabulary — complete (SPEC §8.5 table L477-493; payload fields from `schema.mjs` `PAYLOAD_FIELDS`)

| op | v | payload fields | register key (fold.mjs `keyOf`) | fold rule |
|---|---|---|---|---|
| `text.verse.set` | 1 | `book, chapter, verse, text` (chapter/verse are **strings** in practice, e.g. `"1"`) | `text|<BOOK>|<ch>:<v>` | LWW + fork detection; generation = root of its `base` chain; MAY carry `generation` |
| `text.skeleton.set` | 1 | `book, skeleton` | `skel|<BOOK>` chain | structural chain link, slot-preserving only |
| `book.add` | 1 | `book, scope, skeleton, initialVerses` (`initialVerses`: slot key → content) | `book|<BOOK>`, `skel|<BOOK>`, `text|<BOOK>|<slot>` | self-contained creation; `scope` reconstructs `currentScope` |
| `book.remove` | 1 | `book` | `book|<BOOK>` chain | structural link; removed book projects NO content |
| `text.structure.apply` | 1 | `book, skeleton, transitions, dispositions` | `skel|<BOOK>` chain + affected keys | the atomic structural action |
| `align.verse.set` | 1 | the open §5.1 record: `book, chapter, verse, alignments, wordBank, targetVerseMd5` + any extra field (`invalid`, `sourceVersion`, `done`) | `align|<BOOK>|<ch>:<v>` | LWW; valid only while I-3 holds; carries `generation` |
| `check.decision.set` | 1 (2 for story target) | `toolId, decision` (full §5.2 record incl. `status`) + `generation` | `dec|<toolId>|<identityKey>` | LWW + fork detection; carries `generation` (book targets) |
| `note.add` | 1 (2 for story target) | `target, text` + `generation`; `target` = exactly one of `{book, chapter, verse}`, `{decisionKey}` (toolId-prefixed), or (v2) `{story, frame}` | none (grow-only) | grow-only; permanent; projects to fold output only |
| `resource.pin.set` | 1 | `slot, entry` or `{slot, removed:true}` | `pin|<slot>` | LWW + forks; removed → absence |
| `project.vrs.set` | 1 | **`name, bytes`** (spec table says `{vrs}`; the code uses `name` + verbatim `bytes`) | single register | immutable first-value register |
| `project.meta.set` | 1 | `path, value` or `{path, removed:true}` | `meta|<path>` | LWW per path; overlay after metadata regeneration; MUST NOT target `format`, `ingredients`, `type`, `meta` |
| `settings.set` | 1 | `path, value` or `{path, removed:true}` | `set|<path>` | LWW per path + forks |
| `text.frame.set` | 2 | `story, frame, text` (frame 0 = title) | `frame|<story>|<frame>` | LWW + forks; no chain, no generation |
| `text.story.ref.set` | 2 | `story, text` | `ref|<story>` | LWW + forks; no chain, no generation |
| `actor.name.set` | 3 | `username` | per actor | first-value register; later ones → `namesRejected` |

Pin slot grammar (`grammar.mjs` `PIN_SLOT_RE`):
`^(languageSets\.(primary|fallback)\.(gatewayLanguage|translationNotes|translationWordsLinks|translationWords|translationAcademy|translationQuestions|simplifiedText|obs|obs-tn|obs-twl|obs-tq|obs-images)|resources\.(originalLanguage|lexicon)\.(nt|ot)|extraScripture\.[A-Za-z0-9_-]+)$`.
An `extraScripture.<id>` entry's `id` must equal the slot suffix.

Rules with ids:
- [R-8.5.1] The vocabulary is closed per version. `v:2` ⊃ `v:1`, and `v:3` ⊃ `v:2`. Older segment sets fold unchanged.
- [R-8.5.2] **Structural chain-link rule:** `book.add` (non-root), `book.remove`, `text.skeleton.set`,
  `text.structure.apply` each carry `base` = the structural predecessor on that book. An unknown `base` **PENDS** (the event
  and its descendants wait, with verse bytes untouched, until the ancestor arrives; reported in `pendingStructural[]`). A `base`
  naming a non-structural event or another book's structure **REFUSES**. Pending propagates transitively.
- [R-8.5.3] **Rootless-on-existing (D53):** a `base:null` `book.add` meeting an existing book CONVERGES if its
  payload (event minus `actor` and `ts`) is identical. It is reported in `autoMerged[]` as `book|<BOOK>` with the canonical
  root as winner, and later references alias to that root. If the payload differs, it FORKS. A rootless `book.remove`/`text.skeleton.set`/
  `text.structure.apply` on an existing book refuses to act and is retained as `rootless-structural`. It never
  refuses the whole fold.
- [R-8.5.14] `text.skeleton.set` is slot-preserving only. A changed slot set, key or order is refused (only
  `text.structure.apply` changes slots). A `base` that the same actor's own skeleton head already advanced past is refused.
  Accepted edits leave verse projections byte-identical.
- [R-8.5.15] A **rootless `text.verse.set` on a live key is a writer defect.** It is retained as `rootless-base` and never
  projected. `align.verse.set` and `check.decision.set` MAY be rootless on a first write (anchored by
  `generation`, and two concurrent first writes still fork). Pins, metadata and settings rootless writes are ordinary.
- [R-8.5.4] `text.structure.apply` is **all-or-nothing**. A missing source, stale source head, or incomplete disposition
  set ⇒ the event is `incomplete`/`conflicted`, the pre-state projects unchanged, and the event is reported.
- [R-8.5.18] `transitions` maps EVERY destination slot of the new skeleton to `{text, sources}`, where `sources` =
  `[{key: <slot key>, ts: <live text head ts>}, …]`. Merge or split text is stated, never inferred. A key not in the skeleton, a
  slot without a transition, or one source claimed twice ⇒ malformed, and the fold is refused.
- [R-8.5.16] Dispositions are CONSTRAINED to the computed affected set. One outside it refuses the whole event.
- [R-8.5.5] Disposition = `{surface, action, ts, key?, to?, post?}`. `surface` ∈ `text|alignment|decision|note`,
  `action` ∈ `re-key|replace|invalidate-retain|orphan-review`. `ts` = the record's head ts. `key` = slot key (text,
  alignment) or decision key (decision). A `re-key` `to` is a §8.4 slot key of the new skeleton (`1:02` stays `1:02`).
  `replace.post` must be a complete, schema-valid §5.1/§5.2 record whose identity matches the target. A disposition never
  changes `generation`. At most one disposition per record.
- [R-8.5.12] Re-key destinations are slots of the new skeleton. A decisionKey-targeted note never re-keys.
- [R-8.5.17] `replace` is not a note disposition. `re-key`/`replace` are not text dispositions (text heads take
  `invalidate-retain` or `orphan-review` only).
- [R-8.5.6] **Book generations:** every `align.verse.set`, `check.decision.set`, `note.add` (book
  target) carries `generation` = `ts` of its `book.add`. Omitting it refuses the fold, seeded events included. A
  `text.verse.set`'s generation is its base chain's root. A present stamp must resolve (via the converged-root alias)
  to that root, else it is retained as `prior-generation`. After `book.remove` + re-add, prior-generation records are retained and
  reported, never resurrected or deleted. The causal generation decides, never the HLC.
- [R-8.5.7] **Prefix collisions** (D54): `settings.set`/`project.meta.set` paths where one is a prefix of the other
  (`ui.pane` vs `ui.pane.width`) write the same place. The later `ts` wins, and the earlier is retained as `prefix-collision`.
  [R-8.5.8] Empty segments and `__proto__`/`prototype`/`constructor` segments are refused. Max 64 segments.
- [R-8.5.9] **I-4 NFC on write:** content text (verse content, initialVerses, transition text, skeletons, note
  text, settings/metadata string values) is normalized to NFC at the seal. The fold does not re-normalize. [R-8.5.13]
  **Identity is refused, never rewritten:** §5.2 identity parts plus `quote`, `quoteString`, `groupId`, `toolId`,
  decisionKey and verse targets, `book`, pin `slot`, dotted `path`, the vrs `name` (and `bytes`), and every object KEY
  at every depth MUST already be NFC, or the event is refused at seal AND fold. The reason is that normalizing an identity can silently merge two records.
- [R-8.5.10] `text.verse.set.text` = the full content slot bytes. Validity hashing uses only the §5.1 plain-text
  extraction (I-3), which refuses content it cannot fully cover. There is no other validity hash (`textMd5`/`skeletonMd5` were removed, D48).
- [R-8.5.11] **Removal semantics:** decisions are never deleted (invalidate-and-retain). Alignment removal = the
  explicit empty-state payload (a record, not absence). `{path, removed:true}` folds to absence, while `value:null` stores
  `null`. Pins `{slot, removed:true}` project to absence.
- [R-8.5.19] `project.vrs.set` MUST carry `seed.source` ∈ `creation|sidecar-migration|tc3-import`.
  `out-of-band-usfm` never seeds a frame. The first value binds. An identical repeat de-duplicates. A different one is surfaced
  (`vrsRejected`) and never applied.
- **Renumbered verses** (issue #209, option A):
  - [R-8.5.20] A renumber = one `text.structure.apply`. Each moved slot's transition names the old key's live text
    head as source, with its text unchanged. All moves are in one action.
  - [R-8.5.21] `re-key` keeps every payload field and `generation`. **The fold projects a re-keyed alignment with
    `invalid:true`.**
  - [R-8.5.22] **A decision never re-keys.** A decision `re-key` refuses the whole event at seal and at fold. The
    conservative form is `invalidate-retain`.
  - [R-8.5.23] Collisions refuse before the action: (a) a destination slot of the base skeleton that holds a live
    text head and is not moved or named as its own source; (b) an alignment re-key onto an occupied key, or two re-keys
    onto one key. The event is reported `conflicted` with detail `collision:…`.
  - [R-8.5.24] **Span membership:** a record keyed `c:v` is "on" its own key and on every slot `c:a-b` with
    a ≤ v ≤ b (`grammar.mjs` `recordOnSlot`). The fold and reconcile share this one predicate.
- **Name action (D98):** [R-8.5.25] `actor.name.set {username}` is `v:3` only. The username matches
  `^[A-Za-z0-9][-.\w]*$`, does not match `[-._]{2,}|[-._]$`, and is ≤40 characters. Its key is the event's own actor. `base` must be `null`.
  It has no `generation` and no `supersedes`. The fold returns `names` (actor → username). [R-8.5.26] It is set once. The first by `ts` binds, and later
  ones go to `namesRejected`. The writer refuses a second one (`actor.name-already-set`). One action carries ≤1.

### 8.6 The fold (SPEC §8.6, L517-530; `journal/fold.mjs`)

Algorithm (L519): input = the union of all actors' accepted segments → validate every event (the same schema as
the writer) → de-duplicate by `ts` (the same ts with different content ⇒ refuse) → sort by `ts` (string compare) → for each
event, update its key's state:
- grow-only ops (`note.add`) accumulate.
- LWW ops **advance their branch** when `base` equals a live head's `ts`.
- They **resolve the key** when they `supersede` all live heads.
- They **advance the same actor's head linearly** when no live head matches `base` but a live head carries the
  event's own actor.
- otherwise they **join as a fork**.
- Exceptions: a rootless content write on a live key ⇒ `rootless-base` retained. A rootless structural op on an
  existing book ⇒ refuses to act.

Then project: §8.4 recomposition, §5 structures, the `project.meta.set` overlay. Then report.

Invariants:
- [R-8.6.1] **DETERMINISM**: `fold(events) ≡ fold(any permutation / duplication / partition-then-union)`. A partial
  union never projects a `ts` it lacks. Caches MUST equal the full fold.
- [R-8.6.2] **CONSERVATION**: every written record ends observable as projected, or in `retained[]`, `invalid[]`,
  `pendingStructural[]`, `forks[]` or `autoMerged[]`, or superseded by a traceable successor. Anything else is a defect.
- [R-8.6.3] **EXCLUSIVITY**: a record is never both projected and retained.
- [R-8.6.4] **Forks**: a key with >1 live head is forked. If all heads have byte-identical payloads, they auto-merge (one
  projected record, reported in `autoMerged[]` with `{key, heads:[ts…], winner}`). Otherwise `forks[]` gets an entry and the projection
  uses the **highest-`ts` head, flagged provisional**. Fork identity is the op payload (built by ADDITION from
  `PAYLOAD_FIELDS`). For `align.verse.set` it is every non-envelope field. An additive envelope field never forks.
  Payload comparison uses `canon()` (canonical JSON, see §8.7.3).
- [R-8.6.5] **Absent books project nothing**: text, alignments, decisions and notes of a removed or not-yet-added
  book go to `retained[]` (`absent-book`).
- [R-8.6.6] **Orphan backstops**: an alignment whose key has no slot ⇒ `invalid[]` (`orphaned:true`). A live text head
  on a slotless key ⇒ `retained[]` (`orphaned-text`). A note dispositioned `orphan-review` ⇒ retained.
- [R-8.6.7] A slot with no live verse event projects as the stub `___`. (In the code, a `book.add` slot with no
  `initialVerses` entry defaults to `'___\n'`.) Stubs and projections carry no `generation`.
- [R-8.6.8] **Structural-branch selection**: per book, select the winning skeleton head. Its ancestry chain is the
  selected branch. Heads whose structural ancestor is outside it ⇒ `unselected-structural-branch`.
- [R-8.6.9] **Closed `retained[]` reason vocabulary**: `superseded`, `prior-generation`, `absent-book`,
  `orphaned-text`, `prefix-collision`, `rootless-base`, `rootless-structural`, `no-structural-ancestor`,
  `unselected-structural-branch`, plus the disposition retention actions `invalidate-retain` | `orphan-review`.
- **Resolution travels**: a resolution is an event (`supersedes`). Two independent conflicting resolutions
  surface again as an ordinary fork.
- `invalid[]` entries are `{book, verse, ts, orphaned?}` for alignments that fail I-3 (or are orphaned). They stay
  projected in the sidecar. Readers treat them as invalid.

Fold output shape (fold.mjs L1049-1062):
```
{ books: {<BOOK>: {usfm, verses, …}}, stories: {<n>: {frames: {<f>: text}, ref}},
  decisions: {<toolId>: [record…]}, alignments: {<BOOK>: {"<ch>:<v>": record}},
  pins: {<slot>: entry}, projectMeta, projectMetaRemoved, settings: {<path>: value}, notes: [event…],
  forks, invalid, retained: [{key, ts, reason}], autoMerged, scope: {<BOOK>: ranges},
  vrs: {name, bytes} | null, vrsRejected, names, namesRejected,
  pendingStructural, headsTs, supersedeRefused, liveHeads, liveNotes }
```

### 8.7 Checkpoints and derived files (SPEC §8.7, L532-556)

A **checkpoint** happens at session close, book done, mode switch, before export or import, and pre-sync (D9; W-4). Sequence: fold → write **all**
derived files → regenerate `metadata.json` ingredients → reconstruct `type.flavorType.currentScope` → apply
the `project.meta.set` overlay → commit. The commit happens only after the full sequence (an app rule).

- [R-8.7.1] **Complete regeneration set**: `<BOOK>.usfm` (recomposed), the §5.1 and §5.2 sidecar mirrors,
  `resources.json` (folded pins), `settings.json` (folded settings), `vrs.json` (verbatim bytes from
  `project.vrs.set`), `metadata.json`, and for OBS the story files. Every shared non-journal file is either derived or
  a tolerated unjournaled class (`ingredients/audio/`).
- [R-8.7.2] `metadata.json` reconstructs `currentScope` from folded `book.add`/`book.remove` state, then
  applies the overlay. An OBS project keeps the template table verbatim (R-10.2.3).
- [R-8.7.3] `ingredients/audio/` is never touched.
- [R-8.7.4] **Mandatory inputs or refuse**: base `metadata.json`, the (tool, book) resolution record for every
  decision file, a versification frame once any book exists, a non-partial `languageSets` (both sets or
  neither), and the base story bytes for OBS. A write that would fold to a partial pin state MUST be refused
  BEFORE its events publish (issue #485).
- [R-8.7.5] **Divergence detection** enumerates from the fold's expected set. An out-of-band edit or deletion of a
  derived file is detected and never silently repaired. (`classifyDivergence` → `{tolerated, diverged, clean}`.)
- [R-8.7.6] Every projected ipath must resolve strictly inside `ingredients/` and satisfy §2. The checkpoint checks this
  itself, independent of the schema.
- **Removable derived classes** (`isRemovableDerivedClass`): `<BOOK>.usfm`, `checking/alignments/<BOOK>.json`,
  `checking/(translationWords|translationNotes)/<BOOK>.json`. A stale file whose projection vanished is removed.
  `resources.json`/`settings.json` instead project an EMPTY document. `vrs.json` is retained forever.

#### 8.7.1 Team sync, round 1 (NORMATIVE, D98) — Bible projects only

A sync combines this computer's journals with one Door43 repository's `main` ("Door43 main"). It reads
Door43 main, adds own journal files, rebuilds derived files, and pushes.

- [R-8.7.7] **G1 own work never lost.** "Own files" = each segment under this actor's dir, its `actor.json`,
  each outbox entry. The result has each own file byte-for-byte, or the project is unchanged. If Door43 main has a different
  file at an own path ⇒ refuse `sync.own-file-differs`.
- [R-8.7.8] **G2 incoming checked.** Each file under `ingredients/checking/journal/` on Door43 main must pass the
  one validator (parse + checksum + schema, name = first ts, single actor = directory, valid `actor.json`, no
  other files). Any failure ⇒ refuse `sync.incoming-invalid`, naming each failing path.
- [R-8.7.9] **G3 derived files rebuilt.** Inputs are exactly: (a) the union of journal segments (Door43 main + local
  + outbox); (b) per (tool, book), the `resource` record **recomputed from the folded pins** by the D30 rule, never
  read from an incoming file; (c) the base `metadata.json` (read, never written). Derived bytes on Door43 are
  never inputs. A different file there is replaced. A derived file the fold no longer projects is removed.
- [R-8.7.10] **G3 `metadata.json` belongs to the server.** A sync MUST NOT write it. A fold that carries a
  `project.meta.set` overlay ⇒ refuse `sync.metadata-overlay`. The checkpoint refuses the same overlay
  (`checkpoint.metadata-unwritable`).
- [R-8.7.11] **G6 determinism.** The same segment set + the same resolution records ⇒ byte-identical derived files, in any
  read order (`metadata.json` is excluded).
- [R-8.7.12] **G7** Only a `text.verse.set` fork is shown to a person. Every other fork keeps its newest
  (highest-ts) head, provisional.
- App rules: **G4** never force-push. A non-fast-forward push restarts the sync from the new Door43 main. **G5** the same path and the same
  actorId after a sync. **G8** a crash during the swap never leaves two projects or loses own work. An OBS sync refuses
  (`sync.obs-unsupported`). The sync Report counts each R-8.7.12 fork once.
- D98 G-table (DECISIONS.md L2675-2682) maps G1–G8 to rules and issues (#674–#681).
- Long-term [PROPOSED]: per-actor publication branches, send-without-receive, zero-trust scratch intake,
  rebuild-and-swap (D16/D67; `docs/plans/TEAM-SYNC-PLAN.md`; JC-18–20; `validate:transport` T1–T4).

#### 8.7.2 Projection byte forms (from `journal/checkpoint.mjs`)

| File | Builder | Shape |
|---|---|---|
| `<BOOK>.usfm` | `recompose(skeleton, verses)` | exact bytes |
| `checking/alignments/<BOOK>.json` | `projectAlignments` | `{schemaVersion:1, book, chapters:{ch:{v:record}}}`; the record carries every payload field except envelope/`op`/`book`/`chapter`/`verse`/`generation` |
| `checking/<tool>/<BOOK>.json` | `projectDecisions` | `{schemaVersion:1, tool, book, resource, decisions}`; story decisions → `book:"OBS"` file |
| `checking/resources.json` | `projectResources` | `{schemaVersion:2, languageSets?, resources?, extraScripture?}` in fixed slot order |
| `checking/settings.json` | `projectSettings` | `{schemaVersion:1, …paths}` |
| `vrs.json` | verbatim `project.vrs.set.bytes` | |
| `metadata.json` | `projectMetadata` | base doc + `currentScope` (Bible) + overlay |
| `content/NN.md` | `applyStoryState(base, state)` | line splices on the base story |

All JSON = `JSON.stringify(doc, null, 2) + "\n"`.

#### 8.7.3 JS key-order semantics a Rust port must reproduce (byte determinism)

- `JSON.stringify` emits object keys in **JS own-property order**: integer-like keys (canonical decimal
  strings `0 … 2^32-2`) first, ascending **numerically**, then string keys in insertion order. This decides
  chapter and verse order in `chapters` (e.g. `"2"` before `"10"`; a span key `"9-10"` is a *string* key and comes
  after all integer keys of that chapter, in insertion order).
- `projectAlignments` inserts vkeys in a **lexical** `.sort()` of `"<ch>:<v>"` strings, then relies on the above
  to reorder integers.
- `canon()` (fold.mjs L55-82) for fork identity and decision sort: integer-like keys ascending numerically,
  then the other keys **sorted lexically**, recursively. Primitives are encoded exactly like `JSON.stringify`.
- Settings key order = first-seen order of `set|` registers during the ts-ordered fold.
- `JSON.stringify` string escaping rules (e.g. U+2028 is not escaped, while control characters are `\u00XX`) also apply to the
  `body` string inside segments. The sha256 is over whatever bytes were written, so a reader only needs to hash the body string as it is.
  A writer that wants byte-identical seeds with the JS implementation must match the escaping.

### 8.8 Reconcile and seeding (SPEC §8.8, L558-563; `journal/reconcile.mjs`)

- [R-8.8.1] **Out-of-band USFM edit** (detected at open when committed bytes ≠ projection): decompose the committed file. If the slot
  set is unchanged, emit `text.verse.set`/`text.skeleton.set` per differing verse or skeleton with
  `seed:{source:"out-of-band-usfm"}` and `base` = the current **live** head (a linear supersede). If the slot set changed,
  emit one `text.structure.apply` with **conservative, COMPLETE dispositions** (identity mapping where possible;
  for every live text head, alignment, decision and verse-targeted note on a removed key, use `invalidate-retain`/
  `orphan-review`, never guessed re-keys). Build them from the fold's **live head sets** (`liveHeads`, `liveNotes`), never from
  the projection. A concurrent journaled edit of the same verse becomes an ordinary fork.
- [R-8.8.2] **Universal, all-or-nothing seeding** (D50): on first open of an unjournaled project, every sidecar
  record becomes one seeded event, and the fold of the seed MUST reproduce the pre-seed state. USFM and vrs are byte-exact. Sidecars are
  record-exact (record order inside arrays is byte form, rewritten to the projection right after seeding). A sidecar that the
  projection cannot represent (an unknown top-level field, or two records on one identity key) MUST refuse the seed.
  - Reference seeder (`seedFromSidecars`): the clock starts at physical `2020-01-01T00:00:00.000Z` and the actor is the
    seeding install. The `seed.batch` ts is issued first (`…|0000|…`). Then come `project.vrs.set` (with `name`), one `book.add` per
    book (`scope` from the file or `[]`, `skeleton`, `initialVerses` = decomposed verses), `resource.pin.set` per slot
    (languageSets, resources, extraScripture), `settings.set` per **top-level** key (skipping
    `schemaVersion`), `project.meta.set` per entry, then `check.decision.set` per decision with
    `ts` physical = `max(clock, modifiedTimestamp)` and `generation` = that book's seed `book.add` ts, then
    `align.verse.set` per record (spreading the complete record). All events have `base:null` and `seed:{source, batch}`.
  - The app stages the seed events in chunks under the 4 MiB cap, then publishes. A failure gives `seed.publish-failed`.
- [R-8.8.3] Seeding is deterministic up to the actor. Two devices that seed the same source independently converge (by the
  rootless convergence rule).
- [R-8.8.4] **Partial pin fold recovery:** if the folded pins have `languageSets.primary.*` but no `fallback.*`, then at open,
  BEFORE divergence classification, journal ordinary `resource.pin.set` events (no `seed`) for the installed
  English fallback suite, and report it (`completePartialPins`). Only this shape is auto-recovered.
- App rule (D53): **joining is not merging**. Never infer or propose merging two projects. In v1 no op combines two
  projects' journals.

### 8.9 Refusal codes (`journal/report.mjs` `REFUSAL_CODES`, closed set)

Rule-bound: `segment.invalid` R-8.1.6, `segment.misnamed` R-8.1.2, `segment.foreign-actor` R-8.1.12,
`segment.differs-from-accepted` R-8.1.5, `outbox.invalid` R-8.1.7, `actor.record-mismatch` R-8.1.13,
`journal.clock-not-ratcheted` R-8.2.4, `open.unexplained-divergence` R-8.7.5, `open.story-divergence`
R-10.7.5, `seed.mismatch` / `seed.publish-failed` R-8.8.2, `story.base-missing` R-10.7.4,
`checkpoint.divergence` R-8.7.5, `checkpoint.scope-mismatch` R-8.7.2, `checkpoint.incomplete-inputs`
R-8.7.4, `checkpoint.unsafe-path` R-8.7.6, `sync.own-file-differs` R-8.7.7, `sync.incoming-invalid`
R-8.7.8, `sync.metadata-overlay` R-8.7.10, `actor.name-already-set` R-8.5.26.
App rules (no R-id): `segment.bad-path`, `outbox.different-action`, `ledger.unreadable`,
`open.scope-unreadable`, `checkpoint.too-deep`, `checkpoint.metadata-unwritable`, `export.*` (read-failed,
checkpoint-failed, unsupported-kind, nothing-drafted, print-failed, print-failed-pictures),
`import.damaged.*` (truncated, no-metadata, checksum-mismatch, usfm-parse, no-manifest, corrupt-entry),
`import.name-exists`, `import.write-failed`, `share.*` (offline, auth-failed, name-exists, create-rejected,
non-fast-forward, push-failed, server-unavailable, sign-in-rejected), `sync.obs-unsupported`.
Every op returns one `Report` `{op, ok, code?, rule?, facts, startedAt, endedAt}`, with `op` ∈ {open,
checkpoint, seed, reconcile, export, import, share} (ARCH §3.5). Tests assert codes, never messages.

### 8.10 Real journal example (`conformance/fixtures/team-sync/journals/`)

Files:
```
translator-a/actor.json
translator-a/segments/2020-01-01T00_00_00.000Z,0001,translator-a.action.json   (the seed: 8 events)
translator-a/segments/2026-10-13T09_00_00.000Z,0000,translator-a.action.json   (actor.name.set, v:3)
translator-a/segments/2026-10-13T10_00_00.000Z,0000,translator-a.action.json   (TIT 1:1 edit)
translator-a/segments/2026-10-13T10_10_00.000Z,0000,translator-a.action.json
translator-b/…  translator-c/…
```

Segment bytes (verbatim, A's TIT 1:1 edit):
```json
{"container":1,"body":"{\"events\":[{\"v\":1,\"op\":\"text.verse.set\",\"actor\":\"translator-a\",\"ts\":\"2026-10-13T10:00:00.000Z|0000|translator-a\",\"base\":\"2020-01-01T00:00:00.000Z|0002|translator-a\",\"book\":\"TIT\",\"chapter\":\"1\",\"verse\":\"1\",\"text\":\"Pablo, siervo de Dios y apóstol de Jesucristo, para la fe de los escogidos de Dios y el conocimiento de la verdad que lleva a la piedad,\\n\"}]}","sha256":"2d7952b0eec100eb75fcd34340816efd9dc90c71910dcd17c5b06c57ebfbb4d4"}
```
`base` points at the seed's `book.add` (`…|0002|…`), which created the `text|TIT|1:1` head.

Decoded seed events (abbreviated):
```jsonc
{"v":1,"op":"project.vrs.set","actor":"translator-a","ts":"2020-01-01T00:00:00.000Z|0001|translator-a","base":null,
 "seed":{"source":"sidecar-migration","batch":"2020-01-01T00:00:00.000Z|0000|translator-a"},"name":"eng","bytes":"{\n  \"maxVerses\": …(27681 chars)"}
{"v":1,"op":"book.add","actor":"translator-a","ts":"2020-01-01T00:00:00.000Z|0002|translator-a","base":null,"seed":{…},
 "book":"TIT","scope":[],"skeleton":"\\id TIT ejemplo_tj\n…\\c 1\n\\p\n\\v 1 \u00011:1\u0001\\v 2 \u00011:2\u0001…",
 "initialVerses":{"1:1":"Pablo, siervo de Dios … con la piedad,\n","1:2":"…", … 46 keys}}
{"v":1,"op":"check.decision.set","actor":"translator-a","ts":"2026-07-02T14:24:39.000Z|0000|translator-a","base":null,"seed":{…},
 "toolId":"translationWords","generation":"2020-01-01T00:00:00.000Z|0002|translator-a","decision":{ /* full §5.2 record */ }}
{"v":1,"op":"align.verse.set","actor":"translator-a","ts":"2026-07-02T15:40:56.000Z|0002|translator-a","base":null,"seed":{…},
 "book":"TIT","chapter":"1","verse":"1","generation":"2020-01-01T00:00:00.000Z|0002|translator-a",
 "alignments":[…],"wordBank":[…],"invalid":false,"targetVerseMd5":"f8aee622daff1496bccc7c2e3b358b43",
 "sourceVersion":"dcs::unfoldingWord/el-x-koine_ugnt@v0.34"}
```

Fixture cases (README): `creation`, `names`, `different-verses` (no fork; the 1:1 alignment becomes invalid by I-3),
`verse-two-ways` (fork on `text|TIT|1:4`; B newest, provisional), `check-two-ways` (non-text fork on the decision
key; C's `todo` projects), `invalid-file` (checksum ⇒ G2 refusal), `changed-own-file` (G1 refusal).

### 8.11 OBS journal vocabulary (§10.7, L647-653)

- [R-10.7.1] `v:2` ⊃ `v:1`. Story ops and targets REQUIRE `v:2`.
- [R-10.7.2] `text.frame.set {story, frame, text}` (frame 0 = title, one trimmed line) and `text.story.ref.set
  {story, text}` (the text without the underscores). Each is an LWW register with forks. There is no chain and no generation. A rootless first
  write is ordinary. Same-actor writes advance linearly. Two actors' rootless writes fork. Twins auto-merge. Never
  `rootless-base` or `prior-generation`.
- [R-10.7.3] `check.decision.set` with a `{story, frame}` reference and `note.add` with `{story, frame}` or a story
  decision key: MUST NOT carry `generation`, MUST be `v:2` (or 3). They resolve like pins.
- [R-10.7.4] Projection `stories[n] = {frames:{f:text}, ref}`. At checkpoint each story file = folded state spliced
  onto the BASE story bytes (the committed file or the seed form), in ascending frame order, then the reference line. A base story is mandatory
  (`story.base-missing`). An OBS project needs no vrs at checkpoint.
- [R-10.7.5] No structural ops for stories. A write to a frame the base lacks is refused. An out-of-band story edit is
  divergence: the app MUST stop and not overwrite (no reconcile defined).
- Platform quirks: an OBS project's checkpoint does **not** rescan (a rescan empties `currentScope`, PN #37). So the OBS
  `ingredients` table stays as the platform stamped it at creation, and `checking/` sidecars stay unlisted. The SB export
  rebuilds the table (ARCH §7 burrito-zip row).

---

## 9. Validation rules enforced by the conformance harness

Run from `conformance/`: `npm install && npm run generate && npm run validate` (52 checks: Stage-1 40,
Stage-2 2, Phase-2 3, OBS 7 — per PN "Verification recipes" and the README; the SPEC §7 text still says 50/38). Journal:
`npm run validate:journal` (368). Gate: `npm run validate:normative` (105 rules). All: `validate:all`.
Env overrides: `BURRITO=<dir>`, `OBS_BURRITO=<dir>`. Libraries: usfm-js 3.4.3, word-aligner 1.0.3,
word-aligner-lib 1.0.1, ajv + ajv-formats over `sb-schema/` (verbatim from pankosmia `resource-core`, with
`$id`s re-keyed and trailing commas removed; PN #11), bible-reference-range.

### 9.1 `validate.mjs` — check by check (line numbers in that file)

**Stage-1 / Stage-2 on `sample-burrito/`:**
1. SB schema: `metadata.json` validates against the bundled schema (root `source_metadata.schema.json`),
   including relationships and x- roles (L75).
2. ingredients: metadata lists **exactly** the on-disk files under `ingredients/` (L88).
3. ingredients: every md5 + size is correct (`checkIngredients` from `src/data/import/burritoCheck.mjs`) (L94).
4. [Stage-2] exactly 5 client-owned role-tagged ingredients; `vrs.json` has no role (L100).
5. versification: `vrs.json` is present, has the 4 keys, `maxVerses` covers every scope book, and an ingredient entry exists (L115).
6. versification: every `mappedVerses` value is a string or a non-empty string array (L128).
7. versification: every mappedVerses key/target parses (key and target regexes above) (L148).
8. scope: the grammar accepts `[]`, `["1:1-2:5"]`, `["3"]`, `["1:2-16","3:1-15"]`; it rejects `"1:1"` (non-array),
   `["banana"]`, `[1]`, `["1:1–2:5"]` (en dash). All stored scopes are valid (L165).
9–11. targetBible: usfm-js gives TIT ch 1-3 with 16/15/15 verses; the 1:1 text is accessible; the `\h` header exists (L176-180).
12. alignment: sidecar → `wordaligner.merge` → `convertVerseDataToUSFM` gives 7 `\zaln-s` (L191).
13. alignment: OL attributes survive (`x-strong`, `x-lemma`, `x-content`) (L193).
14–15. alignment: round-trip re-extraction (`unmerge`) equals the stored alignments and wordBank (21 words) (L205-207).
16. alignment: staleness guard, `targetVerseMd5 === md5(trim(concat verseObjects.text))` (L209).
17. alignment: `done` is additive (the same zaln output, and boolean where present) (L219).
18. selections: stored 1:1 selections are valid (`validateVerseSelections`) (L232).
19. selections: an edited verse is detected (L236).
20. decisions: all tC3 fields are present on every decision (L254).
21. decisions: the tN `quote` is a word/occurrence array (L260).
22. decisions: `verseEdits`+`invalidated` are representable (L263).
23. derive+merge: saved decisions re-attach to TSV-derived items by stable key; progress 2/3 (L291).
24. derive honors scope, using `bible-reference-range` `doesReferenceContain`, including span items (L313).
25. derive gate: the tw article gate (#643) (L346).
26. derive+merge: cross-language re-attach plus the groupId tiebreak; irreducible ambiguity stays unplaced (L388).
27. carry-over: D36 invalidate-and-keep, plus the `userInvalid` round-trip semantics with negative controls (L456).
28. multi-book: TIT + JON in `currentScope`, and both USFM ingredients are scoped (L468).
29. resources: two sets, each tn+twl+tw+tA, fallback `en`, originals and lexicons, sha REQUIRED, version optional (L494).
30. resources: the twl pin ends in `_twl` on both rungs; the one-repo form refuses; the ruling-7 form accepts; the OBS fallback is also checked (L521).
31. resources: `books` is optional; codes or `["BIBLE"]`; a mix refuses (L551).
32. resources: the coverage verdict predicate (covered+absent→fetch; uncovered→unwarned fallback; no
    record→warned; `[]`→warned; `["BIBLE"]` covers all) (L571).
33. resolution: decision files' `resource` matches a rung by repoPath+sha; equal pins→primary;
    `languageSet` must agree; the ladder (L608).
34. resources: extraScripture shape, unique ids, sha required, empty version refused (L631).
35. resources: optional tq + simplifiedText slots (L644).
36. [Stage-2] `metadata.relationships` **equals** `relationshipsFromPins(resources.json)` exactly (L654).
37. export: whole-book aligned USFM via `weaveBook` (L664).
38. spans: `\v 9-10` → key `"9-10"` only (L676).
39. spans: identity-key string normalization; `Number()` banned [covers R-8.4.4] (L681).
40. decisions: `status` ∈ {valid,invalid,todo}; the fixture carries one (L693).
41. alignment: a renumber re-keys 1:1→1:2 with every field kept and `invalid:true` (seed→`shiftChapter`→fold→project) (L728 of the second half).
42. decisions: a renumber never moves a decision; a decision re-key refuses the fold (L740).

**Phase-2 (temp git repo):**
43. A naive two-actor git merge conflicts on `metadata.json`.
44. Resolve derived files either side + regenerate post-union ⇒ a clean two-parent commit, both journals listed
    with role `x-journal`, and the ingredients table matches disk.
45. The fixture journals are §8.1 sealed segments plus a valid actor.json [covers R-8.1.10].

**OBS on `sample-burrito-obs/`** (each check also fires on a broken copy):
46. O1 schema `gloss/textStories`; stories `text/markdown`; a misspelt flavor or extra flavor field fails [R-10.1.1].
47. O2 layout equals the template plus sidecars; front/back/LICENSE byte-identical; each story is in seed form or a §10 draft of
    it; two-digit paths [R-10.2.1, R-10.2.2, R-10.2.4].
48. O3 frame model (16 frames in story 1, ref line, refusals) [R-10.3.1–3].
49. O4 byte-strict write; frame 17 of 16 refuses; a multi-line title refuses [R-10.3.4, R-10.7.5].
50. O5 `currentScope` equals the template (keys + ranges; order not significant) [R-10.2.3].
51. O7 `obs-tq` member pin, slot grammar, and order [R-10.6.3].
52. O6 the v2 fold of four segments projects `content/01.md` byte for byte; a v1 seed still folds; v1 refuses story ops;
    a missing base story refuses [R-10.7.1, R-10.7.4, R-10.2.3].

### 9.2 `validate-journal.mjs` — journal suite, JC-1…JC-35 (368 checks; fast-check seeded, 200 runs/property)

JC-1 HLC ordering/ratchet/overflow · JC-2 skeleton codec byte identity · JC-3 fold determinism
(property) · JC-4 LWW linear history · JC-5 fork detection + identical auto-merge · JC-6 supersedes · JC-7 I-3
composition · JC-8 out-of-band reconcile (+8b partial pin completion) · JC-9 three-device convergence ·
JC-10 sneakernet ≡ merge · JC-11 RETIRED · JC-12 end-to-end git regeneration · JC-13 dedup by ts · JC-14 schema
totality + version policy (14b envelope, 14c payloads, 14d identity-key grammar, 14e no self-supersede, 14f
I-4 NFC, 14g hostile input) · JC-15 Phase-1 sidecar seed (golden vs sample; full-state partial-scope fixture)
· JC-16 section-vs-verse · JC-17 definitions closure (settings, reserved meta roots, orphans, NFC) · JC-18
Pankosmia transport scratch · JC-19 actor publication lifecycle · JC-20 zero-trust intake · JC-21 structural
action (21b reconcile dispositions, 21c invalidated records, 21d constrained dispositions, 21e disposition
schema, 21f replace.post validated) · JC-22 structural lineage (22b stale base, 22c transitive pending, 22d one
predecessor set) · JC-23 sealed segments (23b immutability, 23c Windows/§2-legal names, 23d action shape,
23e no traversal, 23f filesystem containment) · JC-24 outbox republication · JC-25 vrs first-value register ·
JC-26 pin golden projection · JC-27 removal semantics · JC-28 actor binding + same-actor rule · JC-29
generations (29c causal, 29d note quarantine, 29e generation required, 29f optional on text.verse.set) · JC-30
unjournaled tolerance + divergence (30b mandatory inputs) · JC-31 value grammars (two layers) · JC-32 base
semantics, conservation, exclusivity (32b silent data loss, 32c–32h) · JC-33 OBS v2 · JC-34 renumber · JC-35 team
sync round 1 (`[G1]`…`[G7]`, the name action).

### 9.3 `normative/check.mjs` — the coverage gate (D55)

It extracts every `[R-x.y.z]` id from SPEC §8 and §10 (it fails on duplicates or a misplaced id). It strips comments from all
`conformance/*.mjs` and collects `[covers R-…]` tags only from the first string argument of `check(`/`prop(`
calls. It fails on an uncovered rule or a stale claim, and on a refusal code bound to a dead rule. It proves that a claim exists and is
live. It does **not** prove that the check fires (mutation hardening is open).

### 9.4 Rig-gated suites (need a live pankosmia-web rig)

- `validate-roundtrip.mjs` (12 checks, R0–R7): custom files write through the API; the read path is byte-faithful;
  `update_ingredients` registers custom files (and drops roles); a full rescan keeps scope, identification and languages; content bytes
  survive (only `metadata.json` is rewritten); delete→revert via `.bak` restores the file; zip export; `remake_burrito_from_zip` rejects an
  unwrapped zip; a wrapped zip imports; export→import is byte-identical; Stage-1 holds on the server-touched copy (Stage-2 1/2).
- `validate-transport.mjs` (10 checks, T1–T4): the rig version (0.18.15); multi-branch `pull-repo` is ordering-steered;
  the JC-19 lifecycle with single-branch publication repos (with `metadata.json` conflicts from PN #48); JC-20 intake
  over HTTP.
- `bench-fold.mjs`: fold cost on an aligned NT or a checked whole Bible (issue #80).

### 9.5 Import validation (`src/data/import/burritoCheck.mjs`, CONTEXT "Burrito check")

`metadata.json` is present, parses and matches the schema. Each listed ingredient is present with its md5 and size. The flavor is
`textTranslation` or `gloss/textStories`. Failures are damaged findings (`import.damaged.no-metadata`,
`checksum-mismatch`, `truncated`). A "tC4 burrito" (has `ingredients/checking/`) keeps its alignments, decisions and
journal on import. A foreign burrito keeps only its text.

---

## 10. User journeys (docs/JOURNEYS.md)

Rules (JOURNEYS L4-17): a journey is a goal of one actor, **defined by its end state on disk**. UI state
never counts. `Jn` ids are stable and never reused. Journal checks are `JC-n`. Steps are unordered.
Activities: Start, Understand, Translate, Check, Exchange, Deliver. Actors: translator, facilitator,
consultant (Phase 2).

| ID | Actor | Activity | Goal | Status | Proof |
|---|---|---|---|---|---|
| J1 | facilitator | Start | Create a Bible project, add books | shipped alpha.1 | `e2e/j01-create-project.spec.ts` |
| J2 | translator | Translate | Draft verses beside sources | shipped (rev. D70 alpha.5) | `e2e/j02-draft-verse.spec.ts` |
| J3 | facilitator | Start | Get pinned resources | shipped alpha.2 | `j03`, `guided-fix`, `internet-consent` |
| J4 | translator | Check | Check a book with tN, tW | shipped (rev. D72 alpha.6) | `j04` |
| J5 | translator | Check | Align a verse | shipped (rev. D72 alpha.6) | `j05` |
| J6 | translator | Translate | Edit a checked verse, see flags | shipped alpha.2 | `j06` |
| J7 | facilitator | Deliver | Export the book | shipped rc.1 | `j07-publish.spec.ts` |
| J8 | translator | Translate | Resume across sessions/books | built | `j08` |
| J9a | facilitator | Exchange | Import a tC3 project | shipped rc.1 | `j09` |
| J9b | — | — | x-tcore import | retired (no data) | — |
| J9c | facilitator | Exchange | Import raw USFM | shipped rc.1 | `j09` |
| J9d | facilitator | Exchange | Import a Scripture Burrito | shipped rc.1 | `j09` |
| J10 | — | — | retired: RTL is an axis on J2/J4/J5/J7 | retired | `j10-rtl` stays |
| J11 | facilitator | Exchange | Share to Door43 (first push of `main`) | shipped rc.1 | `j11-share` |
| J12 | facilitator | Start | Upgrade pinned resources | shipped alpha.6 | `j12` |
| J13 | facilitator | Start | Change the gateway-language set | shipped alpha.2 | `j13` |
| J14 | — | — | retired: isolation is a MUST NOT on J1/J2 | retired | `j14` stays |
| J15 | — | — | retired: slow open is a quality req. | retired | `j15` stays |
| J16 | translator | Understand | Read with helps, record a user comment | shipped alpha.5 | `j16` |
| J17 | translator | Exchange | Receive a project, continue offline | Phase 2 | none |
| J18 | translator | Translate | Keep one version of a conflicted verse | Phase 2 (review screen built #679) | component tests |
| J19 | consultant | Check | Record findings | Phase 2 | none |
| J20 | facilitator | Start | Create an OBS project | shipped alpha.7 | `j20` |
| J21 | translator | Translate | Translate a story frame by frame | shipped alpha.7 | `j21` |
| J22 | translator | Check | Check an OBS story | shipped alpha.7 | `j22` |
| J23 | facilitator | Deliver | Export OBS as Markdown + PDF | shipped rc.1 | `j07` OBS block |
| J24 | facilitator | Exchange | Share an OBS project | shipped rc.1 (rides J11) | `j11` OBS case |
| J25 | translator | Understand | Read a story with helps, user comment | shipped alpha.7 | `j25` |

### J1 Create a Bible project (L62-74)
- Pre: fresh install. Steps: first run · choose UI language · choose target language + script direction · name the
  project · add one or more books.
- **End state:** one new git repo that is a valid SB after the create-project and add-book checkpoints (**≥3 commits**,
  D9); one USFM ingredient per added book; its own actor id.
- MUST NOT touch any other project repo (from J14: a second project is a second repo with a different actor id; the
  first stays byte-identical).
- e2e assertions (`j01-create-project.spec.ts` L73-158): exactly one new repo dir; `metadata.json` + `.git`;
  `languages[0] = {tag:"es-419", name:{en:"Spanish (419)"}}`; `ingredients/TIT.usfm` exists; `vrs.json` has
  `maxVerses.TIT`; `resources.json` `schemaVersion:2`, sets `[fallback, primary]`, `fallback.gatewayLanguage`
  English, **`primary` equals `fallback`**, every pin `repoPath` `^git\.door43\.org/`, `version` `^v[\d.]+$`, `flavor`
  truthy, `sha` 40-hex when present; OL nt `el-x-koine_ugnt`, ot `hbo_uhb`, lexicon nt `en_ugl`; no v1 keys;
  `extraScripture` ult and ust `v91` with sha; `settings.textDirection === "ltr"`, `textFont` truthy, no `checkingLanguage`;
  the seeded TIT has 3 `\c`, `\v 1 ___` stubs, `\p` structure from the source, no `\ts`, no `\zaln`; commit count ≥3. The
  teardown verifies that every journaled project is a byte materialization of its journal.
- Error state: an invalid language code shows a refusal (at the pinned server, `es-419` is accepted).

### J2 Draft verses beside the sources (L76-94)
- Steps: open a book · read the sources · read the OL pane · type a section straight through · place verse numbers ·
  stack two verse numbers to make a span, or drag one past text to break one · leave the section · see progress. Verse-by-verse
  is still available.
- **End state (D70):** the section's verse texts in the book's USFM with every verse number placed; text outside the
  edited verses is byte-identical; **one `text.verse.set` segment per changed verse**; when a span was created or broken, **one
  `text.structure.apply`** with the new skeleton and conservative dispositions, and the alignments and decisions on the affected verses
  **marked invalid and kept**. The save indicator is a step, not the end state.
- MUST NOT: write to another project; commit on every save (commits happen at checkpoints only, D9); re-key an
  alignment or decision (Increment 5 uses `invalidate-retain` only).

### J3 Get pinned resources (L96-108)
- Steps: choose the gateway language · pick the resource set · download.
- **End state:** §5.3 pins with `sha`, `repoPath`, and a `version` that is not `master`; the pinned resources are installed and match the
  pins; the helps open offline; a missing resource shows its missing state. Download is always offered. While offline it
  asks "Turn on the internet?", and Not now sends nothing (D95). Pending: a proof that text ingredients are unchanged.

### J4 Check a book (L110-124)
- Steps: open the derived tW/tN list · read a note or article · triage an item · write a check comment · set a bookmark.
- **End state (D72):** `check.decision.set` segments in the journal; the §5.2 sidecar at checkpoint; text
  unchanged. The check comment and bookmark are fields of the same record (`comments`, `reminders`) and survive
  reopen. Clearing a comment writes `false`. Neither changes progress.
- MUST NOT: store a check comment anywhere but the decision; count bookmarked or commented items
  differently.

### J5 Align a verse (L126-143)
- Steps: link/unlink word pairs · turn suggestions on · confirm or reject each, or all.
- **End state (D72):** `align.verse.set` segments; the §5.1 sidecar at checkpoint; text unchanged. An accepted
  suggestion = the same segment as a manual link. A suggestion that was shown and not accepted leaves **no trace**.
- MUST NOT: write an unconfirmed suggestion; count a verse resolved or a suggested word placed while a suggestion stands;
  store the suggestions switch in the project; make the translator wait for training (D93).

### J6 Edit a checked verse (L145-151)
- Steps: edit a verse that has decisions · see the affected checks flagged.
- **End state:** new verse text; affected decisions **invalidated and retained** (§5.2, D36); nothing deleted.

### J7 Export the book (L153-166)
- Steps: preview the typeset text · choose an export · save it outside the project.
- **End state:** files written **outside** the project directory: a dated PDF; dated aligned USFM; plain USFM; a
  Scripture Burrito zip. The project is byte-identical except the D9 checkpoint commit. No publish record is kept in the project.
- File names (ARCH §7): `<BOOK>-aligned-<YYYY-MM-DD>.usfm`, `<BOOK>-<YYYY-MM-DD>.usfm` (plain = stored
  bytes including any BOM), `<project>-<YYYY-MM-DD>.zip` (repo zip minus `.git/`, `*.bak`, `.DS_Store`;
  `metadata.json` gains the `relationships` mirror + the `dcs` authority), `<BOOK>-<YYYY-MM-DD>.pdf`.

### J8 Resume work (L168-176)
- Steps: leave Translate or the project · come back later · open another book.
- **End state:** leaving with uncommitted changes makes **exactly one** checkpoint commit with message prefix
  `Checkpoint, leaving Translate:` or `Checkpoint, leaving the project:`. Leaving without changes makes none (the checkpoint reads `GET
  /git/status` first, because a clean-tree add-and-commit still commits; PN #9). The last draft is what reopens.

### J9a/c/d Import (L178-194)
- Sources: tC3 project zip(s); raw USFM (`.usfm/.sfm/.txt`); a Scripture Burrito zip (flat or wrapped).
- **End state:** one **new** project repo, a valid SB at its first commit; imported segments carry `seed.source`
  (`tc3-import` for tC3, `sidecar-migration` for the others); imported text byte-identical to the source; the project
  name = the Bible name from the review page (toast, Home card, `metadata.json`). Import never writes into an existing project in 4.0.0. "Look up
  on Door43" is an explicit internet step. A local import sends no external request.
- MUST NOT: create anything from damaged input; leave a partial repo (rollback deletes it).
- An `archive` burrito import is stored as-is (no open, no checkpoint). Its first open seeds what its journal lacks.

### J11 Share to Door43 (L196-249)
- Steps: press Share on the card · sign in (unless a token exists; "Stay signed in" → OS keychain) · see the author notice ·
  choose own account or an org with create permission · check the repo name (default = folder name) and the books or stories ·
  the app creates the repo, adds `origin`, pushes `main` · shows the URL.
- **End state:** the repo exists on the server; remote `main` == local `main`; the project is byte-identical except the D9
  checkpoint commit; `origin` is in `.git/config`; nothing about remotes is in the installation; the token is in memory (and in the keychain only if
  kept); no name, email or login is stored; the commit author = the computer's account name (PN #47). Later: "Upload changes" pushes
  `main` again.
- Refusals: name exists; non-fast-forward (another device pushed); offline; auth failed; create rejected.
- MUST NOT: create a publication branch or outbox; integrate or receive; force-push; write the token anywhere but the keychain;
  put a token in a URL or log; store the password; write into the project.

### J12 Upgrade pinned resources (L251-272)
- Steps: Project Settings · Manage source texts · check for updates · see offers per set and scripture text ·
  accept explicitly · re-derive.
- **End state (D72):** the set's pins carry the new `sha`+`version`; every affected book's decisions are carried over
  or invalidated and retained (D36); the other set's pins are unchanged; text ingredients byte-identical; a failed or
  interrupted download changes nothing. An OL text upgrade marks every alignment record of the covered books `invalid:true`
  and keeps them (the confirmation states the verse count). A gateway-Bible upgrade moves only its pins.
- MUST NOT: upgrade silently; move a pin before every resource of the release is installed and sha-verified; let
  a help-set upgrade touch OL or gateway-Bible pins; let a scripture-text upgrade touch decisions; let a
  gateway-Bible upgrade touch alignments.

### J13 Change the gateway-language set (L274-286)
- Steps: Project Settings · choose another package in "Checking language" · confirm.
- **End state:** primary pins name the new set (`version`, `sha`, `repoPath`); the English fallback is unchanged;
  decisions re-attach or are invalidated and retained; on a Bible project `extraScripture` names the new package's
  literal and simplified Bibles (or the English ULT/UST where the package cannot fill them).

### J16 Read with helps, record a user comment (L288-297)
- Steps: open the passage · open a ULT or UST tab · open a translation note · open an Academy article · write a user
  comment.
- **End state:** **one `note.add` segment bound to the passage**; text ingredients byte-identical. Reading without
  commenting is not a journey.

### J17 / J19 (Phase 2) — end states are defined when the sync ops ratify (issue #22).

### J18 Keep one version of a conflicted verse (L305-330)
- Pre: two translators changed the same verse from the same base, and a sync put both in the journal.
- Steps: the Home card shows "N conflicts · Review" (book tiles show counts) · open the review (also from the verse marker or
  the toolbar) · one verse at a time, "Conflict n of m", versions newest first, with owner and date, plus ULT · click a version
  · "Keep this version" · Previous/Next · "Back to …" at any time.
- **End state:** for each kept verse, **one `text.verse.set` segment of this actor whose `supersedes` names every
  version** (R-8.3.5). The fold shows no conflict. The others are in `retained[]` with reason `superseded`. The verse shows the kept
  text. The next sync sends it.
- MUST NOT: list check-result or alignment conflicts (G7); color by position ("Mine" is light blue, teammates
  cream); start a sync; write when the translator leaves the review.

### J20 Create an OBS project (L332-342)
- Steps: choose Open Bible Stories · choose language, name, gateway set.
- **End state:** a repo that is the `text_stories` template byte for byte, except `metadata.json` and the 50 title lines
  (`# N.`); every frame = image line + empty paragraph; no reference line; `currentScope` = the template table;
  front/back copied from the gateway source. The harness validates it as OBS.
- MUST NOT: offer a story subset; copy source text into a frame; rewrite an image line.

### J21 Translate a story frame by frame (L344-355)
- Steps: open a story · read the gateway frame + picture · write the frame · write the title and the reference line.
- **End state:** the story file differs only inside the written paragraph, the `# N.` line, or the closing `_…_` line;
  one `text.frame.set` or `text.story.ref.set` segment per save; one paragraph per frame, with single newlines kept.
- MUST NOT: touch another frame's bytes; write a second paragraph into a frame; save an image line.

### J22 Check an OBS story (L357-367)
- Steps: open a frame's note or word link · see the source phrase · select target words · comment or bookmark · preview the
  drafted stories.
- **End state:** §5.2 records under the `story:frame` key for tN and tW; a later frame edit flags them invalid and
  retains them; the story file byte-identical.
- MUST NOT: count `Occurrence`; offer an Align tool; write a PDF.

### J23 Export OBS (L369-376) — files outside the project: story Markdown zip (`<project>-stories-<date>.zip`,
`content/…` entries byte-identical), a dated PDF, a Scripture Burrito. The project is byte-identical except the D9 checkpoint.

### J24 Share OBS (L378-381) — J11 unchanged, with an OBS project.

### J25 Read a story with helps, user comment (L383-393)
- **End state:** one `note.add` segment with a `{story, frame}` target per comment; the story file byte-identical;
  Resume returns to Understand on the same story. MUST NOT: change progress or write into the story file.

---

## 11. Server interaction rules (SPEC §6) and what changes without Pankosmia

SPEC §6 W-rules (verified against pankosmia-web 0.16→0.18.15):
- **W-1** writes go through `POST /burrito/ingredient/raw/…?ipath=<path under ingredients/>` with body
  `{"payload":"<string>"}`. The handler creates missing directories.
- **W-2** registration is opt-in (`update_ingredients`, or `POST /burrito/metadata/remake-ingredients/…`). A rescan rebuilds
  the whole table from disk and **drops `x-` roles**. No HTTP route writes `metadata.json`.
- **W-3** `no_bak` (its semantics are inverted from the doc comment) skips the `.bak` backup. Keep `.bak` for USFM, and it is optional for sidecars.
- **W-4** nothing auto-commits. Call `add-and-commit` at checkpoints. It sweeps **all** pending changes. A branch switch
  refuses on a dirty tree.
- **W-5** whole-file writes only. The read-modify-write race is accepted in Phase 1 (the journal removes it).

### 11.1 PLATFORM-NOTES that matter if the Rust app drops Pankosmia

These are behaviors the format currently *accommodates* and that a standalone Rust storage layer either inherits as
format facts or can drop:

| PN | Fact | Implication for a Rust storage layer |
|---|---|---|
| #1 | The platform drafting editor strips `\zaln`/`\w` on save | the reason for I-1. Keep I-1: other SB tools may still edit the USFM |
| #2 | usfm-js parses occurrences as strings; word-aligner needs ints | I-2. Normalize at the boundary |
| #4 | usfm-js chunk parse returns `verses`, whole-book returns `chapters` | relevant for reproducing the I-3 extraction |
| #5, W-2 | a rescan wipes roles and the server regenerates the ingredients table | without Pankosmia, Rust writes `metadata.json` itself and can keep roles (Stage-2 2/2) |
| #6, §2 | ipath sanitizer: no dot segments, forbidden chars | keep it, because projects must still open in Pankosmia and DCS tools |
| #8 | `.bak` single-level undo; delete = rename to `.bak`; `.gitignore` `**/*.bak` | keep `.gitignore`; `.bak` behavior is optional |
| #9 | `add-and-commit` sweeps all; a clean-tree commit is empty (0.18.5) or rewrites `metadata.json` (≥0.18.10) | the checkpoint must check `git status` first; Rust can commit only the changed paths |
| #17, #21, #33 | `pull-repo` has no branch param; after a normal merge the worktree is stale | the long-term topology used single-branch publication repos; Rust with libgit2/gix can merge directly |
| #20 | `add-and-commit` panics on a repo with zero commits | every created repo needs an initial commit |
| #27/#28/#43 | create routes: BCP-47 check; at ≤0.18.5 a failure left a git-init'd debris repo | Rust creation must validate the language tag first and clean up on failure |
| #29 | zip import needs explicit directory entries (`ingredients/`) | if Rust produces sideload zips for Pankosmia, write directory entries |
| #30/#31 | DCS export metadata names the org at export time; repo paths are case-insensitive addresses | store `repoPath` exactly as DCS reports it, and compare case-insensitively (`samePath`) |
| #35 | DCS catalog flavor labels differ from sb-zip export labels for OBS helps | record the export's flavor in pins |
| #36/#37 | the OBS template lacks `localizedNames`; a rescan empties OBS `currentScope` | Rust must keep the template scope verbatim and write `localizedNames:{}` |
| #38 | removing a directory during a rescan panics and poisons the server | moot without Pankosmia |
| #40/#46/#49 | push: credentials in the body, `NotFastForward` = 500 + libgit2 code, a local remote must be bare, clone writes to a fixed path | map errors to `share.*`/`sync.*` codes by libgit2 code |
| #41 | the server zip export includes `.git`, `*.bak`, `.DS_Store`; on Windows the entries use `\` | the SB export must filter these; normalize `\`→`/` when reading zips |
| #44 | the platform audit fails tC4 projects (`checking/`, `journal/` are "unexpected") | use the tC4 burrito check, not the platform audit |
| #45 | no shell reports download results | app-level (export UX) |
| #47 | the commit author is fixed at creation to the OS username `<user>@localhost` | Rust can choose (D7 wants user-controlled; D85 says store no name or email) |
| #48 | each `add-and-commit` rewrites `metadata.json` (dateCreated, revision UUID, timestamp, sorted keys, one compact line, roles dropped) | **this is why `metadata.json` conflicts on every merge, and why R-8.7.10 says "metadata.json belongs to the server"**. Without Pankosmia, Rust owns `metadata.json` and can generate it deterministically from the fold (R-8.7.2) |
| #16 | DCS `/sb/<tag>.zip` preserves `\zaln`; verify the SHA on every import | resource fetch path |
| #26 | DCS sb-zips are wrapped (one top dir); the platform import wants unwrapped | resource installer must unwrap |
| #32 | Electronite (Graphite fonts) is required for complex scripts | a UI concern (Rust UI must shape Graphite fonts) |
| #39 | the server persists `app_resources_dir` | moot |

Endpoints the app uses today (ARCH §3.1) map onto what a Rust storage layer must provide natively: read and write
ingredient (raw), list paths, delete/revert (.bak), remake ingredients, metadata summaries, zip export and import,
create project (writes `vrs.json` from the chosen scheme and scaffolds books from `maxVerses`), git ops (status,
add-and-commit, push, remotes, clone, pull, copy, delete), versification list/scheme, and the net gate.

---

## 12. Glossary (CONTEXT.md, key terms)

- **Translator** — mother-tongue person who translates and checks. Their needs decide tradeoffs. **Facilitator** —
  sets up and moves projects (create, import, upgrade, publish, send). The role is not stored. **Consultant** — reviews,
  records findings, Phase 2.
- **Journey** (`Jn`) — a goal defined by its **end state** on disk. **End state** — the only definition of done.
  **Activity** — Start, Understand, Translate, Check, Exchange, Deliver. **Increment** — a GitHub milestone.
- **Bible project** / **OBS project**. **Story**, **Frame** (image line + one paragraph), **Story title**
  (frame 0), **Reference line** (`_…_`, never a check target), **Frame locator** (`story:frame`, `1:0` =
  title), **Image pack** (shared frame images), **Source phrase** (an OBS help's quote in the gateway OBS language).
- **Section** — a run of verses drafted as a unit, from the source's `\ts\*`. Presentation only.
- **Verse span** — one key covering consecutive verses, `\v 9-10` → `"9-10"`. (Avoid "bridge".)
- **Structural change** — changes the set of verse keys (span create/break, renumber, add/remove verses or chapters).
  One all-or-nothing journal action. Editing text inside a verse is never structural.
- **User comment** — free text on a passage (Understand) → `note.add`. **Translation note** — the tN resource entry.
  **Check comment** — free text on one check item → `decision.comments`. **Bookmark** → `decision.reminders`.
- **Alignment suggestion** — proposed from the project's own confirmed alignments. It is never saved until confirmed.
- **Journal check** — `JC-n`.
- **Export** — files leaving via a download (PDF, USFM, SB zip, story Markdown). The project is byte-identical except the D9
  checkpoint. **Export kernel/producer**, **Weave** (sidecar → `\zaln`/`\w` on export only), **Print document**,
  **Relationships mirror**, **Page setup** (in memory, never stored).
- **Share** — first push of `main` to Door43 (send only). **Share-born main**, **Door43 adapter**, **Door43
  session** (the token only may go to the keychain), **Share target**, **Shared card** (its state is read from the `origin` remote).
- **Import bundle / parser / shell**, **Version request** (an import never stores a pin without a sha), **Burrito check**,
  **tC4 burrito** vs **foreign burrito** (whether it has `ingredients/checking/`).
- **Report** `{op, ok, code?, rule?, facts, startedAt, endedAt}`. **Ops record/log** (in the per-client settings
  document, never in a burrito). **Refusal code** (closed table).
- Team sync: **Sync** (one press of "Sync with Door43"), **Door43 main** (only moves forward), **Guarantee** G1–G8,
  **Journal**, **Actor** (one translator on one computer in one project; id from the installation secret + project path),
  **Fold**, **Conflict** (= spec "fork"; only verse text goes to a person), **Conflict review**, **Keep a version**
  (writes one verse edit whose `supersedes` names all versions), **Outbox** (saved here, not yet in the repo),
  **Rig**, **Scratch copy**.
- Avoided words that matter for naming in code: not "merge" for fold, not "bridge" for span, not "note" alone, not
  "publish" for export, and not "sync" for the first share.

---

## 13. Surprises, inconsistencies and gotchas for the Rust team

1. **Doc drift inside the normative spec.**
   - §2 tree still says `vrs.json  role x-versification`. §4.3 withdrew it: **no role**.
   - §8.5 table says `project.vrs.set` payload `{vrs}`. The schema and seeder use **`{name, bytes}`**
     (`schema.mjs` L338-347, L392). `name` is how the scheme name is recovered (§4.3 ladder).
   - §7 says "50 checks … Stage-1 38 … OBS (6 checks)". The header (1.22), README and PLATFORM-NOTES say **52
     (40/2/3/7)**. The suite's own summary line is authoritative.
   - The spec says that seeding uses "counter 0000". The reference seeder increments the counter within a millisecond (e.g. the
     alignment event `…15:40:56.000Z|0002`), and issues `seed.batch` at `…|0000` as a ts that no event carries.
2. **The Bible reference sample has no journal.** It is an unjournaled Phase-1 project that is seeded on first open. Real journal
   bytes are only in `fixtures/team-sync/`. The sample's `settings.json` is the pre-D91 shape (`checkCategories`, `ui`).
   The app writes `textDirection/textFont/languageName`. Both are legal, because keys are open.
3. **User comments (`note.add`) are journal-only.** There is no notes sidecar. A tool that reads only the sidecars never sees
   them.
4. **`metadata.json` ownership is split, and this is the hard part for a Pankosmia-free build.** Today the server
   rewrites it on every commit (new revision UUID, `dateCreated` = commit time, sorted keys, one compact line, roles
   dropped) (PN #48). So the app refuses any `project.meta.set` overlay (`checkpoint.metadata-unwritable`), and sync
   never writes `metadata.json` (R-8.7.10). The checkpoint verifies that the server's rescanned `currentScope` equals the fold's scope
   (`checkpoint.scope-mismatch`). That means passage-set scopes depend on the server's file-based scope derivation, which only
   knows whole files. A Rust storage layer that owns `metadata.json` can implement R-8.7.2 and the overlay properly, but it
   must decide the byte form (pretty or compact) and whether to keep `x-` roles.
5. **Byte determinism depends on JavaScript object-key ordering** (integer-like keys first, numerically) and on
   `JSON.stringify` escaping. R-8.7.11 (G6) requires byte-identical derived files across computers. A Rust port must
   reproduce `canonKeys`/`canon` and JS key order exactly, or every mixed fleet (JS app + Rust app) diverges.
6. **I-3 hashing is defined by usfm-js 3.4.3 behavior** ("plain text" = concatenated verseObject `text`, trimmed).
   This is the single riskiest compatibility point. Port it and test it against the sample hash `f8aee622…` and the harness
   fixtures.
7. **Type asymmetry:** `text.verse.set`/`align.verse.set` events carry `chapter`/`verse` as **strings** (`"1"`), while
   §5.2 `reference.chapter`/`verse` are JSON **numbers** for single verses and **strings** for spans. Identity
   comparisons always stringify.
8. **Verse content includes the trailing newline** and any following `\p` markers up to the next `\v`/`\c`. A paragraph
   break between verses is an edit to the preceding verse.
9. **Decision `summary` blocks vanish** after the first open (the projection does not emit them).
10. **OBS projects skip the server rescan.** Their `ingredients` table goes stale (the template checksums, `checking/` unlisted)
    until an SB export rebuilds it. `currentScope` is a non-normalized table with overlapping ranges, and the server reorders its keys.
11. **`relationships` and roles are Stage-2, non-durable by design.** Readers must never depend on them. Authority is
    `resources.json` + paths.
12. **Pin identity is (repoPath + sha), and repoPath compares case-insensitively** (DCS casing is stored as-is). Tags are
    labels only.
13. **Actor ids are per project path.** Moving or copying a project dir changes the actor id (deliberately). A sync must keep the
    same path (G5). Without the installation secret, a reinstall becomes a new actor.
14. **The 4 MiB segment cap** forces chunked seeding of large projects. Each chunk is a separate action with its own
    filename (first ts). Seeding is still "all-or-nothing" at the app level via the outbox and a ledger.
15. **Conflict UX policy is normative:** only verse-text forks go to a person (G7). Every other fork silently keeps the
    newest head (flagged provisional, counted in the Report).
16. **`text.structure.apply` is heavy:** it must carry transitions for every destination slot and dispositions for every
    affected live record (computed from live heads, not the projection), including prior-generation and losing-fork heads.
    The reconcile builder and the fold must share one affected-set predicate.
17. **Same-frame pass-through is mandatory:** do not "normalize" eng references through org. That loses verses.

---

## 14. Suggested Rust storage-layer module map (derived from the above)

- `paths` — ipath grammar (§2 + extra refusals), book codes, story paths, the segment filename codec.
- `burrito::metadata` — SB metadata model; the ingredient table builder (md5, size, mime, scope, role); scope grammar;
  the `relationshipsFromPins` mirror; schema validation (embed `sb-schema/`).
- `usfm` — skeleton/slot codec (§8.4, byte-identical), the I-3 plain-text extraction (usfm-js-compatible),
  span-aware verse keys.
- `sidecars` — typed §5.1/§5.2/§5.3/§5.4 documents with additive-field preservation (unknown keys kept), the
  identity-key serializer, I-2 normalization, and a serializer that reproduces JS `JSON.stringify(doc,null,2)+"\n"`
  key order.
- `journal::seg` — seal and validate (`container`/`body`/`sha256`), immutability, outbox republish, actor.json,
  HLC clock + ratchet, actor id = `'a' + hex15(HMAC-SHA256(secret, repoPath))`.
- `journal::schema` — envelope and per-op validators (v1/v2/v3), NFC identity refusal and content normalization.
- `journal::fold` — registers keyed `text|`, `align|`, `dec|`, `pin|`, `meta|`, `set|`, `frame|`, `ref|`, plus
  `book|`/`skel|` chains; forks, auto-merge, supersedes, generations, structural actions, retained reasons.
- `checkpoint` — projection set, mandatory inputs, divergence classification, removal of stale derived files.
- `reconcile` — out-of-band USFM, universal seeding, partial pin completion.
- `sync` (round 1) — G1–G8.
- `obs::story` — parse, seed form, byte-strict splices.
- Conformance: run the existing JS harness against Rust-produced directories (`BURRITO=`, `OBS_BURRITO=`), and port or bridge
  `validate-journal.mjs` cases (or feed Rust-written segments into the JS fold and compare projections byte for byte).
