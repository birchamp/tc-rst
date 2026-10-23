#!/usr/bin/env bash
# The one gate list. CI runs each step by name (`--step <name>`), every local runner runs this
# script, and `.audit/tools/gates-sync.test.mjs` checks the Windows VM's list against it.
#
# The workspace build always has the test hooks through dev-dependencies, so the release-like
# clippy steps (clippy-release-bins, clippy-release-app) check what a release actually compiles.
set -euo pipefail

STEPS=(
  "fmt|cargo fmt --all --check"
  "clippy|cargo clippy --locked --workspace --all-targets -- -D warnings"
  "clippy-release-bins|cargo clippy --locked -p tc-server -p tc4 --lib --bins -- -D warnings"
  "clippy-release-app|cargo clippy --locked -p tc-app --lib -- -D warnings"
  "clippy-windows|cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings"
  "test|cargo test --locked --workspace"
  "alignment-budget|cargo test --locked -p tc-align --release --test suggestion_budget -- --ignored --nocapture"
  "wasm-ui|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings"
  "wasm-ui-gallery|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features gallery -- -D warnings"
  "wasm-ui-app|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings"
  "wasm-ui-hooks|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features test-hooks -- -D warnings"
  "wasm-api|cargo clippy --locked -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings"
  "node|node --test parity/tc4-baseline/baseline.test.mjs parity/tc4-baseline/mock-server/server.test.mjs parity/app/state.test.mjs parity/app/observe.test.mjs parity/app/env-rust.test.mjs parity/view-gallery/view-gallery.test.mjs crates/tc-ui/app/recovery.test.mjs parity/app/journeys/*.test.mjs .audit/tools/*.test.mjs"
)
NIGHTLY=()

# A gate checks; it never blesses a snapshot or rewrites a file (INV-GATES-NEVER-BLESS).
for name in UPDATE_REPORT UPDATE_GOLDEN UPDATE_PREPARE TC_JOURNAL_BLESS TC_WRITE_PROBE; do
  if [[ -v $name ]]; then
    echo "gates: refusing to run: $name is set; a gate never blesses or rewrites files (INV-GATES-NEVER-BLESS)" >&2
    exit 2
  fi
done

root="$(git rev-parse --show-toplevel)"
cd "$root"
target="$(realpath -m "${CARGO_TARGET_DIR:-target}")"

list() {
  local n=1 entry
  for entry in "${STEPS[@]}"; do printf '%s\t%s\t%s\n' "$n" "${entry%%|*}" "${entry#*|}"; n=$((n + 1)); done
  for entry in "${NIGHTLY[@]}"; do printf 'nightly\t%s\t%s\n' "${entry%%|*}" "${entry#*|}"; done
}

# The command of a step, by name or number; empty when there is none.
command_of() {
  local n=1 entry
  for entry in "${STEPS[@]}" "${NIGHTLY[@]}"; do
    if [[ "$1" == "${entry%%|*}" || "$1" == "$n" ]]; then echo "${entry#*|}"; return; fi
    n=$((n + 1))
  done
}

run_step() {
  local name="$1" cmd
  if [[ "$name" == journeys ]]; then journeys; return; fi
  cmd="$(command_of "$name")"
  if [[ -z "$cmd" ]]; then echo "gates: no step named $name" >&2; return 1; fi
  if [[ "$name" == clippy-windows || "$cmd" == *x86_64-pc-windows-gnu* ]] && ! { command -v x86_64-w64-mingw32-gcc && command -v nasm; } > /dev/null; then
    echo "gates: clippy-windows needs x86_64-w64-mingw32-gcc and nasm: sudo apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 nasm" >&2
    return 1
  fi
  bash -c "$cmd"
}

tree_now() { git rev-parse 'HEAD^{tree}'; }
receipt_path() { echo "$target/gates/$(tree_now).ok"; }

check_receipt() {
  local path
  path="$(receipt_path)"
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "INV-NO-UNGATED-MERGE: no passing gate receipt for tree $(tree_now) (HEAD $(git rev-parse --short HEAD)); run scripts/gates.sh on this exact tree (the tree has changes)"
    return 1
  fi
  # The journeys are part of the list since app-e2e (G5); a run without them is partial.
  if [[ -f "$path" ]] && grep -qx 'journeys yes' "$path"; then
    echo "gates: receipt $path"
    return 0
  fi
  ci_receipt && return 0
  if [[ -f "$path" ]]; then
    echo "INV-NO-UNGATED-MERGE: no passing gate receipt for tree $(tree_now) (HEAD $(git rev-parse --short HEAD)); run scripts/gates.sh on this exact tree (journeys not run; drop --no-journeys)"
    return 1
  fi
  echo "INV-NO-UNGATED-MERGE: no passing gate receipt for tree $(tree_now) (HEAD $(git rev-parse --short HEAD)); run scripts/gates.sh on this exact tree, or scripts/gates.sh --wait-ci <card> for GitHub's runs"
  return 1
}

