# Plan for fix unit FIX-helps: the reordered-quote title (#773) and the tW-only pin change (#661)

Planner's revision 1, 2026-10-19. Commit this file with your first commit; it is the only file
the planner wrote.

Worktree `/home/bot/tc-rst/.claude/worktrees/app-fhelps`, branch `app-fhelps`, cut from `tc4fix`,
which was created for this run from `claude/focused-ride-5uzj7p` at `c1bfa78` ("Map tC4's newer
changes and issues onto the rebuild"). Before you start: `git -C /home/bot/tc-rst log --oneline -3
tc4fix`; if `tc4fix` moved (another FIX unit merged), merge it into `app-fhelps` first.

Every shell command that runs cargo, rustc or node starts with `. ~/.tc-env && `. Use `-j 6`.
Use only this worktree's `target/`. The planner left a scratch build in `target/scratch-hl/`
(1.6 GB); delete it with `rm -rf target/scratch-hl` before your first build if disk is tight
(`df -h /home`; prune with `python3 -I /home/bot/tc-rst/.audit/tools/prune-target.py
/home/bot/tc-rst/.claude/worktrees/app-fhelps/target` only below 30 GB free and with no rustc
running here). Never touch branch `storage`, `.claude/worktrees/storage`, other worktrees or
`refs/wip/*`. Tests never touch the network.

## 0. What this unit is

A fix unit, not a design unit: `docs/TC4-ISSUES.md` is its spec. Its two rows:

- **§1 row 1, tC4 #773** (tc-helps `highlight_quote`, used by `CheckItem::title`): when the ULT
  orders a multi-word quote's words differently from the Hebrew or Greek, the strict match finds
  no candidate and today's fallback lights *every* alignment group that names *any* quote word,
  so a repeated אֶת, כָּל or καὶ pulls words from elsewhere in the verse into the check's title
  and the ULT-pane highlight. Port tC4's `shortestRunInstances` (tC4 `src/data/sourceHighlight.ts`
  at `9ad4d5c`, lines 171-206) into the multi-word fallback. Regression test
  `a_reordered_quote_lights_only_its_own_phrase` (tC4's five Genesis vectors on `GEN-773.usfm`,
  plus TIT 2:2 "in love").
- **§1 row 11, tC4 #661** (tc-check `change::consequences`): changing only the en_tw pin (v90 to
  v91) when a decided Words row (2SA `vwrw`, `kt/arcofthecovenant`) loses its article reports
  "unaffected 1, affected 0", so the upgrade dialog says nothing changes while the check leaves
  the list. Give `consequences` each side's tW article set and count a decision whose check
  leaves the list. Regression test `a_tw_only_change_that_drops_a_decided_article_is_counted`,
  with a no-change control.

### Items of TC4-ISSUES.md and TC4-DELTA-2026-10-19.md that name this unit's code

| Item | Status there | This unit |
|---|---|---|
| #773 / C39 (tc-helps; tc-check) | rebuild bug | **Fixed** (steps 2-4). tc-check needs no code change: `CheckItem::title` (`crates/tc-check/src/item.rs:301-307`) passes the quote string to `gateway_quote` and maps occurrence 0 to `NotTied`, as tC4's `gatewayTitleFor` does. |
| #661 (tc-check; C8, B5a) | rebuild bug | **Fixed** in tc-check (step 5). Calling `consequences` with the article sets and wording the dialog is C8's; listing the off-list decision is B5a's (`Session::off_list` already holds it). |
| #765 / C34 and #664 (tc-helps) | covered, test "to add" | **Test added** (step 3, `genesis_maqaf_quotes_title_in_the_ult`) and the whole-Genesis "no Hebrew title" count (step 4). No code change: today's code already gives all five maqaf titles (verified). |
| #769 / C43 (tc-check, tc-helps, B5a, B7) | reference change | **Not handled.** It replaces "By category" with "By figure" on the Notes rail, which the d310bf3 baseline screens do not show, and needs catalog keys (`check.sort.byFigure`, `check.foldAll`, …) that are vendored only when the reference moves (DELTA §3.9). It belongs with B5a/B7 and the reference move. |
| #711 (tc-helps, tc-resources, tc-check) | owner question | **Not handled**: Q33 has no answer yet. |
| #768 / C35 (reuse `normalize_word`) | unit todo for B6 | **Not handled** (B6's label code). `normalize_word` stays public and unchanged. |
| #594, #580, #643, #646, #648 (tc-check) | covered | Nothing to do; their tests must stay green. |

### What the planner verified by running code (scratch crates outside the worktree)

- Today's rebuild gives the wrong title for 4 of tC4's 5 Genesis vectors (zczu is right):
  mix3 33:5 "his eyes … the women … whom God has graciously given to your servant", gyfv 24:48
  "Yahweh … the daughter of my master s brother", r2u9 9:11 "that never again will all flesh be
  cut off … and never", h9hm 49:6 "Do not let … enter into their council … not". A port of
  `shortestRunInstances` gives all five right, on tC4's extract and on the full v91 `GEN.usfm`.
- Mutations: dropping the per-word count (need 1 instance per distinct word) breaks only zczu
  ("and Lot Haran s son his son"); measuring the run (`end - start`) instead of the marked groups
  breaks only h9hm ("Do … let … enter into their council … not"). mix3, gyfv and r2u9 guard the
  shortest run itself. Replacing the strict `<` with `<=` (last of equally short runs) changes
  PSA 63:1 ovd5 from tC4's "for you my flesh faints" to "my flesh faints for you": that is tC4's
  "one known tie" (DELTA §3.5 item 2); keep tC4's answer.
- **Differential check against tC4's own code:** tC4 9ad4d5c's `gatewayQuote` (run in Node with
  usfm-js 3.4.3) and the port agree on **all 27,049** quoted rows of en_tn and en_twl v91 for
  Genesis (5,758 tN, 5,640 TWL) and Psalms (7,921 tN, 7,528 TWL) against en_ult v91, and en_tn v91
  Titus (202) against the ULT v89 the parity baseline shows. Today's code differs on 126 of them
  (93 Genesis tN, 26 Psalms tN, 7 Titus tN, 0 TWL). No row of any table has a null title (no
  Hebrew fallback; #765 at d310bf3 had 95 tN and 260 tW Hebrew titles in Genesis).
- Of the 2,224 Genesis tN *checks* (rows with a SupportReference and a quote), 832 take the
  fallback and 38 change title: exactly the 38 rated `Approximate`, the number TC4-ISSUES gives.
  `MatchQuality` tallies do not change anywhere (the quality rule stays), so
  `real_notes_on_titus_light_their_rendering_in_the_ult` (195 Exact, 7 Approximate) stays green.
- Speed (release, this PC, verses parsed beforehand, titles of every tN and TWL row): Psalms
  15,449 rows: today 143 ms, a straight port 229 ms, the port with each source word normalized
  once per verse (step 2) 55 ms; Genesis 11,398 rows: 135 / 228 / 42 ms. The keyed port's output
  is identical to the straight port on every row. The Check screen's whole open budget is 250 ms
  (PERFORMANCE.md), and the rail titles every row, so the straight port alone would spend most of
  it on the reference laptop: do step 2.
- #661: with the prototype of step 5, the 2SA scenario gives `affected = [{Words, 2SA,
  checked_against: en_twl v91, decisions: 1, articles_left: 1, blocked: false}]`, unaffected 0;
  the carry-over plan on the gated v91 list makes `vwrw` `Unplaced` and the control `Stays`, and
  `summary(Some(&twl))` gives carried 1, invalidated 1, undecided 3,314. Today's code gives
  unaffected 1, affected 0.
- Baseline before any change: `cargo test -p tc-helps -p tc-check` passes (highlight.rs 15 tests
  in 0.33 s, carry.rs 16).

## 1. Order of work and commits

Each step ends in a commit that builds and passes its tests. Write each regression test first and
see it fail on today's code (record the failing titles or counts in the commit message).

1. Step 1 (with step 3's commit): vendor `GEN-773.usfm` and `GEN-1.usfm`.
2. Step 2, commit "tc-helps: normalize each source word once per verse": no behavior change.
3. Step 3, commit "tc-helps: a reordered quote lights only its own phrase (tC4 #773)".
4. Step 4, commit "tc-helps: every gateway title of Genesis, Psalms and Titus is tC4 9ad4d5c's".
5. Step 5, commit "tc-check: a tW-only pin change counts the decisions whose article leaves
   (tC4 #661)".
6. Step 6, commit "docs: …" (or fold each doc edit into the step it belongs to).
7. Step 7: the gates, before the last commit.

End every commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` (or the
attribution your own system reminder gives).

## 2. Step 1: vendor tC4's two Genesis extracts

Copy byte for byte from tC4 commit `9ad4d5c506466819e5b10f255bbb4a36d394e498` (the checkout at
`/home/bot/unfoldingword/translationcore4` is at d310bf3 but holds that commit's objects; it is
read-only, so use `git show`, never checkout):

```sh
T=/home/bot/unfoldingword/translationcore4
W=/home/bot/tc-rst/.claude/worktrees/app-fhelps
git -C $T show 9ad4d5c:test/fixtures/en_ult/GEN-773.usfm > $W/fixtures/tc4/test-fixtures/en_ult/GEN-773.usfm
git -C $T show 9ad4d5c:test/fixtures/en_ult/GEN-1.usfm   > $W/fixtures/tc4/test-fixtures/en_ult/GEN-1.usfm
sha256sum $W/fixtures/tc4/test-fixtures/en_ult/GEN-*.usfm
# GEN-773.usfm a2a247ff5e2a5522d35d0abb386b4f2c4e835785861e97507097d1b63ecbc55c (208 lines; blob 06fb7c2)
# GEN-1.usfm   fd7610d26c50184ce11662491330ff7f3bdc4ab473db26c8f443fd72b8a6915b (819 lines; blob 972eb91)
```

- `fixtures/tc4/test-fixtures/README.md`: add tC4's two table rows **verbatim** from
  `git -C $T show 9ad4d5c:test/fixtures/README.md` (the `en_ult/GEN-1.usfm` row right after the
  `en_ult/TIT.usfm` row, the `en_ult/GEN-773.usfm` row after the `hbo_uhb/JON.usfm` row, which is
  tC4's order without the rows of files not vendored here). Add one sentence after the table:
  "`en_ult/GEN-1.usfm` and `en_ult/GEN-773.usfm` were copied from tC4 commit 9ad4d5c on
  2026-10-19. tC4's `test/sourceHighlight.test.ts` (issues #765 and #773) reads them, and so does
  tc-helps' `tests/highlight.rs`."
- `fixtures/README.md`, section `tc4/`: after "Copied from translationCore 4 at commit `d494a9d…`
  on 2026-10-14." add "Later copies name their commit in their own README row:
  `tc4/test-fixtures/en_ult/GEN-1.usfm` and `GEN-773.usfm` come from `9ad4d5c` (2026-10-19)."
- Licence: en_ult is CC BY-SA 4.0, already covered by `fixtures/tc4/LICENSE-CONTENT.md`.
- **Trap (merge):** unit FIX-text (#782) is likely to vendor `GEN-1.usfm` (and `GEN-1-1.usfm`,
  `PSA-3.usfm`) from the same commit. An identical file added on both branches merges cleanly;
  the README rows may conflict as adjacent edits. Copying tC4's rows verbatim, in tC4's order,
  keeps that conflict trivial.

## 3. Step 2: normalize each source word once per verse (no behavior change)

File `crates/tc-helps/src/highlight.rs`. Today `has` and every `instances_of` call run
`normalize_word` (NFC plus a trim, one allocation) on every source of every group for every quote
word and every anchor (lines 241-246, 274-280, 404-422), and `Instance = (String, u32)` clones the
content. The fallback of step 3 multiplies those calls. Change:

- `AlignedVerse` (line 64-68): add a private field `keys: Vec<Vec<String>>`, parallel to
  `alignments`: `keys[i][j] == normalize_word(&alignments[i][j].content)`. Fill it in
  `add_alignment` (line 76-79) before pushing `sources`. The derives stay (`Default`, `PartialEq`
  stay correct: `keys` is a function of `alignments`). Nothing outside the module builds an
  `AlignedVerse` literally (grep: only `tests/highlight.rs` and `tc-check` use the type, through
  `new`, `add_alignment`, `push_word`, `push_text`).
- `struct Group<'a>`: add `keys: &'a [String]`; `groups()` sets it from `verse.keys[id.0]`
  (`id` is an `AlignmentId` that `push_word` checked, so the index exists; prefer `verse.keys.get(id.0)`
  and skip like the `sources` lookup does, to keep the code free of index panics).
- `type Instance<'a> = (&'a str, u32)` (content borrowed from the verse; the identity is the raw
  `content` and the occurrence, as tC4's `${content}#${occurrence}`).
- Free functions instead of the closure at lines 274-280:

```rust
fn has(group: &Group<'_>, word: &str, occurrence: Option<u32>) -> bool {
    group.keys.iter().zip(group.sources)
        .any(|(key, s)| key == word && occurrence.is_none_or(|o| s.occurrence == o))
}

/// The instances of `word` among `group`'s original words.
fn instances_of<'a>(group: &Group<'a>, word: &str) -> impl Iterator<Item = Instance<'a>> {
    group.keys.iter().zip(group.sources)
        .filter(move |(key, _)| *key == word)
        .map(|(_, s)| (s.content.as_str(), s.occurrence))
}

/// True when `group` renders one of `instances`.
fn renders(group: &Group<'_>, instances: &BTreeSet<Instance<'_>>) -> bool {
    group.sources.iter().any(|s| instances.contains(&(s.content.as_str(), s.occurrence)))
}
```

  (`instances_of` may return a `Vec` if the borrow checker fights the `move` closure.)
- `Candidate<'a> { instances: BTreeSet<Instance<'a>>, span }`; `tightest` becomes a nested
  `fn tightest<'a>(class: &[Candidate<'a>]) -> Option<BTreeSet<Instance<'a>>>` (a closure with
  `'_` in its signature does not compile: "lifetime may not live long enough"), called as
  `.and_then(|c| tightest(c))`. The lit loop at lines 356-363 uses `renders`; `unambiguous`
  (lines 408-422) counts `instances_of(g, word)` over the groups.
- No public signature changes. `normalize_word`, `quote_words`, `highlight_plain`,
  `tokenize_plain` are untouched.

Proof: the existing 15 tests of `crates/tc-helps/tests/highlight.rs` and all of tc-check pass
unchanged. Measure (see §8) and put today's and the new time in the commit message.

## 4. Step 3: the shortest run (tC4 #773)

### 4.1 Code (`crates/tc-helps/src/highlight.rs`)

In `highlight_quote`, the `None =>` arm (lines 366-386) becomes tC4's fallback
(`matchQuote`, sourceHighlight.ts:294-301 at 9ad4d5c):

```rust
None => {
    let exact = match occurrence {
        Occurrence::Nth(n) if words.len() == 1 => Some(n.get()),
        _ => None,
    };
    if words.len() > 1 {
        let run = shortest_run(&groups, &words);
        for g in &groups {
            if renders(g, &run) {
                lit.extend(g.tokens.iter().copied());
            }
        }
    } else {
        // One word: its own occurrence numbers (never occurrence 1's words for 2).
        for g in &groups {
            if has(g, first, exact) {
                lit.extend(g.tokens.iter().copied());
            }
        }
    }
    // quality: unchanged (WordOccurrence / Exact when first_occurrence && unambiguous /
    // Approximate)
}
```

`words` here is the list after `quote_words` (maqaf compounds split, `&` and `…` dropped), as
tC4's `quoteToWords`; a single maqaf compound is therefore a multi-word quote, as in tC4. The
strict path (candidates, classes, tightest) and the `chosen` match are unchanged.

```rust
/// The original-word instances of the shortest run of groups, in gateway order, that names
/// every quote word the verse has, each as many times as the quote names it, capped by the
/// instances the verse has; a word the verse lacks drops out (tC4 `shortestRunInstances`,
/// #773). A run is measured over every group its instances are rendered by, not over the run:
/// en_ult GEN 49:6 renders תָּבֹא#1 as "Do … let … enter", so a run from "enter" only looks
/// short. Of equally short runs the first wins, as in tC4 (PSA 63:1 ovd5 is such a tie).
fn shortest_run<'v>(groups: &[Group<'v>], words: &[String]) -> BTreeSet<Instance<'v>>
```

Algorithm, exactly tC4's (do not "improve" it; the differential test of step 4 holds it to tC4):

1. `need: BTreeMap<&str, usize>`: for each quote word, how many times the quote names it. Then
   for each word, `in_verse` = the number of *distinct* instances of it over all groups; drop the
   word if 0, else `need = min(need, in_verse)`.
2. For each `start` whose group holds any word of `need` (`has(.., None)`): `seen:
   BTreeMap<&str, BTreeSet<Instance>>`, one empty set per needed word; walk `end` from `start` to
   the last group, adding every instance of each needed word in group `end` to its set; at the
   first `end` where every word has `seen[w].len() >= need[w]`:
   - `instances` = the union of **all** seen sets (tC4 keeps every seen instance, even beyond the
     count: do not cap it);
   - `marked` = the indices of the groups (all of them, not only `start..=end`) that `renders`
     any of `instances`; `extent = last(marked) - first(marked)` (non-empty: group `start` holds
     a seen instance; get first/last without `unwrap`, e.g. `let (Some(a), b) = (it.next(),
     it.last()) else { continue }`);
   - replace `best` only when `extent` is **strictly** smaller; then stop walking `end` (`break`).
3. Return `best`'s instances, or an empty set (then nothing is lit and `highlight_quote` returns
   `Highlight::none()` through its existing `lit.is_empty()` check).

The same fallback applies to every occurrence kind (`Nth` with too few classes, `NotTied`, `All`
and `Unrecorded` with no class), as in tC4, which reads -1, 0 and an empty cell as the first
occurrence. Do not keep "every group" for `All`: it is what let a repeated word pull in other
clauses. Checks never carry `All` (`SkipReason::EveryOccurrence`).

Docs to change in the same commit (each says something false today):
- `highlight_quote`'s doc, last paragraph (lines 263-267): it says the fallback lights every group
  and "tC4 lights the same words in both cases". Say instead: a one-word quote resolves through
  its occurrence numbers; a longer quote lights the shortest run ([`shortest_run`], tC4 #773),
  because lighting every group that renders any quote word let a repeated אֶת or כָּל pull in
  words from elsewhere (en_tn GEN 33:5 mix3); that is `Exact` when the verse holds each quote word
  exactly as often as the quote does, else `Approximate`.
- `MatchQuality::Approximate` (lines 135-137): "The quote did not match in the original order, so
  the shortest run of groups that renders every quote word the verse has is lit (tC4 #773).
  Visible, but not a confirmed rendering."
- `MatchQuality::Exact` may stay; the `highlight_quote` doc explains the unambiguous case.

### 4.2 Tests (`crates/tc-helps/tests/highlight.rs`)

Add to `tests/common/mod.rs` a reader for the bundled exports (zip is already a dev-dependency):

```rust
/// A text file of the repository: `bundle/en/<export>.zip:<entry>` reads one entry of a burrito
/// export, any other path a file. Paths are relative to the repository root.
pub fn repo_text(source: &str) -> String
```

(`CARGO_MANIFEST_DIR/../..` is the root; split on `".zip:"`; `zip::ZipArchive::by_name`; panic
with the path on failure. The file allows `clippy::panic`; `expect` is **not** allowed in
`tests/highlight.rs` (`#![allow(clippy::unwrap_used, clippy::panic)]` only) — use `unwrap` or
`unwrap_or_else(|e| panic!(..))`, or extend the allow line.)

**`a_reordered_quote_lights_only_its_own_phrase`** (the TC4-ISSUES name):
- Verse: a `gen773(chapter, verse)` helper like `ult()` but on
  `fixture("tc4/test-fixtures/en_ult/GEN-773.usfm")` (each verse starts `\v N ` after its `\c N\n`
  line; the existing `aligned()` converter handles the nested `\zaln` of these verses).
- Quotes and occurrences: the real rows of en_tn v91 Genesis, read with
  `parse_notes(&repo_text("bundle/en/en_tn_v91.sb.zip:en_tn/ingredients/GEN.tsv"),
  TsvScope::Book(BookId::Genesis))` and found by `upstream_row_id()`. They equal tC4's test
  literals byte for byte (verified, U+2060 word joiners included), so no Hebrew is retyped.
  `h9hm` is a plain note (no SupportReference); `parse_notes` keeps it.
- Assert `gateway_quote(&verse, &note.quote, note.occurrence)` equals:

| id | verse | title (tC4 9ad4d5c) | guards |
|---|---|---|---|
| mix3 | 33:5 | `whom God has graciously given to your servant` | the shortest run (today: "his eyes … the women … whom God …") |
| gyfv | 24:48 | `the daughter of my master s brother` | the shortest run (today: "Yahweh … the daughter …") |
| r2u9 | 9:11 | `that never again will all flesh be cut off` | the shortest run (today: "… cut off … and never") |
| zczu | 11:31 | `and Lot Haran s son his son s son` | the per-word count: בֶּן twice |
| h9hm | 49:6 | `Do not let … enter into their council` | the extent over every group the instances expand to |

  ("master s", "Haran s": the converter and tC4 both make `'s` a separate word token, and the
  title joins words with spaces.)
- Also assert `highlight_quote(..).quality == MatchQuality::Approximate` for mix3 (it took the
  fallback; the verse repeats אֶת).
- TIT 2:2: `gateway_quote(&ult(2, 2), &Quote::new("τῇ ἀγάπῃ"), Occurrence::nth(1).unwrap())` is
  `Some("in love")` (the strict path's tightest class still runs before any fallback).
- Without step 3's fallback, four of the five fail (see §0): note the failures in the commit.

**`genesis_maqaf_quotes_title_in_the_ult`** (C34, #765, #664; passes today, pins it):
- Verse from `fixture("tc4/test-fixtures/en_ult/GEN-1.usfm")` (chapter 1 only).
- Rows by id from the v91 tables: tN `xxe6` 1:5 → `Then there was evening and there was morning`;
  tN `xfx7` 1:7 → `And it was so`; TWL `vzz2` 1:28 (`parse_word_links` on
  `bundle/en/en_twl_v91.sb.zip:en_twl/ingredients/GEN.tsv`, quote `orig_words`) → `the earth`;
  controls without a maqaf: tN `c9yz` 1:5 → `And God called`, tN `xmx5` 1:5 → `one day`.
- Assert each quote of the first three contains `'\u{05BE}'` so the test keeps testing the maqaf.
- The whole-Genesis count of Hebrew titles that must be 0 is in step 4's test (the Genesis tables
  have no null title).

**Property** (extend the proptest block; it passes on today's code too, and guards the new run
against losing or inventing words): `a_highlight_lights_only_quote_words_and_each_one_the_verse_renders`,
same generators as `highlights_are_sorted_word_tokens`. With `qw` = the quote's words as
`quote_words` makes them (re-derive in the test: split on whitespace and U+05BE, drop `&` and
`…`, `normalize_word`, drop empty):
- every lit token's alignment holds a source whose `normalize_word(content)` is in `qw`;
- unless the quality is `None` or `WordOccurrence`, every word of `qw` that some aligned word
  token's alignment holds is held by some lit token's alignment.
The planner ran both over 200,000 random verses on the port (130,651 took the shortest run):
no failure.

Existing tests: all 15 stay as they are (their expected values do not change). The comment in
`real_notes_on_titus_light_their_rendering_in_the_ult` ("The rest repeat a word (καὶ) …") stays
true.

## 5. Step 4: the differential test against tC4's own code

The lever that proves the port, not just five rows: tC4 9ad4d5c's `gatewayQuote` on every quoted
row of five real tables, committed as vectors, and a test that the rebuild gives the same title for
each.

### 5.1 Generator `crates/tc-helps/tools/gen-gateway-title-vectors.mjs`

The planner wrote and ran this script (4.6 s; Node v22.23.3; output 1.27 MB, 27,049 rows, the
same order of size as `fixtures/check/psalms-vectors.json` and `fixtures/usfm/weave-vectors.json`).
Commit it as written:

```js
// Generates fixtures/helps/gateway-title-vectors.json: the gateway title tC4 gives every
// quoted Translation Notes and Translation Words row of Genesis, Psalms and Titus.
//
// The title is tC4's `gatewayQuote(tokenizeVerse(verse), quote, occurrence)`
// (src/data/sourceHighlight.ts at 9ad4d5c, which holds the #765 maqaf split and the #773
// shortest run): the ULT words that render the quote, in verse order, with `…` across a
// gap, or null when nothing renders it. A tN quote goes in as tC4's `tnQuoteWords`, a
// TWL quote as its string, as tC4's Check screen passes them (`gatewayTitleFor`).
//
// A row is every TSV line after the header whose Reference is `<chapter>:<verse>` in
// digits, whose quote cell (tN `Quote`, TWL `OrigWords`) is not empty, and whose verse
// usfm-js finds in the ULT. Inputs: the en_ult, en_tn and en_twl v91 exports in
// bundle/en (Genesis and Psalms), and for Titus the ULT v89 and en_tn v91 that the parity
// baseline shows (fixtures/tc4/test-fixtures).
//
// sourceHighlight.ts imports nothing, so the script takes it from the commit with
// `git show` and runs it as written (Node 22.18 or later runs TypeScript). Run from the
// repository root:
//   node crates/tc-helps/tools/gen-gateway-title-vectors.mjs <tC4 repository> <node_modules> \
//     > fixtures/helps/gateway-title-vectors.json
// <tC4 repository> needs commit 9ad4d5c in its history (its checkout may be at any commit);
// <node_modules> must hold usfm-js 3.4.3 and fflate 0.8.2, the versions tC4 pins.
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const COMMIT = "9ad4d5c506466819e5b10f255bbb4a36d394e498";
const FILE = "src/data/sourceHighlight.ts";
const [tc4, nm] = process.argv.slice(2);
if (!tc4 || !nm) throw new Error("usage: gen-gateway-title-vectors.mjs <tC4 repository> <node_modules>");
const pinned = { "usfm-js": "3.4.3", fflate: "0.8.2" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const require = createRequire(import.meta.url);
const usfm = require(path.join(nm, "usfm-js"));
const { unzipSync, strFromU8 } = require(path.join(nm, "fflate"));

const git = (...args) => execFileSync("git", ["-C", tc4, ...args], { encoding: "utf8" });
const blob = git("rev-parse", `${COMMIT}:${FILE}`).trim();
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "gateway-titles-"));
const copy = path.join(dir, "sourceHighlight.ts");
fs.writeFileSync(copy, git("show", `${COMMIT}:${FILE}`));
const { gatewayQuote, tokenizeVerse } = await import(pathToFileURL(copy).href);
fs.rmSync(dir, { recursive: true });

/** tC4's `tnQuoteWords` (src/data/derive.ts at 9ad4d5c), copied verbatim. */
const tnQuoteWords = (quote) => {
  const seen = {};
  return quote
    .split(/\s+/)
    .filter((w) => w !== "" && w !== "&")
    .map((word) => {
      seen[word] = (seen[word] ?? 0) + 1;
      return { word, occurrence: seen[word] };
    });
};

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const inputs = {};
const zips = {};
/** A text file: `<zip>:<entry>` reads one entry of a burrito export, anything else a file. */
const read = (source) => {
  const [file, entry] = source.split(":");
  if (!entry) {
    const bytes = fs.readFileSync(file);
    inputs[file] = sha256(bytes);
    return bytes.toString("utf8");
  }
  if (!zips[file]) {
    const bytes = fs.readFileSync(file);
    inputs[file] = sha256(bytes);
    zips[file] = unzipSync(new Uint8Array(bytes));
  }
  if (!zips[file][entry]) throw new Error(`${file} has no ${entry}`);
  return strFromU8(zips[file][entry]);
};

const ULT = "bundle/en/en_ult_v91.sb.zip";
const TN = "bundle/en/en_tn_v91.sb.zip";
const TWL = "bundle/en/en_twl_v91.sb.zip";
const specs = [
  { book: "GEN", helps: "tn", ult: `${ULT}:en_ult/ingredients/GEN.usfm`, tsv: `${TN}:en_tn/ingredients/GEN.tsv` },
  { book: "GEN", helps: "twl", ult: `${ULT}:en_ult/ingredients/GEN.usfm`, tsv: `${TWL}:en_twl/ingredients/GEN.tsv` },
  { book: "PSA", helps: "tn", ult: `${ULT}:en_ult/ingredients/PSA.usfm`, tsv: `${TN}:en_tn/ingredients/PSA.tsv` },
  { book: "PSA", helps: "twl", ult: `${ULT}:en_ult/ingredients/PSA.usfm`, tsv: `${TWL}:en_twl/ingredients/PSA.tsv` },
  {
    book: "TIT",
    helps: "tn",
    ult: "fixtures/tc4/test-fixtures/en_ult/TIT.usfm",
    tsv: "fixtures/tc4/test-fixtures/resources/en_tn@v91/TIT.tsv",
  },
];

const tables = specs.map((spec) => {
  const chapters = usfm.toJSON(read(spec.ult)).chapters;
  const lines = read(spec.tsv).split("\n");
  const rows = [];
  for (const line of lines.slice(1)) {
    if (line.trim() === "") continue;
    const cells = line.split("\t");
    const [ref, id] = cells;
    const [quote, occurrence] = spec.helps === "tn" ? [cells[4], cells[5]] : [cells[3], cells[4]];
    const m = /^(\d+):(\d+)$/.exec(ref);
    if (!m || !quote) continue;
    const [chapter, verse] = [Number(m[1]), Number(m[2])];
    const found = chapters[String(chapter)]?.[String(verse)];
    if (!found) continue;
    const words = spec.helps === "tn" ? tnQuoteWords(quote) : quote;
    rows.push([id, `${chapter}:${verse}`, gatewayQuote(tokenizeVerse(found), words, Number(occurrence))]);
  }
  return { ...spec, rows };
});

const out = {
  generator: "crates/tc-helps/tools/gen-gateway-title-vectors.mjs",
  tc4: { commit: COMMIT, file: FILE, blob },
  libraries: pinned,
  node: process.version,
  inputs,
  tables,
};
// One row per line keeps the file reviewable and its diffs small.
const json = JSON.stringify(out, null, 1).replace(/\[\n\s+("[^"]*"),\n\s+("[^"]*"),\n\s+("(?:[^"\\]|\\.)*"|null)\n\s+\]/g, "[$1, $2, $3]");
process.stdout.write(`${json}\n`);
```

Run it from the worktree root:

```sh
. ~/.tc-env && node crates/tc-helps/tools/gen-gateway-title-vectors.mjs \
  /home/bot/unfoldingword/translationcore4 /home/bot/unfoldingword/tc4-ref/node_modules \
  > fixtures/helps/gateway-title-vectors.json
```

**Trap:** `/home/bot/unfoldingword/translationcore4` has **no** `node_modules`. tC4's installed
`node_modules` (usfm-js 3.4.3, fflate 0.8.2) are in `/home/bot/unfoldingword/tc4-ref/` (a clone of
the same repository at d310bf3). That is why the script takes the two paths apart. Expected
`tc4.blob`: `969dd79f5c0408370713858c177114ba0cdd50f6`; table row counts GEN tn 5758, GEN twl
5640, PSA tn 7921, PSA twl 7528, TIT tn 202; no `null` title; input sha256s: en_ult
`6a1989e8…`, en_tn `53943fdf…`, en_twl `2fbd8e56…`, TIT.usfm `8e49b16e…`, TIT.tsv `66d65571…`.
Spot checks: GEN tn mix3 `whom God has graciously given to your servant`; TIT f4iy `It is
necessary to stop them`; PSA 63:1 ovd5 `for you my flesh faints`.

Add a row to the "Generated from tC4" table of `fixtures/helps/README.md`: path
`gateway-title-vectors.json`; what it is (the paragraph at the top of the script, plus `inputs`
with their sha256 and the `tc4` commit and blob); how it was made (the command above, Node
version, date 2026-10-19, and that tC4's `node_modules` came from a d310bf3 clone because the
libraries did not change in the range).

### 5.2 Test `every_gateway_title_of_genesis_psalms_and_titus_is_tc4s`

In `tests/highlight.rs`. Read `fixture("helps/gateway-title-vectors.json")` with `serde_json`
(already a tc-helps dependency). For each table:

1. `let usfm = repo_text(&table["ult"])` (cache by source in a `HashMap<String, String>`: Genesis
   and Psalms ULT are each used twice).
2. Index the verses once: `verse_bodies(&usfm) -> HashMap<(u16, u16), &str>`: split on `"\\c "`,
   the chapter number up to the first whitespace; inside it split on `"\\v "`, the verse number up
   to the first whitespace, the body the rest up to the next `\v` (the next split piece). This is
   what `ult()` cuts for one verse; for PSA 3:15-style `\q1 \v 15` lines it still finds the verse.
3. Convert each verse **once** with `aligned()` and cache it (`HashMap<(u16, u16), AlignedVerse>`).
   **Trap:** re-slicing the verse from the start of the book for each row (as `ult()` does) took
   49 s for Psalms tN in a debug build; with the index and the cache all five tables take about
   3 s in debug.
4. Rows: `parse_notes` (tn) or `parse_word_links` (twl) on `repo_text(&table["tsv"])` with
   `TsvScope::Book(book.parse::<BookId>())`; keep the rows whose reference is
   `HelpRef::Bible` with `BibleLocus::Verses(list)` holding one range of one verse
   (`list.ranges().len() == 1 && start == end`), a non-empty quote (`note.quote` /
   `link.orig_words`), and a verse in the index. The planner checked that this keeps exactly the
   generator's rows, in the same order, with the same ids, places and quote bytes, for all five
   tables.
5. Occurrence as `CheckItem::title` passes it: `Occurrence::Nth(_)` as is, anything else
   `Occurrence::NotTied` (tC4 reads 0, -1 and an empty cell as the first occurrence:
   `Math.max(1, Number(occurrence) || 1)`).
6. Compare `(row id, "c:v", gateway_quote(..))` with the fixture's rows: same length, and on any
   difference fail with the first ten differing rows (expected and got) and the count.
7. Also assert no Genesis row's expected title is `null` (C34's "0 Hebrew titles over all of
   Genesis"), so a regenerated fixture cannot silently drop that.

Print with `eprintln!` the time spent titling each table (after parsing and conversion), so the
release run of §8 is the measurement.

Without step 3 this test fails on 126 rows (93 GEN tn, 26 PSA tn, 7 TIT tn).

## 6. Step 5: the tW-only pin change (tC4 #661)

### 6.1 Why the count is wrong today

`crates/tc-check/src/change.rs:83-131` resolves each unit's list resource (for Words: the TWL pin)
before and after, and counts a unit affected only when that resource moves. A Words list of a book
is also filtered by the articles of the translation words pin of the language set that answers it
(`derive::word_links_for_book(links, articles, spec)`, `derive.rs:156-189`; tC4 `withArticle`,
#643; "the resolving set's translation words", `fix.rs:202-206` and `missing_pin`
`fix.rs:217-232`). A change of only that tW pin leaves the TWL resolution alone, so a decided check
whose article the new tW lacks leaves the list while the dialog says nothing changes. tC4 4.0 has
the same gap (its `consequencesOfGatewayChange` compares only the decisions file's resource; #661
is open in tC4 and nothing in d310bf3..9ad4d5c touches `gatewayChange.ts` or `derive.ts`), so this
is a **declared difference** (step 6).

### 6.2 Code (`crates/tc-check/src/change.rs`)

Signature (the only callers are `crates/tc-check/tests/carry.rs`, lines 424, 434, 446, 457, 496,
501, 507, 532, 572; tc-app does not call it yet, C8 will):

```rust
pub fn consequences<'a>(
    current: &dyn Ladder,
    next: &dyn Ladder,
    units: &[Unit<'_>],
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
    articles: impl Fn(&ResourceRef) -> Option<&'a BTreeSet<WordArticleId>>,
) -> Consequences
```

`articles` gives the articles an installed translation words resource carries (what
`tc_helps::metadata::BurritoMetadata::word_articles()` reads from its `metadata.json`); `None` when
they are not known here, which filters nothing, exactly as `word_links_for_book(.., None, ..)`.

`Affected` gains one field (after `decisions`):

```rust
    /// Of `decisions`, those whose check leaves the list because the translation words that
    /// filter it after the change lack the decision's article (tC4 #661): kept, and listed off
    /// the list, not checks to do again.
    pub articles_left: usize,
```

Per unit (only units with decisions, as now):

1. `answer_before = current.answer(tool, collection, Unrecorded::Installed(&local))`,
   `answer_after` likewise on `next`; `before`/`after` = their `resource()` (as today).
2. `moves_away` as today. Factor the "two optional resources differ" test into one helper and use
   it twice:
   `fn moved(a: Option<&ResourceRef>, b: Option<&ResourceRef>) -> bool { match (a, b) { (Some(a), Some(b)) => !a.same_resource(b), (a, b) => a.is_some() != b.is_some() } }`.
3. The tW pin that filters the list on each side: only for `(ToolId::TranslationWords,
   Collection::Book(_))` with an answer: `pins.pin(answer.rung(), SetSlot::TranslationWords)
   .map(|p| p.resource)` (the rung that answers the TWL, never the other rung; a slot that is
   absent or holds an unusable pin gives `None`: no filter known). Stories lists are never filtered
   (derive's OBS lists take every link).
4. `articles_move = moved(tw_before, tw_after)`. **Only when it is true**, call `articles` for
   the two sides (at most two calls per unit; none for a Notes unit, a Stories unit, or when the
   tW pin stays). Callers may read metadata from disk inside `articles`; the count test of §6.3
   proves the laziness.
5. For each decision: `holds_now = before.is_some_and(|r| r.same_resource(&d.decision.resource))`.
   If it holds: `holding += 1`; `leaves = articles_move && on_list(gate_before, &keys) &&
   !on_list(gate_after, &keys)` with `keys = article_keys(d)`; `at_risk += 1` when `moves_away ||
   leaves`; `articles_left += 1` when `leaves`. Else if it holds after: `revived += 1` (as today).
   `detached += len - holding - revived` (as today).
6. Push `Affected { checked_against: before, decisions: at_risk, articles_left, blocked:
   after.is_none(), .. }` when `before` is some and `at_risk > 0`; else `unaffected += 1`. When the
   resolution moves, `at_risk == holding` exactly as today, so every existing assertion holds.

```rust
/// The article a Words decision is on: its recorded category (`DecisionRecord::category`, which
/// `decision_edit` writes from the check and tC4's files carry) and its group id, the article
/// slug (tC4 `withArticle`: `${category}/${groupId}`). With no recorded category it is the slug in
/// each of the three categories; with a group id that is no slug, none (the decision then never
/// counts as leaving).
fn article_keys(d: &StoredDecision) -> Vec<WordArticleId>

/// Whether a check on one of `keys` is on a list filtered by `articles` (`None`: no filter).
fn on_list(articles: Option<&BTreeSet<WordArticleId>>, keys: &[WordArticleId]) -> bool {
    articles.is_none_or(|set| keys.is_empty() || keys.iter().any(|k| set.contains(k)))
}
```

`WordCategory::parse` takes `kt`, `names`, `other`; `Slug::parse` refuses anything that is not a
slug. Both are public in tc-helps; tc-check already depends on it.

Docs to change (each is false or incomplete today):
- `change.rs` module doc (lines 1-17): add that for a Words list of a book a decision is also at
  risk when the change removes its article from the translation words that filter the list, so
  its check leaves the list (#661); the article sets come from `articles`, read only when the
  translation words pin moves; tC4 4.0 counts only the resource.
- `carry.rs` `Outcome::Unplaced` (lines 93-95) says it "computes as invalidated". Not when only
  the article left: its resource still matches. Say: "Nothing is written. The decision is kept and
  a session lists it off the list. It computes as invalidated when the change moved its resource;
  when only its article left the translation words, its resource still matches, and it shows again
  as it was if a later release carries the article again."
- `carry.rs` `Summary::invalidated` (lines 326-328): add "or whose check is no longer on the new
  list (its article left, #661)".

What the carry-over does with such a decision (the "so it is invalidated by the carry-over" of
the unit's task, read precisely): C8 plans every affected unit with `carry::plan` on the list
derived with the new articles. The decision finds no check (`Unplaced`) and `Plan::summary`
counts it in `invalidated`, the same as the existing TWL-change case
(`a_decision_on_a_link_whose_article_left_is_kept_and_counted_as_to_check`). Nothing rewrites the
decision: its record keeps its judgement and resource, the fold still computes it valid against
the unchanged TWL, and the session lists it off the list. Writing an invalidation into the record
would rewrite a translator's decision and stop it from coming back when the article returns; do
not add one.

### 6.3 Tests (`crates/tc-check/tests/carry.rs`)

Update every existing call with a fifth argument. A fn item avoids closure-lifetime inference:

```rust
/// No article set is known: no Words list is filtered.
fn no_articles(_: &ResourceRef) -> Option<&'static BTreeSet<WordArticleId>> { None }
```

Shared setup for the new tests (the planner ran it):
- `links = word_links("en_tw@v91/2SA.tsv", BookId::SecondSamuel)`, `spec = eng(2SA)`.
- `after = parse_burrito_metadata(&resource("en_tw@v91/metadata.json")).unwrap().word_articles()`;
  `ark = WordArticleId::Dcs { category: WordCategory::KeyTerm, slug: Slug::parse("arcofthecovenant") }`;
  assert `!after.contains(&ark)` (v91 does not carry it; neither does the v87 fixture, so a "v90"
  that still carried it is built: `before = after ∪ {ark}`; say so in a comment).
- Lists: `word_links_for_book(&links, Some(&before), &spec).items` has 3,316 checks, with `after`
  3,315.
- Pins: `twl = pin("unfoldingWord/en_twl", "v91", &["2SA"])`, `tw90 = pin("unfoldingWord/en_tw",
  "v90", &[])`, `tw91 = pin("unfoldingWord/en_tw", "v91", &[])`; a set holds
  `Slot::TranslationWordsLinks → twl` and `Slot::TranslationWords → tw`; `Pins { primary: Some(set),
  fallback: Some(set) }`.
- Decisions: `decided(vwrw_item, &twl.resource)` (its `category` is `Some("kt")`, group id
  `arcofthecovenant`) and a control `decided(&items_before[0], &twl.resource)` whose article stays.
- `articles = |r: &ResourceRef| if r.same_resource(&tw90.resource) { Some(&before) } else if
  r.same_resource(&tw91.resource) { Some(&after) } else { None }`.

**`a_tw_only_change_that_drops_a_decided_article_is_counted`** (the TC4-ISSUES name):
- `consequences(&pins(tw90), &pins(tw91), &[Words 2SA unit], |_| None, articles)`:
  `affected == [Affected { tool: TranslationWords, collection: Book(2SA), checked_against:
  twl.resource, decisions: 1, articles_left: 1, blocked: false }]`, `unaffected == 0`,
  `revived == 0`, `detached == 0`, `!harmless()`. (Today: unaffected 1, affected empty.)
- No-change control: `consequences(&pins(tw90), &pins(tw90), ..)` is harmless with
  `unaffected == 1`.
- The carry-over: `p = plan(&items_after, &twl.resource, &stored)`; `vwrw` is
  `Outcome::Unplaced`, the control `Outcome::Stays { by: Match::Identity, .. }`;
  `p.summary(Some(&twl.resource), no_text)` has `invalidated == c.at_risk() == 1` and
  `carried == 1` (the dialog's two numbers agree).
- Kept, not lost: `open(items_after, &projected(&stored, Some(&twl.resource), no_text),
  Some(&twl.resource), no_text).off_list == [0]`, and the control is placed.

**`an_article_change_is_judged_by_the_set_that_answers_the_list`**:
- The new tW's articles are not known (`articles` gives `None` for tw91): harmless, as the list
  would be derived unfiltered.
- TWL and tW both change (`twl v90 → v91` and `tw90 → tw91`): `at_risk == 2` (every holding
  decision, as today), `articles_left == 1` (no double count).
- Only the fallback's tW changes while the primary answers 2SA with its own TWL and tW: harmless
  (the fallback's articles do not filter this list).
- A decision with no recorded category (`stored[0].decision.category = None`) is still counted
  (its slug matches `kt/arcofthecovenant` in the old set and no category in the new).

**`article_sets_are_read_only_when_the_translation_words_pin_moves`** (an operation-count test,
PERFORMANCE rule 2): wrap `articles` with a `Cell<usize>` counter. tw-only change on one Words unit
and one Notes unit (a `stored_against(..)` Titus unit with a tN pin in the same sets): 2 calls.
Same tW on both sides (a TWL-only or tN-only change): 0 calls. Without the laziness of §6.2 item 4
the second count is 2 per Words unit.

The existing `a_gateway_change_counts_the_decisions_it_puts_at_risk`,
`a_decision_a_carry_over_left_behind_is_not_put_at_risk_again`,
`a_change_back_to_the_resource_a_decision_was_made_against_revives_it` and
`a_change_onto_an_unusable_pin_is_blocked` must pass with only the extra argument; their sets have
no tW slot, so `articles` is never called.

Imports the tests need: `std::collections::BTreeSet`, `tc_helps::{Slug, WordArticleId,
WordCategory}`, `tc_helps::metadata::parse_burrito_metadata` (already used inline),
`tc_check::change::Affected`.

## 7. Step 6: the documents

- `docs/TC4-DELTA-2026-10-19.md` §5, "Differences to tell the owner", the **Check:** bullet
  (line 564): add "A change of only the translation words pin that drops the article of a decided
  Words check counts that decision in the upgrade and language-change dialogs, where tC4 4.0 says
  nothing changes and the check leaves the list unannounced (#661)."
- Same file §3.1 item 2, the "others the triages recommend" list (line 158): add `#661`.
- Same file §3.11 item 2 (line 307, expected changes when the baseline is recaptured at v4.0.0):
  add "the titles of seven Titus notes (C39: f4iy 1:11, xyz7 2:13, and m9zd, qt8f, xy27, tl5n,
  dec4 at 3:3) in `translate-tit1` and `en-translate-tit1` (helps note cards) and in the rails of
  `check-tn-session`, `en-check-tn-session`, `en-check-tn-session-620`, `en-check-tn-session-row`,
  `ar-check-tn-session-620`, `ar-check-tn-session-row` and `en-check-ta-drawer`. Until then the
  rebuild shows tC4 9ad4d5c's titles there (for example "It is necessary to stop them" where the
  d310bf3 capture shows "It is necessary to stop them … they should"), and B4a and B5b record each
  as a residual citing C39." (The planner found the old titles in exactly those nine
  `parity/tc4-baseline/dom/*.json` files. Do **not** add rows to `parity/app/residuals.json` now:
  the gate refuses a residual on a row that is still `unported`,
  `a_residual_excuses_only_a_row_that_differs`.)
