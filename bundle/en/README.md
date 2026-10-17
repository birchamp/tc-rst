# The bundled English suite

English is always installed as the fallback language set, so the app ships these
unfoldingWord releases and works offline from the first run. `tc-resources` holds the same
pins in `suite::bundled_pins()`, ported from tC4's `src/data/installedSuite.js`. The
benchmarks and the scale corpus (`docs/PERFORMANCE.md`) also read these files.

## Shipped: the DCS Scripture Burrito export of each pinned tag

As in tC4, a pin with a version tag ships as the DCS burrito export of that tag. An export is
installed only when its `metadata.json` names the pinned commit as its DCS revision and every
ingredient matches its md5 and size. `SHA256SUMS` lists each file. The project owner
downloaded the exports on 2026-10-15 (the timestamp in each `metadata.json`), and every one
passed those checks against the pins below.

| Pin (tC4 `installedSuite.js`) | Repository | Version | Commit | File | Ingredients | sha256 |
|---|---|---|---|---|---:|---|
| `originalLanguage.ot` | `unfoldingWord/hbo_uhb` | v3.0.0 | `74022f0fed012a3ef169886f595dd98e7b200543` | `hbo_uhb_v3.0.0.sb.zip` | 40 | `7cc625318984213db671eab867ca115d1281e25c0a17b711ae155865d559af45` |
| `originalLanguage.nt` | `unfoldingWord/el-x-koine_ugnt` | v0.34 | `fc95b2b8aad08bb65ab54628ab685413a1139e97` | `el-x-koine_ugnt_v0.34.sb.zip` | 28 | `748b83459d8ffbb59fbc4cd6d956f37e0f4beab633031e37e21389c55859a549` |
| `extraScripture` `ult` | `unfoldingWord/en_ult` | v91 | `35d215957f3203fd2e2fac5702ce14902d417f9d` | `en_ult_v91.sb.zip` | 61 | `6a1989e89cd3f152f5cf6a1ad5866abda74ea90d0e8478cd3a5f742979e52e9f` |
| `extraScripture` `ust`, `simplifiedText` | `unfoldingWord/en_ust` | v91 | `85f274a74245cb418f85266e1a5b524bc3e91e9c` | `en_ust_v91.sb.zip` | 60 | `71b9c711428a0cebc15fb1dae35dfdb02715c9a38d71d489efe99df6d91445c8` |
| `translationNotes` | `unfoldingWord/en_tn` | v91 | `e586762e330f482a60c52aedd1c7b3a2f155df8a` | `en_tn_v91.sb.zip` | 60 | `53943fdfa3d87f59b2810a61693f5df7703b89bc3a79879bd2fbb8c39075d92c` |
| `translationWordsLinks` | `unfoldingWord/en_twl` | v91 | `7cbb3419f79780ce84b92ad7d42bd476cc5d8a64` | `en_twl_v91.sb.zip` | 1014 | `2fbd8e56ab64d412383af8ba3ff347f044e28305753a72d0a292133e4499ff5c` |
| `translationWords` | `unfoldingWord/en_tw` | v91 | `ff5b3852c27c3a0d01b109e482eb26047dcd20e2` | `en_tw_v91.sb.zip` | 1021 | `7e5fa483c5c9783b7f85d7107c8bec8f6ae9bd2cb7736287d5bfce409e0d6577` |
| `translationAcademy` | `unfoldingWord/en_ta` | v91 | `ce9a1bb9431317ca888e8c1f9620caa7f5fe45fd` | `en_ta_v91.sb.zip` | 735 | `d22c6cbcb6466ebd048f3c04c1f8fbde191baa212b665ca71ccedec0802cd032` |
| `translationQuestions` | `unfoldingWord/en_tq` | v91 | `8be02772584ff5a5fea893a392b4f019e3efcc77` | `en_tq_v91.sb.zip` | 60 | `010096509ce29cf7658b0d68f1e94ac6e8168b2341b7d4509eac390241ad60eb` |
| `obs` | `unfoldingWord/en_obs` | v9 | `d39a1dc7a7557ac54e4a8fecc3462147fe7eec3b` | `en_obs_v9.sb.zip` | 53 | `e447e239c1bfc0f7a915bbf69120f0ffcb290dd4ec44ac42d927e1ef328540dc` |
| `obs-tn` | `unfoldingWord/en_obs-tn` | v13 | `e86138ea13f619f09f7a6dcaa60592716d407fe4` | `en_obs-tn_v13.sb.zip` | 2 | `ccdb79cf2c4b9fd1c95460d182f82f3116ca29d5ae02b192b33dcc74dfd79f4c` |
| `obs-twl` | `unfoldingWord/en_obs-twl` | v3 | `44ebc9fafe8101665f985007d566f5036a2be85b` | `en_obs-twl_v3.sb.zip` | 2 | `fba8f254438f41ce775500343b3260f043749743c984460089abb84dc1fb375c` |
| `obs-tq` | `unfoldingWord/en_obs-tq` | v10 | `01b92fe8793d62cff3a2221f5174c768cbad3dc1` | `en_obs-tq_v10.sb.zip` | 2 | `c5a96d0b4c877954d55042f7b4b21ccad45edb174d32bfccc5f9584e4e99b6e5` |

