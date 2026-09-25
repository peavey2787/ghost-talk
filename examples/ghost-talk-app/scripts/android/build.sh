#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/bootstrap.sh"
trap mobile_cleanup_android_build_state EXIT
profile=release
for arg in "$@"; do [[ "$arg" == "--debug" ]] && profile=debug; done
cd "$(mobile_native_root)"
cargo tauri android build --ci --apk --aab --target aarch64 armv7 "$@"
mobile_stage_artifacts android "$profile"
printf '\nGhost Talk Android %s build complete.\nFinal files: %s\nCompatibility mirror: %s\n' \
  "$profile" "$(mobile_repo_root)/target/android/$profile" "$(mobile_repo_root)/target/dist/android/$profile"
