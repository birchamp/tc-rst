#!/usr/bin/env bash
# INV-NO-BLESS-IN-CI: every #[ignore] test has a line in scripts/ignored-tests.txt naming the CI
# job that runs it, or `local: <why>`; a listed name that is no ignored test fails too.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
listed="$(grep -v '^#' scripts/ignored-tests.txt | grep -v '^$' | cut -f1 | sort -u)"
# cargo's own errors stay on stderr, so a build that fails says why.
found="$(cargo test --locked --workspace -- --list --ignored | sed -n 's/: test$//p' | sort -u)"
status=0
while read -r name; do
  [[ -z "$name" ]] && continue
  grep -qxF -- "$name" <<< "$listed" || { echo "INV-NO-BLESS-IN-CI: ignored test $name has no line in scripts/ignored-tests.txt"; status=1; }
done <<< "$found"
while read -r name; do
  [[ -z "$name" ]] && continue
  grep -qxF -- "$name" <<< "$found" || { echo "INV-NO-BLESS-IN-CI: scripts/ignored-tests.txt lists $name, which is no ignored test"; status=1; }
done <<< "$listed"
exit "$status"
