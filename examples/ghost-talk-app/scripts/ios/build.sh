#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/bootstrap.sh"
trap mobile_cleanup_ios_build_state EXIT
profile=release
for arg in "$@"; do [[ "$arg" == "--debug" ]] && profile=debug; done
cd "$(mobile_native_root)"
cargo tauri ios build "$@"
mobile_stage_artifacts ios "$profile"
printf '\nGhost Talk iOS %s build complete.\nFinal files: %s\n' "$profile" "$(mobile_repo_root)/target/dist/ios/$profile"
