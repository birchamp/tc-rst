# The enforcement funnel

Every bug, function and feature goes through four layers, from the top down. Spend the most
effort at the top. Use a lower layer only for what the layers above it cannot carry.

1. **Code.** Make the wrong thing impossible: simpler functions, types that encode invariants,
   one entry point per job, explicit state models.
2. **Checks.** Catch automatically what code cannot prevent: tests, lint rules, type checks, CI,
   scripted runs of the real app. Each check is deterministic, fast, tests behavior rather than
   implementation, and names the violated invariant when it fails.
3. **Prose.** Short comments or `AGENTS.md` notes, only for context that code and checks cannot
   carry.
4. **Human.** Product direction, risk, genuine ambiguity: the owner decides.

A fix that is only a test is a fix at the second layer. It is allowed only when the card says
why no type or structure can carry the rule.

## 1. The funnel card

Every work item's card has a section `## Funnel card`. Fill it in **before** you write product
code, commit it, and keep it current. A reviewer reads it first. Copy this template:

```markdown
## Funnel card

- **Invariants** (one sentence each; give each a name you will reuse in tests and messages):
  - INV-<NAME>: <the rule that must always hold>
- **Wrong states** this work could create, or that exist today:
  - <state> (how a translator would meet it)
- **Code** (make them impossible):
  - Types: <newtype / enum / state machine / private constructor / capability token>
  - One entry point: <the single function every caller goes through>
  - Removed: <duplicates, flags, old paths this work deletes>
  - Left to checks, and why a type cannot carry it: <rule: reason>
- **Checks** (catch the rest):
  - <test name, named after the invariant> in <file>; kind: unit | property | model | crash |
    count | contract | journey | lint | architecture; runs in: <CI job>
  - Proof it can fail: <the mutation or seeded bug, and the failure it printed>
- **Prose** (only if needed): <file: the one non-obvious reason>
- **Human** (only if needed): <question, options and their effect on translators,
  recommendation, ruling file once answered>
```

## 2. How a bug goes through the funnel

1. **Reproduce it through a reachable scenario.** Write a test through the public API that fails
   today. Name it after the invariant it breaks
   (`a_save_never_erases_a_newer_draft_of_its_field`, not `test_bug_123`). If you cannot reach it
   through the app's screens, its API, a crash, another program, a teammate's work, a full disk
   or a dropped network, it is not a bug for this project: drop it.
2. **Ask what structure allowed it.** Was a state representable that should not be? Was there a
   second path that skipped the rule? Was a flag standing in for a state? Was an error matched
   with a wildcard? Write the answer in the funnel card.
3. **Fix the structure first.** Change the type, remove the second path, or turn flags into an
   enum, so the bug cannot be written again. Then fix the instance if anything is left.
4. **Prove the test can fail.** Put the bug back (a mutation, or a revert of the fix) and show
   the test fails with a message that names the invariant. Restore the fix. Record the failing
   output in the card's log.
5. **Look for its siblings.** Search for the same pattern elsewhere (`grep` the call, the flag,
   the wildcard). Fix them in the same item, or open a card for each.
6. **Add a check if it is a rule other code must follow.** If the bug came from breaking a rule
   (writing a file directly, a network call outside `tc-door43`, a new bag of booleans), add or
   extend a lint or architecture test so the next author cannot break it.
7. **Prose and Human only last.** A comment explains a reason the code cannot show. An owner
   question is only for what a translator would notice.

## 3. How a feature or function goes through the funnel

1. **Name its invariants** before any code: what must always hold for the translator.
2. **Find the existing structure it should use.** Most remaining units need one that exists or is
   being built once in M1: typed jobs (UI-1), ports (UI-2), one leave path and one network helper
   (UI-3), one network gate (DOM-1), one reader per job (DOM-2), the store's single write entry
   `ProjectHandle::submit`, the save machine `Saves`, the atomic helpers in `tc-store`. If you are
   about to write a loop, a flag or a parser that already exists, stop and reuse it.
3. **Model decisions about a translator's work as a typed state machine**: a `State` enum, an
   `Event` enum, one pure exhaustive `next(state, event) -> (state, actions)`, and an executor
   that does the I/O. Prove it with a model-based property test that includes crashes and
   restarts. `crates/tc-store/src/ledger.rs` on the S2 branch is the example to copy.
4. **Write the checks** that prove the invariants, and make sure they run in CI.
5. **Port the tC4 behavior and its known bugs' tests.** The card lists the tC4 issues the item
   carries (`docs/TC4-ISSUES.md`). Each becomes a red-first test.

## 4. The Code layer: patterns this repository uses

