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
  # nextest names a test that runs past 60 s and stops it at its limit (.config/nextest.toml,
  # CHK-8). Each part is its own step, so one failure hides none of the others.
  "test|cargo nextest run --locked --workspace --no-fail-fast --exclude tc-app"
  # nextest runs no doctests.
  "test-doc|cargo test --locked --workspace --doc --no-fail-fast"
  # tc-app alone took 39 min locally: in one CI job with the rest it ran past the job's 75 min.
  "test-app|cargo nextest run --locked -p tc-app --no-fail-fast"
  # Its op-count tests run on libtest, which shares their 7 GB corpora (.config/nextest.toml);
  # CI's job limit bounds them.
  "test-app-op-counts|cargo test --locked -p tc-app --test op_counts --no-fail-fast"
  "alignment-budget|cargo nextest run --locked -p tc-align --release --test suggestion_budget --run-ignored only --no-capture"
  "ignored|scripts/check-ignored.sh"
  "wasm-ui|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings"
  "wasm-ui-gallery|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features gallery -- -D warnings"
  "wasm-ui-app|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings"
  "wasm-ui-hooks|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features test-hooks -- -D warnings"
  "wasm-api|cargo clippy --locked -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings"
  # --test-timeout bounds each file, which node runs as one test (CHK-8).
  "node|node --test --test-timeout=120000 parity/tc4-baseline/baseline.test.mjs parity/tc4-baseline/mock-server/server.test.mjs parity/app/state.test.mjs parity/app/observe.test.mjs parity/app/env-rust.test.mjs parity/view-gallery/view-gallery.test.mjs crates/tc-ui/app/recovery.test.mjs scripts/release/fetch-pinned.test.mjs parity/app/journeys/*.test.mjs .audit/tools/*.test.mjs"
  "board|python3 -I -B .audit/tools/board_test.py && python3 -I -B .audit/tools/union_test.py && python3 -I -B .audit/tools/board.py --check"
)
NIGHTLY=(
  "model-long|TC_MODEL_ROWS=1 cargo test --locked -p tc-store --test store model::random_histories_lose_no_work_long -- --ignored --exact --nocapture"
  "sync-model-long|cargo test --locked -p tc-sync --test sync model::random_syncs_lose_no_work_long -- --ignored --exact"
)

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
  if [[ -f "$path" ]]; then
    # The journeys are part of the list since app-e2e (G5); a local run without them is partial.
    if grep -qx 'journeys yes' "$path"; then
      echo "gates: receipt $path"
      return 0
    fi
    ci_receipt && return 0
    echo "INV-NO-UNGATED-MERGE: no passing gate receipt for tree $(tree_now) (HEAD $(git rev-parse --short HEAD)); run scripts/gates.sh on this exact tree (journeys not run; drop --no-journeys)"
    return 1
  fi
  ci_receipt && return 0
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
    --jq '.workflow_runs[] | select(.event == "push" and .status == "completed" and .conclusion == "success") | "\(.name) \(.id) \(.head_sha) \(.head_branch)"' 2> /dev/null)" || return 1
  ci_run="$(green_run_for ci "$runs" "$tree")"
  e2e_run="$(green_run_for app-e2e "$runs" "$tree")"
  [[ -n "$ci_run" && -n "$e2e_run" ]] || return 1
  echo "gates: CI receipt: ci run $ci_run, app-e2e run $e2e_run on ${sha:0:8}"
}

