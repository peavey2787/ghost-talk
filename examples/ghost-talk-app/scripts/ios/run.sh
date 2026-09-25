#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/bootstrap.sh"
cd "$(mobile_native_root)"
cargo tauri ios dev "$@"