Each export also holds a few files its metadata does not list (`README.md`, `LICENSE.md`,
`.gitignore`, `.github/…`, `.gitea/…`). Nothing verified them, so the installer leaves them
out (`tc_door43::VerifiedSbExport::unlisted`).

The tW and TWL exports each carry a copy of the other resource, and both copies are ignored
(owner ruling, as tC4 D96 ruling 5). Word-link rows come only from the `translationWordsLinks`
pin (`en_twl`), and articles come only from the `translationWords` pin (`en_tw`):

- The tW export carries 66 TWL book files that DCS builds from `en_twl` master, not from the
  pinned release (which has 59 books). They are never read, and they do not make the tW pin
  cover any book: its articles serve every book.
- The TWL export carries 954 tW articles under `ingredients/payload/`. They are never read,
  and they do not make the TWL pin cover the whole Bible: its coverage is its book files.

Both copies are listed and checksummed in the export's `metadata.json`, so they stay installed
unchanged, and nothing reads them. In the TWL export DCS also rewrites each `TWLink` from
`rc://*/tw/dict/bible/<category>/<slug>` to `./payload/<category>/<slug>.md`; `tc-helps` reads
both forms as the same article id, which is then looked up in the `en_tw` articles. The other
TSV and USFM files are the same bytes in the export and in the repository.

## Read by the scale corpus

The scale corpus generator (`tc-corpus`, `docs/PERFORMANCE.md`) reads its real books from these
exports: the ULT, tN and TWL v91 book files and the UHB and UGNT books, each
`<repo>/ingredients/<BOOK>.<usfm|tsv>`. It reads a TWL link in either form, so the exported
`./payload/<category>/<slug>.md` gives the same article as the repository's `rc://` link, and
it skips the ULT's front matter (`FRT.usfm`) and everything under `ingredients/payload/`. The
Door43 git archives of the same commits, which it read before, are gone from this folder.

## The git archive of each commit-only pin

The two lexicons have no tag, so their source is the Door43 git archive of their commit, as in
tC4 (D71); the installer ships each one repackaged as one JSON file (below). Each zip comment
is the pinned commit. The `en_ugl` archive's `metadata.json` and
first entry are byte-identical to tC4's fixture of the same commit
(`fixtures/tc4/test-fixtures/resources/en_ugl@d9d29e2d5892`), which tC4 fetched and verified
on 2026-10-02. The project owner supplied both archives on 2026-10-15.