| Pattern | Use it when | Example in the code |
|---|---|---|
| Typed state machine (State, Event, pure `next()`, executor, model test) | Logic decides about a translator's work, or a lifecycle has more than two states | `tc-store/src/ledger.rs` (S2 branch); `tc-api/src/model/draft.rs` `next` |
| Newtype for an identifier | Any id or number with rules (non-zero, a format) | `BookId`, `ActorId`, `OpId`; DOM-2 moves `ChapterNo`, `VerseNo`, `StoryNo`, `FrameNo` into `tc-core` |
| Enum instead of booleans | Two or more flags that describe one state | AD `FieldSave`; UI-3 turns `busy`/`sent`/`outcome` into one enum. C7a's `KeepPhase` (`crates/tc-ui/src/app/review.rs`) is the first instance of a network-reply state that UI-3 generalises into `NetEnd<T>` |
| One entry point per job | Every write, every network call, every job follow, every leave | `ProjectHandle::submit`; `tc-store` `atomic_write`; UI-1 `follow`; UI-3 `leave` and `run_net` |
| Capability token | Only some code may do something (write a project, use the network, edit a read-only project) | `WriteCap` (UI-3 makes editors require it) |
| Exhaustive match, no wildcard | Classifying errors, refusals, outcomes | `tc-ui/src/app/save/classify.rs` ("no wildcard") |
| Parse at the boundary | Files, HTTP, JSON, USFM, TSV, user input | `tc-usfm` codec; `tc-api` wire types refuse unknown keys |
| Pure core, I/O at the edges | Everything that can be computed | `tc-journal`'s fold; `tc-check`; `tc-align`; `tc-export` |
| Delete the duplicate | Two readers, two copies of a rule | DOM-2 (one reader per import job); S3 (one `preserve` helper) |

## 5. The Checks layer

### Rules for every check

- **Deterministic.** No wall-clock assertions: inject a clock or use tokio's paused time. No
  sleep-then-assert: wait on a signal. CI uses a fixed `PROPTEST_RNG_SEED`; a nightly job uses
  random seeds and prints them.
- **Fast.** No test binary over 300 s in CI. Long variants (model seeds, 4,096 property cases,
  whole-Bible runs) go to the nightly job.
- **Behavior, not implementation.** Test through the public API with real fixtures from
  `fixtures/`. A test fails if the behavior it names breaks, and only then.
- **Names the invariant.** The test's name is the invariant. Its failure message starts with
  `INV-<NAME>:` or the test name, then says what broke and where:
  `INV-ATOMIC-WRITES: crates/tc-app/src/x.rs:12 writes a project file with std::fs::write; use
  tc_store::atomic_write`. A clippy lint cannot carry a custom message, so it names its invariant
  in the comment above it in `Cargo.toml` (`# INV-NO-BAGS-OF-BOOLEANS`) and a `disallowed-*`
  entry names it in `reason`; `board.py`'s messages start with `INV-TRACKER-TRUE:`. One name per
  rule: the registry is `python3 -I .audit/tools/board.py --invariants`.
- **Runs in CI.** A check that runs only on someone's machine is a note, not a check. A
  local-only check needs a card that moves it into CI.

### Kinds of check, and when to use each

| Kind | Use for | Where it lives |
|---|---|---|
| Unit test through the public API | One rule of one module | `crates/<crate>/tests/` |
| Property test (`proptest`) | Parsers (round trip), merges (convergence, no loss) | `tests/properties.rs` per crate |
| Model-based property test | Every typed state machine, with crashes and restarts | `tc-store/tests/store/model.rs` (S2) |
| Crash injection | Every write point of save, sync, import, checkpoint (P4) | `FaultFs`; `tc-store/tests/store/crash.rs` |
| Operation count | Interactive actions cost what they touch (PERFORMANCE.md) | `tc-app/tests/op_counts/` |
| Contract test | A seam with two implementations (fake and real port, memory and store backend) | `tc-app/tests/store_contract.rs` |
| Journey | A tC4 journey end to end in the real app (P1) | `parity/app/journeys/*.test.mjs` |
| Parity gate | Same look as tC4 (P7) | `crates/tc-ui/tests/screen_record.rs` |
| Lint | A rule about how code is written | `clippy.toml`, `[workspace.lints]` |
| Architecture test | Crate edges, I/O and network boundaries | `crates/tc-arch` (CHK-1) |
| Ledger test | A document's claim that a test exists | CHK-3 |

### Checks the repository must have (built in M0 and M1)

