# Tools

| Tool | What it does |
|---|---|
| `lane-state.mjs` | Saves and reads durable unit/stage reports bound to a clean current HEAD; use `node .audit/tools/lane-state.mjs --help` for the CLI |
| `run-gates.mjs` | Runs the full integration suite or one focused command with prerequisite checks, per-command logs/receipts and a heavy-run lock shared across worktrees; use `node .audit/tools/run-gates.mjs --help` for the CLI |
| `prune-target.py <target> [min_age_minutes]` | Removes build artifacts no build still needs (orphans, superseded hashes older than 3 h) |
| `status.py [ref]` | Writes `.audit/STATUS.md`: every unit done or not, what it waits for, and the lane's restart command, from main's merge commits |

| `board.py [--check] [--json] [--invariants] [--ref REF]` | The board: every work item of `.audit/roadmap/units/`, its state, owner and next step, from the cards and git; `--check` exits 1 on any contradiction (`INV-TRACKER-TRUE`); `--invariants` lists every `INV-` name with its cards. Run as `python3 -I .audit/tools/board.py` |
| `board_test.py` | The board's own tests: `python3 -I .audit/tools/board_test.py` |
| `union_test.py` | Proves that two branches appending to `.audit/minors.tsv` or `.audit/rebuild.tsv` merge without a conflict (`INV-LOGS-NEVER-CONFLICT`): `python3 -I -B .audit/tools/union_test.py` |
| `status.py [ref]` | History: superseded by `board.py` on 2026-10-21; do not run it (it rewrites STATUS.md) |
| `watch-runs.py <workflows dir> '<runs json>'` | The orchestrator's watcher, run under the Monitor tool: one line per finished gate, verify, fix, merge or push in the given workflow runs (from their `journal.jsonl`), new `windows-*` branches, low disk (moves idle build folders to `/var/tmp`, prunes); on each new main it starts the Windows VM gates as a systemd user unit when Rust or Cargo files changed. Its state lives in `~/.cache/tc-watch` |
| `winvm/setup.ps1` | One-time setup of the Windows 11 VM: Build Tools, Git, Node, Rust |
| `winvm/run.sh <commit> '<powershell>'` | Runs a command on a commit in the VM (`WINVM_DIR` names the checkout) |
| `winvm/gates.ps1` | The workspace gates on Windows |

Read [`../PROCESS.md`](../PROCESS.md) before adopting the new tools. Adopt after the current
stage finishes; preserve in-flight jobs and their logs. The runner's receipt rules decide what
can be reused on the same clean revision. Missing, failed or stale evidence never counts as a
pass. Include the unit's browser, crash/no-loss, Burrito and performance checks as well as the
default integration commands; Windows/release checks retain their existing scope.

Keep one executor for shared test evidence. Reviewers do not mutate the builder checkout, and
mutation checks use a separate checkout and target. The shared lock covers cooperating gate
runners; it does not stop existing jobs launched by older scripts.

Run from the owning worktree; source `~/.tc-env` before Cargo/Node work. For example:

```sh
. ~/.tc-env
node .audit/tools/lane-state.mjs read . S2 verify
node .audit/tools/lane-state.mjs begin . S2 verify
node .audit/tools/lane-state.mjs save . S2 verify /tmp/s2-verify-report.json
node .audit/tools/run-gates.mjs --jobs 6
node .audit/tools/run-gates.mjs -- cargo test -p tc-store --offline -j 6
```

`read` returns `{reusable, report?, reason}`; `begin` records the attempt's start and input SHA;
`save` takes a JSON object with the current full `sha` and nonempty `summary`, and requires a
clean checkout. Reports belong to the actual unit and stage, and must cite their execution
proof. State is under `.enjoy-lane/stages/`; gate receipts are under
`.enjoy-lane/gates/receipts/` and logs under `.enjoy-logs/gates/`. These are ignored local
artifacts. `run-gates --fresh` ignores old receipts. A busy shared lock stops the new runner;
wait for its holder to finish. `--cwd` must stay within the owning worktree.
The lock file is in Git's shared directory, so separate job `TMPDIR` values cannot split
the lock between worktrees. A runner still requires the host's permission to access it.

Before adopting on an older continuing branch, include the process changes and their ignore
rules; the state/log directories must not become untracked source inputs. Focused editing
checks do not replace AGENTS.md's full checks before each commit.

