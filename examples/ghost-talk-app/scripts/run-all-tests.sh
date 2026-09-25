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
REPO_ROOT="$(cd "$ROOT/../.." && pwd)"
cd "$ROOT"

if command -v python3 >/dev/null 2>&1; then PYTHON=python3
elif command -v python >/dev/null 2>&1; then PYTHON=python
else echo "ERROR: Python 3 is required for architecture/test-matrix/CRAP checks." >&2; exit 1
fi
command -v cargo >/dev/null 2>&1 || { echo "ERROR: Rust/Cargo is required." >&2; exit 1; }

run() { printf '\n==> %s\n' "$*"; "$@"; }
require_lcov() { [[ -s "$1" ]] || { echo "ERROR: required LCOV report is missing or empty: $1" >&2; exit 1; }; }

run "$PYTHON" scripts/check-test-matrix.py
run "$PYTHON" scripts/check-architecture.py
run cargo fmt --all -- --check
run cargo fmt --manifest-path crates/ghost-wasm/Cargo.toml -- --check
# Prove native source integrity before optional WASM target provisioning.
run cargo check --workspace --all-targets
run cargo check --workspace --all-targets --all-features
run cargo check --manifest-path crates/ghost-wasm/Cargo.toml --all-targets
run cargo check --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features
command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup is required to prepare the wasm32 target." >&2; exit 1; }
bash "$REPO_ROOT/scripts/tooling/ensure-wasm-target.sh"
cargo llvm-cov --version >/dev/null 2>&1 || { echo "ERROR: cargo-llvm-cov is required for LCOV/CRAP (cargo install cargo-llvm-cov)." >&2; exit 1; }
command -v wasm-pack >/dev/null 2>&1 || { echo "ERROR: wasm-pack is required for browser-level WASM tests." >&2; exit 1; }
run cargo clippy --workspace --all-targets -- -D warnings
run cargo clippy --workspace --all-targets --all-features -- -D warnings

# Native/default and all-feature configurations are both mandatory so tests
# behind either cfg(feature) or cfg(not(feature)) cannot disappear.
run cargo test --workspace --all-targets --no-fail-fast
run cargo test --workspace --all-targets --all-features --no-fail-fast
run cargo test --workspace --doc --no-fail-fast
run cargo test --workspace --doc --all-features --no-fail-fast

# ghost-wasm is intentionally a standalone workspace and therefore needs its
# own native, doctest, Clippy and wasm32/browser surfaces.
run cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --all-targets -- -D warnings
run cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features -- -D warnings
run cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings
run cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings
run cargo test --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --no-fail-fast
run cargo test --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features --no-fail-fast
run cargo test --manifest-path crates/ghost-wasm/Cargo.toml --doc --no-fail-fast
run cargo test --manifest-path crates/ghost-wasm/Cargo.toml --doc --all-features --no-fail-fast

WASM_BROWSER="${GHOST_TALK_WASM_BROWSER:-firefox}"
case "$WASM_BROWSER" in firefox|chrome) ;; *) echo "ERROR: GHOST_TALK_WASM_BROWSER must be firefox or chrome." >&2; exit 1 ;; esac
run wasm-pack test --headless "--$WASM_BROWSER" crates/ghost-wasm

mkdir -p target/coverage "$REPO_ROOT/target/coverage"
ROOT_DEFAULT_LCOV="${GHOST_TALK_ROOT_LCOV_DEFAULT:-$REPO_ROOT/target/coverage/root-default.lcov}"
ROOT_ALL_FEATURES_LCOV="${GHOST_TALK_ROOT_LCOV_ALL_FEATURES:-$REPO_ROOT/target/coverage/root-all-features.lcov}"
if [[ -z "${GHOST_TALK_ROOT_LCOV_DEFAULT:-}" ]]; then
  run cargo llvm-cov --manifest-path "$REPO_ROOT/Cargo.toml" --workspace --all-targets --no-fail-fast --lcov --output-path "$ROOT_DEFAULT_LCOV"
fi
if [[ -z "${GHOST_TALK_ROOT_LCOV_ALL_FEATURES:-}" ]]; then
  run cargo llvm-cov --manifest-path "$REPO_ROOT/Cargo.toml" --workspace --all-targets --all-features --no-fail-fast --lcov --output-path "$ROOT_ALL_FEATURES_LCOV"
fi
require_lcov "$ROOT_DEFAULT_LCOV"
require_lcov "$ROOT_ALL_FEATURES_LCOV"

APP_DEFAULT_LCOV="$ROOT/target/coverage/workspace-default.lcov"
APP_ALL_FEATURES_LCOV="$ROOT/target/coverage/workspace-all-features.lcov"
WASM_DEFAULT_LCOV="$ROOT/target/coverage/ghost-wasm-default.lcov"
WASM_ALL_FEATURES_LCOV="$ROOT/target/coverage/ghost-wasm-all-features.lcov"
run cargo llvm-cov --workspace --all-targets --no-fail-fast --lcov --output-path "$APP_DEFAULT_LCOV"
run cargo llvm-cov --workspace --all-targets --all-features --no-fail-fast --lcov --output-path "$APP_ALL_FEATURES_LCOV"
run cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --no-fail-fast --lcov --output-path "$WASM_DEFAULT_LCOV"
run cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features --no-fail-fast --lcov --output-path "$WASM_ALL_FEATURES_LCOV"
require_lcov "$APP_DEFAULT_LCOV"
require_lcov "$APP_ALL_FEATURES_LCOV"
require_lcov "$WASM_DEFAULT_LCOV"
require_lcov "$WASM_ALL_FEATURES_LCOV"

run "$PYTHON" scripts/check-crap.py \
  "$ROOT_DEFAULT_LCOV" \
  "$ROOT_ALL_FEATURES_LCOV" \
  "$APP_DEFAULT_LCOV" \
  "$APP_ALL_FEATURES_LCOV" \
  "$WASM_DEFAULT_LCOV" \
  "$WASM_ALL_FEATURES_LCOV" \
  --max-crap 25 \
  --report target/coverage/crap-report.tsv \
  --unmeasured-report target/coverage/crap-unmeasured.tsv

printf '\nALL QUALITY GATES PASSED. Every configured application test surface, every LCOV feature surface, and CRAP completed.\n'
if [[ "${GHOST_TALK_NO_PAUSE:-0}" != "1" && -t 0 ]]; then read -r -p "Press Enter to close..." _ || true; fi
