#!/usr/bin/env bash
# Re-resolve p2p-net to the newest commit of its default branch in every
# workspace lockfile, so the suites always build against the current main.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP="$ROOT/examples/ghost-talk-app"
for manifest in "$ROOT/Cargo.toml" "$APP/Cargo.toml" "$APP/crates/ghost-wasm/Cargo.toml" "$APP/e2e/harness/Cargo.toml"; do
  printf '\n==> cargo update -p p2p-net --manifest-path %s\n' "$manifest"
  cargo update -p p2p-net --manifest-path "$manifest"
done
