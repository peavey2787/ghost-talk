#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
ROOT="$(ghost_app_root)"
cd "$ROOT"
echo "Ghost Talk - Devuan Linux Rust bootstrap"
if ! grep -qi devuan /etc/os-release 2>/dev/null; then echo "WARNING: Devuan is the supported Linux release-test target." >&2; fi
sudo apt-get update
sudo apt-get install -y build-essential curl pkg-config libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
command -v rustup >/dev/null 2>&1 || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source "$HOME/.cargo/env"
rustup toolchain install 1.98.0 --component rustfmt
ghost_ensure_wasm_target 1.98.0
cargo install trunk --version "$GHOST_TRUNK_VERSION" --locked
cargo install tauri-cli --version "$GHOST_TAURI_CLI_VERSION" --locked --force
echo "Ghost Talk Devuan Rust bootstrap complete."
