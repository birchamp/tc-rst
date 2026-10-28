#!/usr/bin/env bash
# A failed nightly job becomes a GitHub issue (rulings/2026-10-24-test-process.md):
#
#   scripts/nightly-issue.sh <job> <log> [--dry-run]
#
# Opens an issue naming the run and the lines of the log that name a seed or a failure.
# --dry-run prints the issue instead, for a local check.
set -euo pipefail
job="${1:?usage: nightly-issue.sh <job> <log> [--dry-run]}"
log="${2:?usage: nightly-issue.sh <job> <log> [--dry-run]}"
date="$(date -u +%Y-%m-%d)"
title="Nightly $job failed on $date"
run="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-local}/actions/runs/${GITHUB_RUN_ID:-none}"
evidence="$(grep -E -i 'seed|panicked|FAILED|minimal failing|INV-' "$log" | head -12 || true)"
body="A nightly long run found a history the push gates did not; it may be a way to lose work.

- The run: $run
- Lines of its log that name a seed or a failure:

\`\`\`
$evidence
\`\`\`

Done when the failure is reproduced from its seed and fixed, or handed to the issue that owns the code."
if [[ "${3:-}" == --dry-run ]]; then printf '%s\n\n%s\n' "$title" "$body"; exit 0; fi
gh issue create --title "$title" --body "$body"
echo "INV-NIGHTLY-FAILURE-IS-AN-ISSUE: $title"
