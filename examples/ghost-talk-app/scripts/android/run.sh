#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/bootstrap.sh"
cd "$(mobile_native_root)"
device="$(ghost_android_arm_device)"
cargo tauri android dev "$device" "$@"
