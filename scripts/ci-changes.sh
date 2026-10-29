#!/usr/bin/env bash
# Whether a CI run's change can affect the Rust tests, the journeys or the shell: prints
# `code=true` or `code=false` (to $GITHUB_OUTPUT when set). Only a change wholly in prose can be
# false: files under docs/ (but not docs/contracts/, which tests read) and top-level *.md files.
# The node step still checks the prose. Anything unknown (no base, a failed compare, a schedule
# or a dispatch) is true.
#
#   scripts/ci-changes.sh                 in CI: BASE, HEAD and GITHUB_REPOSITORY from the event
#   scripts/ci-changes.sh --classify      the file list on stdin, one path per line
set -uo pipefail

classify() {
  local path any=false
  while IFS= read -r path || [[ -n "$path" ]]; do
    [[ -z "$path" ]] && continue
    any=true
    case "$path" in
      docs/contracts/*) echo true; return ;;
      docs/*) ;;
      */*) echo true; return ;;
      *.md) ;;
      *) echo true; return ;;
    esac
  done
  # An empty list is a compare that said nothing: treat it as code.
  if [[ $any == true ]]; then echo false; else echo true; fi
}

if [[ "${1:-}" == --classify ]]; then
  classify
  exit 0
fi

code=true
if [[ -n "${BASE:-}" && -n "${HEAD:-}" && ! "$BASE" =~ ^0+$ ]]; then
  # The compare lists at most 3000 files across its pages; a change that large is code anyway.
  if files=$(gh api --paginate "repos/$GITHUB_REPOSITORY/compare/$BASE...$HEAD" --jq '.files[].filename'); then
    code=$(classify <<< "$files")
  fi
fi
echo "code=$code"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then echo "code=$code" >> "$GITHUB_OUTPUT"; fi
