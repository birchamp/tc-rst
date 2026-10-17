# Local task: head-to-head journal benchmark (tC4 vs the rebuild)

Run this in a Claude session on a quiet computer (close other heavy programs). The cloud
container that builds the rebuild has 4 shared CPUs at a load of about 8 and little disk, so
its timings are noisy. The owner suggested using tC4's current journaling method, which tC4's
team measured carefully; this run gives the numbers the decision needs. Read `AGENTS.md` and
`docs/PERFORMANCE.md` in `github.com/birchamp/tc-rst` first.

## Setup

1. Clone `github.com/birchamp/tc-rst` (or pull it) and fetch the branch `storage`. That branch
   holds the rebuild's journal (`crates/tc-journal`) and its scale corpus (`crates/tc-corpus`).
2. Clone `github.com/unfoldingWord/translationCore4` and check out commit `d310bf3`. Run
   `npm ci` in it.
3. Record the machine: CPU model, cores, RAM, disk type, operating system, and the load before
   each run.

## Runs

Interleave the two sides (tC4, rebuild, tC4, rebuild, ...) and take at least 5 runs of each.
Report the median and the spread.

- **tC4:** run its own whole-Bible fold benchmark as `docs/evidence/bench-fold-2026-08-25.md`
  describes (for example `node conformance/bench-fold.mjs --bible`), and its open-time harness
  as `docs/evidence/open-time-2026-09-05.md` and `docs/evidence/open-time-windows-2026-09-28.md`
  describe, if it runs without the network.
- **The rebuild:** on the `storage` branch, run the fold benchmark in release with the command
  in `docs/evidence/storage-fold-*.md` (the whole-Bible corpus of `tc-corpus`), and the
  cold-open measurement the same file describes.

Compare like with like. For every number, say which stages it covers (read files, verify
hashes, parse, fold, project) and the corpus size (events or changes, and bytes). If the two
corpora differ in size, also run each side on a scaled corpus so the cost per event can be
compared.

## Output

Write `docs/evidence/journal-head-to-head-<date>.md` on a new branch `bench-local` from
`claude/focused-ride-5uzj7p`, with the machine, every run, the medians and spreads, the
stage breakdown, and what dominated each side. Commit and push `bench-local`. Do not push to
`claude/focused-ride-5uzj7p` or `storage`.
