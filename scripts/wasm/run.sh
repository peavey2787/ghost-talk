#!/usr/bin/env bash
set -euo pipefail
COMMON="$(dirname "$0")/../linux/_common.sh"
if [[ -f "$COMMON" ]]; then source "$COMMON"; ghost_relaunch_in_terminal "$0"; trap ghost_pause EXIT; fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
rustup target add wasm32-unknown-unknown
cargo build -p ghost-wasm --target wasm32-unknown-unknown
cd crates/ghost-app
if [[ -f package-lock.json ]]; then npm ci; else npm install; fi
npm run dev -- --host
