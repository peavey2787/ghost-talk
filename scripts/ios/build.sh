#!/usr/bin/env bash
set -euo pipefail
if ! command -v xcodebuild >/dev/null; then echo "ERROR: Xcode is required." >&2; exit 2; fi
ver="$(xcodebuild -version | awk '/Xcode/{print $2}')"
[[ "$ver" == "16.2" ]] || { echo "ERROR: Ghost Talk requires Xcode 16.2; found $ver" >&2; exit 2; }
export IPHONEOS_DEPLOYMENT_TARGET=15.0
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT/crates/ghost-app"
if [[ -f package-lock.json ]]; then npm ci; else npm install; fi
[[ -d src-tauri/gen/apple ]] || npm run tauri -- ios init
npm run tauri -- ios build
