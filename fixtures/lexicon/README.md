# Lexicon fixtures for `tc-resources`

Tests read these files read-only. Do not edit them by hand.

| File | What it is | How it was made |
|---|---|---|
| `en_uhl_excerpt-7791a4fdd72748b5d59ed8744c1a9ccc9ed9543a.zip` | The `git archive` of a six-file repository made for these tests: `metadata.json`, `ingredients/README.md` and `ingredients/content/1.json`, `2.json` and `10.json`, each unchanged from `bundle/en/en_uhl-72df5ac25acf9d51e826b20e3ad883a5a657ef4e.zip` (so the metadata keeps its invalid timestamp and its ingredient table that lists only `README.md`), and `ingredients/index.json`, which holds the items `H1`, `H2` and `H10` of en_uhl's index, written without spaces. | git 2.43.0. Committed as `fixture <fixture@example.org>`, author and committer date `2026-10-16T00:00:00Z`, then `git archive --format=zip --prefix=en_uhl_excerpt/ HEAD`. The commit is `7791a4fdd72748b5d59ed8744c1a9ccc9ed9543a` and its tree, from `git rev-parse HEAD^{tree}`, is `83455fb87ea50c37c6a8fbb86be297fa016b473a`. The zip is 2,670 bytes, sha256 `0d31b69214e4f87eaa99d32a06e6637a707565e9bc8c9f958634b27622dbeec6`. |
| `fetched.lock` | A lock in the format of `bundle/en/fetched.lock` with the one entry for that archive, so a test can confirm a lexicon archive offline. Nothing in it came from Door43. | Written by hand from the facts above. Its `lexiconSha256`, `6242cc6552271165531e5d0e15ab3c65b99db56bafd84429dbb86ad1564557e9` (1,981 bytes), is what `tc_resources::repackage_lexicon` makes of the archive; an independent Python writer gave the same digest. |
| `strongs-vectors.json` | Which lexicon entry tC4's rule gives for a word's `strong` attribute: 97 edge cases in full, and, for every `strong` attribute of the bundled UHB 3.0.0 and UGNT 0.34 exports, the counts, the parts the bundled lexicons lack and the SHA-256 of one line per distinct attribute. | `node crates/tc-resources/tools/gen-strongs-vectors.cjs /path/to/translationcore4 fixtures/lexicon`, run from the repository root on 2026-10-16 with tC4 at `d310bf304cfee784f4ca45dee474a3faf0d5616e`. It runs `lexiconHelpers` of word-aligner-lib 1.0.1 from tC4's `node_modules` and refuses any other version. |

The generator's rule is the only Strong's rule in tC4 4.0: tC4 adds no lexicon reader of its
own (tC4 `docs/PACKAGING.md:996-997`, issue #703). Read `crates/tc-resources/src/strongs.rs`.

## Licenses

en_uhl, UHB and UGNT are © unfoldingWord, CC BY-SA 4.0. The vectors hold attributes of those
texts and what word-aligner-lib (MIT) computes from them. The generator is part of this
repository (MIT); it runs word-aligner-lib and fflate (MIT) from tC4's
`node_modules` without copying them.
