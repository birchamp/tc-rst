# The enforcement funnel

**Answers:** how every bug, function and feature is built and reviewed. **Wins over:** nothing;
it is the method `AGENTS.md` and `CONTRIBUTING.md` require. **Does not cover:** how an issue is
filed or a pull request lands (`CONTRIBUTING.md`).

Owner's instruction from early in the rebuild. Every change goes through four layers, from the
top down. **Spend the most effort at the top.** Use a lower layer only for what the layers above
it cannot carry.

1. **Code.** Make the wrong thing impossible: simpler functions, types that encode invariants,
   one entry point per job, explicit state models.
2. **Checks.** Catch automatically what code cannot prevent: tests, lints, type checks, CI,
   scripted runs of the real app. Each check is deterministic, fast, tests behavior, and names the
   invariant it guards when it fails.
3. **Prose.** A short comment or one line in a document, only for context that code and checks
   cannot carry.
4. **Human.** Product direction, risk, real ambiguity: the owner decides.

A fix that is only a test is a fix at the second layer. It is allowed only when the pull request
says why no type or structure can carry the rule.

## 1. The funnel section

An issue for planned work, and every pull request, has a `## Funnel` section. Fill it before you
write product code; a reviewer reads it first.

```markdown
## Funnel

- **Journey and promise:** Jn (docs/JOURNEYS.md), Pn (docs/PLAN.md)
- **Invariants** (one sentence each; reuse the name in tests and messages):
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
    count | contract | journey | lint | architecture
  - Proof it can fail: <the mutation or the reverted fix, and the failure it printed>
- **Prose** (only if needed): <file: the one non-obvious reason>
- **Human** (only if needed): <question, options and their effect on translators,
  recommendation, ruling file once answered>
```

## 2. How a bug goes through the funnel

1. **Reproduce it through a reachable scenario.** Write a test through the public API that fails
   today. Name it after the invariant it breaks
   (`a_save_never_erases_a_newer_draft_of_its_field`, not `test_bug_123`). A scenario a translator
   cannot reach (`AGENTS.md`) is not a bug for this project: drop it.
2. **Ask what structure allowed it.** Was a state representable that should not be? Did a second
   path skip the rule? Did a flag stand in for a state? Was an error matched with a wildcard?
3. **Fix the structure first.** Reuse the working code that does the same job, change the type,
   remove the second path, or turn flags into an enum, so the bug cannot be written again
   (`CONTRIBUTING.md` "Fix a bug").
4. **Prove the test can fail.** Put the bug back (a mutation, or a revert of the fix) and show the
   test fails with a message that names the invariant. Put the fix back. Paste the failure into the
   pull request.
5. **Look for its siblings.** Search for the same pattern elsewhere. Fix them in the same pull
   request when they are the same defect in the same files; otherwise file an issue for each.

## 3. How a feature goes through the funnel

1. **Name its journey and invariants** before any code (`docs/JOURNEYS.md`).
2. **Find the structure it should use.** If you are about to write a loop, a flag or a parser
   that already exists, stop and reuse it (`graft ask`, `grep`).
3. **Model decisions about a translator's work as a typed state machine**: a `State` enum, an
   `Event` enum, one pure exhaustive `next(state, event) -> (state, actions)`, and an executor
   that does the I/O. Prove it with a model-based property test that includes crashes and
   restarts. `crates/tc-store/src/ledger.rs` is the example to copy.
4. **Write the checks** that prove the invariants, and make sure they run in CI.
5. **Port tC4's behavior and its known bugs' tests.** `docs/TC4-ISSUES.md` lists the tC4 issues
   an area carries. Each becomes a test that fails first.

## 4. The Code layer: patterns this repository uses

| Pattern | Use it when | Example |
|---|---|---|
| Typed state machine (State, Event, pure `next()`, executor, model test) | Logic decides about a translator's work, or a lifecycle has more than two states | `crates/tc-store/src/ledger.rs`; `crates/tc-api/src/model/draft.rs` |
| Newtype for an identifier | Any id or number with rules | `BookId`, `ActorId`, `OpId`, `ChapterNo`, `VerseNo` in `tc-core` |
| Enum instead of booleans | Two or more flags that describe one state | `KeepPhase` in `crates/tc-ui/src/app/review.rs` |
| One entry point per job | Every write, every network call, every job, every leave | `ProjectHandle::submit` (`crates/tc-store/src/handle.rs`); `tc_store::atomic_write`; `run_net` (`crates/tc-ui/src/app/web/app_ctx.rs`) |
| Capability token | Only some code may do something (write a project, use the network) | `WriteCap` |
| Exhaustive match, no wildcard | Classifying errors, refusals, outcomes | `crates/tc-ui/src/app/save/classify.rs` |
| Parse at the boundary | Files, HTTP, JSON, USFM, TSV, user input | `tc-usfm`; `tc-api` wire types refuse unknown keys |
| Pure core, I/O at the edges | Everything that can be computed | `tc-journal`'s fold; `tc-check`; `tc-align`; `tc-export` |
| Delete the duplicate | Two readers, two copies of a rule | Merge them into one and move every caller |

## 5. The Checks layer

### Rules for every check

- **Deterministic.** No wall-clock assertions: inject a clock or use paused time. No
  sleep-then-assert: wait on a signal. A random test prints its seed so a failure can be rerun;
  long and many-seed variants run in the nightly job.
- **Fast.** A test still running after 60 s is named SLOW and after 5 minutes is stopped
  (`.config/nextest.toml`); a browser journey stops after 2 minutes. Long variants go to the
  nightly job.
