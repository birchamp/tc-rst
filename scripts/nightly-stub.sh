#!/usr/bin/env bash
# A failed nightly job becomes a card the integration lane picks up (rulings/2026-10-24-test-process.md):
#
#   scripts/nightly-stub.sh <job> <log> [--no-push]
#
# Writes .audit/roadmap/units/NIGHTLY-<date>-<job>.md (state todo) naming the run and the lines of
# the log that name a seed or a failure, commits it on a new branch nightly/<date>-<job> and pushes
# it. --no-push stops after the commit, for a local check.
set -euo pipefail
job="${1:?usage: nightly-stub.sh <job> <log> [--no-push]}"
log="${2:?usage: nightly-stub.sh <job> <log> [--no-push]}"
date="$(date -u +%Y-%m-%d)"
id="NIGHTLY-$date-$job"
card=".audit/roadmap/units/$id.md"
run="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-local}/actions/runs/${GITHUB_RUN_ID:-none}"
evidence="$(grep -E -i 'seed|panicked|FAILED|minimal failing|INV-' "$log" | head -12 | sed 's/^/    /' || true)"
cat > "$card" <<CARD
---
id: $id
title: Nightly $job failed on $date
track: A
lane: integration
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: false
size: unknown (a nightly failure, not yet read)
merged: none
updated: $date
next: Read the run, reproduce the seed, then fix it or open the card that owns the failing code.
blocked_by: none
---

# $id: Nightly $job failed on $date

## Goal

The failure is reproduced from its seed and fixed, or handed to the card that owns the code.

## Why it matters to a translator

A nightly long run found a history the push gates did not; it may be a way to lose work.

## Before you start

- The run: $run
- Lines of its log that name a seed or a failure:

$evidence

## Funnel card

- **Invariants**: the failing test's own.
- **Checks**: the failing test, rerun with the seed above.

## Steps

### Step 1: Reproduce, then fix or hand over

- Do: rerun the failing test with the seed; fix the cause or open a card for its owner.
- Record: a log line \`step 1\`.

## Log

- $date | nightly ($job) | created | none | the scheduled $job job failed | $run
CARD
# DONE depends on every card (board.py INV-TRACKER-TRUE).
python3 -I - "$id" <<'PY'
import re, sys
p = ".audit/roadmap/units/DONE.md"
s = open(p).read()
m = re.search(r"^depends: \[(.*)\]$", s, re.M)
ids = sorted(set(m.group(1).split(", ") + [sys.argv[1]]), key=str.upper)
open(p, "w").write(s[:m.start()] + "depends: [" + ", ".join(ids) + "]" + s[m.end():])
PY
git -c user.name="tc nightly" -c user.email=noreply@github.com checkout -q -b "nightly/$date-$job"
git add "$card" .audit/roadmap/units/DONE.md
git -c user.name="tc nightly" -c user.email=noreply@github.com commit -q -m "Nightly $job failed on $date: a card for the integration lane" -m "$run"
[[ "${3:-}" == --no-push ]] || git push -q origin "HEAD:refs/heads/nightly/$date-$job"
echo "nightly: $card on nightly/$date-$job"
