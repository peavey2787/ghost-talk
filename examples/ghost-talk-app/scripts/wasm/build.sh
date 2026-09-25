#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../linux/_common.sh"
ghost_relaunch_in_terminal "$0"
trap ghost_pause EXIT
cd "$(ghost_app_root)"
repo_root="$(ghost_repo_root)"
rm -rf "$repo_root/target/build/frontend" "$repo_root/target/dist/web/release"
ghost_build_wasm_frontend
python3 scripts/artifacts/stage.py --platform web --profile release
echo "Ghost Talk Web release build complete."
echo "Final files: $repo_root/target/dist/web/release"
