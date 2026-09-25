#!/usr/bin/env bash
set -euo pipefail

GHOST_TRUNK_VERSION="0.21.14"
GHOST_TAURI_CLI_VERSION="2.11.4"

ghost_app_root() { cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd; }
ghost_repo_root() { cd "$(ghost_app_root)/../.." && pwd; }
ghost_ensure_wasm_target() { bash "$(ghost_repo_root)/scripts/tooling/ensure-wasm-target.sh" "${1:-}"; }

ghost_relaunch_in_terminal() {
  local script="$1"; shift || true
  if [[ "${GHOST_TALK_IN_TERMINAL:-0}" == "1" || -t 1 ]]; then return 0; fi
  local resolved
  resolved="$(readlink -f "$script" 2>/dev/null || realpath "$script" 2>/dev/null || printf '%s' "$script")"
  for terminal in x-terminal-emulator xfce4-terminal gnome-terminal mate-terminal konsole lxterminal xterm; do
    command -v "$terminal" >/dev/null 2>&1 || continue
    case "$terminal" in
      xfce4-terminal) "$terminal" --command="env GHOST_TALK_IN_TERMINAL=1 bash '$resolved'" ;;
      gnome-terminal|mate-terminal) "$terminal" -- env GHOST_TALK_IN_TERMINAL=1 bash "$resolved" ;;
      konsole|lxterminal|xterm|x-terminal-emulator) "$terminal" -e env GHOST_TALK_IN_TERMINAL=1 bash "$resolved" ;;
    esac
    exit 0
  done
}

ghost_pause() {
  if [[ "${GHOST_TALK_NO_PAUSE:-0}" != "1" && -t 0 ]]; then
    printf '\nPress Enter to close...'; read -r _ || true
  fi
}

ghost_require_rust_web_tools() {
  command -v cargo >/dev/null 2>&1 || { echo "ERROR: Rust/Cargo is required." >&2; return 1; }
  command -v rustup >/dev/null 2>&1 || { echo "ERROR: rustup is required." >&2; return 1; }
  command -v trunk >/dev/null 2>&1 || { echo "ERROR: trunk ${GHOST_TRUNK_VERSION} is required; run the platform bootstrap script." >&2; return 1; }
}

ghost_require_tauri_cli() {
  command -v cargo-tauri >/dev/null 2>&1 || { echo "ERROR: tauri-cli ${GHOST_TAURI_CLI_VERSION} is required; run the platform bootstrap script." >&2; return 1; }
}

ghost_stage_wasm_frontend() {
  local frontend="$(ghost_repo_root)/target/build/frontend"
  [[ -f "$frontend/index.html" ]] || { echo "ERROR: Ghost WASM frontend output is missing: $frontend" >&2; return 1; }
}

ghost_build_wasm_frontend() {
  export CARGO_TARGET_DIR="$(ghost_repo_root)/target"
  rm -rf "$(ghost_app_root)/target" "$(ghost_app_root)/crates/ghost-wasm/dist" "$(ghost_app_root)/crates/ghost-talk-native/frontend"
  ghost_require_rust_web_tools
  ghost_ensure_wasm_target
  (cd "$(ghost_app_root)/crates/ghost-wasm" && trunk build --release)
  ghost_stage_wasm_frontend
}
