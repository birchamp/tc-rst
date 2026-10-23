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
  cmd="$(command_of "$name")"
  if [[ -z "$cmd" ]]; then echo "gates: no step named $name" >&2; return 1; fi
  if [[ "$name" == clippy-windows || "$cmd" == *x86_64-pc-windows-gnu* ]] && ! command -v x86_64-w64-mingw32-gcc nasm > /dev/null; then
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
  if [[ -z "$(git status --porcelain)" && -f "$path" ]]; then
    echo "gates: receipt $path"
    return 0
  fi
  echo "INV-NO-UNGATED-MERGE: no passing gate receipt for tree $(tree_now) (HEAD $(git rev-parse --short HEAD)); run scripts/gates.sh on this exact tree"
  return 1
}

journeys() {
  cargo build --locked -p tc4 --features test-hooks,fixture-backend
  cargo build --locked -p tc-door43-fake
  crates/tc-ui/build-app.sh --out "$target/tc-ui-app"
  crates/tc-ui/build-app.sh --test-hooks --out "$target/tc-ui-app-hooks"
  local tap
  tap="$(TC4_BIN="$target/debug/tc4" TC4_UI="$target/tc-ui-app" TC4_UI_HOOKS="$target/tc-ui-app-hooks" \
    TC4_DOOR43_FAKE="$target/debug/tc-door43-fake" \
    node --test --test-reporter=tap parity/app/journeys/*.test.mjs | tee /dev/stderr)"
  local failed skipped
  failed="$(printf '%s\n' "$tap" | sed -n 's/^# fail \([0-9]*\)$/\1/p' | tail -1)"
  skipped="$(printf '%s\n' "$tap" | sed -n 's/^# skipped \([0-9]*\)$/\1/p' | tail -1)"
  # Everything is built, so a skip is a missing build.
  if [[ "${failed:-1}" != 0 || "${skipped:-1}" != 0 ]]; then
    echo "gates: journeys failed: ${failed:-?} fail, ${skipped:-?} skipped" >&2
    return 1
  fi
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
    echo "journeys $([[ "$with_journeys" == yes ]] && echo yes || echo no)"
  } > "$path"
  echo "gates: all $total steps passed; receipt $path"
}

case "${1:-}" in
  "") run_all no ;;
  --journeys) run_all yes ;;
  --list) list ;;
  --step) run_step "${2:?gates: --step needs a name or number}" ;;
  --check-receipt) check_receipt ;;
  *) echo "usage: scripts/gates.sh [--list | --step <name or number> | --journeys | --check-receipt]" >&2; exit 2 ;;
esac
