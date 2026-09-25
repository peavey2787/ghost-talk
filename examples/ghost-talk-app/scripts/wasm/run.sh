#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../linux/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
cd "$(ghost_app_root)"
export CARGO_TARGET_DIR="$(ghost_repo_root)/target"
ghost_require_rust_web_tools
ghost_ensure_wasm_target
(cd "$(ghost_app_root)/crates/ghost-wasm" && trunk serve --address 0.0.0.0)
