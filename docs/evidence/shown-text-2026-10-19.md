# The shown text before and after FIX-text (2026-10-19)

Unit FIX-text changed `tc_usfm::shown_text_v1`, the text Translate, Check, the Align bank, the
print model and the store's verse-save reflow read, to a port of tC4's `verseText` at 9ad4d5c
(#782: words under `\zaln` at any depth) with tC4 PR #800's space after a note between two words
(#795), widened to every marker the walk leaves out, to a space where usfm-js keeps none after
the marker and the words would join, and to the words after a character span nested in another
(declared differences, `docs/PLAN.md`). Nothing hashed changed: `plain_text_v1` and every fold
output are byte-identical.

- Branch `app-ftext`, measured at `335ae8e` (the walk is unchanged since `667eac2`). "Before" is
  the same tree with `crates/tc-usfm/src/derivations/mod.rs` of `8d3690f` (the code of `c1bfa78`).
  The census and the parity run were taken again after the review fixes (the space where no
  `nextChar` is kept, the nested spans); only ULT 2SA 23:8 moved, as noted below.
- Machine: the owner's Linux PC (`docs/evidence/linux-machine-2026-10-18.md`), shared with other
  agents' builds; load average 17 to 23 during the timings.
- Data: `bundle/en/en_ult_v91.sb.zip` and `en_ust_v91.sb.zip`, each verse cut by
  `tc_usfm::codec::decompose` with its alignment markup inline (content = `words + layout`), as a
  tC4 4.0 book added from aligned USFM holds it and as the import keeps a verse it cannot split.

## Words lost and joined

From `shown_text_census_of_the_bundle` (`cargo test --release -p tc-align --test shown_words --
--ignored --nocapture`). "Lose `\w` words": the file's `\w` words (notes left out) are not all in
the shown words, in order. "Differ from split": the verse's shown words are not the shown words of
its split plain text (`parse_aligned`). "Kept as written": `split_aligned` gives `None`, so the
import keeps the markup. "Bank is not export": on the split plain text, the shown words are not
`target_words`.

| | ULT before | ULT after | UST before | UST after |
|---|---:|---:|---:|---:|
| verses | 23,900 | 23,900 | 23,823 | 23,823 |
| lose `\w` words | 10,514 | 0 | 19,787 | 2 |
| differ from split | 10,502 | 0 | 19,752 | 1 |
| Genesis: lose words | 633 of 1,533 | 0 | 1,305 of 1,533 | 0 |
| Psalms: lose words | 236 of 2,461 | 0 | 1,346 of 2,456 | 0 |
| kept as written | 269 | 269 | 261 | 261 |
| kept as written and lose words | 68 | 0 | 229 | 2 |
| split refused | 4 | 4 | 39 | 39 |
| bank is not export | 49 | 0 | 2 | 1 |

The before column equals the planner's measurement at `c1bfa78` (`.audit/plans/FIX-text.md` F1).
Before, the 49 ULT verses whose bank was not the export's were all a footnote joining two words
(1CO 5:13, 9:20, 10:9, 15:49; 1TH 2:7, 3:2, 4:9; 2CO 1:12, 7:8, 12:15; 2TH 2:13; 2TI 2:3, 2:14,
4:1; ACT 3:22; COL 4:8; DEU 14:14; EPH 1:15; HEB 1:8, 9:14; LUK 2:33, 10:1, 18:28; MAT 5:22, 5:44,
5:47, 6:33, 7:14, 8:18, 9:8, 15:6, 17:4, 18:14, 24:7, 26:28; MRK 3:32, 7:9, 10:24, 11:3, 15:12;
PRO 1:27, 2:7, 4:26, 8:24, 22:20, 23:26; REV 8:13; ROM 5:1, 13:11), and the UST's were LUK 8:43
(a footnote join) and EXO 15:21.

After, what is left is what tC4 does too:

- UST 2SA 23:8 ("HeonceHe", "killed800"): alignment markup usfm-js misreads. UST 1KI 10:12
  splits "such" into the aligned words "s" and "uch", shown as "such". The import cannot split
  either, so they stay as written, and tC4 #794's fix keeps them out of the aligner. ULT 2SA
  23:8 ("spearagainst" in tC4, an unbalanced `\zaln-e\*` that usfm-js makes an object of, with
  the line break after it) lost a word until the review fixes; the space put back where no
  `nextChar` is kept now keeps "spear against" apart.
- UST EXO 15:21: its aligned form shows "Sing to Yahweh … enemies." (the words stand beside a
  `\qm1` object, a marker usfm-js 3.4.3 does not know), but in the split plain text they are the
  `\qm1` object's `content`, which tC4's `verseText` hides while the export counts them. Its 12
  imported links to those words would drop at open and at the next verse save. Filed in
  `docs/TC4-ISSUES.md` §1, "Found by FIX-text".

The tests that hold these numbers: `every_genesis_and_psalms_verse_of_the_bundle_keeps_every_word`
(all 7,983 Genesis and Psalms verses of both, three ways) and
`the_bank_and_the_export_split_every_plain_verse_into_the_same_words` (427 fixture and draft verses,
47,680 split bundle verses, four named exceptions) in `crates/tc-usfm/tests/shown_text.rs`.

## Links kept at open

`imported_mat_5_22_opens_with_all_44_links` (`crates/tc-align/tests/shown_words.rs`): the ULT verse
split as the import splits it, opened against the UGNT v0.34 verse.

| Verse | Links the split places | Kept at open, before | Kept at open, after |
|---|---:|---:|---:|
| MAT 5:22 | 44 | 42 (2 dropped, "brotherwill") | 44 |
| MAT 5:44 | 17 | 15 (2 dropped, "enemiesand") | 17 |
| 1TH 3:2 | 24 | 22 (2 dropped, "Godin") | 24 |

## Parity with tC4

`tools/dump-shown-text.mjs` runs tC4's own `verseText` over every verse usfm-js finds in every book
of both archives (47,723 verses), and `the_shown_text_is_what_tc4_reads_on_a_dump` (ignored, with
`TC_USFM_SHOWN_TEXT_DUMP`) compares `shown_text_v1` of the codec's cut with it:

- against tC4 PR #800's head `5dcd1472f9a3dec36bbc2f4e811c40d1387e7068`: **1 of 47,723 differs**,
  ULT 2SA 23:8 as written, "spear against" where tC4 shows "spearagainst" (declared in the
  test's list); the wider separator rule and the nested spans change no other published verse;
- against tC4 main `9ad4d5c506466819e5b10f255bbb4a36d394e498`: 93 differ, each a verse where main
  joins two words across a note ("judgesthe", "myselfin" …).

The committed `fixtures/usfm/shown-text-vectors.json` (8,143 cases: tC4's aligned fixtures, 26
drafts, and the md5 of every Genesis and Psalms verse of both) holds the same comparison in CI.

## The separator rule

PR #800 puts the character after a note's end marker back only for `\f`, `\fe`, `\ef`, `\x`,
`\ex` and `\fig`. In tC4 5dcd147, `siervo<marker> de Dios` still shows "siervode Dios" for
`\va 2\va*`, `\vp 1b\vp*`, `\ca 3\ca*`, `\rq …\rq*`, `\cat …\cat*`, `\ndx …\ndx*`, `\pro …\pro*`,
a lone `\xt …\xt*`, `\fdc …\fdc*` and the chunk milestone `\ts\*` (the `hidden-*` drafts of the
vectors hold six of them). The rebuild lets every skipped object offer it.
`woven_verses_show_the_words_of_their_plain_text` (`crates/tc-usfm/tests/properties.rs`) passes
30,000 cases with the rebuild's rule and fails at once with PR #800's alone (minimal case
`a\va 2\va* a`).

The first review found two holes, both fixed: a marker followed by a letter with no space
(`siervo\f + \ft o esclavo\f*de`, `a\fig …\fig*b`, `Je\ts\*sus`) still joined, because usfm-js
keeps a `nextChar` only for a space or a line break; and the words of a character span after a
span nested in it (`\wj Worship the \+nd Lord\+nd* your God\wj*`) were hidden, as tC4 hides
them. The walk now puts a space back where the characters on either side would read as one word
or number, and walks the children of a span with text. The property's generator holds both
cases now; with them it passes 5,000 cases and fails at once on the walk before the fixes
(minimal case `a\fig a|src="b"\fig*a`).

## Cost

Release builds, the same machine and load as above.

| | Before | After |
|---|---:|---:|
| `shown_text_v1` over the 7,983 Genesis and Psalms verses of both (contents cut beforehand), best of 5 | 902 ms | 833 ms, 860 ms (two runs) |
| Aligner open, PSA 42:4 (`cargo test --release -p tc-align --test psalms`), median | 37.9 µs, 65.7 µs | 37.7 µs, 69.9 µs, 54.1 µs |

The parse of usfm-js's port takes nearly all of the 108 µs per verse; the walk adds nothing
measurable on this loaded machine. No budget changes.
