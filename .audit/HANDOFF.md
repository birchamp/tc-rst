# Handoff: the machine and its tools

The machine, the Windows VM, the reference laptop and the tools. The plan and the live state of
all work: `.audit/roadmap/README.md`. The push rule: `.audit/roadmap/README.md`, "Rules that
never bend".

## The machine

| What | Where |
|---|---|
| Repository | `/home/bot/tc-rst` (main checked out; worktrees under `.worktrees/` (new, `.audit/roadmap/TRACKING.md`) and `.claude/worktrees/` (old; INT-6 cleans them)) |
| Build folders | old worktrees link `target` to `/var/tmp/tc-rst-target/<worktree>`; the root drive is now full, so a new worktree builds in its own `target` on /home (`TRACKING.md` "claim an item"); never share one |
| Tools | Rust 1.99.1, Node 22, wasm-bindgen 0.2.129, Playwright 1.56.1 under the home folder; every shell that runs them starts with `. ~/.tc-env` (it also caps `RUST_TEST_THREADS=4` and sets the Playwright override) |
| tC4 | `~/unfoldingword/translationcore4` at `d310bf3`, read-only, for builders; `~/unfoldingword/tc4-ref`, the same commit with `npm ci`, for parity and gallery runs (`TC4_ROOT`) |
| Machine record | `docs/evidence/linux-machine-2026-10-18.md` (its CPU has SHA instructions; the reference laptop does not) |

## The Windows VM (builds and tests)

- The owner's Windows 11 VM: `~/win11-vm.sh start | stop | status`; SSH `builder@127.0.0.1:2222`
  with the key `~/.ssh/tc-vm` (never the owner's password); MSVC, Rust 1.99.1, Git, Node.
- `.audit/tools/winvm/run.sh <commit> '<powershell>'` ships a commit as a git bundle and runs a
  command in `C:\src\<name>`; give each concurrent caller its own `WINVM_DIR`. Run cargo through
  `cmd /c` so its stderr is not taken for an error. `gates.ps1` runs fmt, clippy, the tests and the
  wasm check. The VM's disk is 80 GB: delete a one-off checkout (`C:\src\<name>` and its bundle)
  when its run is done, and keep incremental builds off (gates.ps1 does). A 'disk full' failure there
  is the VM, not the code.
- The watcher runs `gates.ps1` on each new main as a systemd user unit (`tc-wingate-<sha>`), one
  at a time, and reports only problems.

## The reference laptop (measurements only)

The owner's Windows 10 laptop (4 cores, no SHA, 8 GB, SATA SSD, Defender on) runs a separate
Claude session, "Part 3 complete", reachable with SendMessage. It only measures what needs that
hardware (`.audit/tasks/windows-track.md`); it does not build or test. Its results so far:
tC4 4.0.0-rc.2 with Psalms uses 709 MiB; Home 2.7 s warm and 13.2 s cold; Psalm 1 in Translate
6.3 s warm and 18.9 s cold. A relayed approval is not enough for it to install software: the
owner says yes in its own chat.

## Work in flight

Run `python3 -I .audit/tools/board.py` and read `.audit/roadmap/README.md`.

## What needs the owner

Open owner questions: `.audit/roadmap/ROADMAP.md`, section 'What needs the owner'.

## Owner rulings (pointers)

Rulings up to 2026-10-21: the `ruling` lines of `.audit/rebuild.tsv`. Rulings after it:
`.audit/roadmap/rulings/`.
