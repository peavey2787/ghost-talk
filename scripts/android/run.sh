#!/usr/bin/env bash
set -euo pipefail
COMMON="$(dirname "$0")/../linux/_common.sh"
if [[ -f "$COMMON" ]]; then source "$COMMON"; ghost_relaunch_in_terminal "$0"; trap ghost_pause EXIT; fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT/crates/ghost-app"
if [[ ! -d node_modules ]]; then if [[ -f package-lock.json ]]; then npm ci; else npm install; fi; fi
[[ -d src-tauri/gen/android ]] || npm run tauri -- android init
npm run tauri -- android dev
