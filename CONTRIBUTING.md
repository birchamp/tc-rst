# Contributing

**Answers:** how a change goes from an issue to `main`, for every contributor, human or agent.
**Wins over:** nothing; it applies the rules of `AGENTS.md` and the method of `docs/FUNNEL.md`.

Read `AGENTS.md` first. Its rules bind humans too.

## Which document wins

When two documents disagree, the one higher in this list wins. Fix the lower one in the same pull
request, and say so in its description.

1. The rules in `AGENTS.md` that protect translators' work.
2. Owner rulings (`docs/rulings/`, newest first; before 2026-10-21, the `ruling` lines of
   `docs/rebuild.tsv`). A ruling updates the documents it touches.
3. `docs/VISION.md`: product direction.
4. `docs/JOURNEYS.md`: what a person does and the end state.
5. `docs/PLAN.md`: the promises every change keeps (P1 to P10).
6. The design documents, each for its own area: `docs/STORAGE.md` (data, journal, merge,
   exchange), `docs/ARCHITECTURE.md` (processes, crates, boundaries), `docs/APP-DESIGN.md` (API,
   app, server, UI), `docs/PERFORMANCE.md` (budgets).
7. The issue.

Code and tests are evidence of what the app does. Documents say what it must do. When they
disagree, fix whichever is wrong, and say which.

## File an issue

Search the open issues first (`gh issue list --search "<words>"`). If one covers the same thing,
add to it instead of filing another.

- **Title:** a plain statement of the work or the wrong result, in a translator's words where
  there is one: "A save during a sync is lost", not "Fix race in submit".
- **Body:**
  - the journey it belongs to (`Jn`) and what the translator sees today;
  - for a bug, the steps from a person's action to the wrong result, through a reachable
    scenario (`AGENTS.md`);
  - what done looks like: the end state, as a test could check it;
  - for planned work, the `## Funnel` section (`docs/FUNNEL.md` §1) and the interruptions (below).
- **Labels:** `bug` or `enhancement`; a `Priority/…` label; `epic` for an issue that groups
  sub-issues; `needs definition` while the scope or the end state is open; `cheap` when a cheaper
  model can do it.
- Write for someone who has never read these documents. Spell out an internal id the first time
  (`H5, the owner's ruling that only verse text conflicts`) and link the file.

## The loop

Every change goes through these steps. Skip none; most take minutes.

1. **Claim the issue.** Assign yourself (`gh issue edit <n> --add-assignee @me`), or comment
   "claiming" if you cannot. An issue labelled `needs definition` is not ready: ask in it first.
   Find the journey (`Jn`), the promise (`Pn`) and the invariants (`INV-…`) it touches.
2. **Fill the funnel section** (`docs/FUNNEL.md` §1) before product code: the invariants, the
   wrong states, and which layer stops each one. It goes in the issue, or in the pull request when
   the issue has none.
3. **Build down the funnel.** Make the wrong state impossible in code first: a type, an enum
   state, one entry point. Add a check only for what code cannot carry. Add prose only for what a
   check cannot carry. Ask the owner only what a translator would notice.
4. **Answer the interruptions.** For every flow with more than one step, or that waits on a
   server, a worker or the person, write what happens when:
   1. the person cancels partway;
   2. a step fails or gets no answer;
   3. the person starts a second action while the first is pending;
   4. an input changes partway (the project, the book, the account).

   A flow with one step and no wait needs one line: "None: one step, no wait." A missing answer
   that a translator would notice goes to the owner in the issue; decide the rest yourself and
   say so.
5. **Review your own diff** before anyone else does:
   - **Failure is not success.** For each write, read or wait you add, follow what happens when
     it errors or gets no answer. Keep "done", "refused" and "no answer" apart.
   - **Inputs that differ.** For each filter, group or "pick first", try two inputs that differ
     and one that is empty.
   - **State read before an `.await`** may have changed after it.
   - **Parsed means kept.** Every value read from input is stored or used, or the pull request
     says why not.
   - **Run it as a person would**, through the journey's screens. Passing tests do not replace
     this. A claim you did not see happen comes out of the pull request.
6. **Prove it.** Run the tests of the crates and journeys you touched, then
   `scripts/gates.sh --touched` with `main` merged in. Every check names the invariant it guards
   and has been seen to fail (`docs/FUNNEL.md` §2, step 4).