| Pin | Repository | Commit | File | Entries | sha256 |
|---|---|---|---|---:|---|
| `lexicon.nt` | `uW/en_ugl` | `d9d29e2d589258ce27f92b59f753a3af03ab7a72` | `en_ugl-d9d29e2d589258ce27f92b59f753a3af03ab7a72.zip` | 5,408 | `f2f9c3e8000ba1dce09b5e26e2f89ff0fff29056eadf66c9805ff57478f20f4f` |
| `lexicon.ot` | `uW/en_uhl` | `72df5ac25acf9d51e826b20e3ad883a5a657ef4e` | `en_uhl-72df5ac25acf9d51e826b20e3ad883a5a657ef4e.zip` | 8,674 | `bbb8e3c4538528d37c352cd8fa5078aeed5e83c6ca2e10b0b462b1d6b1d09cd3` |

These are Scripture Burritos (flavor `peripheral/x-lexicon`, made by Burrito Truck on
2025-11-24), but their metadata cannot vouch for them:

- The ingredient table lists only `README.md`, by an ingredient-relative path, and leaves
  out every entry (`ingredients/content/<Strong's number>.json`, `{brief, long}`).
- The metadata names revision `1` of `uWBurritos/TA`, not a DCS commit.
- The `en_uhl` metadata has an invalid timestamp, `2025-11-24T55:20:15.878Z`. Readers must
  accept the resource anyway.

So the archive's whole file tree is what gets checked: `tc_door43::verify_archive` compares
it with the commit's tree. That needs the commit's tree id from Door43 once. The release
pipeline looks it up and records it in `fetched.lock`, and from then on the installer checks
the tree offline.

