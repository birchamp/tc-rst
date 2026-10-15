# Aquifer fixtures

Tests read these files read-only. Do not edit them. To refresh them, check out the
repositories again at a new tag, run `subset.py`, and record the new tag and commits here.

## Source

The files come from four public BibleAquifer repositories on GitHub, all at the release
tag `v2026-10-12`. Aquifer builds these four repositories from unfoldingWord English
release v91 (each repository's README, "Sources").

| Repository | Tag | Commit |
|---|---|---|
| https://github.com/BibleAquifer/UWTranslationNotes | `v2026-10-12` | `dbfb46e027ee0807563918f4b95c80879cc32c60` |
| https://github.com/BibleAquifer/UWTranslationQuestions | `v2026-10-12` | `f8cc284556dbf4b1bfbd5d8c7bdd7d41ee8775e9` |
| https://github.com/BibleAquifer/UWTranslationWords | `v2026-10-12` | `f8ebe522a769f0f5311a5915d1d61f9e28e6b617` |
| https://github.com/BibleAquifer/UWTranslationManual | `v2026-10-12` | `b993c3bc252262eca6162f5cea2a019b2e069dc4` |

The files were read with `git cat-file blob <commit>:<path>` from clones at those commits,
on 2026-10-15. The languages are `eng` (English) and `por` (Portuguese).

## What is here

| Path here | Source path | Contents |
|---|---|---|
| `UWTranslationNotes/{eng,por}/json/56.content.json` | `<lang>/json/56.content.json` | All of Titus (book 56 in Aquifer's canonical order), 206 notes per language. **Byte-identical**: git blob `f953b4d4…` (eng) and `ea60b53b…` (por). |
| `UWTranslationQuestions/{eng,por}/json/56.content.json` | `<lang>/json/56.content.json` | All of Titus, 36 questions per language. **Byte-identical**: git blob `022929df…` (eng) and `88ff58cb…` (por). |
| `UWTranslationWords/{eng,por}/json/tit-1-1.content.json` | `<lang>/json/NNN.content.json` | Five articles each that Aquifer associates with Titus 1:1. eng: apostle (26324), chosen (26553), faith (26583), godly (26647), Paul (26937). por: Apóstolo (237603), Eleito (237828), Fé (237858), Piedoso (237922), Paulo (238209). |
| `UWTranslationManual/{eng,por}/json/subset.content.json` | `<lang>/json/001.content.json` | Eight articles each, chosen to cover every HTML element the manual uses: Abstract Nouns, Metaphor (both linked from Titus notes), Biblical Money (`table`), File Formats (`pre`, `code`), Chapter and Verse Numbers (`sup`, `br`, `blockquote`), The Translation Process (`img`), Introduction to Translation Checking (`h5`), Create Equal Translations (`u`). The por articles are the same eight, linked through `article_metadata.localizations`. |
| `*/{eng,por}/metadata.json` | `<lang>/metadata.json` | The resource metadata, cut down (see below). |

### How the files were cut

`subset.py` in this directory made every file. It re-serializes JSON with
`json.dumps(indent=0, ensure_ascii=False)`, which reproduces the release bytes exactly, so
each copied article object is byte-identical to its source object except for these
changes:

- **Long reverse-link lists are trimmed.** Some tA articles carry thousands of
  `associations.resource` entries, which are reverse links from notes, mislabeled with the
  code `UWTranslationManual`. Abstract Nouns has 4,886, Metaphor 8,572, Biblical Money
  76, Substantivos abstratos 2,168, Metáfora 3,302 and Dinheiro bíblico 51. Lists longer
  than 20 entries keep their first 20. Nothing else in those articles changed.
- **`metadata.json` keeps only what the fixtures need.** `resource_metadata` is
  unchanged. `scripture_burrito` is unchanged except that `ingredients` lists only the
  source files the fixture articles came from. `article_metadata` (39.5 MB for eng notes)
  keeps only the entries of the fixture articles. For notes and questions, it keeps the
  Titus book introduction, the chapter 1 introduction and the Titus 1:1 entries.

## License and attribution

All four resources are licensed CC BY-SA 4.0
(https://creativecommons.org/licenses/by-sa/4.0/). The attribution, as each
`metadata.json` states it (`license_info`, `scripture_burrito.copyright` and
`adaptation_notice`):

- unfoldingWord® Translation Notes © 2022 unfoldingWord.
- unfoldingWord® Translation Questions © 2022 unfoldingWord.
- unfoldingWord® Translation Words © 2022 unfoldingWord.
- unfoldingWord® Translation Academy © 2022 unfoldingWord.

Adaptation notice (the same for all four, with the resource name changed): "*unfoldingWord®
Translation Notes* © 2022 unfoldingWord. Released under CC BY-SA 4.0 license.
*unfoldingWord® Translation Notes* has been adapted in the following languages: Tok
Pisin, Arabic (عربي), French (Français), Hindi (हिंदी), Indonesian (Bahasa Indonesia),
Portuguese (Português), Russian (Русский), Spanish (Español), Swahili (Kiswahili), and
Simplified Chinese (简体中文) from *unfoldingWord® Translation Notes* © 2022 unfoldingWord.
Released under CC BY-SA 4.0 license by Mission Mutual."

The cut-down copies here are an adaptation of those works and are also released under
CC BY-SA 4.0.