- `docs/APP-DESIGN.md` line 660 cites `crates/tc-helps/src/highlight.rs:268` for
  `highlight_quote`. Update the line number to where `pub fn highlight_quote` ends up.
- Not a declared difference: #773 is in tC4's release branch (rc.2), so v4.0.0 will have it;
  the rebuild only adopts it before the reference moves. Do not add it to PLAN.md.
- Do not rewrite the statuses or the counts table of `docs/TC4-ISSUES.md`: it is a dated
  snapshot. Report in your final answer which rows this unit fixed.

## 8. Measurements

1. Title speed, release, before step 2, after step 2 and after step 3:
   `cargo test --release -p tc-helps --test highlight every_gateway_title -j 6 -- --nocapture`
   (the per-table times the test prints). Planner's numbers on this PC for all Psalms tN+TWL
   rows: 143 ms today, 229 ms for a straight port, 55 ms for the keyed port. Put yours in the
   commit messages. If the keyed port is not well under today's time, profile before going on
   (PERFORMANCE rule 6).
   In the same test, assert in release builds only (`!cfg!(debug_assertions)`) that titling the
   Psalms tN table takes under 750 ms, three times the Check screen's 250 ms open budget
   (PERFORMANCE rule 5, the pattern of `crates/tc-check/tests/performance.rs`).