Until `fetched.lock` has an entry for a lexicon's commit, that lexicon is unconfirmed: the
packaging step (`tc_resources::install_lexicon_zip`) installs nothing for it, and the app shows
it as unavailable. A release build then lacks a pinned file and fails (see "Fetched when the
app is packaged"). This folder has no `fetched.lock` yet, so both lexicons are unconfirmed
today. Their zips' files hash to the trees `4d24ed9c8ceb768df1069feed0cc375d32cd93ce`
(`en_ugl`) and `ad07f5e41f7052598d90eb8dc2b9ecad811b0b17` (`en_uhl`), which Door43 must confirm
(`.audit/tasks/door43-facts.md`); nothing records them as confirmed.

### Repackaged as one JSON file per lexicon

Thousands of tiny files are slow to install, copy and scan, so the lexicons are not installed
as archives (owner ruling, 2026-10-15). `tc-resources` repackages each verified archive into
one JSON file, and the app loads that file whole and looks entries up in memory. The same
function runs when the app is packaged and whenever a lexicon is installed later, so there is
one format and one code path. The installer ships only the JSON files; the archives above stay
here as their verified source.

An installed lexicon is one content file, `lexicon.json`, and its receipt names the packaging
`lexicon`. The file is UTF-8 JSON without spaces and with a final newline, its keys in the
order below, so that repackaging the same archive always gives the same bytes.
`fetched.lock` records each file's sha256 as `lexiconSha256` in its archive's entry, next to
the tree id, and the packaging step refuses a repackaged file with another sha256, or an entry
without one (it then names the sha256 it found). The repackaged files are:

| Lexicon | Entries | Bytes | sha256 of the repackaged file |
|---|---:|---:|---|
| `en_ugl` | 5,408 | 500,021 | `7798835a14559f03b3d3e4b804b6d88f6118923fd07ff39c44e14258a9f3f094` |
| `en_uhl` | 8,674 | 2,055,993 | `b3a002bbfe52720624b9e97a1130b138cfb4ecfc30a5d4111e7fa1f1581cc258` |

A test repackages both archives and checks these values (`crates/tc-resources/tests/lexicon.rs`),
and an independent writer in Python gave the same bytes.

```json
{
  "format": "tc-lexicon",
  "version": 1,
  "source": { "repo": "uW/en_uhl", "commit": "72df5ac2…", "archiveSha256": "bbb8e3c4…" },
  "metadata": { "…": "the archive's metadata.json, unchanged as a value (license and credits)" },
  "entries": { "1": { "brief": "father", "long": "…" }, "2": { "…": "…" } },
  "index": [ { "id": "H1", "name": "אָב" } ],
  "files": { "ingredients/README.md": "# unfoldingWord Hebrew Lexicon\n\n" }
}
```

- `source.archiveSha256` is the sha256 of the zip the files came from: the bundled zip, or
  the zip a later install downloaded.
- `metadata` must declare the flavor `x-lexicon` (`type.flavorType.flavor.name`, the test tC4
  `resourceArchive.mjs:20` makes); anything else is not repackaged as a lexicon.
- `entries` holds every `ingredients/content/<n>.json` value unchanged, keyed by `<n>`, in
  ascending numeric order. `<n>` is a decimal number without leading zeros that fits in 32
  bits; another file in that folder is one of `files`. An entry is a JSON object, and its
  `brief` and `long`, when present, are text; an entry with fields other than `brief` and
  `long` keeps them. An archive without entries is refused.
- `index` is `ingredients/index.json` unchanged, and is present only when the archive has
  one (`en_uhl`).
- `files` holds every other file of the archive (today only `ingredients/README.md`) as its
  text, byte for byte, keyed by its path below the archive's top folder, so the notes and
  attribution the publisher ships travel with the entries. A file that is not UTF-8 text
  makes the archive refused, in words. tC4 keeps every source file of a lexicon in its
  compact copy (#740); this is the same promise (the owner's ruling on Q26 in
  `docs/TC4-DELTA-2026-10-16.md`).
- A JSON file with a key twice has no single value, so it is refused, in words, as is a
  symbolic link.
- A reader refuses a `format` it does not know or a `version` newer than it reads, in words,
  and the app shows the lexicon as unavailable rather than guessing.

A lexicon installed later goes through the same function: `tc_resources::install_dcs`
downloads the commit's archive, whose tree Door43 reports at that moment, and repackages it.
A lexicon is always installed from its archive, even when its pin names a tag; a burrito export
of a lexicon is refused, because its ingredient table vouches for none of its entries.

The app reads a project's two lexicons once per session (`tc_resources::Lexicons`): `ugl` from
the `lexicon.nt` pin and `uhl` from the `lexicon.ot` pin. A lookup reads no file. A word's
`strong` attribute names its entries by tC4's rule, `lexiconHelpers` in word-aligner-lib 1.0.1
(tC4 4.0 adds no lexicon reader of its own: tC4 `docs/PACKAGING.md:996-997`): the attribute
splits at each `:`, a part that starts with `G` names `ugl` and any other part `uhl`, and the
entry number is JavaScript's `parseInt` of the part without its first ASCII word character
and, unless the part starts with `H`, without its last character (`G35880` is entry 3588 of
`ugl`, `c:H1254a` is entry 1254 of `uhl`). With these two lexicons every numbered part of the
UHB 3.0.0 finds its entry; in the UGNT 0.34, 54 words (18 attributes, such as `G40560`) name
an entry that `en_ugl` at `d9d29e2d` lacks (`fixtures/lexicon/README.md`). Loading `en_uhl`
and looking up every word of Psalms are measured in `docs/evidence/lexicon-2026-10-16.md`.

## Fetched when the app is packaged

The OBS image pack is too large to keep in git, so the packaging step downloads it. The
person who installs the app never downloads it: the installer carries it, and the first run
seeds it into the resource store like every file above.

| Pin | Repository | Commit |
|---|---|---|
| OBS images (`EN_OBS_IMAGES` in tC4) | `uW/obs_images_360` | `7146d5b504f6b63b9e11f7dc0b18c594d0ae179d` |

- The packaging step downloads the archive of exactly that commit. The release pipeline
  runs it, because this build environment cannot reach Door43.
- The first download records the archive's sha256 and size in `fetched.lock` in this
  directory (beside the lexicons' tree ids), and that file is committed. Every later download must match it, or the build
  fails. A changed pin means a new commit and a new lock entry, made on purpose.
- A build whose installer lacks a pinned file fails. The app never falls back to
  downloading a bundled file.

Licenses: the unfoldingWord resources are CC BY-SA 4.0, © unfoldingWord.
