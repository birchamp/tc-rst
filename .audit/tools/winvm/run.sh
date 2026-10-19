#!/usr/bin/env bash
# Run a command in the Windows test VM on a given commit of this repository.
# Usage: .audit/tools/winvm/run.sh <commit-ish> <powershell command run in C:\src\tc-rst>
# The VM gets the code as a git bundle over SSH, so it needs no GitHub sign-in.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
PORT="${WINVM_PORT:-2222}"; USER="${WINVM_USER:-builder}"; KEY="${WINVM_KEY:-$HOME/.ssh/tc-vm}"
SSH=(ssh -i "$KEY" -p "$PORT" -o BatchMode=yes -o StrictHostKeyChecking=accept-new "$USER@127.0.0.1")
rev="$(git -C "$REPO" rev-parse "${1:?commit}")"; shift
bundle="$(mktemp --suffix=.bundle)"; trap 'rm -f "$bundle"' EXIT
git -C "$REPO" bundle create "$bundle" "$rev" --all-reflogs >/dev/null 2>&1 || git -C "$REPO" bundle create "$bundle" "$rev" >/dev/null
scp -i "$KEY" -P "$PORT" -o BatchMode=yes -q "$bundle" "$USER@127.0.0.1:C:/src/tc.bundle"
"${SSH[@]}" "powershell -NoProfile -Command \"\$ErrorActionPreference='Stop'; Set-Location C:\\src; if (-not (Test-Path tc-rst\\.git)) { git clone -q --no-checkout tc.bundle tc-rst }; Set-Location tc-rst; git fetch -q ..\\tc.bundle; git checkout -q --force $rev; git clean -q -fdx -e target; \$env:Path = \\\"\$env:USERPROFILE\\.cargo\\bin;\$env:Path\\\"; $*\""