2. The debug time of the new tc-helps tests: the differential test should take about 3 s
   (`cargo test -p tc-helps --test highlight`); if it takes much more, the verse index or the
   per-verse cache is missing.
3. #661 costs no time when the tW pin does not move: the count test of §6.3. When it moves, the
   extra work is two map lookups per held Words decision.

## 9. The gates (before the last commit)

```sh
cd /home/bot/tc-rst/.claude/worktrees/app-fhelps && . ~/.tc-env
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -j 6 -- -D warnings
cargo test --workspace -j 6
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features gallery -j 6 -- -D warnings
cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -j 6 -- -D warnings
cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
```

tc-ui has the features `gallery` and `app` (no `harness` yet: check `crates/tc-ui/Cargo.toml`
again before the run). This unit changes no tc-ui or tc-api code, but the gates run anyway.

## 10. Invariants and traps

- **No translator's work is lost, hidden or rewritten.** The highlight is display only; nothing it
  computes is stored. `consequences` only counts; the carry-over never deletes or rewrites a
  decision, and the off-list decision stays reachable (`Session::off_list`). The #661 change must
  only *raise* counts (a decision moves from unaffected to affected), never lower an existing one:
  every existing `carry.rs` assertion stays as it is.
- **Match tC4 exactly** in the highlight: same strict path, same fallback, same tie rule, same
  word splitting. The differential test is the arbiter; if it fails on a row, fix the port, not
  the fixture. Regenerate the fixture only from tC4's code with the generator.
