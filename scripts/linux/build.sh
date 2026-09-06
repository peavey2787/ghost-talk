#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
ROOT="$(ghost_repo_root)"
cd "$ROOT"
echo "Ghost Talk - Devuan Linux build"
cd crates/ghost-app
if [[ -f package-lock.json ]]; then npm ci; else npm install; fi
npm run tauri -- build --bundles deb
echo "Ghost Talk Devuan build complete."
