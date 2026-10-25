# INT-6: may two finished copies and their build files be deleted?

- Asked: 2026-10-23, by the integration lane (Claude Code 3d07d5f8), through the orchestrator
  session, for INT-6 step 3.
- Options: yes, delete `.claude/worktrees/lane-b4b` and `.claude/worktrees/lane-b5b` with their
  build folders `/var/tmp/tc-rst-target/lane-b4b` (30 GB) and `lane-b5b` (43 GB), about 73 GB,
  branches kept (recommended); no, they stay and nothing breaks.
- Answer: "yes delete", 2026-10-23.

## The rule

Exactly the listed worktrees and build folders are deleted, with `git worktree remove` (never
`--force`); their branches stay.

## Effect for translators

None. Nothing a translator wrote was in them: both were merged into main and held no unsaved
change.

## Unblocks and changes

INT-6 step 4.
