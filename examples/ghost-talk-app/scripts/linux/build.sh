#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
ROOT="$(ghost_app_root)"
REPO_ROOT="$(ghost_repo_root)"
export CARGO_TARGET_DIR="$REPO_ROOT/target"
cd "$ROOT"
echo "Ghost Talk - Devuan Linux Rust/WASM build"
ghost_build_wasm_frontend
ghost_require_tauri_cli
cd "$ROOT/crates/ghost-talk-native"
cargo tauri build --bundles deb
python3 "$ROOT/scripts/artifacts/stage.py" --platform linux --profile release
echo "Ghost Talk Devuan build complete."
echo "Final files: $REPO_ROOT/target/dist/linux/release"