- `quote_words` splits on whitespace **and** U+05BE before dropping `&`/`…`: a one-compound quote
  is multi-word. U+2060 (word joiner) is not whitespace in Rust or in JavaScript's `\s`, and
  `normalize_word` keeps it inside a word; never strip it.
- Instance identity is the **raw** `content` plus occurrence (tC4's `${content}#${occurrence}`),
  while word matching uses the normalized key. Two different raw forms of one word (ע֖וֹד and ע֛וֹד
  in GEN 9:11 differ by accent and are different words) are different instances.
- In `shortest_run`, `instances` is every instance *seen* in the run, not capped at the needed
  count, and `marked` spans all groups that render them, not just `start..=end`.
- `AlignedVerse` gains a private field; there is no serde on it and no struct literal outside the
  module, so nothing else changes. Keep `SourceWord` as it is (public fields, used by tests).
- Workspace lints: `unwrap_used`, `expect_used`, `panic` warn and clippy runs with `-D warnings`.
  Library code: no `unwrap`/`expect`/indexing that can panic without a comment proving it.
  `tests/highlight.rs` allows `unwrap_used` and `panic` only.
- `CheckItem::title` maps `key.occurrence` 0 to `NotTied`; check keys never hold -1. Nothing in
  tc-check changes for #773, and no tc-check test asserts a title today
  (`fixtures/check/check-vectors.json` has no title vectors), so no tc-check vector breaks.
- The parity gate: all 62 rows are `unported`, so nothing in `parity/app` changes or breaks now;
  the nine screens of §7 will show the difference when B4a and B5b capture them.
- The scratch crates the planner used live in the planner's scratchpad, not in the worktree; the
  only thing they left here is `target/scratch-hl/`, which you may delete.
- An unusable tW pin on the answering rung is treated as "articles not known" (no filter), as in
  derive; it is the preflight's business (`fix.rs:202-206` refuses to open the tool) and is not
  counted as blocked here. Note it in the commit message as a known limit, not a fix.
- The 2SA scenario's "v90 still carried the article" is constructed; no real tW release in the
  fixtures carries `kt/arcofthecovenant` (en_tw v87 and v91 metadata both lack it). Say so in the
  test's comment; do not invent a fixture file for it.

## 11. Done when

- `a_reordered_quote_lights_only_its_own_phrase`, `genesis_maqaf_quotes_title_in_the_ult`, the
  property, and `every_gateway_title_of_genesis_psalms_and_titus_is_tc4s` pass in tc-helps; the
  first and the last fail on today's code (4 of 5 titles; 126 rows), as the commit messages say.
- `a_tw_only_change_that_drops_a_decided_article_is_counted` (with its no-change control),
  `an_article_change_is_judged_by_the_set_that_answers_the_list` and
  `article_sets_are_read_only_when_the_translation_words_pin_moves` pass in tc-check; the first
  fails on today's code (unaffected 1, affected 0).
- Every existing test passes; the gates of §9 pass; the measurements of §8 are in the commit
  messages; the documents of §7 are updated.
- The final answer lists: #773, #661 and C34/#765/#664 as handled; C43/#769, #711 and #768 as not
  handled, with §0's reasons; the declared difference for #661, for the owner; and the nine
  baseline screens whose Titus titles will differ from the d310bf3 capture until the reference
  moves.
