#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
ROOT="$(ghost_repo_root)"
cd "$ROOT"
echo "Ghost Talk - Devuan Linux bootstrap"
if ! grep -qi devuan /etc/os-release 2>/dev/null; then echo "WARNING: Devuan is the supported Linux release-test target." >&2; fi
sudo apt-get update
sudo apt-get install -y build-essential curl pkg-config libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf nodejs npm python3
command -v rustup >/dev/null 2>&1 || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source "$HOME/.cargo/env"
rustup toolchain install 1.95.0 --component clippy,rustfmt
rustup target add wasm32-unknown-unknown
cargo install cargo-fuzz --locked
cd crates/ghost-app
if [[ -f package-lock.json ]]; then npm ci; else npm install; fi
echo "Ghost Talk Devuan bootstrap complete."
