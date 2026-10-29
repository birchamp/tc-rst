---
name: Planned work (agent-ready)
about: A feature, change or chore written so a person or an AI agent can build it without a follow-up conversation.
title: ""
labels: enhancement
---

<!--
Write for a builder who has never read this repository's documents and cannot ask you
questions. If a fact matters and is not here or in the repository, it does not exist.
Title: a plain statement of the work in a translator's words ("A translator can restore an
older check decision"), not an activity ("Add restore").
Search first: gh issue list --search "<words>". Add to an open issue instead of filing a copy.
Spell out an internal id the first time (H5, the owner's ruling that only verse text conflicts)
and link the file. Delete these comments and any section that is truly empty.
-->

## Goal

<!-- One to three sentences: what is true when this is done, and why a translator or the project
needs it. The "why" lets the builder decide the details you did not foresee. -->

## Journey and what the translator sees today

<!-- Journey id (Jn, docs/JOURNEYS.md) and promise (Pn, docs/PLAN.md). Then what a person sees or
cannot do today, in a reachable scenario (AGENTS.md "Count only scenarios a translator can reach"). -->

## Where to look

<!-- Start the builder in the right place; this prevents reinventing code that exists.
- Crates, files and functions this touches: `crates/<crate>/src/<file>.rs`
- The closest existing code to copy: "Follow `crates/tc-store/src/ledger.rs`"
- Documents and sections to read: `docs/STORAGE.md` §0–1, `docs/APP-DESIGN.md` §…
- Related issues and pull requests: #n
Mark any path you did not check as "unchecked". -->

## Requirements

<!-- Numbered. Each is one statement of behavior that a test could check.
1. When <a person does X>, <the app does Y>.
2. When <input Z is invalid or missing>, <the app does W>. -->

## Out of scope

<!-- What a keen builder might do but must not: neighbouring refactors, other screens, new
dependencies, changes to the storage format. -->

## Decisions already made

<!-- Fixed choices to follow, not reopen: owner rulings (docs/rulings/H…), types or APIs other code
depends on, libraries to use or avoid, budgets (docs/PERFORMANCE.md). -->

## Funnel

<!-- Fill this before product code (docs/FUNNEL.md §1). Spend the most effort at the top. -->

- **Journey and promise:** Jn, Pn
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
  - <test name, named after the invariant> in <file>; kind: unit | property | model | crash | count | contract | journey | lint | architecture
  - Proof it can fail: <the mutation or the reverted fix, and the failure it printed>
- **Prose** (only if needed): <file: the one non-obvious reason>
- **Human** (only if needed): <question, options and their effect on translators, recommendation>

## Interruptions

<!-- For a flow with more than one step, or one that waits on a server, a worker or the person
(CONTRIBUTING.md "The loop", step 4). For a one-step flow write: "None: one step, no wait."
1. The person cancels partway:
2. A step fails or gets no answer:
3. The person starts a second action while the first is pending:
4. An input changes partway (the project, the book, the account): -->

## Done when

<!-- Checkboxes the builder can verify by running something. Name the commands.
- [ ] `cargo test -p <crate> <test_name>` passes, and was seen to fail with the fix reverted
- [ ] Journey test `parity/app/journeys/<…>` is mapped under Jn in `coverage.json`
- [ ] `scripts/gates.sh --touched` passes with `main` merged in
- [ ] Run through the journey's screens as a person would; the result matches Requirements 1–n -->

## Assumptions and open questions

<!-- Defaults you chose where the request was silent, so the builder can check them.
Questions to answer by reading the repository. If the builder finds that something named above
does not exist, it should stop and say so, not invent a replacement.
Questions only the owner can answer (what a translator would notice): list them here and add
`needs definition`. -->
