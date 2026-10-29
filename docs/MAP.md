# Map of the documents

**Answers:** which document holds what, its layer in the tower (`AGENTS.md` "The tower"), and
whether it is a **rule** (says what is true now; edit it when the truth changes) or a **record**
(evidence of a moment; never edit it, write a new one). **Wins over:** nothing.

`scripts/tools/docs.test.mjs` fails when a Markdown file at the root or under `docs/` (outside
`docs/evidence/` and `docs/rulings/`) has no row here, or when a row names a file that does not
exist. A new document gets its row in the same pull request.

## Entry

| Document | Layer | Kind | Holds |
|---|---|---|---|
| `AGENTS.md` | entry | rule | Where to start, the tower, what to read per task, the rules that never bend. Loaded by every agent (`CLAUDE.md` imports it) |
| `CLAUDE.md` | entry | rule | One line: `@AGENTS.md` |
| `CONTRIBUTING.md` | work | rule | How to file an issue, and the loop every change follows to `main`: precedence, funnel, interruptions, self-review, bugs, findings, evidence, where knowledge goes |
| `docs/MAP.md` | entry | rule | This file |

## The tower

| Document | Layer | Kind | Holds |
|---|---|---|---|
| `docs/VISION.md` | vision | rule | What tC4 is, for whom, commitments V1–V8, how to choose |
| `docs/JOURNEYS.md` | journeys | rule | J1–J30: steps, end states, "must not" lines, differences from tC4 |
| `docs/PLAN.md` | promises | rule | The promises P1–P10 every change keeps, and how a new feature enters the documents |
| `docs/DIFFERENCES.md` | journeys | rule | The reference tC4 version, and every place the app differs from tC4 4.0 on purpose |
| `docs/P-LEDGER.md` | checks | rule | For each of P1–P10, the CI job and the tests that show it (checked by `scripts/tools/claims.test.mjs`) |
| `docs/ARCHITECTURE.md` | design | rule | Processes, the local server's security, the internet switch, the crates and their rules, UI, PDF |
| `docs/STORAGE.md` | design | rule | Normative: the journal, the fold, the store, merge, sync, exchange, import, crash safety, format versions |
| `docs/APP-DESIGN.md` | design | rule | `tc-api`, `tc-app`, `tc-server`, the UI port, parity testing; §0 says how to read it |
| `docs/PERFORMANCE.md` | design | rule | The project sizes, the budgets per action, how they are proven |
| `docs/RELEASE.md` | design | rule | How the app is built, signed and released per platform |
| `docs/contracts/SHELL.md` | design | rule | The frozen contract between the server and the desktop shell |
| `docs/TC4-ISSUES.md` | design | rule | tC4's open issues and how the rebuild covers each |

## Work

Work itself is tracked in GitHub issues and lands by pull request (`AGENTS.md` "How work lands").

| Document | Layer | Kind | Holds |
|---|---|---|---|
| `docs/FUNNEL.md` | work | rule | The enforcement funnel (code, checks, prose, human), the funnel section, the review gate |
| `docs/GLOSSARY.md` | work | rule | Every word this repository uses |
| `docs/rulings/` | rulings | rule | One file per owner ruling since 2026-10-21 |
| `scripts/tools/cards.json` | work | record | The retired tracker's cards and their last states, for the `pending:` markers documents still carry |

## Records

Evidence of a moment. Read them for the reasons behind a rule; never update them.

| Document | Layer | Kind | Holds |
|---|---|---|---|
| `docs/evidence/` | evidence | record | Measurements, audits and run logs, one file each, dated |
| `docs/JOURNAL-DECISION-2026-10-16.md` | evidence | record | Why the journal has its design (decision memo and red-team review) |
| `docs/BUDGET-REVIEW-2026-10-17.md` | evidence | record | The review of the speed budgets |
| `docs/LOCAL-BENCH-FINDINGS-2026-10-16.md` | evidence | record | Local benchmark findings for the journal and the budgets |
| `docs/STORAGE-DELTA-2026-10-21.md` | evidence | record | A change to the storage design, applied to `docs/STORAGE.md` |
| `docs/TC4-DELTA-2026-10-16.md` | evidence | record | tC4's changes in one range, and the owner's rulings on them (§5) |
| `docs/TC4-DELTA-2026-10-19.md` | evidence | record | tC4's changes in the next range |
| `docs/TC4-DELTA-2026-10-23.md` | evidence | record | tC4's changes in the next range |
| `docs/TC4-REQUEST-MOVED-NOTICE.md` | evidence | record | A proposal sent to the tC4 team |
| `docs/rebuild.tsv` | rulings | record | The decision log before 2026-10-21, with the owner's early rulings |

## Outside this repository's main line

- The retired work tracker (cards, plans, research): a `tracker:<path>` link is a file in the
  history of `main` at commit 9249a60d.
- tC4's own documents (its `VISION.md`, `JOURNEYS.md`, `CONTRIBUTING.md`, `BURRITO-SPEC.md`,
  `DECISIONS.md`): `git show origin/legacy-4.0:<path>`, or at the reference,
  `git show d25b7871:<path>`. The rebuild's `docs/VISION.md` and `docs/JOURNEYS.md` replace the
  first two.
