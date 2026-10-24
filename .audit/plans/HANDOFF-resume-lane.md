History: superseded by .audit/roadmap/ on 2026-10-21; do not update.

# Handoff: the resume lane (kept current)

Any agent can pick this up. Read, in order: this file, `.audit/STATUS.md`, `.audit/plans/RESUME-2026-10-20.md`
(one checkpoint per finished unit), then `.audit/plans/<unit>.md` for a unit in progress.

- Branch `resume-build-lane`, worktree `/home/bot/tc-rst/.worktrees/resume-build-lane`. Work only here.
  Nothing from this branch is pushed; `main` is behind it. Do not push without the owner.
- Units build in lane worktrees `target/lanes/<unit>` on branches `rl-<unit>`, then merge into
  `resume-build-lane`. A lane that still exists is unfinished: read its `git log` and `git status`.
- The orchestration is `.audit/workflows/resume-lane.js` (Workflow tool), with
  `.audit/workflows/resume-lane-args.json` as its args. To restart: drop finished units from `units`,
  put a finished build's report under `built`, and give a partly built lane a line in `notes`.
- Fix rule (owner, 2026-10-20): every blocker, and majors only when they harm a translator's data or
  what they see; the rest goes to `.audit/minors.tsv`. Reachable scenarios only.
- Usage limits cut agents off every few hours. A cut-off merge leaves its merge commit with unfinished
  records; the merge step handles that ("already in").

## State, 2026-10-21 12:20

On `resume-build-lane`: 38 of 48 app units (C3, B8, C4, B11 added in this stretch). Running: B11's
post-merge records, then C7a, then C7b (needs the owner's Q-9 design sign-off), then one full gate.
The other 8 app units wait on storage (S2, X, I, P), which a separate S2 session is working on.
