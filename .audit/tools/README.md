# Tools

| Tool | What it does |
|---|---|
| `prune-target.py <target> [min_age_minutes]` | Removes build artifacts no build still needs (orphans, superseded hashes older than 3 h) |
| `board.py [--check] [--json] [--invariants] [--ref REF]` | The board: every work item of `.audit/roadmap/units/`, its state, owner and next step, from the cards and git; `--check` exits 1 on any contradiction (`INV-TRACKER-TRUE`); `--invariants` lists every `INV-` name with its cards. Run as `python3 -I .audit/tools/board.py` |
| `board_test.py` | The board's own tests: `python3 -I .audit/tools/board_test.py` |
| `status.py [ref]` | History: superseded by `board.py` on 2026-10-21. Wrote `.audit/STATUS.md` from main's merge commits |
| `watch-runs.py <workflows dir> '<runs json>'` | The orchestrator's watcher, run under the Monitor tool: one line per finished gate, verify, fix, merge or push in the given workflow runs (from their `journal.jsonl`), new `windows-*` branches, low disk (moves idle build folders to `/var/tmp`, prunes); on each new main it regenerates and pushes `.audit/STATUS.md` from `.claude/worktrees/status`, and starts the Windows VM gates as a systemd user unit when Rust or Cargo files changed. Its state lives in `~/.cache/tc-watch` |
| `winvm/setup.ps1` | One-time setup of the Windows 11 VM: Build Tools, Git, Node, Rust |
| `winvm/run.sh <commit> '<powershell>'` | Runs a command on a commit in the VM (`WINVM_DIR` names the checkout) |
| `winvm/gates.ps1` | The workspace gates on Windows |
