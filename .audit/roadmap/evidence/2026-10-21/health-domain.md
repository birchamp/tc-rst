# health-domain

## Summary
The domain and foundation crates on resume-build-lane (snapshot of f9067bc) are in good shape. Library code has 0 unwrap and 0 panic!. Only 2 expect sites remain, and each carries a proving comment or a reason. All 12 unreachable! sites carry a reason. Lints are inherited by all 13 crates, and every big parser has proptest round trips. The pure crates do no I/O, and tc-ui depends only on tc-api and tc-core (checked with cargo metadata). The weak spot is that nothing mechanical enforces ARCHITECTURE.md's rules. No CI step or test checks dependencies or I/O, and tc-resources already uses reqwest directly. The second problem is duplication that the remaining units (I/F1, X, D2) will step on:
- Two or three readers for each import job (aligned USFM, tC4 alignment JSON, OBS questions). In each case the faithful reader is test-only, or the one the app wired in is the stricter one.
- StoryNo is defined 3 times and FrameNo twice. ChapterNo and VerseNo live only in tc-journal, while tc-core uses a raw u16, so tc-core accepts "TIT +1:1".
- The JavaScript whitespace rule is copied in 6 crates.
- The print-model types are twinned between tc-api and tc-export.
- USFM scanning is done outside tc-usfm (tc-store/src/seed.rs, tc-app/src/create.rs, and tc-align's own tree walkers).

X also needs git2 network calls that do not exist yet. They would be a second network path that skips the internet switch and the proxy finder unless they are built inside tc-door43. Recommended order:
1. An architecture-check test, the cheapest step at the top of the funnel.
2. One reader per import job, decided before tc-import is written.
3. A single network gate: move reqwest out of tc-resources and put git2 transport in tc-door43.
4. One set of identifiers in tc-core, done after the branches merge.
5. Round-trip property tests for the roughly 27 small FromStr parsers.

Do not split the longest functions. They are bit-exact ports of usfm-js and wordMAP.

# Code health of the domain and foundation crates (resume-build-lane)

**Scope.** 13 crates in the plain-file snapshot of `resume-build-lane` @ f9067bc. The prefix `lane:` stands for `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/snap/resume-build-lane/`.

**Method.** Read-only. I used grep, `cargo metadata --no-deps --offline` and three Python scripts kept in `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/work/code-health/` (`strip.py`, `fnlen.py`, `dead.py`). Counts the scripts produce are labelled heuristic. Nothing was compiled or run.

## 1. Size

| Crate | src files | lib lines | inline test lines | tests/ lines |
|---|---|---|---|---|
| tc-core | 3 | 538 | 83 | 0 |
| tc-usfm | 16 | 7,490 | 1,405 | 4,510 |
| tc-versification | 9 | 2,290 | 548 | 1,744 |
| tc-burrito | 13 | 3,996 | 683 | 1,966 |
| tc-helps | 16 | 6,633 | 266 | 4,480 |
| tc-door43 | 16 | 4,688 | 1,280 | 3,485 |
| tc-door43-fake | 9 | 4,161 | 13 | 992 |
| tc-resources | 19 | 8,648 | 4,038 | 5,811 |
| tc-check | 14 | 3,347 | 234 | 4,413 |
| tc-align | 14 | 4,708 | 128 | 6,481 |
| tc-obs | 13 | 2,774 | 49 | 3,371 |
| tc-export | 5 | 755 | 0 | 1,279 |
| tc-corpus | 5 | 3,249 | 97 | 98 |

Lib lines are the lines before the first `#[cfg(test)] mod … {`.

## 2. ARCHITECTURE.md and AGENTS.md rules against the code

| Rule | Result | Evidence |
|---|---|---|
| Pure crates do no I/O (tc-usfm, tc-versification, tc-helps, tc-journal, tc-check, tc-align, tc-obs, tc-export) | **Holds** | grep `std::fs\|tokio::fs\|std::net\|reqwest\|File::\|OpenOptions` in their `src/` finds only `tc-usfm/src/derivations/tests.rs`, which is `#[cfg(test)]` (`derivations/mod.rs:202-203`). tc-burrito reads files (`files.rs:83,100`, `validate.rs:217,226`), but it is not on the pure list. |
| Only tc-door43 opens network connections (ARCHITECTURE.md:89) | **Wording drift** | AGENTS.md:16 allows tc-resources. tc-resources depends on reqwest (`Cargo.toml:15`) and calls `.send()` on the reqwest client that `tc_door43::Http::client()` hands out after the switch check (`aquifer.rs:283-293`; `tc-door43/src/http.rs:148-163`). Other network code: tc-server's loopback listener, and tc-door43-fake. |
| tc-ui depends only on tc-api and tc-core | **Holds** | cargo metadata: `tc-ui -> tc-api,tc-core`. The wasm-only dependencies are external crates. |
| Newtypes for ids, enums for states | **Mostly** | Gaps are in §3 and finding F6. Few bool bags: `tc-check/src/status.rs:33` keeps `has_selections` and `nothing_to_select` as separate bools. There is no language-tag type: ≥8 public `language: String` fields (`tc-helps/src/metadata.rs:96`, `tc-door43/src/catalog.rs:82`, `tc-resources/src/sets.rs:60` …). |
| Parsers have proptest round trips | **Big parsers yes; small id parsers no** | Big parsers (USFM, codec, versification, burrito, TSV, articles, OBS) are covered. tc-core has no proptest dev-dependency. Heuristic: these FromStr parsers do not appear in any proptest file: 8 in tc-door43 `names.rs`, 6 in tc-resources, 3 in tc-versification, 3 in tc-burrito. |
| No unwrap/expect/panic in library code without a proof | **Holds** | Library code has 0 `.unwrap()` and 0 `panic!`. expect: `tc-versification/src/standard.rs:81-86` (`#[allow]` plus a comment) and `tc-helps/src/blocks.rs:91-94` (`#[expect(reason)]`). 12 `unreachable!`, each with a reason: versification `reference.rs:262,355`, `table.rs:160`; door43 `names.rs:407`; resources `gateway_change.rs:141`, `gateways.rs:342`, `suite.rs:190,198,207,260`; obs `ids.rs:33`, `story.rs:49`. |
| Is any rule enforced mechanically? | **No** | `.github/workflows/ci.yml` runs fmt, 7 clippy variants, test, alignment-budget and parity-baseline. None checks dependencies or I/O. No test reads Cargo metadata (grep). |

Workspace lints (`Cargo.toml`): `unsafe_code = forbid`; clippy `unwrap_used`, `expect_used`, `panic`, `dbg_macro`, `todo` at warn. `unreachable`, `allow_attributes_without_reason` and `unreachable_pub` are not set.

## 3. Duplicated parsing and types

| What | Copies | Evidence |
|---|---|---|
| Aligned USFM → alignment value | 3 readers over `tc_usfm::parse_aligned`, plus 1 separate tree walker | `tc-journal/src/alignment.rs:235` `split_aligned` (production: `tc-store/src/commands.rs:1033`, STORAGE §13.5). `tc-align/src/usfm.rs:58` `from_aligned`: tests only; its doc at :7-13 says tC4's card shape differs. `tc-align/src/suggest/mod.rs:114-215` `TrainingVerse::from_aligned_usfm`: tests only, walks `Document`/`Node` itself. |
| tC4 alignment sidecar JSON | 2 | `tc-journal/src/fold/sidecar.rs:132` `decode_alignments` (outside edits; seed `tc-app/src/backend/seed/records.rs:10`). `tc-align/src/record.rs:51` `from_record`: no non-test caller. |
| OBS tQ TSV | 2 | `tc-obs/src/questions.rs:40`: lenient, "one bad row must not cost fifty stories" (:17-19); no non-test caller. Production uses the strict `tc_helps::tsv::parse_questions` (`tc-resources/src/load.rs:417`). |
| USFM marker scanning outside tc-usfm | 4 places | `tc-store/src/seed.rs:31,46` (own `markers()` scanner and `\h` reader); `tc-app/src/create.rs:143` (`skeleton`), `:334` (`id_book`); `tc-align/src/words.rs:76,189,312-369` (Document walk); `tc-align/src/suggest/mod.rs:215` (`markers::classify`). |
| USFM readers inside tc-usfm | 4, one per job (deliberate) | `Document` (lossless), `Book` (sources: `tc-app/src/sources.rs:162`, `tc-align/src/words.rs:126`), `codec` (the project's own text), `derivations/usfmjs.rs` (usfm-js port for hashes). |
| "C:V" reference text | ≥10 production parsers | `tc-core/src/lib.rs:129`; `tc-helps/src/refs.rs:305`, `tsv.rs:814,822`; `tc-check/src/derive.rs:342`, `scope.rs:161`; `tc-versification/src/spec.rs:45,149`, `versification.rs:461`; `tc-burrito/src/scope.rs:131,137`; `tc-api/src/check.rs:129`; `tc-app/src/understand.rs:664` (re-parses text it just printed); `tc-ui/src/app/translate.rs:267`, `align.rs:130`. Several are deliberate tC4 ports. |
| Digit-only number parsers | ≥7, with different leading-zero rules | `tc-helps/src/refs.rs:259`; `tc-versification/src/spec.rs:307,318`; `tc-usfm/src/book.rs:596`, `codec.rs:630`; `tc-api/src/check.rs:148`; `tc-resources/src/lexicon.rs:182`. |
| JavaScript whitespace / trim | 6 identical copies, 1 equivalent | `tc-api/src/model.rs:96,111`; `tc-app/src/js.rs:7,22`; `tc-check/src/js.rs:7`; `tc-helps/src/blocks.rs:126,141`; `tc-resources/src/strongs.rs:171`; `tc-obs/src/js.rs:8,23`; `tc-usfm/src/codec.rs:261`. The codec version is equivalent by the Unicode White_Space set (inference). |
| StoryNo / FrameNo | 3 / 2 | `tc-api/src/ids.rs:433`, `tc-journal/src/ids.rs:319,333`, `tc-obs/src/ids.rs:14,46`. Fallible converters: `tc-app/src/story.rs:276`, `create.rs:132`, `conflicts.rs:124`. |
| Chapter and verse numbers | 3 shapes | `NonZeroU16` newtypes in `tc-journal/src/ids.rs:289,293`. Public `u16` fields in `tc-core/src/lib.rs:111-115` and `tc-helps/src/refs.rs:9-12`. `u16` with 0 allowed in tc-versification (documented at `spec.rs:10-11`). |
| Git object id | 2 | `tc_core::GitOid` (`digest.rs:109-115`) and `tc-door43/src/git_tree.rs:18` `ObjectId` (FromStr `Err = ()`, accepts uppercase). |
| Print model | twin | `tc-api/src/model/print.rs:1-13` declares itself "the twin of tc-export's print parts". 16 items duplicate `tc-export/src/page.rs` and `book.rs`. They are held equal by `tc-api/tests/print.rs:252`. |
| Atomic write and held-file retry | 2 (tracked) | `tc-resources/src/store.rs:1106-1150` with `held.rs`, against `tc-store/src/atomic.rs:106`. `held.rs:9-11` says they become one helper once storage lands. |

## 4. Public API surface

Heuristic: counts of `pub` declarations in non-test source, including items inside private modules.

| Crate | pub fn | struct | enum | trait | pub mod | pub use | pub fields | pub fns with no non-test reference |
|---|---|---|---|---|---|---|---|---|
| tc-core | 20 | 4 | 3 | 0 | 0 | 2 | 6 | 1 |
| tc-usfm | 81 | 27 | 10 | 0 | 7 | 6 | 63 | 4 |
| tc-versification | 69 | 17 | 12 | 0 | 0 | 8 | 21 | 7 |
| tc-burrito | 78 | 37 | 26 | 1 | 0 | 13 | 122 | 7 |
| tc-helps | 136 | 47 | 42 | 0 | 9 | 6 | 146 | 26 |
| tc-door43 | 60 | 38 | 14 | 2 | 3 | 16 | 108 | 6 |
| tc-resources | 195 | 54 | 34 | 2 | 6 | 14 | 150 | 13 |
| tc-check | 74 | 25 | 15 | 0 | 11 | 13 | 93 | 5 |
| tc-align | 109 | 20 | 8 | 2 | 10 | 11 | 78 | 10 |
| tc-obs | 75 | 20 | 15 | 0 | 10 | 3 | 53 | 16 |
| tc-export | 19 | 15 | 19 | 0 | 3 | 1 | 46 | 5 |

That is 100 public fns with no reference in any crate's non-test source (names ≥6 characters, common names excluded). Examples: `tc-obs/src/template.rs:401` `create_obs_project`; `story.rs:433` `write_frame`; `tc-align` `from_aligned`, `from_record`, `weave_verse`, `from_book`; `tc-export` `print_book`, `print_stories` (for D2); `tc-versification` `from_vrs`, `to_vrs`; `tc-burrito/src/build.rs:309,329` `check_field`, `check_scope` (these return `Result<(), String>`).

## 5. Largest files and functions

Largest files by total lines: `tc-usfm/src/derivations/usfmjs.rs` 2,631; `tc-resources/src/store.rs` 2,112 (1,154 lib); `tc-corpus/src/history.rs` 1,803; `tc-door43-fake/src/api.rs` 1,590; `tc-usfm/src/align.rs` 1,433 (876 lib).

Functions (heuristic, brace counting): 2,297 non-test fns; 102 are over 50 lines and 18 over 100. The top three are ports that must stay bit-exact:

| Function | Lines | Kind |
|---|---|---|
| `usfmjs.rs:1242` `end_span` | 207 | usfm-js port |
| `tc-align/src/suggest/wordmap.rs:323` `run` | 203 | wordMAP port; floating-point order matters (:1-9) |
| `usfmjs.rs:1783` `to_json_chunk` | 182 | usfm-js port |

Longest functions that are not ports: `tc-helps/src/highlight.rs:305` (131), `tc-usfm/src/codec.rs:478` `decompose` (126), `tc-burrito/src/build.rs:148` (114), `tc-versification/src/versification.rs:371` `from_vrs` (114), `tc-resources/src/store.rs:876` `install_in` (113).

## 6. Ranked simplification and hardening work

| # | Funnel layer | Work | Why now | Size |
|---|---|---|---|---|
| 1 | Checks | New publish=false crate `tc-arch` with tests that run `cargo metadata --no-deps --offline` and scan sources. Each failure names its rule: (a) tc-ui's normal deps ⊆ {tc-api, tc-core}; (b) reqwest, hyper and git2 normal deps only in tc-door43; (c) pure crates have no tokio, reqwest, git2 or zip normal dep and no `std::fs`, `tokio::fs`, `std::net` in non-test src; (d) `tc_usfm::Document` and `Node` only inside tc-usfm (allowlist tc-align until #8). Fix ARCHITECTURE.md:89 and the crate table (tc-corpus missing; tc-sync and tc-import listed at :77-78 but absent). | X adds git2, D2 adds producers to a pure crate, I adds a crate. | S |
| 2 | Code + Checks | One reader per import job, decided before tc-import is written. First write differential tests: `decode_alignments` against `from_record` over the 3,194 tC4 fixture records cited at `tc-align/src/record.rs:20`; `split_aligned` against `from_aligned` on en_ult Titus. Then keep one of each and move test-only readers into test support. | I/F1 | M |
| 3 | Code | OBS tQ: either call `tc_obs::questions::parse_story_questions` from `tc-resources/src/load.rs:417`, or delete it. Add a load test with an `x` occurrence row. | This is translator-visible: one bad row hides every question. | S |
| 4 | Code | Keep a single network gate. tc-door43 gains `Http::get(url, accept)` and a redirect-rule type, and reqwest leaves `tc-resources/Cargo.toml:15`. X then adds `tc_door43::git::{fetch, push, ls_remote}` on the `git2::Repository` that tc-store opened. Each checks the `InternetSwitch` before the call and in its progress callbacks, and takes its proxy from the same `ProxyFinder` (`http.rs:47-60`). | X | M |
| 5 | Code | One set of identifiers in tc-core: `ChapterNo`, `VerseNo`, `StoryNo`, `FrameNo`, built on `NonZeroU16` where 0 is illegal. Make the fields of `Reference` and `ChapterVerse` private; `VerseSpan::single` refuses 0. Delete the 4 duplicate types and the 3 fallible converters. Fold `ObjectId` into `GitOid`. Display forms stay the same, so the wire format does not change. | Do it after the branches merge, since it touches tc-journal, tc-obs, tc-api and tc-app. | M |
| 6 | Checks | Round-trip proptests for every small FromStr: `parse(x.to_string()) == x`. Today `Reference` takes "TIT +1:1" (`lib.rs:130` uses `u16::parse`) while `VerseSpan` refuses "+1" (`lib.rs:80-83`, test :165). `VerseSpan::single(0)` prints "0", which its own parser refuses. | Cheap | S |
| 7 | Code | `tc_core::js::{is_space, trim}`; delete the 6 copies. tc-usfm's UTF-16 version in `derivations/js.rs:40` stays. | Cheap | S |
| 8 | Code | Move USFM text rules into tc-usfm: `seed_book` and `source_book_name` from tc-store, `skeleton` and `id_book` from tc-app, and tc-align's two Document walkers become tc-usfm functions. Then tighten rule 1(d). | | M |
| 9 | Code | Before D2: define the page-setup and print-part types once (tc-api's model is pure and builds for wasm) and have tc-export use them. Keep the differential test until then. The rail twin (`tc-api/src/model/rail.rs` against `tc-check/src/rail.rs`) is a deliberate fast path checked against an oracle, so keep it. | D2 | M |
| 10 | Checks | Workspace lints: clippy `unreachable`, `allow_attributes_without_reason`; rust `unreachable_pub`. Change the 3 bare `#[allow]` (`tc-versification/src/standard.rs:81`, `tc-resources/src/dcs.rs:141`, `choice.rs:36`) to `#[expect(…, reason)]`. | | S |
| 11 | Code | The 100 unused public fns: each one either names the unit that will use it (D2, I, X) or is deleted. | | S–M |
| 12 | Code | After S2 lands: one atomic-write and held-retry helper, in a small crate that tc-store and tc-resources both use. | | S |
| — | Leave | Do not split the ported functions in §5. Fixture vectors guard them. | | |

## 7. What the remaining units need from these crates

| Unit | Uses | What exists | Gap |
|---|---|---|---|
| D2 Exports | tc-export, tc-usfm, tc-journal, tc-align, tc-obs, tc-burrito | Print model: `tc-export/src/book.rs:65,209,264`, `stories.rs:102`, `page.rs:119`. Plain project USFM: `tc-journal/src/fold/project.rs:501` `book_usfm`. `tc_usfm::weave` and `strip_alignment`. `AlignmentValue::usfm_groups`. `tc-burrito/src/build.rs:266` `build_metadata`. | No producers yet (`tc-export/src/lib.rs:1-5`). tc-export has no error type and depends only on tc-obs. Aligned book USFM needs weaving per valid alignment, and two weave entry points exist (`tc-align/src/usfm.rs:33`, `tc-journal/src/alignment.rs:270`); pick one. tc-export must stay I/O-free and hand bytes to tc-store or the download stream. The print twin is item 9. |
| I / F1 Import | tc-usfm, tc-journal, tc-align, tc-burrito, tc-versification, tc-obs | `codec::decompose` (`codec.rs:478`) with line-numbered refusals. `split_aligned`, `decode_alignments`/`decode_decisions` (`sidecar.rs:132,230`), `records_from_usfm` (:342). `from_record`. `Versification::from_json`/`from_vrs` (`versification.rs:257,371`). The tC4 seed (`tc-app/src/backend/seed.rs` plus `seed/`, 4,231 lines, feature `fixture-backend` only at `backend/mod.rs:41-42`), which AD7 replaces with I (`seed.rs:39-40`). | The tc-import crate is absent. No converter for tC4 4.0 journal events (§13.1); only the `ImportSource` enum exists. Duplicate readers (item 2). No production zip reader or writer for burritos beyond tc-door43's unpack. |
| X Sync and USB | tc-door43, tc-door43-fake, tc-journal, tc-burrito, tc-usfm, tc-obs, tc-store | Token and repo REST: `tc-door43` `secret.rs`, `repos.rs`. Push refusal parsing: `push.rs`. A fake smart-HTTP git server: `tc-door43-fake/src/smart_http.rs`, `git.rs`. Fold, intake and conflicts are done in J2. | git2 is a dev-dependency only (`tc-door43/Cargo.toml:33-35`). There are no fetch, push or list calls. Local git exists only on s2-proof-executor (`tc-store/Cargo.toml:36-42` there). No production `ZipWriter` anywhere (the only ones are in `#[cfg(test)]` or fixture-backend code). tc-sync is absent. STORAGE §9.1 S3 uses `proxy_auto()`, which bypasses the PAC/WPAD finder FIX-proxy built (owner decision, item 4). |

## Findings
- [F1] (high/high) ARCHITECTURE.md's dependency and I/O rules hold today, except the wording of the network rule, but no CI step or test enforces any of them.
  - evidence: cargo metadata --no-deps --offline in lane snapshot: tc-ui -> tc-api,tc-core; pure crates have no tokio/reqwest/git2/zip normal deps; grep std::fs|tokio::fs|std::net|reqwest|File::|OpenOptions in src of tc-usfm/tc-versification/tc-helps/tc-check/tc-align/tc-obs/tc-export/tc-journal: only lane:crates/tc-usfm/src/derivations/tests.rs (cfg(test) at derivations/mod.rs:202-203); lane:.github/workflows/ci.yml steps: fmt, clippy (7 variants), test, alignment-budget, parity-baseline; none checks dependencies; lane:docs/ARCHITECTURE.md:89 says only tc-door43 opens network connections; lane:AGENTS.md:16 allows tc-resources too; ARCHITECTURE.md:77-78 lists tc-sync and tc-import, which do not exist; tc-corpus is missing from the table
  - rec: Add a publish=false crate tc-arch whose tests run cargo metadata and a source scan, each failure naming the rule: tc-ui deps; reqwest/hyper/git2 only in tc-door43; no I/O deps or std::fs/tokio::fs/std::net in pure crates; tc_usfm::Document/Node only inside tc-usfm (allowlist tc-align until moved). Fix ARCHITECTURE.md:89 and the crate table.
- [F2] (medium/high) tc-resources uses reqwest directly. Requests still pass the internet switch through tc_door43::Http, but reqwest types cross the crate boundary, so 'reqwest only in tc-door43' cannot be checked.
  - evidence: lane:crates/tc-resources/Cargo.toml:15 reqwest dependency; lane:crates/tc-resources/src/aquifer.rs:21-22,209,281-293,303,429 use reqwest StatusCode, HeaderMap, redirect::Policy, Response and .send(); lane:crates/tc-door43/src/http.rs:148-163 Http::client() checks the switch, then returns &reqwest::Client
  - rec: Give tc-door43 a typed Http::get(url, accept) returning its own response type, plus a redirect-rule type. Remove reqwest from tc-resources so the tc-arch rule can be absolute.
- [F3] (high/high) Unit X needs git2 fetch, push and list calls that do not exist. As STORAGE §9 sketches them (libgit2 proxy_auto), they would be a second network path that skips tc-door43's ProxyFinder.
  - evidence: lane:crates/tc-door43/Cargo.toml:33-35 git2 is under [dev-dependencies]; no git2:: in tc-door43/src; lane:crates/tc-door43/src/push.rs only interprets refused-push text; lane:docs/STORAGE.md §9 (lines 3230-3237) puts fetch/push/list in tc-door43; §9.1 S3 uses proxy_options(proxy_auto()); lane:crates/tc-door43/src/http.rs:47-60 ProxyFinder/system_proxy_finder; git2 is in tc-store only on s2-proof-executor (snap/s2-proof-executor/crates/tc-store/Cargo.toml:36-42)
  - rec: Add tc_door43::git::{fetch, push, ls_remote} taking the git2::Repository that tc-store opened. Check InternetSwitch before each call and inside transfer/push callbacks, take proxy URLs from the same ProxyFinder, and test against tc-door43-fake smart_http.rs. Ask the owner to confirm the proxy rule for git.
- [F4] (high/high) Import has duplicate readers for one job. In two cases the reader the app uses and a tC4-faithful reader both exist, and the tC4-faithful one is test-only.
  - evidence: lane:crates/tc-journal/src/alignment.rs:235 split_aligned, used by lane:crates/tc-store/src/commands.rs:1033 (STORAGE §13.5); lane:crates/tc-align/src/usfm.rs:58 from_aligned: tests only; doc at :7-13 says tC4 keeps one card per outermost source word, unlike tc-usfm's per-run groups; lane:crates/tc-journal/src/fold/sidecar.rs:132 decode_alignments (outside edits; tc-app seed records.rs:10) against lane:crates/tc-align/src/record.rs:51 from_record (no non-test caller); lane:crates/tc-align/src/suggest/mod.rs:114-215 TrainingVerse::from_aligned_usfm: tests only, with its own Document walker
  - rec: Before writing tc-import, add differential tests: decode_alignments against from_record over tC4's 3,194 fixture records, and split_aligned against from_aligned on en_ult Titus. Keep one reader per job and move test-only readers to test support. Check in the aligner whether split_aligned's per-run groups show a source word twice.
- [F5] (medium/high) For OBS projects the app loads translation questions with tc-helps' strict TSV reader. tc-obs' lenient tC4-faithful reader, written so one bad row does not hide all questions, is never called.
  - evidence: lane:crates/tc-obs/src/questions.rs:17-19,40 parse_story_questions; grep finds callers only in tc-obs/tests; lane:crates/tc-resources/src/load.rs:417 uses tc_helps::tsv::parse_questions for questions parts; Scan of git show resume-build-lane:bundle/en/en_obs-tq_v10.sb.zip OBS.tsv: 672 data rows, all Reference N:N, all IDs valid, Occurrence empty in all, so the bundled table is not affected
  - rec: Route OBS questions through tc_obs::questions::parse_story_questions, or delete it and record the decision. Add a tc-resources load test with an 'x' occurrence row.
- [F6] (medium/high) Identifier types are duplicated across crates, which forces fallible conversions in tc-app. Chapter and verse numbers have three shapes, and only tc-journal's enforces non-zero.
  - evidence: StoryNo: lane:crates/tc-api/src/ids.rs:433, tc-journal/src/ids.rs:319, tc-obs/src/ids.rs:14; FrameNo: tc-journal/src/ids.rs:333, tc-obs/src/ids.rs:46; ChapterNo/VerseNo NonZeroU16 only in tc-journal/src/ids.rs:289,293; tc-core/src/lib.rs:111-115 Reference has pub chapter: u16; tc-helps/src/refs.rs:9-12 ChapterVerse has pub u16 fields although new() refuses 0; Converters: tc-app/src/story.rs:276, create.rs:132, conflicts.rs:124; Git oid twice: tc-core digest.rs GitOid and tc-door43/src/git_tree.rs:18 ObjectId (FromStr Err = ())
  - rec: After the branches merge, move ChapterNo, VerseNo, StoryNo and FrameNo into tc-core (wasm-safe and already a dependency of every crate). Make Reference and ChapterVerse fields private, delete the duplicates and converters, and fold ObjectId into GitOid. Display forms stay the same.
- [F7] (medium/high) tc-core's parsers have no property tests, and they accept spellings they never print. About 20 other small FromStr parsers have no direct round-trip property either.
  - evidence: lane:crates/tc-core/Cargo.toml dev-dependencies: serde_json only; lane:crates/tc-core/src/lib.rs:130 parses the chapter with u16::parse, which accepts a leading '+', so 'TIT +1:1' parses; VerseSpan refuses '+1' (lib.rs:80-83, test :165); lib.rs:32-37 VerseSpan::single(0) builds a value whose Display '0' its own FromStr refuses; Heuristic: no proptest file references tc-door43 names.rs (8 FromStr), tc-resources (6), tc-versification BookCode/VerseRef/RefPart, tc-burrito IngredientPath/MimeType/ScopeBook
  - rec: Add proptests: parse(x.to_string()) == x for every id type, and parse(s) ok implies to_string() is the one canonical spelling. Fix the chapter parse and VerseSpan::single(0).
- [F8] (low/high) The ECMAScript whitespace rule and JS trim are copied in 6 crates.
  - evidence: Identical is_space/js_space: lane:crates/tc-api/src/model.rs:96, tc-app/src/js.rs:7, tc-check/src/js.rs:7, tc-helps/src/blocks.rs:126, tc-resources/src/strongs.rs:171, tc-obs/src/js.rs:8; Equivalent formula: tc-usfm/src/codec.rs:261 (equivalence inferred from Unicode White_Space minus U+0085 plus U+FEFF); trim copies: tc-api model.rs:111, tc-app js.rs:22, tc-helps blocks.rs:141, tc-obs js.rs:23
  - rec: Add tc_core::js::{is_space, trim} with one table test and delete the copies. Keep tc-usfm's UTF-16 version (derivations/js.rs:40).
- [F9] (medium/high) tc-api holds a twin of tc-export's print model: 16 types and print_parts, held equal by a differential test. D2 will build producers on tc-export's copy.
  - evidence: lane:crates/tc-api/src/model/print.rs:1-13 says it is 'the twin of tc-export's print parts'; items at :19-383; lane:crates/tc-export/src/page.rs:8-170 and book.rs:109-264; lane:crates/tc-api/tests/print.rs:252 print_parts_is_tc_exports_on_every_preview_page; lane:crates/tc-export/Cargo.toml:9-10 depends only on tc-obs
  - rec: Before D2, define page setup and print parts once (tc-api's pure model) and have tc-export use them. Keep the rail twin, which is a deliberate fast path checked against an oracle.
- [F10] (medium/high) USFM text rules are implemented outside tc-usfm in four places.
  - evidence: lane:crates/tc-store/src/seed.rs:31 source_book_name, :46 markers() (port of tC4 MARKER_RE); lane:crates/tc-app/src/create.rs:143 skeleton, :334 id_book; lane:crates/tc-align/src/words.rs:16,76,189,312-369 and suggest/mod.rs:118-215 walk tc_usfm::Document/Node and markers::classify
  - rec: Move these into tc-usfm as named pure functions (seed_book, blank_book, book_id, source words, aligned walk), then ban Document/Node outside tc-usfm in tc-arch.
- [F11] (low/high) Panic discipline is clean, but unreachable!, #[allow] without a reason, and needless pub are not lint-enforced.
  - evidence: Non-test library code of the 13 crates: 0 .unwrap(), 0 panic!; expect only at tc-versification/src/standard.rs:81-86 and tc-helps/src/blocks.rs:91-94; 12 unreachable! sites, all with a reason (list in report §2); lane:Cargo.toml [workspace.lints.clippy] has unwrap_used, expect_used, panic, dbg_macro, todo only; Bare #[allow]: tc-versification/src/standard.rs:81, tc-resources/src/dcs.rs:141, choice.rs:36
  - rec: Add clippy unreachable and allow_attributes_without_reason, and rust unreachable_pub, as warn (CI denies warnings). Change the bare #[allow] attributes to #[expect(..., reason)].
- [F12] (low/medium) About 100 public functions in these crates are not referenced by any non-test source in the workspace.
  - evidence: Heuristic scan (dead.py, names of 6 or more characters, common names excluded): tc-helps 26, tc-obs 16, tc-resources 13, tc-align 10, tc-burrito 7, tc-versification 7, tc-door43 6, tc-check 5, tc-export 5, tc-usfm 4, tc-core 1; Examples: tc-obs/src/template.rs:401 create_obs_project, story.rs:433 write_frame (used only in tc-app seed tests), tc-burrito/src/build.rs:309,329 check_field/check_scope returning Result<(), String>
  - rec: For each function, add a doc line naming the unit that will use it (D2, I or X), or delete it. unreachable_pub keeps new surface small.
- [F13] (low/high) The longest functions are bit-exact ports and should not be split.
  - evidence: usfmjs.rs:1242 end_span 207 lines; tc-align/src/suggest/wordmap.rs:323 run 203; usfmjs.rs:1783 to_json_chunk 182; wordmap.rs:1-9 says confidences follow wordMAP's order of floating-point operations; 2,297 non-test fns: 102 over 50 lines, 18 over 100 (brace-count heuristic)
  - rec: Leave the ports alone. Split non-port long functions only when a unit touches them: codec.rs:478 decompose (126), resources store.rs:876 install_in (113), burrito build.rs:148 (114).
- [F14] (high/high) The remaining units need missing pieces from these crates: tc-import, tc-sync, git2 transport, export producers, a production zip writer and a tC4 4.0 event converter.
  - evidence: crate list of lane snapshot has no tc-import or tc-sync; ZipWriter appears only in #[cfg(test)] code (tc-resources aquifer.rs:767+, tc-door43 unpack.rs:453+) or fixture-backend code (tc-app/src/backend/mod.rs:38-39); tc-export has 0 thiserror types; lib.rs:1-5 says the producers come later; No Rust reader of tC4 journal events found; only the ImportSource enum (tc-journal change.rs/canon.rs); tc-app/src/backend/seed.rs plus seed/ is 4,231 lines, behind feature fixture-backend (backend/mod.rs:41-42); seed.rs:39-40 says unit I replaces it
  - rec: Use the unit-to-crate table in report §7 as the dependency map for D2, I/F1 and X. Do items 1-4 and 9 of report §6 before those units start.
- [F15] (low/high) Atomic write and held-file retry are implemented twice. This is known and tracked.
  - evidence: lane:crates/tc-resources/src/store.rs:1106-1150 replace_synced/write_synced/sync_dir and held.rs; lane:crates/tc-store/src/atomic.rs:106 atomic_write; lane:crates/tc-resources/src/held.rs:9-11 says they become one helper when the storage branch lands
  - rec: After S2 merges, move atomic write and held retry into one small crate that tc-store and tc-resources both depend on. tc-resources cannot depend on tc-store because of layering.

## Unverified
- Nothing was compiled or run (the task forbids it), so I don't know how many warnings the proposed lints (unreachable_pub, clippy::unreachable) would raise. Run cargo clippy with them once the heavy-test slot is free.
- The dead-API, function-length, pub-count and index-expression numbers come from heuristic scripts, not the compiler. They can miss calls through traits or macros. Confirm with rustc's dead_code after making items pub(crate), or a cargo public-api diff.
- I did not test whether split_aligned's per-run groups make the aligner show one source word on two cards. Add en_ult Titus as a USFM book (reachable through Add books) and open TIT 1:1 in Align.
- I did not check whether tc_journal::fold::decode_alignments and tc_align::record::from_record agree on edge cases such as string occurrences, fractions and extra fields. A differential test over tC4's tC3 and burrito import fixtures would settle it.
- I don't know whether published non-English OBS tQ tables contain rows that tc-helps' strict reader refuses. The bundled en_obs-tq v10 does not. Sample the Door43 catalog.
- Whether git sync must use tc-door43's ProxyFinder (PAC/WPAD) rather than libgit2's proxy_auto is a design question for the owner. STORAGE §9.1 shows proxy_auto.
- The status header in the snapshot's .audit/STATUS.md says it was generated at 3eac396 and reports 37 of 48 units, while the task says 38. I did not reconcile this; it is outside this dimension.
- I did not run git merge-tree for lane against s2-proof-executor in these crates. diff -rq of the snapshots shows lane and s2 differ in 24 tc-door43 files, 25 tc-align, 16 tc-usfm, 16 tc-resources and 11 tc-helps, and s2 has no tc-export, so expect conflicts when they merge.