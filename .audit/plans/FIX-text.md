# Plan for fix unit FIX-text: the shown text in tc-usfm

Planned 2026-10-19. Worktree `/home/bot/tc-rst/.claude/worktrees/app-ftext`, branch `app-ftext`,
cut from `tc4fix` at `c1bfa78` (= `claude/focused-ride-5uzj7p`). Nothing is committed yet. The
planner changed no file in the worktree but this one; its probes built into the worktree's
`target/` (about 17 GB), which you may reuse. **Commit this file with your first commit.**

FIX-text is a fix unit, not a design unit. Its spec is `docs/TC4-ISSUES.md` §1 rows 2 (#782),
3 (#795) and 5 (#796, word list), and `docs/TC4-DELTA-2026-10-19.md` §3.4 (C45, C46), with §3.1
item 2, §3.3 item 1 and §4 "tc-usfm (now)". The task text adds: words inside nested alignments
are never dropped; words never join across a footnote **or other skipped marker**; the word list
the aligner banks is the list the export checks; port tC4's tests and vectors; a whole-book
property test over every Genesis and Psalms verse of `bundle/en`.

Read before starting: AGENTS.md ("Matching tC4"), this plan, `crates/tc-usfm/src/derivations/`
(`mod.rs`, `usfmjs.rs` lines 155-330 and 1765-1800 and 2020-2200, `tests.rs`, `js.rs` lines 1-75),
`crates/tc-usfm/tests/derivations.rs` (lines 230-391), `crates/tc-usfm/tests/properties.rs`
(lines 330-616), `crates/tc-align/src/words.rs` lines 323-430, `crates/tc-align/src/aligner.rs`
lines 54-175, `crates/tc-align/tests/common/mod.rs`, `crates/tc-journal/src/alignment.rs`
lines 230-275 (`split_aligned`), `fixtures/usfm/README.md`.

Every shell command that runs cargo or node starts with `. ~/.tc-env && `. Use `-j 6`.

---

## 0. What changes, in one paragraph

`tc_usfm::shown_text_v1` (the text Translate rows, Check, the Align bank, the print model and the
store's reflow read) today joins each usfm-js verse object's `text`, or its children's `text` one
level down. That drops every word under a nested `\zaln` (GEN 1:1 "the heavens", PSA 34:4
"Yahweh", the ULT's aligned "Selah" inside `\qs … \qs*`) and joins the words on either side of a
footnote ("brotherwill"). It becomes a literal port of tC4's `tokenizeVerse` walk
(`src/data/sourceHighlight.ts`, which tC4 9ad4d5c's `verseText` uses since #782) at any depth,
iterative, with tC4 PR #800's separator rule, **extended** so that every object the walk skips
(not only notes, cross references and figures) gives back the character after its end marker
when that character is all that separates two words. Nothing hashed changes: `plain_text_v1`
and every fold output stay byte-identical.

---

## 1. Facts the planner established (do not re-derive; re-measure only where a step says so)

All measured on `c1bfa78` with a scratch copy of the worktree
(`/tmp/claude-1000/-home-bot-tc-rst/8969a572-b4b3-440d-b68e-470f3f1198e1/scratchpad/ftext-probe-ws`,
probe tests in `crates/tc-align/tests/zz_probe.rs` there; the prototype walk is `shown_walk` at
the end of its `crates/tc-usfm/src/derivations/usfmjs.rs`, with probe-only env switches `FT_OLD`
and `FT_GENERAL` that must **not** be copied).

F1. **Today's loss.** Over the codec cut (`codec::decompose`, content = `join_verse(words,
layout)`) of the bundle books with their alignment markup inline (what a tC4 4.0 import keeps
inline, and what the import keeps as written):
- ULT v91: 10,502 of 23,900 verses show other words than their stripped text; GEN 633 of 1,533,
  PSA 236 of 2,461. UST v91: 19,752 of 23,812; GEN 1,305 of 1,533, PSA 1,346 of 2,455.
- GEN 1:1 shows `"In the beginning God created  and the earth."` (8 words);
  PSA 34:4 `"I sought  and he answered me,\nand from all of my fears he delivered me."`;
  PSA 3:2 has no "Selah".
- `split_aligned` keeps 269 ULT verses as written (68 of them lose words) and 261 UST verses
  (229 lose words). PSA 34:4 is kept as written (`split_aligned` returns `None`).
- On the split plain texts, the bank's words (`words(shown_text_v1(plain))`) differ from the
  export's (`target_words(plain)`) in 49 ULT verses (all footnote joins: 1CO 5:13, 9:20, 10:9,
  15:49, 1TH 2:7, 3:2, 4:9, 2CO 1:12, 7:8, 12:15, 2TH 2:13, 2TI 2:3, 2:14, 4:1, ACT 3:22, COL 4:8,
  DEU 14:14, EPH 1:15, HEB 1:8, 9:14, LUK 2:33, 10:1, 18:28, MAT 5:22, 5:44, 5:47, 6:33, 7:14, 8:18,
  9:8, 15:6, 17:4, 18:14, 24:7, 26:28, MRK 3:32, 7:9, 10:24, 11:3, 15:12, PRO 1:27, 2:7, 4:26, 8:24,
  22:20, 23:26, REV 8:13, ROM 5:1, 13:11) and 2 UST verses (LUK 8:43, a footnote join; EXO 15:21,
  see F6).
- MAT 5:22 split, opened with UGNT MAT 5:22 (`Aligner::open(Some(&value), TargetVerse::new(&plain),
  &source, Basis::run(v22, v22, …))`): 44 links placed in the split value, 42 kept at open,
  2 dropped, 43 words. MAT 5:44: 17 → 15, 2 dropped. 1TH 3:2: 24 → 22, 2 dropped.

F2. **tC4's rules.**
- tC4 9ad4d5c (`9ad4d5c506466819e5b10f255bbb4a36d394e498`, main): `verseText = (vObj) =>
  tokensText(tokenizeVerse(vObj)).trim()` (`src/data/bookModel.js:19`). `tokenizeVerse` at
  9ad4d5c is byte-identical to d310bf3's; #782 only made `verseText` use it.
- tC4 PR #800 (open; branch `origin/issue-795-one-hash`, head `5dcd1472f9a3dec36bbc2f4e811c40d1387e7068`,
  the display part in `52a024637c471e226b31b119e0912f3f196ccb1b`): adds `gap`/`last` and
  `noteGap` to `tokenizeVerse`. `git diff 9ad4d5c 5dcd147 -- src/data/sourceHighlight.ts
  src/data/bookModel.js` is exactly that change. 5dcd147 is **not** a descendant of 9ad4d5c,
  but those two files are 9ad4d5c's plus the gap rule.
- `journal/fold.mjs` (`verseTextMd5`, the hashed text) and `verseTextIndex`/`indexBook` are
  unchanged from d310bf3 to 9ad4d5c and 5dcd147. `package-lock.json` pins the same usfm-js.
- tC4's own walk (literal, PR #800):

```js
const NOTE_TAG = /^(f|fe|ef|x|ex|fig)$/;
let gap = '', last = '';
push(tok): if (tok.text !== '') { if (gap && /\S/.test(last) && /^\S/.test(tok.text)) out.push(gap);
           gap = ''; last = tok.text.slice(-1); }  out.push(tok)
noteGap(vo): if ((vo.type === 'footnote' || NOTE_TAG.test(vo.tag ?? '')) && !gap
             && typeof vo.nextChar === 'string') gap = vo.nextChar;
walk(vos): for (const vo of vos) {
  if (vo.type === 'footnote' || vo.tag === 'f') { noteGap(vo); continue; }
  if (vo.tag === 'zaln' && vo.type === 'milestone') { walk(vo.children ?? []); continue; }
  if (vo.type === 'word') { push(vo.text ?? ''); continue; }
  if (vo.text != null && vo.type !== 'section') { push(vo.text); continue; }
  if (vo.children) walk(vo.children); else noteGap(vo);
}
```

F3. **The prototype matches tC4.** The literal port over the usfm-js port's heap gives tC4
5dcd147's `verseText` on **all 47,723 verses** of the ULT v91 and UST v91 (every usfm-js verse
key, `\q1 \v N` verses included), on all 450 cases of `fixtures/usfm/check-text-vectors.json`,
and on all 8,143 cases of the draft shown-text vectors (§3 step 3) except the six deliberate
`hidden-*` drafts of D2. With the prototype, every test of tc-usfm, tc-align, tc-check,
tc-export, tc-journal, tc-store, tc-corpus and tc-app passes unchanged (the tc-app state-seed
byte comparison included), also with the five new tC4 fixture files added. No existing test pins
the old behaviour, so the new tests below are what prove the fix.

F4. **Not a format derivation.** STORAGE §12.1 (storage branch) lists `format.json`'s derivations
as `plainText`, `verseSplit`, `tokenizer`, `selections`. The shown text is none of them. The fold
decides alignment validity by the md5 of `plain_text_v1`, the span, the source sha and
`marked_invalid` (`tc-journal/src/alignment.rs:52-75`); no fold output reads the shown text.
So no derived byte changes and no version is raised (§12.2 rules 3 and 7). The shown text does
decide which words a new alignment holds (the aligner banks them) and which links the store's
verse-save reflow keeps (`tc-store/src/commands.rs:335,682` build `TargetVerse`), so it must be
settled before format 1.0 (DELTA §3.3 item 1). The task text's "the shown text is a derivation
(STORAGE §12.1)" is therefore not accurate; say so in the final answer.

F5. **Vectors that do not change.** `fixtures/usfm/verse-text-vectors.json` is `plain_text_v1`'s
text (the hash), not the shown text: it must not change (DELTA §3.4 item 3 is wrong to list it).
`fixtures/usfm/check-text-vectors.json` regenerated at 9ad4d5c or 5dcd147 is byte-identical to the
committed file (the 450 plain-draft cases have no nesting and no note between two words).

F6. **What stays, by tC4's rule (not fixed here; reported).**
- The shown text hides the items of a `\li` list and the lines of markers usfm-js 3.4.3 does not
  know and keeps as `content` (`\qm1`). The export's `target_words` counts those words. So the
  bank and the export differ on exactly: `fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm`
  2:2, the `lists` draft of `check-text-vectors.json` 1:1 and 1:2, and UST v91 EXO 15:21 (split
  plain). In UST EXO 15:21 the aligned form shows "Sing to Yahweh … enemies." (the words are
  top-level siblings of the `\qm1` object) but the split plain hides them, so an imported
  alignment of that verse would lose those 12 links at open and at the next verse save.
- Words still join in three published verses with unbalanced `\zaln-e\*` (ULT 2SA 23:8
  "spearagainst", UST 1KI 10:12, UST 2SA 23:8 "HeonceHe", "killed800"), and where a note stands
  inside an alignment run right before `\zaln-e\*` (usfm-js misparses the note; the separator ends
  up in the `content` of a bogus `zaln-e\*` object). tC4 (PR #800 included) does the same. These
  occur only in verses that still hold alignment markup; none is in GEN or PSA.
- Text after a stray end marker (`a\nd* a`, no opening `\nd`) is hidden (usfm-js keeps it as the
  marker's `content`). tC4 does the same.

F7. **Which markers still join under PR #800 alone** (tC4 5dcd147, `siervo<marker> de`): `\va 2\va*`,
`\vp 1b\vp*`, `\ca 3\ca*`, `\rq …\rq*`, `\cat …\cat*`, `\ndx …\ndx*`, `\pro …\pro*`, a standalone
`\xt …\xt*`, `\fdc …\fdc*` and the chunk milestone `\ts\*` all give `"siervode Dios"`. Each such
object carries `nextChar: " "` and no `text`/`children`. `\f`, `\fe`, `\ef`, `\x`, `\ex`, `\fig`
give `"siervo de Dios"`. `\k-s\*`, `\qt-s\*` and an empty `\zaln-s…\zaln-e\*` already give
`"siervo de Dios"` (the space is in a text object). Rule G (D2) gives `"siervo de Dios"` for all.

F8. **Rule G changes nothing on real data.** With rule G the walk still equals tC4 5dcd147 on all
47,723 bundle verses, the 450 check-text cases and every vector draft but the `hidden-*` ones. A
proptest (step 4) of 30,000 woven verses with notes and hidden markers directly between words
(nested milestones up to three deep) passes with rule G and fails with PR #800's rule alone
(minimal case `"a\\va 2\\va* a a"`).

F9. **After the change** (prototype with rule G): GEN and PSA of ULT and UST: 0 verses lose or
join words by any oracle (raw `\w` words in order; shown(aligned) = shown(split plain);
shown(plain) = `target_words(plain)`), `parse_aligned` fails on none of their 7,983 verses.
Whole bundle: bank ≠ export only on UST EXO 15:21 (47,680 split plains compared; `parse_aligned`
fails on 43 verses, none in GEN/PSA). MAT 5:22 opens with 44 of 44 links, 0 dropped (5:44 17/17,
1TH 3:2 24/24). GEN 1:1 inline: 10 words, "the" ×3. Fixtures and the final drafts: 427 verses
without word markup compared, bank ≠ export only on the three `\li` verses of F6.

F10. **Cost.** `shown_text_v1` over the 7,983 GEN/PSA verses: 1.77 s old vs 1.85 s new (release,
shared machine, one run each, includes reading and cutting the books; noise-level). The aligner
on PSA 42:4 (`cargo test --release -p tc-align --test psalms`): open median 67 µs both. tc-usfm
builds at `opt-level = 2` in dev (`Cargo.toml` `[profile.dev.package.tc-usfm]`), so the bundle
tests below take about 7 s in a dev build.

F11. **Environment traps.** The tC4 checkout `/home/bot/unfoldingword/translationcore4` (detached
at d310bf3) has **no `node_modules`**, despite the task text. A usable copy (usfm-js 3.4.3,
fflate 0.8.2) is at
`/tmp/claude-1000/-home-bot-tc-rst/8969a572-b4b3-440d-b68e-470f3f1198e1/scratchpad/a7-tc4/node_modules`
(read it, never modify or delete it). Never `npm install` (network). Never check anything out
in the tC4 folder: use `git -C <tc4> show <commit>:<path>` and `git -C <tc4> archive <commit> src
| tar -x -C <your scratch dir>`. Planner-made trees already exist (read-only for you):
`…/scratchpad/ftext-tc4-5dcd147` and `…/scratchpad/ftext-tc4-9ad4d5c` (each `src journal test
conformance package.json` plus a `node_modules` symlink). Node is v22.23.3 (TypeScript runs as
is). tC4's `test/fixtures` at 9ad4d5c are also in `…/scratchpad/ftext-tools/tc4fx/`.

---

## 2. Decisions

| Id | Decision | Why |
|---|---|---|
| D1 | `shown_text_v1` is a literal port of F2's walk over the usfm-js port's heap, in a new module `crates/tc-usfm/src/derivations/shown.rs`, iterative with an explicit stack, no depth limit, an `on_path` guard that turns an object inside itself into a throw, and the old fallback (the content trimmed) wherever usfm-js or the walk throws. Keep the name `shown_text_v1` and every caller. | tC4 parity is by definition the JS walk on usfm-js objects; the port of usfm-js already matches it on 47,723 verses (F3). JS recursion dies a few thousand levels down (RangeError); the existing derivations' convention (`deep_nesting_has_the_value_of_shallow_nesting`, STORAGE §7.7) is "no depth limit, the walk's value". The fold does not call it, but the store's reflow and the aligner do, and a stack overflow aborts the process. Renaming would touch tc-journal, tc-align, tc-export and STORAGE for nothing: the shown text is not a format derivation (F4). |
| D2 | **Rule G**: every object the walk skips (the footnote branch, and the "no text and no walkable children" branch) offers its `nextChar` as the gap, not only tags `f|fe|ef|x|ex|fig`. Everything else is PR #800's rule (first gap wins, inserted only between a non-space last code unit and a non-space first code unit, reset by any non-empty token). Declared difference in PLAN.md. | The task requires that words never join across a footnote **or other skipped marker**; PR #800 still joins across `\va`, `\vp`, `\ca`, `\rq`, `\cat`, `\ndx`, `\pro`, `\xt`, `\fdc`, `\ts\*` (F7). A joined word cannot be checked or aligned, and the export then drops the verse's alignment (#796): tC4's behaviour works against the feature (AGENTS.md "Matching tC4"). Rule G equals tC4 5dcd147 on every real verse (F8), so it costs no parity on real data. |
| D3 | New vector file `fixtures/usfm/shown-text-vectors.json` from a new generator `crates/tc-usfm/tools/gen-shown-text-vectors.mjs`, run on tC4 5dcd147 with tC4 9ad4d5c as the `main` column. `check-text-vectors.json` stays (its generator is fixed so it runs on tC4 ≥ 9ad4d5c, and its regenerated bytes are identical, F5). `verse-text-vectors.json` is not touched. | The check-text generator goes through `indexBook`, which skips `\q1 \v N` verses (PSA-3's both verses), so it cannot hold tC4's Selah case; the new file lists every usfm-js verse key. A `main` column makes PR #800's difference visible and checkable. |
| D4 | The `\li`/`\qm` hide, the residual joins and the stray-end-marker hide (F6) are **not** changed. The bank/export CI check lists the four `\li`/`\qm` verses by name, so a new one fails CI. They are reported as findings (TC4-ISSUES §1 addendum, evidence file, final answer) with a recommendation for the owner. | They are tC4's `verseText` rule for hidden content, not joins; changing them is a decision on which hidden content counts as verse text, on two screens, beyond #782/#795/#796. TC4-ISSUES row 5's CI check as written ("every fixture verse with no word markup") is not satisfiable because of them: fix that design text (step 6). |
| D5 | New test files, not edits of busy ones: `crates/tc-usfm/tests/shown_text.rs`, `crates/tc-align/tests/shown_words.rs`; unit tests in `crates/tc-usfm/src/derivations/tests.rs`; one proptest added to `crates/tc-usfm/tests/properties.rs`. | Other fix units (FIX-align: #787, #789, #790, #794) edit `tc-align/tests/aligner.rs` and `aligner.rs`; separate files avoid merge conflicts. |
| D6 | No test calls `Aligner::open` on content that holds `\zaln` inline. Inline content is tested through `TargetVerse::new` / `TargetText::shown`; `Aligner::open` is used only on split plain text. | TC4-ISSUES row 7 (#794, another unit) makes `Aligner::open` refuse a verse that holds alignment markup (`Unavailable::AlignedInText`). |
| D7 | Do not edit `docs/STORAGE.md` (main's copy is merged from the storage branch) nor anything on the storage branch. The STORAGE change goes into the final answer as STORAGE-DELTA notes (§8 below). | Task text; the storage team owns STORAGE.md. |
| D8 | Vendor tC4 9ad4d5c's `test/fixtures/README.md` verbatim and the five files it adds (`en_ult/GEN-1-1.usfm`, `en_ult/GEN-1.usfm`, `en_ult/GEN-773.usfm`, `en_ult/PSA-3.usfm`, `hbo_uhb/GEN-1-1.usfm`), byte for byte. | The rebuild's `fixtures/tc4/test-fixtures/README.md` is byte-identical to tC4 d310bf3's. A sibling unit (tc-helps, #773/#765) needs `GEN-1.usfm` and `GEN-773.usfm` from the same commit; identical files and an identical README merge cleanly. `GEN-773.usfm` is vendored because the README names it. |

---

## 3. Steps (each one commit; build and run its tests before committing)

### Step 1. Vendor tC4 9ad4d5c's test fixtures

Files:
- `fixtures/tc4/test-fixtures/README.md` ← `git -C /home/bot/unfoldingword/translationcore4 show
  9ad4d5c:test/fixtures/README.md` (verbatim).
- `fixtures/tc4/test-fixtures/en_ult/{GEN-1-1,GEN-1,GEN-773,PSA-3}.usfm` and
  `fixtures/tc4/test-fixtures/hbo_uhb/GEN-1-1.usfm` ← `git show 9ad4d5c:test/fixtures/<path>`.
  Check sha256 against tC4's README: GEN-1-1 `44fe75ea…d405`, PSA-3 `6068efd5…b05a`,
  hbo GEN-1-1 `db5b57f9…02f5`, GEN-773 `a2a247ff…bc55c`; GEN-1.usfm is lines 1–819 of the v91
  `GEN.usfm` (planner's sha256 `fd7610d2…915b`).
- `fixtures/README.md`, section `tc4/`: add one sentence: these five files and
  `tc4/test-fixtures/README.md` come from tC4 commit `9ad4d5c506466819e5b10f255bbb4a36d394e498`
  (2026-10-19), for tC4 #782, #768 and #773; everything else is still from `d494a9d`.

Invariant: no other byte under `fixtures/tc4/` changes. `cargo test -p tc-usfm` (its
`every_fixture_writes_back_byte_for_byte` and `every_fixture_book_round_trips_byte_for_byte` walk
every fixture, the new ones included; the planner ran them: they pass).

Commit message: why the files come from 9ad4d5c and that README and files are verbatim.

### Step 2. Words at any depth (#782): the walk, without the separator rule

Code:
- `crates/tc-usfm/src/derivations/usfmjs.rs`: add `pub(super) fn len(&self) -> usize` to `Heap`
  (the number of items, for `on_path`). Add a `#[cfg(test)] pub(super) fn cyclic_verse() -> Parsed`
  next to `walks_of_cycles` (line ~2573) that builds, by hand, one verse whose object holds an
  object that holds the first one (use the same `object`/`tie` helpers pattern), for the cycle
  test.
- New `crates/tc-usfm/src/derivations/shown.rs` (declare `mod shown;` in `derivations/mod.rs`):

```rust
//! module doc: what tC4's verseText is (9ad4d5c bookModel.js:19 + tokenizeVerse), which objects
//! give text, which are skipped, why the walk is iterative and when it is a throw.
use super::js::{JsStr, eq, is_space};
use super::usfmjs::{Key, Parsed, Ref, Thrown, Val, truthy};

/// The shown text of every verse of `parsed`, joined and not yet trimmed; `Err(Thrown)` where
/// tC4's walk throws.
pub(super) fn text(parsed: &Parsed) -> Result<JsStr, Thrown>
```

  Per verse (`for (_, verse) in &parsed.verses`, in that order): reset `gap: JsStr` and
  `last: Option<u16>` (tC4 calls `tokenizeVerse` per verse), then walk `heap.elems(*verse)` with a
  stack of frames `(owner: Option<Ref>, list: &[Ref], at: usize)`. For each object `vo`, decide in
  this order (strings compared with `heap.s(vo, &key).is_some_and(|s| eq(s, "…"))`):
  1. `type == "footnote"` or `tag == "f"` → **skip** (offer the gap in step 3; in step 2 do
     nothing here).
  2. `tag == "zaln"` and `type == "milestone"` → children: absent → nothing; `Val::Str` → nothing
     (JS iterates the characters of a string and each gives nothing); an array `Val::Ref` → walk
     it; anything else → `Err(Thrown)`.
  3. `type == "word"` → text: absent → push `""`; `Val::Str(t)` → push `t`; anything else →
     `Err(Thrown)` (JS calls `.slice` on it).
  4. `text` present (any value) and `type != "section"` → `Val::Str(t)` → push `t`; anything else
     → `Err(Thrown)`.
  5. Otherwise children: present and `truthy` → `Val::Str` nothing, array → walk, else
     `Err(Thrown)`; absent or falsy → **skip**.
  Walking an array owned by object `r`: if `on_path[r]` → `Err(Thrown)`; set it; clear it when
  that frame is exhausted. `push(t)` in step 2 just appends `t`. Do not use recursion, `unwrap`
  or indexing that can panic (`t.first()`, `t.last()`).
- `derivations/mod.rs` `shown_text_v1`: parse as today; if `parsed.thrown` → content trimmed;
  else `shown::text(&parsed)` → `to_string(trim(&text))`, `Err` → content trimmed. Rewrite its doc
  comment: tC4 9ad4d5c's `verseText` (`src/data/bookModel.js:19`, the walk of
  `src/data/sourceHighlight.ts`), what is shown (words under `\zaln` at any depth, superscription,
  `\qs` Selah also when aligned inside `\qs`), what is not (notes, `\li` items, a section's own
  text, content usfm-js keeps for markers it does not know), the throw cases, that it is never
  hashed, and that it is not one of `format.json`'s derivations (STORAGE §12.1) though it decides
  which words an alignment holds. Remove `Ref`/`Val`/`truthy` imports from `mod.rs` if unused.
- Doc comments that name the old rule: `crates/tc-align/src/words.rs:323-340` (`TargetVerse`: "at
  d310bf3" → 9ad4d5c; add "every word of a nested alignment, such as Genesis 1:1 'the heavens'");
  `crates/tc-usfm/src/text.rs:1-8` says `display_text` is "the text of `verseTextIndex` that tC4's
  Check screen shows and its aligner banks": correct it to "the text tC4's source panes show
  (`src/views/verseText.js`)" (lib.rs already says so). Nothing else in tc-journal/tc-check needs a
  change (their docs say "tC4's verseText" generically).

Tests (all must fail on `c1bfa78` except where noted, and pass now):
- `crates/tc-usfm/tests/shown_text.rs` (new; `#![allow(clippy::unwrap_used, clippy::expect_used,
  clippy::panic)]`; content of a verse = `codec::decompose` of the file, `join_verse(words,
  layout)`):
  - `shown_text_reads_words_inside_nested_alignments`: `fixtures/tc4/test-fixtures/en_ult/GEN-1-1.usfm`
    1:1 → exactly `"In the beginning God created the heavens and the earth."`; its
    `tc_usfm::words::words` are 10, "the" with occurrences 1, 2, 3 of 3, "heavens" 1 of 1.
    Also GEN-1.usfm 1:1 the same, and 1:4 exactly `"And God saw the light, that {it was} good.
    Then God separated between the light and the darkness."` (tC4's "Translate reads the same
    text" case).
  - `every_verse_of_genesis_1_holds_its_words_in_file_order` (tC4 ways 1 and 4): for every verse
    of GEN-1.usfm, the `\w` words read from the raw text (footnotes cut: regex-free version: walk
    `tc_usfm::Document` spans, skip `f`/`x`/`fe`/`ef`/`ex`/`fig` spans, take the text before `|` of
    each `w` span) appear in the shown words in order, each once; and at least one verse nests
    (`1:1`).
  - `selah_inside_qs_shows`: `en_ult/PSA-3.usfm` 3:2 → exactly `"{There are} many sayers about
    me,\n“There is no salvation for him in God.”\nSelah"` (ends with `"\nSelah"`, as tC4 asserts).
  - `footnotes_and_headings_stay_out_of_the_shown_text` (tC4 way 2, passes before too):
    `usfm-corpus/exotic-poetry-footnotes.usfm` verses 1:1, 1:2, 1:3, 1:4-5: no
    "manuscripts", "explained", "carry a footnote", "one span verse", "Poetry".
- `crates/tc-usfm/src/derivations/tests.rs`:
  - `a_verse_with_no_nested_alignment_keeps_its_one_level_text` (tC4 way 3): keep the old
    algorithm here as `fn one_level(content: &str) -> String` (the body of today's
    `shown_text_v1`, verbatim) and `fn nests(heap, verse) -> bool` (a `zaln`/`milestone` object
    below the top level, at any depth). For every verse of the 8 files tC4 lists
    (`usfm-corpus/exotic-poetry-footnotes.usfm`, `exotic-partial-book.usfm`,
    `sample-TIT-draft.usfm`, `sample-JON-span.usfm`, `en_ult-TIT-aligned.usfm`,
    `en_ust-TIT-aligned.usfm`, `en_ult/GEN-1.usfm`, `en_ult/PSA-3.usfm`) that does not nest,
    `shown_text_v1 == one_level`; more than 50 compared; and `one_level` of PSA-3 3:2 has no
    "Selah" (pins the defect the fix removes).
  - `the_shown_text_has_no_depth_limit`: 20,000 nested `\zaln-s |x-content="a"\*` around
    `\w x\w*` (with their `\zaln-e\*`) and 100,000 nested `\add ` give the same value as depth 3,
    in a dev build, no stack overflow (same shape as `deep_nesting_has_the_value_of_shallow_nesting`).
  - `a_shown_walk_that_never_ends_is_a_throw`: `shown::text(&usfmjs::cyclic_verse())` is `Err`.

Commit message: #782; why iterative; that the Psalms Selah and GEN 1:1 now show; no hash moves.

### Step 3. Words never join across a skipped marker (#795, rule G), and the vectors

Code:
- `shown.rs`: add the gap. On **skip** (branches 1 and 5): if `gap` is empty and
  `heap.get(vo, &Key::NextChar)` is a `Val::Str(n)`, set `gap = n.clone()` (an empty string sets
  nothing, as JS's `!gap`). `push(t)`: if `t` is not empty: when `gap` is not empty, `last` is a
  code unit that is not `is_space`, and `t.first()` is not `is_space`, append `gap` first; then
  clear `gap` and set `last = t.last()`. Then append `t`. (`js::is_space` is ECMAScript's `\s`
  set, the same as `trim`'s; code units, not code points, as JS's `slice(-1)` and `/^\S/`.)
- Module doc and `shown_text_v1` doc: describe the rule, that PR #800 limits it to `\f`, `\fe`,
  `\ef`, `\x`, `\ex`, `\fig`, and that the rebuild applies it to every skipped object (D2),
  listing F7's markers, and the residuals of F6 (a note inside an alignment run, unbalanced
  `\zaln-e\*`).
- `crates/tc-usfm/tools/gen-check-text-vectors.mjs`: import `tokenizeVerse` and `tokensText` from
  `<tc4>/src/data/sourceHighlight.ts` and pass them into the `new Function` that runs
  `bookModel.js`'s `verseText` (harmless at d310bf3, required from 9ad4d5c on); update its header
  comment. Regenerate with the 5dcd147 tree and `cmp` it with the committed file: identical (F5).
  Do not regenerate `verse-text-vectors.json`.
- New `crates/tc-usfm/tools/gen-shown-text-vectors.mjs`: start from the planner's draft
  `/tmp/claude-1000/-home-bot-tc-rst/8969a572-b4b3-440d-b68e-470f3f1198e1/scratchpad/ftext-tools/gen-shown-text-vectors.mjs`
  (it runs; read it whole before you copy it). What it does: checks usfm-js 3.4.3; runs each
  tree's own `bookModel.js` `verseText` declaration with that tree's `tokenizeVerse`/`tokensText`;
  emits, for every usfm-js verse key (not `front`):
  - full `text` cases for `usfm-corpus/en_ult-TIT-aligned.usfm`, `en_ust-TIT-aligned.usfm`,
    `exotic-poetry-footnotes.usfm`, `en_ult/GEN-1-1.usfm`, `GEN-1.usfm`, `PSA-3.usfm`;
  - full `text` cases for drafts held in the file's `drafts`: PR #800's cases built from the
    sample Titus 1:1 line as a one-verse book (`note-f`, `note-x`, `note-fe`, `note-ef`,
    `note-fig`, `note-after-space`, `two-notes`, `note-space-note`, `note-before-comma`,
    `note-first`, `note-last`, `note-line`), the rule-G drafts (`hidden-va`, `hidden-vp`,
    `hidden-ca`, `hidden-rq`, `hidden-cat`, `hidden-ts`), and hand-written ones (`note-newline`,
    `note-poetry`, `note-rtl`, `note-astral`, `note-nbsp`, `aligned-notes`, `nested-three`,
    `nested-in-add`);
  - `md5` cases (md5 of the UTF-8 text) for `en_ult/ingredients/{GEN,PSA}.usfm` and
    `en_ust/ingredients/{GEN,PSA}.usfm` read from `bundle/en/*.sb.zip` with tC4's `fflate`;
  - with the second tree, `main` on each full-text case whose 9ad4d5c text differs, and a throw
    if any bundle verse differs between the two trees.
  Header: `generator`, `tc4` (full 5dcd147 sha), `main` (full 9ad4d5c sha), `libraries`, `drafts`,
  `cases`. Run it from the repository root:

```sh
. ~/.tc-env && node crates/tc-usfm/tools/gen-shown-text-vectors.mjs \
  <scratch>/tc4-5dcd147 5dcd1472f9a3dec36bbc2f4e811c40d1387e7068 \
  <scratch>/tc4-9ad4d5c 9ad4d5c506466819e5b10f255bbb4a36d394e498 > fixtures/usfm/shown-text-vectors.json
```

  (the planner's run: 8,143 cases, 1.15 MB; `main` on exactly `note-f`, `note-x`, `note-fe`,
  `note-ef`, `note-fig`, `two-notes`, `note-line`, `note-newline`, `note-poetry`, `note-rtl`,
  `note-astral`, `aligned-notes`.)
- `fixtures/usfm/README.md`: a row for `shown-text-vectors.json` (what, how made, from which
  trees, with `git archive` + linked `node_modules`, the residual `aligned-notes` case), the
  check-text row's "from tC4 checkout d310bf3" → "from tC4 5dcd147 (identical at d310bf3 and
  9ad4d5c)", and the regenerate block. Licence paragraph: the file holds verse text of tC4's
  fixtures (CC BY-SA 4.0) and md5s of bundle verses.
- `docs/PLAN.md`, after the bullet list of "Where the rebuild differs from tC4 4.0 on purpose"
  (after "Every other question in that §5 follows its recommendation."): a short paragraph
  "Under AGENTS.md "Matching tC4" (2026-10-19):" with two bullets:
  - Words inside nested alignments show in Translate, Check, Align and print (tC4 #782, on tC4
    main at 9ad4d5c); the reference d310bf3 drops them (Q32 of `docs/TC4-DELTA-2026-10-19.md`).
  - A footnote, cross reference, figure or any other marker the shown text leaves out (`\va`,
    `\vp`, `\ca`, `\rq`, `\cat`, `\ts\*` …) between two words keeps the space after it, so the
    words never join; tC4's open PR #800 does this for notes, cross references and figures only
    (C46).

Tests:
- `crates/tc-usfm/tests/shown_text.rs`:
  - `a_footnote_between_words_keeps_its_space`: content `"Pablo, siervo\\f + \\ft o esclavo\\f* de
    Dios."` → `"Pablo, siervo de Dios."`; and `"…\\f + \\fr 1:1 \\ft o esclavo\\f* de Dios.\n"`
    likewise (PR #800's last case).
  - `a_note_of_any_kind_between_words_keeps_its_space`: on the sample Titus 1:1
    (`fixtures/tc4/sample-burrito/ingredients/TIT.usfm`, the line starting `\v 1 Pablo`), each
    replacement of `"siervo de"` by `siervo\x - \xt Rom 1:1\x* de`, `\fe…\fe*`, `\ef…\ef*`,
    `\fig a|src="b"\fig*`, `siervo \f…\f* de`, two notes with and without a space between them
    gives the unmodified 1:1 text; `siervo\f…\f*, de` gives it with `"siervo, de"`; a note right
    after `\v 1 ` and one after the last `"piedad,"` give it unchanged (PR #800's cases, ported).
  - `any_skipped_marker_between_words_keeps_its_space` (rule G): `siervo<m> de Dios` for each m of
    F7 (`\va 2\va*`, `\vp 1b\vp*`, `\ca 3\ca*`, `\rq Rom 1:1\rq*`, `\cat x\cat*`, `\ndx x\ndx*`,
    `\pro x\pro*`, `\xt x\xt*`, `\fdc x\fdc*`, `\ts\*`) → `"siervo de Dios"`; and `"a\\f + \\ft
    n\\f*\u{a0}b"` stays `"a\u{a0}b"` (no space added before a no-break space).
  - `the_shown_text_is_what_tc4_reads`: read `fixtures/usfm/shown-text-vectors.json`; assert the
    header's `tc4` and `main` shas; for each book (source: `drafts`, else `"<archive> <entry>"` read
    from the zip with the `zip` dev-dependency, else a path under the repository root) the codec's
    verse keys equal the vectors' keys (`codec::label(v, last)` per chapter); every `text` case
    equals `shown_text_v1`, except the six `hidden-*` drafts, where ours must equal tC4's text
    with `"siervode"` replaced by `"siervo de"` and must differ from it (the declared difference);
    every `md5` case equals the md5 of ours (`md-5` dev-dependency); the set of books that carry
    `main` is exactly the twelve listed above (PR #800's rule is tC4's only change from main);
    pin the case count.
- `crates/tc-usfm/tests/derivations.rs`: extend `time_derivations` to time
  `shown_text_v1(text)` with `selections_v1` (so `derivations_take_time_linear_in_the_verse` and
  `a_verse_of_several_megabytes_takes_seconds` cover it), and add two `hostile_verses`: "nested
  alignments" (`\zaln-s |x-content="a"\*` repeated, then `\w x\w*`, then the `\zaln-e\*`s) and
  "notes between words" (`"a\\f + \\ft n\\f*"` repeated).
- `the_check_text_is_what_tc4_reads` stays as it is and must pass unchanged.

Commit message: #795 and C46; rule G and why (F7, F8); declared difference; the vectors and why
`check-text-vectors.json` and `verse-text-vectors.json` do not change.

### Step 4. The whole-book property test, the bank/export check (#796) and a proptest

`crates/tc-usfm/tests/shown_text.rs`:
- `every_genesis_and_psalms_verse_of_the_bundle_keeps_every_word`: for `en_ult_v91.sb.zip` and
  `en_ust_v91.sb.zip`, books `GEN` and `PSA` (entries `<id>/ingredients/<BOOK>.usfm`), every
  `VersePart::Text` verse of `codec::decompose`; with `shown = words(shown_text_v1(content))` as
  `(text, occurrence, occurrences)`:
  1. the raw `\w` words of the content outside notes (Document walk as in step 2), split with
     `words::words`, appear in `shown` in order (tC4's `holdsInOrder`);
  2. `parse_aligned(&content)` succeeds (it does for all 7,983) and
     `words(shown_text_v1(&plain)) == shown` (the bank of a verse kept as written equals its bank
     after the import's split);
  3. `target_words(&plain)` equals `words(shown_text_v1(&plain))` (the export's list);
  4. assert the verse counts 1,533 + 2,461 + 1,533 + 2,456 = 7,983 and no failure, listing every
     failure with book and reference.
  (Fails on `c1bfa78`: 633 + 236 + 1,305 + 1,346 verses fail 1 or 2.)
- `the_bank_and_the_export_split_every_plain_verse_into_the_same_words` (the tc-usfm part of
  #796): for every verse whose content `target_words` accepts (no word markup) of every plain
  fixture (`fixtures/tc4/sample-burrito/ingredients/{TIT,JON}.usfm`, the four plain
  `usfm-corpus` drafts, `fixtures/tc4/conformance-fixtures/import/usfm/{32-JON.usfm,57-TIT.usfm,no-id.sfm}`,
  `fixtures/tc4/conformance-fixtures/import/damaged/57-TIT-unclosed.usfm`), of every draft of
  `check-text-vectors.json` and `shown-text-vectors.json`, and of the split plain
  (`parse_aligned(content).plain`) of every verse of every book of both bundle archives:
  `words(shown_text_v1(c)) == target_words(c)`, except exactly this named list (assert the list
  of mismatches equals it, so a new one fails): `exotic-poetry-footnotes.usfm 2:2`,
  `check-text-vectors lists 1:1`, `check-text-vectors lists 1:2` (items of a `\li` list), and
  `en_ust EXO 15:21` (a `\qm1` line) — each with a comment pointing to the finding (step 6).
  Planner's counts with the final drafts: 427 fixture/draft verses (53 skipped for word markup),
  47,680 bundle plains, 43 bundle verses `parse_aligned` refuses. (`\ndx`, `\pro`, `\fdc` and a
  standalone `\xt` are not in these drafts: `target_words` may count their text while the shown
  text hides it, the F6 class, so keep them out of the vector drafts.) About 7 s in a dev build;
  keep it a default test.
- `crates/tc-usfm/tests/properties.rs`: proptest `woven_verses_show_the_words_of_their_plain_text`
  with a new strategy `note_verse()`: words from `WORDS` joined by gaps from `" "`, `", "`, `"—"`,
  `" {"`, `"} "`, `". "`, `" “"`, `"” "`, `" \\f + \\ft note\\f* "`, `"\\f + \\ft note\\f* "`,
  `"\\f + \\ft note\\f*,"`, `"\\f + \\ft a\\f*\\f + \\ft b\\f* "`, `"\\x - \\xt Rom 1:1\\x* "`,
  `"\\fig a|src=\"b\"\\fig* "`, `"\\va 2\\va* "`, `"\\ts\\* "`, `"\\rq Rom 1\\rq* "`,
  `"\\fe + \\ft n\\fe* "`, `"\n\\q2 "`, `", \\nd x\\nd* "`; ends `""`, `"."`, `".\n"`,
  `"\\f + \\ft end\\f*"`, `"\n\\p\n"`. With `slots = target_words(&verse)`, `groups =
  group_words(&slots, &runs, split)` (nested milestones up to three sources), `woven = weave(&verse,
  &groups)`: `words(shown_text_v1(&verse)) == slots` and `words(shown_text_v1(&woven)) == slots`
  (as triples). Use the file's `config(…)`. Never pair `\nd*` without `\nd` (a stray end marker
  hides the text after it, F6). Passes 30,000 cases with rule G; fails with PR #800 alone.

Commit message: the property, the CI check and its four named exceptions and why they stay.

### Step 5. tc-align: the bank holds every word, MAT 5:22 keeps its links, the siervo verse weaves

New `crates/tc-align/tests/shown_words.rs` (`mod common;` for `read`, `fixture`; read bundle
entries with `zip`; `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`):
- `a_tc4_imported_verse_with_nested_zaln_banks_every_word` (TC4-ISSUES row 2, "a tC4-imported
  verse with nested `\zaln`", and DELTA §3.4's "`TargetVerse` holds 10 words"): GEN 1:1 content
  from `fixtures/tc4/test-fixtures/en_ult/GEN-1-1.usfm`, inline as tC4 4.0 stores a book added
  from aligned USFM: `TargetVerse::new(&content)` text is the full sentence, its words are 10 in
  verse order, "the" 1..3 of 3, "heavens" present. No `Aligner::open` on it (D6). Also: the split
  plain (`split_aligned(&content, VerseNo(1))` is `Some`) opened with `SourceVerse::from_book` of
  `hbo_uhb/GEN-1-1.usfm` (7 Hebrew words) and
  `Basis::run(v1, v1, "dcs::unfoldingWord/hbo_uhb@v3.0.0".into(), None)` places 10 of 10 links and
  drops none (passes before too; documents the J5 bank).
- `a_verse_kept_as_written_shows_yahweh`: PSA 34:4 from `fixtures/scale/psa/en_ult/19-PSA.usfm`
  (byte-identical to the bundle's ULT PSA, sha256 `f3753ca7…abbe`): `split_aligned(&content,
  VerseNo(4))` is `None`; `TargetVerse::new(&content).text() == "I sought Yahweh, and he answered
  me,\nand from all of my fears he delivered me."`; its words hold ("Yahweh", 1, 1).
- `imported_mat_5_22_opens_with_all_44_links`: ULT MAT 5:22 (`en_ult/ingredients/MAT.usfm` in
  `bundle/en/en_ult_v91.sb.zip`) through `split_aligned(&content, VerseNo(22))`; source
  `SourceVerse::from_book(&Book::parse(UGNT MAT), 5, VerseSpan::single(22))` from
  `bundle/en/el-x-koine_ugnt_v0.34.sb.zip` entry `el-x-koine_ugnt/ingredients/MAT.usfm`; basis
  `Basis::run(v22, v22, common::SOURCE_NT.into(), None)`; `Aligner::open(Some(&value),
  TargetVerse::new(&plain), …)`: `value.placed() == 44`, aligner placed 44, `dropped()` empty,
  44 words, "brother" and "will" separate. Same assertions for MAT 5:44 (17) and 1TH 3:2 (24)
  in the same test. (Today 42/44, 2 dropped.)
- `a_footnote_right_after_a_word_banks_both_words_and_weaves` (TC4-ISSUES row 5): plain
  `"Pablo, siervo\\f + \\ft o esclavo\\f* de Dios."`; `TargetVerse::new(plain)` words equal
  `tc_usfm::target_words(plain)` (as `(text, occurrence, occurrences)`); build an
  `AlignmentValue` with one card per target word (any `SourceWord`, `span_end` 1, empty bank;
  build it field by field as `split_aligned` does, `tc-journal/src/alignment.rs:235-275`) and
  `tc_align::usfm::weave_verse(plain, &value)` is `Ok`, contains `\f + \ft o esclavo\f*` and
  `\w siervo|x-occurrence="1" x-occurrences="1"\w*` and `\w de|…`, and
  `tc_usfm::parse_aligned(&woven).plain == plain`. (Today `TargetWordNotFound("siervode")`.)
- `the_bank_is_the_export_word_list_for_every_note_draft`: for each note draft of
  `shown-text-vectors.json` with no word markup, `TargetVerse::new(c).words()` equals
  `target_words(c)` (covers the `TargetVerse` path of the step-4 check).
- `#[ignore]` `shown_text_census_of_the_bundle` (release, prints): per archive and book, verses,
  verses with markup, verses whose shown words differ from the raw `\w` words or from the split
  plain's, kept as written (`split_aligned` `None`) and how many of those lose words, split plains
  whose bank ≠ export (with references), `parse_aligned` failures. Used for the evidence file;
  `cargo test --release -p tc-align --test shown_words -- --ignored --nocapture`.

Commit message: #795's second test, #796's word list in tc-align, why no test opens inline text.

### Step 6. Design text, findings and evidence

- `docs/TC4-ISSUES.md` §1 (edit only these cells; sibling units edit neighbouring rows):
  - row 2 (#782), Fix: "Regenerate the vectors" → "Add tC4's vectors
    (`fixtures/usfm/shown-text-vectors.json`); the existing vectors do not change"; "Update
    STORAGE §11" → "Update STORAGE §7.7 ('Shown text and hashed text')".
  - row 3 (#795), Fix: add "and after every other marker the shown text leaves out (`\va`, `\vp`,
    `\ca`, `\rq`, `\cat`, `\ts\*`…), which PR #800 does not (declared difference, PLAN)".
  - row 5 (#796 word list), Regression test: the CI check runs on every plain fixture verse and
    every split plain verse of the ULT and UST v91, "except the verses whose text tC4's
    `verseText` hides (the items of a `\li` list; a `\qm1` line), which the check names".
  - after the "Other findings…" list, a short "Found by FIX-text (2026-10-19)" list with F6's
    three findings: (a) `\li` items and lines of markers usfm-js does not know are hidden from
    Check and the Align bank, though the export counts them; UST EXO 15:21's 12 imported links
    would drop at open and at the next verse save; recommendation: owner decision on showing that
    text (a declared difference) in a follow-up unit; (b) residual joins in text that still holds
    alignment markup (a note inside a run; unbalanced `\zaln-e\*`: ULT 2SA 23:8, UST 1KI 10:12,
    2SA 23:8), as in tC4; (c) a stray end marker hides the text after it, as in tC4.
- `docs/TC4-DELTA-2026-10-19.md`: §3.3 item 1 "§11" → "§7.7"; §3.4 item 2 add rule G; §3.4
  item 3: regenerating `check-text-vectors.json` changes no case, `verse-text-vectors.json` holds
  the hashed text and must not change, and tC4's new cases go into
  `fixtures/usfm/shown-text-vectors.json`; §3.4 item 4 last bullet: implemented as
  `every_genesis_and_psalms_verse_of_the_bundle_keeps_every_word`, ULT and UST; §3.1 item 2's C46
  bullet: add rule G.
- New `docs/evidence/shown-text-2026-10-19.md`: F1 (before) and F9 (after) numbers from the census
  (re-run it; cite the planner's before-numbers from this plan, measured at `c1bfa78`), the tC4
  parity result (0 of 47,723 bundle verses differ from tC4 5dcd147; how measured), F7/F8, F6's
  residual list, F10's timings re-measured (release: `shown_text_v1` over GEN+PSA best of 5,
  before via a `git archive c1bfa78` copy in your scratch dir, after in the worktree; and the
  `tests/psalms.rs` aligner line), the machine (`docs/evidence/linux-machine-2026-10-18.md`) and
  the commit.

Commit message: the design-text corrections and why each was wrong (row 5's check unsatisfiable;
§11 vs §7.7; verse-text vectors are the hash's), and the new findings.

### Step 7. Gates (before the last commit)

```sh
. ~/.tc-env && cargo fmt --all -- --check
. ~/.tc-env && cargo clippy --workspace --all-targets -j 6 -- -D warnings
. ~/.tc-env && cargo test --workspace -j 6
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 --features gallery -- -D warnings
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 --features app -- -D warnings
. ~/.tc-env && cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
```

tc-ui has the features `gallery` and `app` today (no `harness`). If `df -h /home` shows under 30 GB
free and no rustc runs in this worktree: `python3 -I /home/bot/tc-rst/.audit/tools/prune-target.py
/home/bot/tc-rst/.claude/worktrees/app-ftext/target` (78 GB free at planning time; the planner's
probes built into this worktree's `target/`).

---

## 4. "Done when" → tests

| Requirement | Test(s) |
|---|---|
| (a) nested words never dropped: GEN 1:1 | `shown_text_reads_words_inside_nested_alignments`, `a_tc4_imported_verse_with_nested_zaln_banks_every_word` |
| (a) PSA 34:4 kept as written | `a_verse_kept_as_written_shows_yahweh` |
| (a) Selah inside `\qs` | `selah_inside_qs_shows` |
| (a) 633 of 1,533 GEN verses → 0; every GEN/PSA verse of bundle/en keeps every word | `every_genesis_and_psalms_verse_of_the_bundle_keeps_every_word`, `every_verse_of_genesis_1_holds_its_words_in_file_order` |
| (a) nothing else moves | `a_verse_with_no_nested_alignment_keeps_its_one_level_text`, `footnotes_and_headings_stay_out_of_the_shown_text`, `the_check_text_is_what_tc4_reads` (unchanged), all existing tests |
| (b) no join across a footnote: MAT 5:22, 44 links | `a_footnote_between_words_keeps_its_space`, `a_note_of_any_kind_between_words_keeps_its_space`, `imported_mat_5_22_opens_with_all_44_links` |
| (b) … or other skipped marker | `any_skipped_marker_between_words_keeps_its_space`, `woven_verses_show_the_words_of_their_plain_text` |
| (c) bank list = export list ("siervode") | `a_footnote_right_after_a_word_banks_both_words_and_weaves`, `the_bank_and_the_export_split_every_plain_verse_into_the_same_words`, `the_bank_is_the_export_word_list_for_every_note_draft` |
| tC4's vectors ported | `the_shown_text_is_what_tc4_reads` |
| linear time, no depth limit, no endless walk | `derivations_take_time_linear_in_the_verse` (extended), `a_verse_of_several_megabytes_takes_seconds`, `the_shown_text_has_no_depth_limit`, `a_shown_walk_that_never_ends_is_a_throw` |

---

## 5. Measurements to take

1. The census (step 5) after the change; before-numbers are F1 (re-run them only if you doubt
   them: a `git archive c1bfa78` copy in your scratch dir with the census test added, built with
   `CARGO_TARGET_DIR` pointing at this worktree's `target`).
2. tC4 parity over the whole bundle: the planner's dump script
   `…/scratchpad/ftext-tools/dump-all.mjs <tc4 tree> <out.json> <usfm files…>` plus the probe
   `probe_vs_tc4` in `…/ftext-probe-ws/crates/tc-align/tests/zz_probe.rs`; expect 0 of 47,723.
   Record it; do not commit the 6 MB dump.
3. Timings of F10 (release, best of 5; rule 5 of PERFORMANCE.md asks 20 runs only for budgets,
   and no budget changes here).

---

## 6. Traps

1. **Inline vs split.** On split plain text the old walk already shows nested words (nothing is
   nested any more), so a GEN 1:1 or Selah test on split text passes before the fix and proves
   nothing. #782's tests must use the inline content. #795's MAT 5:22 shows on split plain text.
2. **Do not open inline content in the aligner** (D6, #794).
3. **`verse-text-vectors.json` is the hash.** Regenerating it with a changed generator would move
   `plain_text_v1`'s vectors. Leave it alone (F5).
4. **The check-text generator breaks at 9ad4d5c** unless `tokenizeVerse`/`tokensText` are passed
   into its `new Function` (step 3).
5. **The check-text generator cannot hold tC4's Selah case**: tC4's `indexBook` skips `\q1 \v 1`
   (PSA-3's verses). That is why the new generator walks every usfm-js key, and why the codec's
   keys must equal usfm-js's keys in `the_shown_text_is_what_tc4_reads` (they do on the bundle).
6. **JS semantics in the port**: `text != null` means "present" (an empty string counts and is
   pushed, so its children are not walked); `children` uses `??` in the zaln branch and
   truthiness elsewhere; a string `children` (a word attribute named `children`) gives nothing;
   `last`/first are UTF-16 code units; `$` in `NOTE_TAG` matches only at the end. Word attributes
   can overwrite `type`, `tag`, `text`, `children` on a word object (`set_attribute`); read them
   from the heap, never assume.
7. **Per-verse state**: reset `gap` and `last` for each verse of `parsed.verses`, join, trim once.
8. **Proptest generator**: never emit `\nd*` without `\nd`, and no `\li`/`\qm` (F6); persisted
   failures land in `tests/properties.proptest-regressions` (commit a regression only if you fix
   a real bug it found).
9. **Merge friendliness**: sibling fix units edit `docs/TC4-ISSUES.md` rows 1, 4, 6, 7…, `docs/PLAN.md`'s
   difference list, and `crates/tc-align/tests/aligner.rs`. Touch only the cells named in step 6,
   add PLAN's paragraph in one place, and keep tc-align tests in the new file.
10. **Clippy**: workspace lints warn on `unwrap`, `expect`, `panic` in library code (`-D warnings`
    makes them errors); test files need the file-level `#![allow(…)]` for helpers outside
    `#[test]` functions (pattern in every existing test file).
11. **`tc_usfm::TargetWord` (`text`) vs `tc_journal::record::TargetWord` (`word`, `extra`)**:
    compare as `(text, occurrence, occurrences)` triples.
12. **The task text is inaccurate in two places**: the shown text is not a STORAGE §12.1
    derivation (F4), and tC4's `node_modules` are not in the tC4 checkout (F11). Say both in the
    final answer.

---

## 7. Design-text fixes (made in step 6; list them in commit messages and the final answer)

- TC4-ISSUES §1 row 5's CI check ("every fixture verse with no word markup") cannot pass: three
  fixture verses hold `\li` items that tC4's `verseText` hides (F6). Narrowed with named
  exceptions; the hide is filed as a finding.
- TC4-ISSUES row 2 and DELTA §3.3 item 1 point at STORAGE §11; the paragraph is in §7.7.
- DELTA §3.4 item 3 asks to regenerate `verse-text-vectors.json`; that file is `plain_text_v1`'s
  text, which this change must not touch; regenerating `check-text-vectors.json` changes no case.
- TC4-ISSUES row 3, DELTA §3.1 item 2 and §3.4 item 2 limit the separator to `\f*`/`\x*` (PR #800);
  the unit's spec ("or other skipped marker") and F7 require rule G.

---

## 8. STORAGE-DELTA notes (for the final answer; do not edit STORAGE.md)

For `docs/STORAGE.md` on the storage branch (and its copy on main, line ~2761):

- §7.7, "Shown text and hashed text": replace "tC4's `verseText` of `src/data/bookModel.js:15`
  (through `verseTextIndex`, `src/state.jsx:352`, at d310bf3) … joins each verse object's `text`,
  or else the `text` of its children one level down, trimmed with JavaScript's `trim`" by: "tC4's
  `verseText` of `src/data/bookModel.js` as tC4 9ad4d5c has it (#782): it parses `"\v 1 " +
  content` with usfm-js 3.4.3 as `plain_text_v1` does and walks the verse objects as
  `tokenizeVerse` (`src/data/sourceHighlight.ts`) does, at any depth: the text of every `\w` word
  under `\zaln` milestones nested to any depth and of every other object with text, a section
  heading's own text left out and its children walked, footnotes left out; joined and trimmed
  with JavaScript's `trim`. Where a footnote or any other object the walk leaves out stands
  between two characters that are not whitespace, the character usfm-js keeps after its end
  marker (`nextChar`) is put back, so the words on either side never join (tC4 PR #800 does this
  for notes, cross references and figures; the rebuild for every object the walk leaves out, a
  declared difference). Where usfm-js keeps no `nextChar` (the end marker is followed by
  anything but a space or a line break) and the characters on either side would read as one
  word or number to tC4's tokenizer, a space is put back (a declared difference: tC4 shows
  "siervode"). The children of an object with a text are walked after its text, so the words
  of a character span after a span nested in it are shown (a declared difference: tC4 hides
  them). The walk has no depth limit; where usfm-js or the walk throws (a
  `children` that is neither a list nor a string, a text that is not a string, an object inside
  itself), the value is the content trimmed. So a Psalm's superscription, its `\qs` Selah, also
  when aligned inside `\qs … \qs*`, and every word of a nested alignment (Genesis 1:1 'the
  heavens') are words to check and to align, while a footnote, the items of a `\li` list and the
  text usfm-js keeps for a marker it does not know are not shown, as in tC4."
- Same paragraph, add: "The shown text is not one of `format.json`'s derivations: no fold output
  reads it (alignment validity is the hash of `plain_text_v1`), so changing it raises no version.
  It decides which words a new alignment holds and which links the reflow of a verse save keeps,
  so it is settled before format 1.0. If the review rule of #796 makes the fold compare an
  alignment's words with the verse's, the word list becomes a derivation and needs a
  `format.json` entry."
- §16.5 item 4: after `check-text-vectors.json`, add "and `fixtures/usfm/shown-text-vectors.json`
  (tC4's `verseText` at PR #800's head 5dcd147, with tC4 main 9ad4d5c beside it, over tC4's aligned
  fixtures, drafts with notes and other markers between words, and every verse of the ULT and UST
  v91 Genesis and Psalms)".

---

## 9. TC4-ISSUES / DELTA items that name this unit's code

Handled: §1 row 2 (#782, all three named tests plus the tC4-imported verse), row 3 (#795, both
tests), row 5 (#796 word list: the tc-align test and the CI check); §2 rows #796 (word list) and
#782; §3 row #782; DELTA §3.4 items 1-4; §4 "tc-usfm (now)" (`bookModel-nested-alignment.test.ts`
ways 1-4 ported; its I-3 stamp case does not apply, because the rebuild never stamps the shown
text, STORAGE §7.7); §3.3 item 1 as STORAGE-DELTA notes; §3.1 item 2's C46 bullet and #782/Q32
in PLAN.

Not handled, with reasons (say so in the final answer):
- §2 #795 "one hash" (covered) and its to-add `an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen`:
  tc-store, the storage team's crate; the behaviour already holds.
- §1 row 6 (#796 review rule), row 7 (#794), rows 4, 8-10: tc-align/tc-store behaviour owned by
  other units.
- §3.12.2 item 4 / C45 J5 (the GEN 1:1 word bank in the app): B6's Playwright journey; the word
  list it needs is proved by `a_tc4_imported_verse_with_nested_zaln_banks_every_word`.
- §3.10 item 4 and §3.3 item 3 (C45 in the import): unit I.
- The F6 findings: reported, not fixed (D4).

---

## 10. Final answer checklist

Plan path; commits; the numbers of F9 as re-measured; the declared differences to tell the owner
(rule G; #782 ahead of the reference, Q32); the design-text fixes of §7; the STORAGE-DELTA notes of
§8; the findings of F6 with the recommendation; the two inaccuracies of the task text (§6 trap
12); the items of §9 not handled and why.

---

## 11. Review round 1 (2026-10-19)

Two majors, fixed in `shown.rs` with regression tests that fail on the walk before the fix:
words joined across a skipped marker followed by a letter with no space (usfm-js keeps no
`nextChar`), and the words of a character span after a nested span were hidden (rule 4 did not
walk children). The four minors (a tC4-saved joined word losing its link at reflow, paragraph
markers right after a word, hidden-text markers such as `\ndx`, PLAN rulings the parity parser
cannot cite) were left for the merge lane's minors list. The half-done merge of `tc4fix` was
aborted; `claude/focused-ride-5uzj7p` (a52bc0c) was merged instead, cleanly.
