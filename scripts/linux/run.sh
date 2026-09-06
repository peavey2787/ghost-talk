#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
cd "$(ghost_repo_root)/crates/ghost-app"
echo "Ghost Talk - Devuan Linux development run"
if [[ ! -d node_modules ]]; then
  if [[ -f package-lock.json ]]; then npm ci; else npm install; fi
fi
npm run tauri -- dev