# The id of a green run of workflow $1 among the lines $2 whose commit has the tree $3.
# Only a run on gate/* or main ran the full tests: a track push of the same commit runs only the
# lints (rulings/2026-10-24-test-process.md), and its green run is no receipt.
green_run_for() {
  local name id run_sha branch
  while read -r name id run_sha branch; do
    [[ "$branch" == gate/* || "$branch" == claude/focused-ride-5uzj7p ]] || continue
    [[ "$name" == "$1" && "$(git rev-parse "$run_sha^{tree}" 2> /dev/null)" == "$3" ]] && { echo "$id"; return; }
  done <<< "$2"
}

# Push HEAD to gate/<card>, wait for GitHub's ci and app-e2e runs on it, then check the receipt.
wait_ci() {
  local card="$1" sha deadline done_runs
  [[ -z "$(git status --porcelain)" ]] || { echo "gates: --wait-ci needs a clean tree" >&2; return 1; }
  command -v gh > /dev/null || { echo "gates: --wait-ci needs gh on PATH" >&2; return 1; }
  # Seconds here, before thirty minutes of CI: the cards, the claimed tests and the journey map.
  python3 -I -B .audit/tools/board.py --check > /dev/null \
    && node --test .audit/tools/claims.test.mjs parity/app/journeys/coverage.test.mjs > /dev/null \
    || { echo "gates: --wait-ci stops before pushing: board.py --check or the claims and coverage checks fail; run them to see why" >&2; return 1; }
  sha="$(git rev-parse HEAD)"
  git push origin "+$sha:refs/heads/gate/$card"
  echo "gates: pushed ${sha:0:8} to gate/$card; waiting for the ci and app-e2e runs"
  deadline=$((SECONDS + 5400))
  while (( SECONDS < deadline )); do
    done_runs="$(gh api "repos/{owner}/{repo}/actions/runs?head_sha=$sha&per_page=50" \
      --jq '[.workflow_runs[] | select(.event == "push" and .status == "completed" and .head_branch == "gate/'"$card"'") | .name] | unique | join(" ")' 2> /dev/null || true)"
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
# `|| return 1` on each build: under `journeys ... || code=1` bash turns set -e off, and the
# journeys must never run against an older build.
journeys() {
  [[ -d "${PLAYWRIGHT_MODULE:-parity/tc4-baseline/node_modules/playwright}" ]] || (cd parity/tc4-baseline && npm ci)
  cargo build --locked -p tc4 --features test-hooks,fixture-backend || return 1
  cargo build --locked -p tc-door43-fake || return 1
  crates/tc-ui/build-app.sh --out "$target/tc-ui-app" || return 1
  crates/tc-ui/build-app.sh --test-hooks --out "$target/tc-ui-app-hooks" || return 1
  local tap="$target/app-e2e.tap" code=0
  (( $# )) || set -- parity/app/journeys/*.test.mjs
  TC4_BIN="$target/debug/tc4" TC4_UI="$target/tc-ui-app" TC4_UI_HOOKS="$target/tc-ui-app-hooks" \
    TC4_DOOR43_FAKE="$target/debug/tc-door43-fake" \
    node --test --test-reporter=tap "$@" | tee "$tap" || code=$?
  # Everything is built, so a skip is a missing build (INV-A-SKIP-IS-A-FAILURE).
  node parity/app/journeys/no-skips.mjs "$tap" || code=1
  return "$code"
}

# Before a hand-over (CHK-7b): the fast checks, then the tests of the crates the branch changed and
# of every crate that depends on them, tc-api's endpoint and golden lists, and the journeys that
# name a changed endpoint or test id. CI on a track branch runs only the lints.
touched() {
  local base="${1:-origin/claude/focused-ride-5uzj7p}" picked crates=() files=() code=0
  python3 -I -B .audit/tools/board.py --check
  node --test .audit/tools/claims.test.mjs parity/app/journeys/coverage.test.mjs .audit/tools/touched.test.mjs
  picked="$(node .audit/tools/touched.mjs "$base")"
  mapfile -t crates < <(node -e 'for (const c of JSON.parse(process.argv[1]).crates) console.log(c)' "$picked")
  mapfile -t files < <(node -e 'for (const f of JSON.parse(process.argv[1]).journeys) console.log(f)' "$picked")
  echo "gates: --touched: ${#crates[@]} crates, ${#files[@]} journey files"
  cargo nextest run --locked --no-fail-fast -p tc-api --test endpoints --test golden || code=1
  if (( ${#crates[@]} )); then
    cargo nextest run --locked --no-fail-fast "${crates[@]/#/--package=}" || code=1
    cargo test --locked --no-fail-fast --doc "${crates[@]/#/--package=}" || code=1
    if [[ " ${crates[*]} " == *" tc-app "* ]]; then cargo test --locked --no-fail-fast -p tc-app --test op_counts || code=1; fi
  fi
  if (( ${#files[@]} )); then
    journeys "${files[@]}" || code=1
  fi
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
  --touched) touched "${2:-}" ;;
  *) echo "usage: scripts/gates.sh [--list | --step <name or number> | --no-journeys | --check-receipt | --wait-ci <card> | --touched [base]]" >&2; exit 2 ;;
esac
