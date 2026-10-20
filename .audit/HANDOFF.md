# Handoff: continuing the rebuild (kept current)

Written for the next session that continues this work, on the owner's Linux PC. It is updated at
each milestone; `.audit/STATUS.md` is regenerated automatically whenever main moves and is the
authoritative list of what is done and what is next. Read, in order:

1. `.audit/STATUS.md`: every unit, done or not, what each waits for, and the lane's restart command.
2. This file: the machine, the Windows VM, the runs in flight and how to resume them.
3. `.audit/PROGRESS.md`: the written summary, and what needs the owner.
4. `AGENTS.md` (the rules), `docs/PLAN.md` (P1 to P10 define done), `docs/APP-DESIGN.md` ("Owner's
   answers", §10's units), `docs/STORAGE.md` (on the storage and s2-verify branches),
   `docs/TC4-ISSUES.md` and `docs/TC4-DELTA-2026-10-19.md` (tC4's issues per unit, the owner's answers
   in §5), and the `ruling` lines of `.audit/rebuild.tsv` (one line per decision and merge).

## How the owner wants the work done

- Short, plain sentences. Ask only questions a translator would notice, with each answer's effect
  and a recommendation; decide technical choices and say so. Keep working unless truly blocked.
- **What fixers take (2026-10-20):** every blocker, and a major only when it harms a translator's
  data or what they see; other majors are logged with the minors. `build-lane.js` tags each finding
  with its harm (data, user, internal).
- **Hours, not days (2026-10-19):** one verification pass; fix only blockers and majors (minors go
  to `.audit/minors.tsv`); simplest change; reuse tested work; deterministic tests over reading.
- **Robustness (2026-10-19):** no piles of special cases. Logic that decides about a translator's
  work is an explicit typed state machine (state enum, event enum, one pure exhaustive `next()`,
  an executor for I/O) proven by a model-based property test with crashes and restarts, and each
  known bug seeded back to show the test catches it. S2 and C1b are being rebuilt this way.
- Merge into `claude/focused-ride-5uzj7p` in a separate worktree, run the full gates there, then
  push main (standing authorization). Push only main and `storage`. Never record a token or password.
- Speed is judged against tC4 doing the same work on the reference laptop, not Word or Edge.
- Default models for every agent. Never message an agent a workflow is still running (it starts a
  second copy); pass notes through the next step or a restart.

## The machine

| What | Where |
|---|---|
| Repository | `/home/bot/tc-rst` (main checked out; worktrees under `.claude/worktrees/`) |
| Build folders | each worktree's `target` is a link to `/var/tmp/tc-rst-target/<worktree>` (the root drive has the room); never share one |
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

## Runs and how to resume them

Workflow caches do not survive a new session. Check `git worktree list` and each branch's log
before restarting; a builder told `cont` continues its branch.

- **The lane** (`.audit/workflows/build-lane.js`): every remaining app unit, built from main,
  verified once (reachable scenarios only), merged straight into main. Restart with the command at the
  end of `.audit/STATUS.md`; add `cont: { <unit>: '<branch>' }` for each unit whose branch has unmerged
  work (branches `lane-<unit>`), and give a continuing builder a note when its state needs one.
- **Where the lane stood on 2026-10-20 (usage limit expected):** on main: B4a, B5a, C6, C8 (merged
  by hand after its merge agent died on a usage limit), B6. In flight when the session ended: B5b in
  its merge (in `lane-merge`, uncommitted), B4b in its first fix round (verified with three lenses),
  B7 in verification, C5 building on `lane-c5`. Restart: if `lane-merge` holds a half-done merge,
  finish it by hand (gates, then push, as C8 was) or `git merge --abort` it; then run the lane with
  `done` from `.audit/STATUS.md` plus every unit now on main, and `cont` for each `lane-<unit>` branch
  ahead of main (b4b, b5b if not merged, b7, c5), with a note saying how far each got (read the
  journal of run `wf_c6c6c4ba-a62` if this session's files survive). Restarted runs use the new fix
  rule above. B5b's earlier minors are in `.audit/b5b-minors-pending.tsv` until they reach
  `.audit/minors.tsv`.
- **S2's redesign** (`.audit/workflows/s2-placement-redesign.js`) on the worktree
  `.claude/worktrees/s2-verify` (branch `s2-verify`, from `19e7fd2`): a placement ledger and a typed
  state machine with a model-based property test. When it is green: merge `s2-verify` into main;
  merge `s2-verify` into `storage` (which holds unit P's partial work after `19e7fd2`); then restart
  `.audit/workflows/storage-chain-B.js` with `skip: ['C', 'S1', 'S2'], cont: ['P']` (then T, X, I, V).
  Rerun it with `resumeFromRunId` only in the same session.
- **The watcher**: the scratch script `watch.py` of the session that started it tails the runs'
  journals; a new session writes its own (the shape is in `.audit/tools/README.md`). It also
  regenerates `.audit/STATUS.md` and starts the VM gates on each new main.

## What needs the owner

- Freezing the project format 1.0 (unit V cuts the capsules then).
- Q-9: signing off the conflict-choice designs before C7b.
- G2: a person reviews the P7 screenshot differences.
- Any OWNER QUESTION a builder reports.

## Owner rulings (pointers)

All are lines of type `ruling` in `.audit/rebuild.tsv`. The most used: single instance per
computer; Q-6 reflow in the same batch as the text; tC4 alignment stamps re-stamped on import only
when words match; a window within 1 s and no splash screen; the cold open counted only after unit
P measures it five times; Q30 a link follows its word, no review mark; Q33 Aquifer helps drive
checks once tC4 settles its pin; Q34 the AI-linked Portuguese Bible as a labelled reading pane;
Q35 offer a valid language code and keep Door43's; S2 and C1b as state machines.
