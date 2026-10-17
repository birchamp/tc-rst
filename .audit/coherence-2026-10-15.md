# Cross-crate coherence: tc-check, tc-align and tc-resources against the storage branch

Most of the disagreement is on shared ground. Validity is computed in four places that give different answers. One concept is modelled three times: the pins and their D30 ladder (primary rung, then fallback). Eight mismatches will block integration if left as they are. Some of them are spec gaps rather than code bugs.

Several mismatches exist because the spec changed on the storage branch after the domain crates were built. Main's STORAGE.md had `valid = run.end == value.span_end` (M:1933). Its pin resolution read recorded `books` only (M:1947-1948). It gave no grammar for `RefPart` or `SlotName`.

**Locations and references.**
- **C** = `.claude/worktrees/wf_e15b2949-342-1/crates/tc-check` (HEAD 9255a74)
- **A** = `.claude/worktrees/wf_e15b2949-342-2/crates/tc-align` (HEAD 299cce0)
- **R** = `.claude/worktrees/wf_e15b2949-342-3/crates/tc-resources` (HEAD 531652c)
- **J** = storage HEAD 6c21b46, `crates/tc-journal`; **U** = the same commit, `crates/tc-usfm`
- **S** = the storage worktree's `docs/STORAGE.md` (working copy); **M** = main's `docs/STORAGE.md`
- The storage worktree has uncommitted edits in progress. They don't touch any area cited here.
- No worktree was changed. Every worktree was read with `git show` or `git archive`.

## What the comparison program measured

The program is at `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/coherence/ws/crates/compare/src/main.rs`. It builds against the HEAD of each worktree and uses the golden vectors from `fixtures/derivations` on the storage branch. To rerun it: `target/debug/compare /home/user/tc-rst/fixtures/tc4 ../vectors`, from `…/coherence/ws`.

- **Plain text.** There are 338 distinct verse contents in the fixtures/tc4 USFM files, as the codec stores them (`words + layout`).
  - `md5(tc_usfm::plain_text)` differs from `md5(plain_text_v1)` on 100 of them: all 92 aligned en_ult and en_ust Titus verses, 4 Hebrew Jonah verses and 4 poetry or partial-book verses.
  - Against the JavaScript golden vectors (1,684 real-verse cases): `plain_text_v1` matches all of them. `plain_text` gives a different md5 on 1,314.
  - The tC4 sample alignment hash for Titus 1:1 is reproduced by `plain_text_v1` and by tc-align's `TargetVerse::new(plain_text_v1(...)).md5()`.
- **Selections.**
  - The journal's `selections_v1` agrees with the JavaScript library on all 900 golden cases.
  - tc-check's `selection::validate` disagrees with the library on 69 of 900 when given `plain_text_v1` (49 involve `\zaln`, 11 `\w`, 4 notes, 5 malformed markup). It disagrees on 50 when given `plain_text` and on 37 when given the raw content.
  - On real fixture verses: 1,116 one-word selections made by tc-check on `plain_text_v1` text. The journal's `selections_v1(content)` refuses 1 of them (a verse with a footnote). Made on `plain_text`, it refuses 15 words in 2 verses.
- **Check identity.**
  - tc-check derives 7,859 checks from the real en_tn Psalms file. The journal's grammar refuses 1 key: `sbh4` at `5:1,3,8,12`.
  - tc-check derives 7,528 checks from en_twl Psalms. None are refused.
- **OBS pin slots.** On `fixtures/tc4/sample-burrito-obs/.../resources.json`, the journal's `SlotName` grammar refuses `obs-tn` and `obs-twl` (primary), and `obs-tn`, `obs-twl`, `obs-tq` and `obs-images` (fallback).

## Mismatches