7. **Open the pull request.** Its description says which issue it closes (`Closes #n`), the
   funnel section, each decision you made and why, what you ran and what it printed, and each
   review finding you did not fix with its reason. A new owner ruling becomes a file in
   `docs/rulings/`, and the documents it changes are updated in the same pull request.
8. **Land.** The pull request needs one approving review and the required checks, and lands
   through the merge queue (`AGENTS.md` "How work lands").

## Fix a bug

Owner rule, 2026-10-16. A bug in one place often has working code for the same job somewhere
else. Do these in order:

1. **Reproduce it** through a reachable scenario, with a test named after the broken invariant.
2. **Find the working code.** Search for code that does the same job correctly
   (`graft ask "<the job>"` when graft is installed, else `grep`). Check that it takes the same
   input, including data already stored.
3. **Reuse it.** Make the broken place call it, move every other caller to it, and delete the
   broken copy.
4. **Simplify.** With no working copy, find the cause and try to remove code: a special case, a
   fallback, a second path for the same data. Never remove the only code that reads data already
   stored.
5. **Add code last**, and say in the pull request why steps 3 and 4 did not fix it.
6. **Look for siblings.** Search for the same pattern; fix them in the same pull request when
   they are the same defect in the same files, else file an issue for each.

Say in the pull request which step fixed the bug.

## Review findings

An adversarial reviewer always finds something. These rules end the loop.

1. **Verify a finding before you fix it.** Reproduce it from a person's action to a wrong
   result. "This could happen if …" is not a finding. An unreachable scenario is dropped.
2. **Fix a finding in this pull request** only when it is a blocker, harms a translator's data
   or what they see, or breaks a rule in `AGENTS.md`.
3. **Any other finding** gets one line in the pull request: "dropped: <reason>", or "filed as
   #n" when it reproduces and no open issue covers it (else "added to #n").
4. **Five review rounds at most**, plus one grace round. Then the pull request lands or goes to
   the owner.

## Evidence

- Every claim in an issue, a pull request or a document names its evidence: a test name, a
  command and what it printed, a commit, a measurement in `docs/evidence/`, or a ruling. A claim
  with no evidence is marked as a proposal.
- Never write a number that moves by itself (tests passed, lines of code, issues open) into a
  document. Print it from its source (the CI run, `gh`).
- A claim about tC4 cites the reference commit: `git show d25b7871:<path>` reads tC4's source at
  the reference version.

## Where knowledge goes

Every fact has one home. Other places link to it by id or path and never restate it.

| You learned | It goes in |
|---|---|
| A rule that code can enforce | Code (a type, a private constructor), else a check (`tc-arch`, a lint, a test) |
| What a person does or sees | `docs/JOURNEYS.md`, the journey's entry |
| A product trade-off | `docs/VISION.md` (only with the owner) |
| A change to a screen or behavior that came from tC4, with its reason | `docs/DIFFERENCES.md` |
| A new guarantee the app keeps | `docs/PLAN.md` (with the owner) |
| How data, merge or exchange works | `docs/STORAGE.md` |
| A crate, a boundary, a process | `docs/ARCHITECTURE.md` |
| The API, the app or the UI | `docs/APP-DESIGN.md` |
| A budget or a measurement | `docs/PERFORMANCE.md`; the measurement in `docs/evidence/` |
| An owner's answer | `docs/rulings/<date>-<id>.md` |
| What you did and why | The pull request's description |
| Work for later | A GitHub issue |
| A word | `docs/GLOSSARY.md` |

Rule documents say what is true now. Dated documents (`*-2026-10-16.md`, `docs/evidence/`) are
evidence of a moment and are never updated. A new document gets a row in `docs/MAP.md`; the docs
check (`scripts/tools/docs.test.mjs`) fails without one.

## Ask the owner

Ask the owner, in the issue, only when:

- a translator would notice the answer (a screen, a word, a behavior, a data choice);
- a step would break a rule in `AGENTS.md`;
- you would have to change files that another open pull request changes;
- a required check fails twice in a row and you cannot tell why.

Give the options, what each means for the person using the app, and your recommendation, in
short plain sentences. Decide technical questions yourself and say so in the pull request.

## Style

- Write in plain, short sentences, close to ASD-STE100 Simplified Technical English: active
  voice, one instruction per sentence. Keep numbers, paths, commands and quoted labels exact.
- A code comment explains a reason the code cannot show. It never narrates the code.
- Write "unfoldingWord" in camel case.

## Licence

MIT. A contribution is accepted under the same licence.
