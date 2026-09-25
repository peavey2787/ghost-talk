#!/usr/bin/env bash
set -euo pipefail

GHOST_RUST_VERSION="1.98.0"
GHOST_TRUNK_VERSION="0.21.14"
GHOST_TAURI_CLI_VERSION="2.11.4"

mobile_script_dir() { cd "$(dirname "${BASH_SOURCE[0]}")" && pwd; }
mobile_app_root() { cd "$(mobile_script_dir)/../.." && pwd; }
mobile_repo_root() { cd "$(mobile_app_root)/../.." && pwd; }
mobile_native_root() { printf '%s/crates/ghost-talk-native\n' "$(mobile_app_root)"; }

mobile_ensure_command() {
  command -v "$1" >/dev/null 2>&1 || { echo "ERROR: required command is unavailable: $1" >&2; return 1; }
}

mobile_ensure_rust() {
  if ! command -v rustup >/dev/null 2>&1; then
    mobile_ensure_command curl
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  rustup toolchain install "$GHOST_RUST_VERSION" --profile minimal --component rustfmt --component clippy
  bash "$(mobile_repo_root)/scripts/tooling/ensure-wasm-target.sh" "$GHOST_RUST_VERSION"
  local trunk="" tauri=""
  trunk="$(trunk --version 2>/dev/null | awk '{print $2}' || true)"
  [[ "$trunk" == "$GHOST_TRUNK_VERSION" ]] || rustup run "$GHOST_RUST_VERSION" cargo install trunk --version "$GHOST_TRUNK_VERSION" --locked
  tauri="$(cargo tauri --version 2>/dev/null | awk '{print $2}' || true)"
  [[ "$tauri" == "$GHOST_TAURI_CLI_VERSION" ]] || rustup run "$GHOST_RUST_VERSION" cargo install tauri-cli --version "$GHOST_TAURI_CLI_VERSION" --locked --force
  export RUSTUP_TOOLCHAIN="$GHOST_RUST_VERSION"
}

mobile_ensure_python() {
  if command -v python3 >/dev/null 2>&1; then
    GHOST_PYTHON="$(command -v python3)"
  elif command -v python >/dev/null 2>&1; then
    GHOST_PYTHON="$(command -v python)"
  else
    echo "ERROR: Python 3 is required to configure generated mobile projects." >&2
    return 1
  fi
  export GHOST_PYTHON
}

mobile_build_frontend() {
  local app="$(mobile_app_root)"
  export CARGO_TARGET_DIR="$(mobile_repo_root)/target"
  mobile_remove_legacy_build_outputs
  (cd "$app/crates/ghost-wasm" && trunk build --release)
  [[ -f "$(mobile_repo_root)/target/build/frontend/index.html" ]] || {
    echo "ERROR: Ghost WASM frontend output is missing from target/build/frontend" >&2
    return 1
  }
}

mobile_stage_artifacts() {
  "$GHOST_PYTHON" "$(mobile_app_root)/scripts/artifacts/stage.py" --platform "$1" --profile "$2"
}

mobile_remove_legacy_build_outputs() {
  rm -rf \
    "$(mobile_app_root)/target" \
    "$(mobile_app_root)/crates/ghost-wasm/dist" \
    "$(mobile_app_root)/crates/ghost-talk-native/frontend"
}

mobile_cleanup_android_build_state() {
  local android="$(mobile_native_root)/gen/android"
  [[ -d "$android" ]] || return 0
  find "$android" -depth -type d \
    \( -name build -o -name .gradle -o -name .cxx -o -name .externalNativeBuild \) \
    -exec rm -rf {} + 2>/dev/null || true
  rm -rf "$android/app/src/main/jniLibs"
}

mobile_cleanup_ios_build_state() {
  rm -rf "$(mobile_native_root)/gen/apple/build"
}

mobile_configure_native() {
  "$GHOST_PYTHON" "$(mobile_script_dir)/configure.py" "$1" "$(mobile_native_root)"
}
