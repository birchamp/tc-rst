#!/usr/bin/env bash
# The one gate list. CI runs each step by name (`--step <name>`), every local runner runs this
# script, and `scripts/tools/gates-sync.test.mjs` checks the Windows VM's list against it.
#
# The workspace build always has the test hooks through dev-dependencies, so the release-like
# clippy steps (clippy-release-bins, clippy-release-app) check what a release actually compiles.
set -euo pipefail

STEPS=(
  "fmt|cargo fmt --all --check"
  "clippy|cargo clippy --locked --workspace --all-targets -- -D warnings"
  "clippy-release-bins|cargo clippy --locked -p tc-server -p tc4 --lib --bins -- -D warnings"
  "clippy-release-app|cargo clippy --locked -p tc-app --lib -- -D warnings"
  # Also inside test; on its own so the pull request lints run it (it takes seconds).
  "arch|cargo nextest run --locked -p tc-arch"
  "clippy-windows|cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings"
  # nextest names a test that runs past 60 s and stops it at its limit (.config/nextest.toml,
  # CHK-8). Each part is its own step, so one failure hides none of the others.
  "test|cargo nextest run --locked --workspace --no-fail-fast --exclude tc-app"
  # nextest runs no doctests.
  "test-doc|cargo test --locked --workspace --doc --no-fail-fast"
  # tc-app alone took 39 min locally: in one CI job with the rest it ran past the job's 75 min.
  "test-app|cargo nextest run --locked -p tc-app --no-fail-fast"
  "alignment-budget|cargo nextest run --locked -p tc-align --release --test suggestion_budget --run-ignored only --no-capture"
  "ignored|scripts/check-ignored.sh"
  "wasm-ui|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings"
  "wasm-ui-gallery|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features gallery -- -D warnings"
  "wasm-ui-app|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings"
  "wasm-ui-hooks|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features test-hooks -- -D warnings"
  "wasm-api|cargo clippy --locked -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings"
  # --test-timeout bounds each file, which node runs as one test (CHK-8).
  "node|node --test --test-timeout=120000 parity/tc4-baseline/mock-server/server.test.mjs parity/app/state.test.mjs parity/app/observe.test.mjs parity/app/env-rust.test.mjs crates/tc-ui/app/recovery.test.mjs scripts/release/fetch-pinned.test.mjs parity/app/journeys/*.test.mjs scripts/tools/*.test.mjs"
)
NIGHTLY=(
  "model-long|TC_MODEL_ROWS=1 cargo test --locked -p tc-store --test store model::random_histories_lose_no_work_long -- --ignored --exact --nocapture"
  # The scale tests at ten times Psalms (rulings/2026-10-26-test-scale.md); pushes and gates run 2.
  "scale-long|TC_SCALE_FACTOR=10 cargo test --locked --no-fail-fast -p tc-journal --test operation_counts -p tc-store --test store -p tc-app --test op_counts"
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
  if [[ "$name" == clippy-windows || "$cmd" == *x86_64-pc-windows-gnu* ]]; then
    run_step_windows_ready || return 1
  fi
  bash -c "$cmd"
}

run_step_windows_ready() {
  { command -v x86_64-w64-mingw32-gcc && command -v nasm; } > /dev/null && return 0
  echo "gates: clippy-windows needs x86_64-w64-mingw32-gcc and nasm: sudo apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 nasm" >&2
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

# Before a pull request (CHK-7b): every lint step a pull request's CI runs (lint-linux,
# lint-windows, lint-wasm), each over the crates the branch changed and every crate that depends
# on them, and skipped when none of its crates is among them; the fast checks; then those crates'
# tests, tc-api's endpoint and golden lists, and the journeys that name a changed endpoint or test
# id. The full gate list runs in the merge queue.
PR_LINTS=(fmt clippy clippy-release-bins clippy-release-app arch clippy-windows
  wasm-ui wasm-ui-gallery wasm-ui-app wasm-ui-hooks wasm-api)

# The lint step `$1` over the picked crates (the rest of the arguments): run, narrowed or skipped.
touched_lint() {
  local step="$1" crates=("${@:2}")
  local has=" ${crates[*]} " packages=("${crates[@]/#/--package=}")
  case "$step" in
    fmt) run_step fmt ;;
    clippy)
      (( ${#crates[@]} )) || return 0
      cargo clippy --locked "${packages[@]}" --all-targets -- -D warnings ;;
    clippy-windows)
      (( ${#crates[@]} )) || return 0
      run_step_windows_ready || return 1
      cargo clippy --locked "${packages[@]}" --all-targets --target x86_64-pc-windows-gnu -- -D warnings ;;
    clippy-release-bins) [[ "$has" == *" tc-server "* || "$has" == *" tc4 "* ]] || return 0; run_step "$step" ;;
    clippy-release-app) [[ "$has" == *" tc-app "* ]] || return 0; run_step "$step" ;;
    wasm-ui*) [[ "$has" == *" tc-ui "* ]] || return 0; run_step "$step" ;;
    wasm-api) [[ "$has" == *" tc-api "* ]] || return 0; run_step "$step" ;;
    # tc-arch reads the whole tree and no crate depends on it, so no change ever picks it.
    arch) run_step arch ;;
    # A lint step this list does not know yet runs whole.
    *) run_step "$step" ;;
  esac
}

touched() {
  local base="${1:-origin/HEAD}" picked crates=() files=() code=0 step
  node --test scripts/tools/claims.test.mjs scripts/tools/docs.test.mjs parity/app/journeys/coverage.test.mjs scripts/tools/touched.test.mjs
  picked="$(node scripts/tools/touched.mjs "$base")"
  mapfile -t crates < <(node -e 'for (const c of JSON.parse(process.argv[1]).crates) console.log(c)' "$picked")
  mapfile -t files < <(node -e 'for (const f of JSON.parse(process.argv[1]).journeys) console.log(f)' "$picked")
  echo "gates: --touched: ${#crates[@]} crates, ${#files[@]} journey files"
  for step in "${PR_LINTS[@]}"; do
    echo "gates: --touched: $step"
    touched_lint "$step" "${crates[@]}" || { echo "gates: step $step failed" >&2; code=1; }
  done
  cargo nextest run --locked --no-fail-fast -p tc-api --test endpoints --test golden || code=1
  if (( ${#crates[@]} )); then
    cargo nextest run --locked --no-fail-fast "${crates[@]/#/--package=}" || code=1
    cargo test --locked --no-fail-fast --doc "${crates[@]/#/--package=}" || code=1
  fi
  if (( ${#files[@]} )); then
    journeys "${files[@]}" || code=1
  fi
  return "$code"
}

run_all() {
  local with_journeys="$1" total=${#STEPS[@]} n=1 entry name
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
  echo "gates: all $total steps passed"
}

case "${1:-}" in
  "" | --journeys) run_all yes ;;
  --no-journeys) run_all no ;;
  --list) list ;;
  --step) run_step "${2:?gates: --step needs a name or number}" ;;
  --touched) touched "${2:-}" ;;
  *) echo "usage: scripts/gates.sh [--list | --step <name or number> | --no-journeys | --touched [base]]" >&2; exit 2 ;;
esac
