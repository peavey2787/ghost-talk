#!/usr/bin/env bash
# Ghost Talk two-instance end-to-end suite on Kaspa testnet-10.
#
# Builds the web release (one WASM module with p2p-net), checks the persistent
# dev wallet (prompting for faucet funding when needed), tops up the two
# instance wallets, starts a local p2p-net relay, and drives two isolated
# browser instances through direct chat, p2p-net, Kaspa-only, and Room flows.
set -euo pipefail
if (( $# != 0 )); then
  echo "ERROR: run-all-e2e accepts no filters; every scenario is mandatory." >&2
  exit 2
fi
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/examples/ghost-talk-app"
E2E="$APP/e2e"
OUT="$ROOT/target/e2e"
NETWORK="${GHOST_E2E_NETWORK:-testnet-10}"
mkdir -p "$OUT"

for tool in cargo trunk node npm python3; do
  command -v "$tool" >/dev/null 2>&1 || { echo "ERROR: $tool is required for the E2E suite." >&2; exit 1; }
done

run() { printf '\n==> %s\n' "$*"; "$@"; }

run bash "$ROOT/scripts/tooling/ensure-wasm-target.sh"
rm -rf "$ROOT/target/build/frontend"
(cd "$APP/crates/ghost-wasm" && run trunk build --release)
run cargo build --release --manifest-path "$E2E/harness/Cargo.toml"
HARNESS="$E2E/harness/target/release/ghost-e2e"

run "$HARNESS" ensure --network "$NETWORK" --min-kas "${GHOST_E2E_MIN_DEV_KAS:-25}"
run "$HARNESS" fund --network "$NETWORK" --min-kas 5 --topup-kas 10
"$HARNESS" status --network "$NETWORK" > "$OUT/wallets.json"

"$HARNESS" relay --network "$NETWORK" > "$OUT/relay.json" 2>"$OUT/relay.err" &
RELAY_PID=$!
trap 'kill "$RELAY_PID" 2>/dev/null || true' EXIT
for _ in $(seq 1 120); do
  grep -q peerId "$OUT/relay.json" 2>/dev/null && break
  sleep 1
done
grep -q peerId "$OUT/relay.json" || { echo "ERROR: local p2p-net relay did not start" >&2; cat "$OUT/relay.err" >&2; exit 1; }

cd "$E2E/playwright"
run npm ci --no-audit --no-fund
run npx playwright install chromium
GHOST_E2E_RELAY_FILE="$OUT/relay.json" GHOST_E2E_WALLETS_FILE="$OUT/wallets.json" run npx playwright test

printf '\nALL E2E SCENARIOS PASSED (report: %s)\n' "$OUT/report/index.html"
