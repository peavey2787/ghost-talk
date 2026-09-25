#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
cd "$(ghost_app_root)"
echo "Ghost Talk - Devuan Linux Rust/WASM development run"
ghost_build_wasm_frontend
cd "$(ghost_app_root)/crates/ghost-talk-native"
ghost_require_tauri_cli
cargo tauri dev