| # | Area | Crates | Where | What differs | Which is right | Change needed | Blocks |
|---|---|---|---|---|---|---|---|
| 1 | Check identity: `RefPart` grammar | C, J | C key.rs:87-95; J keys.rs:197-210; S:235 | tc-check accepts any non-empty text without `:`, and its docs name `3,5` as an example. The journal accepts 1–32 ASCII letters, digits and `-`. The real Psalms check `sbh4 5:1,3,8,12` cannot be stored. | Spec | tc-check parses with the journal's grammar (or reuses its type). The spec decides whether comma references become skipped rows or the grammar allows `,`. | **Yes** |
| 2 | Re-judging a decision does not update `resource` | C, J, S | C status.rs:346; S §5.4:1444-1448; J command.rs:140-161 | tc-check's Valid and Invalid record the current list's pin as `resource`. In the journal, `DecisionEdit` can set `resource` only through `basis`, on a first decision. A re-judged decision whose old resource wasn't carried over stays `!resource_ok`: invalid forever in the file, valid on screen. | tc-check (spec gap) | §5.4 and the actor: a judgement change also sets `resource` to the current resolution. | **Yes** |
| 3 | What "To do" stamps | C, S | C status.rs:332-335; S:1446-1448 | tc-check's To do changes only the status. The spec re-stamps the verse md5 and clears `imported_invalid` on any status change, so stale selections then count as decided. | Spec as written; tc-check's reason is sound | Decide one rule in §5.4. tc-check's `apply` and the actor must then do the same thing. | No |
| 4 | The decision edit is implemented twice | C, J | C status.rs:305-349; J command.rs:40-44, 137-161 | tc-check's `apply(Edit)` returns a whole `Decision`. The journal's actor applies a list of `DecisionChange`s and does its own stamping. Rows 2 and 3 are where they already differ. | Spec (the actor owns it) | tc-check turns its `Edit` into `DecisionChange`s and stops stamping, or the actor calls tc-check. One owner. | **Yes** |
| 5 | Decision whose verse has no text | C, J, S | C status.rs:156-164; J project.rs:968-996, 1118-1132; S:2192 | tc-check treats a missing text as no evidence, as tC4 does: the tC4 bundle's `jf` skips `""` and `___`. The journal uses `""`, so the md5 differs, the selections fail, and the decision shows invalidated with `verseEdits`. A `___` stub does the same. | Spec is silent; tc-check matches tC4 | Add a no-text rule to §7.7; the journal follows it. | No |
| 6 | Which text `selections_v1` reads | C, J, U, S | S:2192-2194; J project.rs:1130; C selection.rs:92-115 | The spec formula passes `plain_text_v1`. The journal passes raw content through `removeMarker`, which agrees with the 900 vectors. tc-check counts substrings in whatever text it is given and never calls `removeMarker`. 69 of 900 disagree. | Spec is ambiguous; the journal and the vectors agree | State the input in §7.7 (recommended: raw content, as the journal does). tc-usfm exports the `removeMarker` text and its count. tc-check records `occurrences` and validates through tc-usfm and drops its own counting. | **Yes** |
| 7 | Which plain text callers and tests use | C, A, U | C tests/common/mod.rs:102-108; C status.rs:32; A words.rs:319-347 | Both crates' docs say `plain_text_v1`. Both crates' tests stamp and compare with `tc_usfm::plain_text`, which gives a different md5 on 100 of 338 real verses. The journal hashes a span target over the joined run contents (J project.rs:968-996). | Spec (`plain_text_v1`) | All callers and tests use `plain_text_v1(words + layout)`. Export one target-text and md5 function from the journal or tc-usfm. | **Yes** |
| 8 | Alignment validity: span rule | A, J, S | A value.rs:172; S:2165-2177; J project.rs:553-559; old rule at M:1933 | tc-align compares `span_end` with the run's end. The spec computes `end = v + (span_end − anchor)`. The Align rail shows every renumbered verse as invalid; the file shows it valid. | Spec | tc-align's `Current` carries the anchor (or the expected end), or tc-align reads validity from the fold. | **Yes** |
| 9 | Alignment value shape | A, J | A value.rs:41-46, 105-127; A record.rs:175-182, 250-256; J record.rs:235-282; S:350-370 | tc-align groups have no `extra`. Source-word extras must be strings. `well_formed` refuses repeated target words. The journal keeps any JSON on groups and words and has no well-formedness rule, so it can hold values the aligner can't open. | Spec | tc-align keeps group extras and JSON-valued word extras, or reports "can't open without losing X". Otherwise the spec adds the rule. | No (no fixture record has these) |
| 10 | Alignment file codec exists twice | A, J | A record.rs:48-155; J project.rs:1046-1100; S:2209-2219 | tc-align's decoder reads `invalid` as `marked_invalid`, which is right only for importing tC3/tC4 files (S §13.1). Its encoder writes `done` only when true and omits `source`, `markedInvalid`, `dismissed`, `anchor` and `spanEnd`. | Spec and journal | Limit tc-align's record codec to imports or remove it. Decoding outside edits (§11.1 X2) belongs to the journal. | No |
| 11 | OBS pin slot names | J vs C, R | J keys.rs:238-256; S:258-260; J canon.rs:1374-1382; C resolve.rs:211-215; R sets.rs:149-155 | The journal's `SlotName` forbids `-`. tC4, tc-check and tc-resources use `obs-tn`, `obs-twl`, `obs-tq` and `obs-images`. The tC4 OBS sample's pins can't be stored. | Domain crates (tC4's names) | `SlotName` allows `-`. | **Yes** (OBS) |
| 12 | OBS decision files and resolution | J vs C, R, S | J project.rs:425-452; J attach.rs:614-661; S:881; C resolve.rs:220-229 | The journal writes decision files per book only, so there is no `OBS.json`. It has no OBS resolution. tc-check and tc-resources let the first rung that pins the slot answer. | Spec and domain crates | The journal writes OBS decision files. §7.7 states the OBS resolution rule. | **Yes** (OBS) |
| 13 | Pin without `books` | C, R vs J, S | S:2186-2188; J attach.rs:637-640; C resolve.rs:79-108, 407-414; R resolve.rs:11-14, 37-42, 201-211 | Spec and journal: an unrecorded pin covers every book. tc-check and tc-resources (in the mode their docs say "the fold uses"): it covers no book. For every imported pre-D41 pin, the screen invalidates every decision and the file keeps them valid. | Spec | In fold mode, no `books` record covers every book. The coverage-from-installed-copy rule stays only for opening a tool (preflight). | **Yes** |
| 14 | Coverage edge values | C, R, J | J attach.rs:638; C resolve.rs:91-100; R pin.rs:173-195, 240-248; R sets.rs:340 | `["BIBLE"]`: whole collection in tc-check and tc-resources, no book in the journal. Lower-case book codes: tc-check upper-cases, tc-resources rewrites them, the journal compares exactly. `books: []`: tc-check and tc-resources treat it as no record, and tc-resources drops it on write, which flips the journal's answer. An unknown code: tc-resources refuses the whole group. | tC4's `coverageFor` (tc-check); spec silent | Spell out the coverage rules in §7.7, with one implementation. | No |
| 15 | Unusable pin on a rung (tag, no sha) | C, J, R | C resolve.rs:445-456; J attach.rs:644-658; R sets.rs:340 | tc-check stops the ladder there. The journal falls through to the fallback rung. tc-resources can't read the set at all. | tc-check (the spec says "tC4's D30 ladder") | The journal stops at it; tc-resources reads it without refusing. | No |
| 16 | Re-attaching decisions when a session opens | C vs S, J | C session.rs:59-91; S §5.8:1510-1519; S §7.7 Detached | tc-check moves decisions onto other checks every time a session opens, without writing, and marks checks "contested". The spec moves them only in an upgrade plan; the fold shows them by key and `MovedTo`, and lists the rest as Detached. Re-deciding such a check leaves the old key behind, and the next open contests it again. | Spec | `Session::open` uses the fold's projection plus the Detached list. `carry::plan` stays as the builder of the §5.8 upgrade plan. | **Yes** |
| 17 | "Gateway change" counts ported twice | C, R | C change.rs:1-60; R gateway_change.rs:74-107 | The two crates give different at-risk numbers for the same input: tc-resources also counts decisions that are already detached. | tc-check (the refined version) | Delete tc-resources' counter. Keep its pin edits (`apply_gateway_change`). | No |
| 18 | Same types modelled several times | all | ResourceRef: J record.rs:228, C resolve.rs:66, A value.rs:86, R resolve.rs:113. Commit sha: J `GitOid` (lower-case only, ids.rs:24-27), C/R `CommitSha` from tc-helps/tc-door43 (any case), A value.rs:60. MD5: J `Md5`, C status.rs:34, A words.rs:422. `RepoPath`: J means a file in the project; C resolve.rs:26 and R pin.rs:40 mean a repository, and R also accepts `https://`. `PinGroup`: J keys.rs:352, R sets.rs:67. Rung, Slot, LanguageSet, Coverage, Resolution and the ladder: C resolve.rs, R sets.rs/resolve.rs, J attach.rs:614. `resource_ok`: C status.rs:161, R resolve.rs:201, J attach.rs:679. Source check: A value.rs:178, R resolve.rs:216, J project.rs:558. `Tool` (R, 2 variants) vs `ToolId` (3); `Collection` (C) vs `CheckUnit` (R). | Each type should have one owner | Identity and value types come from tc-journal (or a light record-types crate). The pure pin model and ladder go in one crate the journal can use; it can't depend on tc-resources, which pulls in reqwest. | No (convertible); rows 13-15 come from it |
| 19 | Unknown `resources.json` keys | R, J, S | R sets.rs:505-516; J project.rs:816-855 | tc-resources keeps top-level keys it doesn't know. No record holds them, so they are lost on import. | tc-resources (no-loss rule) | The spec adds a record for them. | No |
| 20 | Coverage backfill scope | R, S | R resolve.rs:366-404; S:1473-1476 | tc-resources also widens recorded lists and backfills scripture panes. The spec backfills only pins without `books`. | Spec as written | Allow widening in the spec, or stop widening. | No |
| 21 | Resource store layout | R, S | R store.rs:1-20; S:1037, 1505 | tc-resources nests `dcs/<owner>/<repo>/<sha>/`. The spec says `resources/<owner>--<repo>/<sha>/`, which is ambiguous when names contain `--`. | tc-resources | Update the spec. | No |
| 22 | Dependency versions outside the workspace | C, A, R | C Cargo.toml:18, 20; R Cargo.toml:16 | md-5 0.10.6 (C, A) vs 0.11.0 (tc-burrito). sha2 0.10.9 (R) vs 0.11.1 (storage workspace). unicode-normalization is declared directly. | AGENTS.md (shared versions in the workspace) | Put one version of each in `[workspace.dependencies]`. | No |
| 23 | Psalms superscriptions (`N:front`) (found while testing) | tc-helps, C | tc-helps tsv.rs:414 | tc-helps rejects the whole en_tn and en_twl Psalms files over 113 and 155 `N:front` rows. So tc-check can't derive Psalms lists at all. The journal accepts `front`. | tC4 | tc-helps reads `N:front`. | **Yes** (Psalms corpus) |
| 24 | Aligned USFM imported as-is | S, C, A | S §13.5; U codec | The verse keeps its `\zaln` markup. `plain_text_v1` then gives only punctuation (all 92 aligned Titus verses), so tc-check has no words to select and tc-align won't open the verse. | Spec gap | The import splits alignments out into Alignment records. | No |