| Check | Invariant it holds | Item |
|---|---|---|
| CI green on main, toolchain pinned, `--locked` | INV-CI-IS-THE-GATE | INT-2 |
| One gate list read by CI and every local runner; a drift test | INV-LOCAL-PASS-MEANS-CI-PASS | INT-3 |
| A merge needs a passing gate for the exact tree | INV-NO-UNGATED-MERGE | INT-3 |
| Browser journeys in CI; a skipped test fails the job | INV-A-SKIP-IS-A-FAILURE (the name G5's job prints), INV-JOURNEYS-RUN (P1) | G5 |
| Crate edges, I/O in pure crates, network only in `tc-door43`/`tc-resources`, shared dependency versions | INV-ARCHITECTURE | CHK-1 |
| No `std::fs` mutation in storage crates outside the atomic helpers | INV-ATOMIC-WRITES | CHK-1 (DOM-3 makes it compile-time) |
| Low-level parser and codec types stay in their crate: no crate outside `tc-usfm` names `tc_usfm::{Document, Node, Marker, Milestone, Span}`; `tc_journal::pack` is read only by `tc-store` (and test scaffolding on the allow list); `tc_door43::http` has no caller outside `tc-door43` | INV-ONE-PARSER-OWNER | CHK-1 |
| Every `tc-server` route is behind the guard (origin, token, method) | INV-LOCAL-SERVER-IS-CLOSED | CHK-1 |
| Every project a test writes is a valid Burrito | INV-VALID-BURRITO (P5) | CHK-2 |
| Every test a document names exists | INV-CLAIMED-TESTS-EXIST | CHK-3 |
| Every live tC4 journey maps to a journey test | INV-P1-COVERED | CHK-3 |
| Fixed seeds, virtual time, no binary over 300 s | INV-DETERMINISTIC, INV-FAST | CHK-4 |
| Long model seeds, mutation recipes, Windows tests, no network in tests | INV-NO-LOSS-LONG, INV-NO-NETWORK-IN-TESTS | CHK-5 |
| `clippy::unreachable`, `allow_attributes_without_reason`, `struct_excessive_bools` (with today's allowlist) | INV-NO-BAGS-OF-BOOLEANS, INV-PROVEN-PANICS | CHK-6 |
| P7 captures match the current UI source | INV-CAPTURES-FRESH (P7) | CHK-6 |
| The board's cards are consistent with git | INV-TRACKER-TRUE | INT-5 (`board.py --check`) |
| Funnel card filled before building: `Types`, `One entry point`, `Removed`, `Left to checks` and `Proof` hold text once a card is `building` | INV-FUNNEL-FIRST | INT-5 (`board.py --check`) |

## 6. The Prose layer

- A comment explains a non-obvious reason. It never narrates what the code does.
- A rule goes in `AGENTS.md` only when no check can hold it, and then one line.
- Design documents (`docs/`) record decisions and declared differences from tC4. When the code
  changes a declared difference, the same commit changes the document.
- Cards record what was done, with evidence (commit, command, result). They are not proof: the
  checks are.

## 7. The Human layer

- The owner answers only what a translator would notice, or what only the owner can give
  (accounts, credentials, money, people, product direction).
- Each question has options, the effect of each on the person using the app, and a
  recommendation. One question per decision.
- An answer becomes a file in `rulings/` (`YYYY-MM-DD-<slug>.md`): the question, the answer, the
  date, and the cards it unblocks. A card that needed it links to it.
- Technical choices are decided by the agent and recorded in the card's log, not asked.

## 8. The review gate

A verifier reviews each item before it merges. It reads the funnel card first and asks, in order:

1. **Code.** Is there a type, an enum, a single entry point or a deletion that would make the
   defect impossible? If the item fixed it only with a test, does the card say why no structure
   can carry the rule? A missing structural fix where one was possible is a major.
2. **Checks.** Does a test named after each invariant exist? Did the item show it fails without
   the fix? Does it run in CI? Is it deterministic and under the time budget?
3. **Reach.** Is every scenario reachable (README "Rules that never bend")? Drop the rest.
4. **Siblings.** Was the same pattern searched for elsewhere?
5. **No loss.** For a no-loss item (◆): three verification lenses and the model test with
   crashes and restarts.
6. **Prose and Human.** Comments only for reasons; owner questions only for what a translator
   would notice, each with a ruling file once answered.

Findings are triaged by the owner's fix rule: every blocker, majors that harm a translator's data
or what they see; every other finding is a minor in `.audit/minors.tsv`.

## 9. Where the effort goes (most at the top)

In M1 the order is deliberate: the structures (Code) before the units that would otherwise copy
patterns, and the mechanical checks before the next large feature lands.

| Layer | Items | Share of M1 effort (target) |
|---|---|---|
| Code | UI-1, UI-2, UI-3, DOM-1, DOM-2, DOM-3, S3, FIX-R's structural fixes | about 60% |
| Checks | INT-2, INT-3, CHK-1 to CHK-6, G5 | about 35% |
| Prose | `AGENTS.md` lines CHK-1 and INT-3 add; declared differences | about 4% |
| Human | The owner decisions H1 to H22 and later rows of `ROADMAP.md` "What needs the owner", asked once, with recommendations | about 1% |
