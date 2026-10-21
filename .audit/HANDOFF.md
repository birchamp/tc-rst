# Handoff: continuing the rebuild (kept current)

## S2: where it stands (updated 2026-10-21 16:20 UTC)

**Owner:** Enjoy conversation `19f61dcb-b38a-49aa-9b95-1d8d5dca69fe`. **Builder:**
`/home/bot/tc-rst/.worktrees/s2-proof-executor`, branch `s2-proof-executor`. Not pushed, not
merged into main or `storage`. The other active owner is the **tC4 Rebuild** conversation
(`d8a954f9…`, branch `resume-build-lane`); never change its checkout. Read, in order:
`.audit/plans/S2-PROOF-2026-10-21.md` (latest), then `S2-PROOF-2026-10-20.md` (history).

Commits on the branch, oldest first (each passed every AGENTS check on its own tree):
`451d833` gate repairs, `1ec5bad` process adoption, `f5e1613` S2 proof fixes, `7d3a5d3` model:
`.tc/external` is no surviving copy, `a8e50e0` two losses the long model found (seal from
memory; edited records after a lost head), `9d8f11e` merge of the carried/Git fix round
(`s2-fix-git` 2d4219d), `2619a72` merge of the metadata.json fix round (`s2-fix-meta` 3d9dfaa).
The last commit merges `s2-fix-proof` 9aa15f3 (tests only: four mutants caught, SOURCE_ALIAS
retired as equivalent) with 13 review minors in `.audit/minors.tsv`; every AGENTS check passed
on the merged tree (fmt, clippy, wasm, parity in `.enjoy-logs/s2-merge-proof-precommit.log`;
1,768 workspace tests in `.enjoy-logs/s2-merge-proof-tests.log`).

Fix round 1 (review `wf_10020f08-0a6`, nine serious findings, all reproduced): every fix commit
was reviewed independently and accepted (minors only). Reproductions:
`/home/bot/tc-rst/.worktrees/s2-review-exec-evidence/`. Fixer worktrees `s2-fix-{git,meta,proof}`
are done and can be removed after the merge.

**Next, in order:** (1) full validation at the merged SHA through `.enjoy-logs/s2-validate-all.sh`
(suite with receipts, mutation executor `.enjoy-lane/mutation-executor/execute.py --cases all`,
44 recipes, then four long model seeds); every recipe must be detected and every long seed
pass; a new long-model failure becomes a named witness, then a product or oracle fix with a
BEFORE log; (2) re-review only the changed areas; (3) save the verified stage with
`.audit/tools/lane-state.mjs`; (4) merge into main in a separate worktree (about 12 conflicts:
see S2-PROOF-2026-10-21.md), with the tc-app/tc-server/tc4 no-hook clippy, Windows VM gates
and storebench release checks; pushing main needs the owner's say-so in the conversation.

The journal-files state machine is the recommended next storage unit after S2 merges
(`.audit/plans/JOURNAL-MACHINE-ASSESSMENT.md`). The owner asked to work at High effort for
speed and to run a final check at a higher effort later.

Local, git-ignored state an agent on this PC can also use: stage reports
`.enjoy-lane/stages/S2/`, gate receipts `.enjoy-lane/gates/`, logs `.enjoy-logs/`, review and
fix results `.enjoy-lane/*result*.json`.

Written for the next session that continues this work, on the owner's Linux PC. It is updated at
each milestone. Check `.audit/STATUS.md`'s recorded ref against the actual builder worktree:
local verified merges can be ahead of main and the generated file can be stale. Read, in order:

1. `.audit/STATUS.md`: every unit, done or not, what each waits for, and the lane's restart command.
2. This file: the machine, the Windows VM, the runs in flight and how to resume them.
3. `.audit/PROGRESS.md`: the written summary, and what needs the owner.
   `.audit/PROCESS.md` gives the process policy and the checklist for adopting it at the next
   completed-stage boundary; running sessions do not reload changed scripts.
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
- **Critical path:** give S2 a named dedicated owner and first claim on the heavy-test slot.
  Start risky changes with the model and known-bug tests. Keep reviewers read-only; one executor
  shares test evidence, and one isolated executor handles mutations. Preserve every required
  integration and platform check; see `.audit/PROCESS.md`.
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
before restarting; a builder told `cont` continues its branch. At the next completed-stage
boundary, adopt `.audit/tools/lane-state.mjs` for durable reports and
`.audit/tools/run-gates.mjs` for resumable gates and the shared heavy-run lock. Do not restart or
kill in-flight work to adopt them. Check each tool's `--help`; the adoption checklist is in
`.audit/PROCESS.md`. This handoff does not claim adoption in another running conversation.

- **The lane** (`.audit/workflows/build-lane.js`): every remaining app unit, built from main,
  verified once (reachable scenarios only), merged straight into main. Restart with the command at the
  end of `.audit/STATUS.md`; add `cont: { <unit>: '<branch>' }` for each unit whose branch has unmerged
  work (branches `lane-<unit>`), and give a continuing builder a note when its state needs one.
- **Historical snapshot, 2026-10-20 (usage limit expected):** on main: B4a, B5a, C6, C8 (merged
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
