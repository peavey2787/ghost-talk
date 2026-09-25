#!/usr/bin/env bash
set -euo pipefail
SCRIPT="$(readlink -f "$0" 2>/dev/null || realpath "$0" 2>/dev/null || printf '%s' "$0")"
if [[ "${GHOST_TALK_IN_TERMINAL:-0}" != "1" && ! -t 1 ]]; then
  for terminal in x-terminal-emulator xfce4-terminal gnome-terminal mate-terminal konsole xterm; do
    command -v "$terminal" >/dev/null 2>&1 || continue
    case "$terminal" in
      xfce4-terminal) "$terminal" --command="env GHOST_TALK_IN_TERMINAL=1 bash '$SCRIPT'" ;;
      gnome-terminal|mate-terminal) "$terminal" -- env GHOST_TALK_IN_TERMINAL=1 bash "$SCRIPT" ;;
      *) "$terminal" -e env GHOST_TALK_IN_TERMINAL=1 bash "$SCRIPT" ;;
    esac
    exit 0
  done
fi
if (( $# != 0 )); then
  echo "ERROR: run-all-tests accepts no test filters/arguments; the full suite is mandatory." >&2
  exit 2
fi
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
if command -v python3 >/dev/null 2>&1; then PYTHON=python3
elif command -v python >/dev/null 2>&1; then PYTHON=python
else echo "ERROR: Python 3 is required for architecture/test-matrix/CRAP checks." >&2; exit 1
fi
command -v cargo >/dev/null 2>&1 || { echo "ERROR: Rust/Cargo is required." >&2; exit 1; }

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}
require_lcov() {
  [[ -s "$1" ]] || { echo "ERROR: required LCOV report is missing or empty: $1" >&2; exit 1; }
}

printf 'Ghost Talk - COMPLETE repository quality gates\n'
printf 'No filters are accepted: default/all-feature tests, integration, doctest, browser/WASM, every LCOV surface, and CRAP are mandatory.\n'

# Fast fail before expensive compilation. The matrix guard inventories every
# first-party manifest and verifies that this runner cannot silently omit a test
# or coverage surface.
run "$PYTHON" examples/ghost-talk-app/scripts/check-test-matrix.py
run "$PYTHON" examples/ghost-talk-app/scripts/check-architecture.py

run cargo fmt --all -- --check
# Native compilation deliberately precedes rustup/WASM provisioning so broken
# source cannot be masked by an optional-target setup failure.
run cargo check --workspace --all-targets
run cargo check --workspace --all-targets --all-features
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup is required to prepare the wasm32 target." >&2; exit 1; }
bash scripts/tooling/ensure-wasm-target.sh
cargo llvm-cov --version >/dev/null 2>&1 || {
  echo "ERROR: cargo-llvm-cov is required for repository-wide LCOV/CRAP coverage (cargo install cargo-llvm-cov)." >&2
  exit 1
}
command -v wasm-pack >/dev/null 2>&1 || {
  echo "ERROR: wasm-pack is required for real-browser SDK/application WASM tests." >&2
  exit 1
}
run cargo clippy --workspace --all-targets -- -D warnings
run cargo clippy --workspace --all-targets --all-features -- -D warnings
run cargo clippy --workspace --target wasm32-unknown-unknown --all-targets -- -D warnings
run cargo clippy --workspace --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings

# Run both feature configurations. This matters for future cfg(feature) and
# cfg(not(feature)) tests; --all-features alone is not a complete matrix.
run cargo test --workspace --all-targets --no-fail-fast
run cargo test --workspace --all-targets --all-features --no-fail-fast
run cargo test --workspace --doc --no-fail-fast
run cargo test --workspace --doc --all-features --no-fail-fast

WASM_BROWSER="${GHOST_TALK_WASM_BROWSER:-firefox}"
case "$WASM_BROWSER" in firefox|chrome) ;; *) echo "ERROR: GHOST_TALK_WASM_BROWSER must be firefox or chrome." >&2; exit 1 ;; esac
run wasm-pack test --headless "--$WASM_BROWSER" crates/ghost-talk-wasm

mkdir -p target/coverage
ROOT_DEFAULT_LCOV="$ROOT/target/coverage/root-default.lcov"
ROOT_ALL_FEATURES_LCOV="$ROOT/target/coverage/root-all-features.lcov"
run cargo llvm-cov --workspace --all-targets --no-fail-fast --lcov --output-path "$ROOT_DEFAULT_LCOV"
run cargo llvm-cov --workspace --all-targets --all-features --no-fail-fast --lcov --output-path "$ROOT_ALL_FEATURES_LCOV"
require_lcov "$ROOT_DEFAULT_LCOV"
require_lcov "$ROOT_ALL_FEATURES_LCOV"
export GHOST_TALK_ROOT_LCOV_DEFAULT="$ROOT_DEFAULT_LCOV"
export GHOST_TALK_ROOT_LCOV_ALL_FEATURES="$ROOT_ALL_FEATURES_LCOV"

printf '\n==> Reference application COMPLETE quality gates\n'
GHOST_TALK_NO_PAUSE=1 GHOST_TALK_IN_TERMINAL=1 bash examples/ghost-talk-app/scripts/run-all-tests.sh

printf '\nALL QUALITY GATES PASSED. Every configured test surface, every LCOV surface, and the combined CRAP gate completed.\n'
if [[ "${GHOST_TALK_NO_PAUSE:-0}" != "1" && -t 0 ]]; then
  read -r -p "Press Enter to close..." _ || true
fi