# A green run of both workflows on a commit with this tree is a receipt too (INT-8): GitHub runs
# the same list in parallel, so lanes need not queue for this PC. Only push runs count: a
# pull_request run tests the pull request's merge ref, not the commit's own tree.
ci_receipt() {
  local tree sha runs ci_run e2e_run
  tree="$(tree_now)"; sha="$(git rev-parse HEAD)"
  command -v gh > /dev/null || return 1
  runs="$(gh api "repos/{owner}/{repo}/actions/runs?head_sha=$sha&per_page=50" \
    --jq '.workflow_runs[] | select(.event == "push" and .status == "completed" and .conclusion == "success") | "\(.name) \(.id) \(.head_sha)"' 2> /dev/null)" || return 1
  ci_run="$(green_run_for ci "$runs" "$tree")"
  e2e_run="$(green_run_for app-e2e "$runs" "$tree")"
  [[ -n "$ci_run" && -n "$e2e_run" ]] || return 1
  echo "gates: CI receipt: ci run $ci_run, app-e2e run $e2e_run on ${sha:0:8}"
}

# The id of a green run of workflow $1 among the lines $2 whose commit has the tree $3.
green_run_for() {
  local name id run_sha
  while read -r name id run_sha; do
    [[ "$name" == "$1" && "$(git rev-parse "$run_sha^{tree}" 2> /dev/null)" == "$3" ]] && { echo "$id"; return; }
  done <<< "$2"
}

# Push HEAD to gate/<card>, wait for GitHub's ci and app-e2e runs on it, then check the receipt.
wait_ci() {
  local card="$1" sha deadline done_runs
  [[ -z "$(git status --porcelain)" ]] || { echo "gates: --wait-ci needs a clean tree" >&2; return 1; }
  command -v gh > /dev/null || { echo "gates: --wait-ci needs gh on PATH" >&2; return 1; }
  sha="$(git rev-parse HEAD)"
  git push origin "+$sha:refs/heads/gate/$card"
  echo "gates: pushed ${sha:0:8} to gate/$card; waiting for the ci and app-e2e runs"
  deadline=$((SECONDS + 5400))
  while (( SECONDS < deadline )); do
    done_runs="$(gh api "repos/{owner}/{repo}/actions/runs?head_sha=$sha&per_page=50" \
      --jq '[.workflow_runs[] | select(.event == "push" and .status == "completed") | .name] | unique | join(" ")' 2> /dev/null || true)"
    if [[ " $done_runs " == *" ci "* && " $done_runs " == *" app-e2e "* ]]; then
      check_receipt
      return
    fi
    sleep 60
  done
  echo "gates: no completed ci and app-e2e runs for ${sha:0:8} after 90 minutes" >&2
  return 1
}

# Every browser journey against real builds; app-e2e.yml runs this as `--step journeys`.
journeys() {
  [[ -d "${PLAYWRIGHT_MODULE:-parity/tc4-baseline/node_modules/playwright}" ]] || (cd parity/tc4-baseline && npm ci)
  cargo build --locked -p tc4 --features test-hooks,fixture-backend
  cargo build --locked -p tc-door43-fake
  crates/tc-ui/build-app.sh --out "$target/tc-ui-app"
  crates/tc-ui/build-app.sh --test-hooks --out "$target/tc-ui-app-hooks"
  local tap="$target/app-e2e.tap" code=0
  TC4_BIN="$target/debug/tc4" TC4_UI="$target/tc-ui-app" TC4_UI_HOOKS="$target/tc-ui-app-hooks" \
    TC4_DOOR43_FAKE="$target/debug/tc-door43-fake" \
    node --test --test-reporter=tap parity/app/journeys/*.test.mjs | tee "$tap" || code=$?
  # Everything is built, so a skip is a missing build (INV-A-SKIP-IS-A-FAILURE).
  node parity/app/journeys/no-skips.mjs "$tap" || code=1
  return "$code"
}

run_all() {
  local with_journeys="$1" before_status before_tree total n=1 entry name
  before_status="$(git status --porcelain)"; before_tree="$(tree_now)"
  total=${#STEPS[@]}
  for entry in "${STEPS[@]}"; do
    name="${entry%%|*}"
    echo "gates: $n/$total $name"
    local code=0
    run_step "$name" || code=$?
    if [[ $code -ne 0 ]]; then
      echo "gates: step $name failed (exit $code)" >&2
      exit 1
    fi
    n=$((n + 1))
  done
  if [[ "$with_journeys" == yes ]]; then
    echo "gates: journeys"
    journeys
  fi
  if [[ -n "$before_status" || -n "$(git status --porcelain)" || "$before_tree" != "$(tree_now)" ]]; then
    echo "gates: all $total steps passed; no receipt: the tree has changes"
    return 0
  fi
  local path
  path="$(receipt_path)"
  mkdir -p "$(dirname "$path")"
  {
    echo "tree $before_tree"
    echo "head $(git rev-parse HEAD)"
    echo "date $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "rustc $(rustc --version)"
    echo "steps $total"
    echo "journeys $with_journeys"
  } > "$path"
  echo "gates: all $total steps passed; receipt $path"
}

case "${1:-}" in
  "" | --journeys) run_all yes ;;
  --no-journeys) run_all no ;;
  --list) list ;;
  --step) run_step "${2:?gates: --step needs a name or number}" ;;
  --check-receipt) check_receipt ;;
  --wait-ci) wait_ci "${2:?gates: --wait-ci needs a card id}" ;;
  *) echo "usage: scripts/gates.sh [--list | --step <name or number> | --no-journeys | --check-receipt | --wait-ci <card>]" >&2; exit 2 ;;
esac
