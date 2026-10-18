# Handoff: moving the rebuild to the owner's Linux machine (2026-10-18)

Written by the cloud session that built everything so far, for the session that continues on the
owner's Linux machine. Read this first, then `AGENTS.md` (the rules), `docs/PLAN.md`,
`docs/APP-DESIGN.md` (start with "Owner's answers" and "G0 results"), `docs/PERFORMANCE.md`, and
`docs/STORAGE.md` on branch `storage`.

## Folders

| Path | What | How to get it |
|---|---|---|
| `~/tc-rst` | this repository | `git clone https://github.com/birchamp/tc-rst ~/tc-rst` |
| `~/unfoldingword/translationcore4` | tC4 4.0, the reference app, read-only | clone `unfoldingWord/translationcore4`, `git checkout d310bf3`, `npm ci` |
| `~/tc-rst/.claude/worktrees/<name>` | one git worktree per unit an agent builds | agents create them |
| `~/tc-rst/.claude/worktrees/.cargo/config.toml` | build settings for every worktree (not in git) | create it, below |

Optional references the research notes cite (`.audit/research/`): Pankosmia `pankosmia-web`,
`proskomma-core`, `scripture-burrito`, `uW-Tools-Collab`, and two Aquifer probes. The cloud
machine kept them in `/home/user/ref`; clone them only when a task needs one.

## Tools

- Rust stable with `rustup target add wasm32-unknown-unknown`. The cloud machine ran 1.97; the
  Windows laptop 1.99, whose new clippy lints are fixed on main.
- Node 22.
- Playwright **1.56.1** with its Chromium (141.0.7390.37, build 1194): `npx playwright@1.56.1
  install chromium`. The tC4 parity baseline (`parity/tc4-baseline/README.md`) was captured with
  exactly this browser, so pixel comparisons only hold with it, on Linux.
- For the Electronite spike tests only: Electronite 37.1.0 for Linux (GitHub releases of
  `unfoldingWord/electronite`), `xvfb-run`, and optionally PowerShell 7 (`pwsh`).
- Python 3 for the tools in `.audit/tools/`.

`~/tc-rst/.claude/worktrees/.cargo/config.toml`:

```toml
# Each worktree builds in its own target folder; a shared one let worktrees overwrite each
# other's crates. Line tables only keep each folder smaller.
[profile.dev]
debug = "line-tables-only"

[profile.test]
debug = "line-tables-only"
```

Never share one `target` between worktrees (`.audit/rebuild.tsv`, 2026-10-18). Prune with
`python3 -I .audit/tools/prune-target.py <worktree>/target` when the disk gets full.

## Branches

| Branch | State |
|---|---|
| `claude/focused-ride-5uzj7p` | main. Everything finished is here: the domain crates, tc-journal, tc-store (storage unit S1), phase-0 units A1, A6a, A10, A11, G0's evidence, the Windows fixes and Door43's tree ids. Push here. |
| `storage` | storage unit S2 in progress (plan `.audit/plans/S2.md`, builder commits up to the handoff). The owner allowed pushing it. |
| `phase0` | integration branch of phase 0 (A1, A6a, A10, A11 merged; all on main) |
| `p0-a2`, `p0-a3` | phase-0 units A2 (`tc4 serve`) and A3 (tc-app) in progress, each with its plan in `.audit/plans/` |
| `windows-fixes`, `door43-facts`, `g0-windows` | the owner's Windows session; all merged into main |

Ask the owner before pushing any other branch. Commit messages end with the two attribution lines
the session gives; never put a model name in a commit or a file.

## Restarting the work

Workflow caches do not move between sessions, so restart each run with arguments that skip
finished units and let in-progress builders continue from their branch:

```js
Workflow({ scriptPath: '.audit/workflows/storage-chain-B.js',
  args: { root: '<abs path of ~/tc-rst>', tc4: '<abs path of tC4>', scratch: '<a scratch dir>',
          skip: ['C', 'S1'], cont: ['S2'] } })
Workflow({ scriptPath: '.audit/workflows/app-phase0.js',
  args: { root: '<abs path of ~/tc-rst>', tc4: '<abs path of tC4>',
          onMain: ['a1', 'a6a', 'a10', 'a11'], cont: ['a2', 'a3'] } })
```

Before that, recreate the storage worktree: `git worktree add .claude/worktrees/storage storage`.
The phase-0 builders recreate their own worktrees from their branches. On a bigger machine,
edit each script's machine sentence ("4 CPUs shared ... cargo -j 2") to match the machine.

Owner's model policy (2026-10-18): **Opus plans and reviews, Sonnet edits, Haiku reads.** The
scripts already give planners and reviewers the default model, builders, fixers and merges
`sonnet`, and gates `haiku`.

## What is left

- Storage chain: S2 (materialize off the actor, Git, held-file retries) → P (scale against the
  reference laptop) → X (sync and USB exchange) → I (tC4 and tC3 import).
- App layer: phase 0 A2, A3, A4a, A9, A4b, A6b, A5, A7, A8; then phases 1 to 6
  (`docs/APP-DESIGN.md` §10): 43 of 48 units are left.
- Windows track (`.audit/tasks/windows-track.md`, done by the owner's Windows session): task 5
  (memory of Word, a browser, tC4 and Paratext with Psalms open, against our 1 GiB and 400 MiB
  limits), task 6 (the Windows hazards from its build report). Merge their branches after the
  Linux gates.
- After a unit's gate is green: merge its branch into main in a separate worktree, run the full
  gates there (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace`, and the two wasm32 clippy runs of tc-ui), push, remove
  the worktree.

## Owner rulings to keep in mind

All are in the documents; this is where to find them.

- Match tC4 4.0, but follow what the translator needs where tC4 works against a feature
  (`AGENTS.md`, "Matching tC4").
- Storage design: option B (`docs/JOURNAL-DECISION-2026-10-16.md`).
- Budgets of 2026-10-18 (`docs/BUDGET-REVIEW-2026-10-17.md`, `docs/PERFORMANCE.md`): saving never
  interrupts the person (25 ms while background work runs); adding a book shows a progress bar;
  the whole app 1 GiB, the project store 400 MiB steady.
- Single instance per computer; people work together only from separate computers
  (`docs/APP-DESIGN.md`, "Owner's answers").
- Q-1: the header filter. Q-6: an edited verse's alignment is reflowed in the same batch.

## Open question for the owner

- tC4's aligner stamps the hash of its display text while its own fold checks another hash, so
  tC4 alignments on poetry verses import as invalid. Should the importer re-stamp them (it would
  rewrite stored values)? Unit I needs the answer.

## Records

- `.audit/rebuild.tsv`: one line per decision and merge, oldest first.
- `.audit/research/`: research notes behind the designs.
- `docs/evidence/`: every measurement, with its machine.
- `.audit/workflows/README.md`: what each workflow script did.