## Integration steps when the storage branch merges

1. Merge storage first, then rebase the three domain branches. Merge the workspace dependencies (md-5, sha2, unicode-normalization, zip) and move the crate-local versions into them.
2. Settle the spec on the storage branch before rewiring anything:
   - `SlotName` allows `-`.
   - Decide comma references in `RefPart`.
   - §5.4: a re-judgement sets `resource`; settle the To do rule.
   - §7.7: the input to `selections_v1`; the no-text rule; coverage rules (`BIBLE`, letter case, `[]`, unusable pin); OBS resolution and OBS decision files.
   - §3.7 store layout; a record for unknown `resources.json` keys; how aligned USFM is imported.
3. tc-usfm: export the `removeMarker` text and its occurrence count, and a `verse_text_md5_v1`. Delete tc-check's own counting, and switch every caller and test from `plain_text` to `plain_text_v1`.
4. Journal: apply the spec decisions above, and export the target-text function the fold uses for decisions.
5. Give each type one owner. The domain crates use tc-journal's `CheckKey`, `ResourceRef`, `GitOid`, `Md5`, `DecisionRecord` and `AlignmentValue` (or a light crate split out of it). One pure pin model and ladder serve both the fold and the screens. The coverage-from-installed-copy rule stays in tc-resources' preflight.
6. tc-check:
   - Adopt the `RefPart` grammar.
   - Turn `Edit` into `DecisionEdit`.
   - Build `Session::open` from the fold's projection and its Detached list.
   - Have `carry::plan` emit the §5.8 upgrade plan.
7. tc-align:
   - Make validity anchor-aware, or read it from the fold.
   - Convert to and from the journal's `AlignmentValue` without losing data (`word_extra` maps to `TargetWord.extra`; group extras).
   - Limit `record.rs` to imports.
8. tc-resources:
   - Map `ProjectPins` to and from `PinGroupValue` under the journal's slot grammar.
   - Read malformed slots without refusing the whole group.
   - Delete its duplicate gateway-change counter.
9. Fix tc-helps for `N:front`.
10. Add one differential test over fixtures/tc4 and the Psalms corpus. It asserts that what the screens compute (tc-check's `assess`, tc-align's staleness) equals the `invalidated`, `verseEdits` and `invalid` flags the fold writes. It would have caught rows 2, 5–8 and 13–16.