- **Behavior, not implementation.** Test through the public API with real fixtures from
  `fixtures/`. A test fails if the behavior it names breaks, and only then.
- **Names the invariant.** The test's name is the invariant. Its failure message starts with
  `INV-<NAME>:` or the test name, then says what broke and where. A clippy lint names its
  invariant in the comment above it in `Cargo.toml`. One name per rule: `grep -rn "INV-<NAME>"`
  before you make a new one.
- **Runs in CI.** A check that runs only on someone's machine is a note, not a check.

### Kinds of check

| Kind | Use for | Where it lives |
|---|---|---|
| Unit test through the public API | One rule of one module | `crates/<crate>/tests/` |
| Property test (`proptest`) | Parsers (round trip), merges (convergence, no loss) | `tests/properties.rs` per crate |
| Model-based property test | Every typed state machine, with crashes and restarts | `crates/tc-store/tests/store/model.rs` |
| Crash injection | Every write point of save, sync, import, checkpoint (P4) | `crates/tc-store/tests/store/crash.rs` |
| Operation count | Interactive actions cost what they touch (`docs/PERFORMANCE.md`) | `crates/tc-app/tests/op_counts/` |
| Contract test | A seam with two implementations (fake and real) | `crates/tc-app/tests/store_contract.rs` |
| Journey | A journey end to end in the real app (P1) | `parity/app/journeys/*.test.mjs`, mapped in `coverage.json` |
| Design-system test | tC4's design system is the app's, byte for byte (P7) | `crates/tc-ui/tests/vendored_assets.rs` |
| Lint | How code is written | `clippy.toml`, `[workspace.lints]` |
| Architecture test | Crate edges, I/O and network boundaries | `crates/tc-arch` |
| Claim test | A document's claim that a test or a file exists | `scripts/tools/claims.test.mjs`, `scripts/tools/docs.test.mjs` |

### Invariants the repository already checks

| Invariant | What holds | Where |
|---|---|---|
| INV-ARCHITECTURE | Crate edges, no I/O in pure crates, network only in `tc-door43` and `tc-resources`, shared dependency versions | `crates/tc-arch` |
| INV-ATOMIC-WRITES | No `std::fs` mutation in storage crates outside the atomic helpers | `crates/tc-arch` |
| INV-ONE-PARSER-OWNER | Low-level parser and codec types stay in their crate | `crates/tc-arch` |
| INV-LOCAL-SERVER-IS-CLOSED | Every `tc-server` route is behind the guard | `crates/tc-arch` |
| INV-VALID-BURRITO | Every project a test writes is a valid Burrito (P5) | `tc-store` and `tc-app` tests |
| INV-CLAIMED-TESTS-EXIST | Every test a document names exists | `scripts/tools/claims.test.mjs` |
| INV-P1-COVERED | Every live journey maps to a journey test | `parity/app/journeys/coverage.test.mjs` |
| INV-JOURNEYS-ONE-LIST | `docs/JOURNEYS.md` lists exactly the mapped journeys | `parity/app/journeys/coverage.test.mjs` |
| INV-DOCS-LINKED | Every document is on the map, and every path it names exists | `scripts/tools/docs.test.mjs` |
| INV-NO-BAGS-OF-BOOLEANS, INV-PROVEN-PANICS | Lints on bools, `unreachable`, allows without reasons | `Cargo.toml`, `clippy.toml` |
| INV-A-SKIP-IS-A-FAILURE | A skipped journey fails the job | `parity/app/journeys/no-skips.test.mjs` |

### Where the checks are strongest

Rigor is highest wherever a mistake loses a translator's work or locks in a format: the journal
and the merge; storage and crash recovery; sync and USB exchange; import; the Burrito writer.
These parts get property tests, crash-injection tests and an adversarial review by several
models before they merge. Screens get end-to-end journeys, and the design system is checked against tC4's files (P7).
Resource parsers get tests against real fixtures from tC4, Door43 and Aquifer.

## 6. The Prose layer

- A comment explains a non-obvious reason. It never narrates what the code does.
- A rule goes in `AGENTS.md` only when no check can hold it, and then in one line.
- Design documents record decisions and declared differences from tC4. When the code changes a
  declared difference, the same pull request changes the document.
- Issues and pull requests record what was done, with evidence. They are not proof: the checks
  are.

## 7. The Human layer

- The owner answers only what a translator would notice, or what only the owner can give
  (accounts, credentials, money, people, product direction).
- Each question has options, the effect of each on the person using the app, and a
  recommendation. One question per decision.
- An answer becomes a file in `docs/rulings/` (`YYYY-MM-DD-<id>.md`): the question, the answer in
  the owner's words, the rule, and its effect for translators.
- Technical choices are decided by the contributor and recorded in the pull request, not asked.

## 8. The review gate

A reviewer reads the funnel section first and asks, in order:

1. **Code.** Is there a type, an enum, a single entry point or a deletion that would make the
   defect impossible? If the change fixed it only with a test, does it say why no structure can
   carry the rule? A missing structural fix where one was possible is a major finding.
2. **Checks.** Does a test named after each invariant exist? Was it shown to fail without the
   fix? Does it run in CI? Is it deterministic and inside the time limits?
3. **Reach.** Is every scenario reachable? Drop the rest.
4. **Siblings.** Was the same pattern searched for elsewhere?
5. **No loss.** For a change that touches a translator's saved work: the three verification
   lenses (`docs/GLOSSARY.md`) and a model test with crashes and restarts.
6. **Prose and Human.** Comments only for reasons; owner questions only for what a translator
   would notice, each with a ruling file once answered.

Findings are triaged by the rules in `CONTRIBUTING.md`, "Review findings".
