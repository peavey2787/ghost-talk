#!/usr/bin/env bash
set -euo pipefail
if (( $# != 0 )); then echo "ERROR: run-release-gates accepts no filters or arguments." >&2; exit 2; fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
if command -v python3 >/dev/null 2>&1; then PYTHON=python3
elif command -v python >/dev/null 2>&1; then PYTHON=python
else echo "ERROR: Python 3 is required." >&2; exit 1
fi
command -v cargo >/dev/null 2>&1 || { echo "ERROR: Cargo is required." >&2; exit 1; }
command -v cargo-deny >/dev/null 2>&1 || cargo deny --version >/dev/null 2>&1 || {
  echo "ERROR: cargo-deny is required (cargo install cargo-deny --locked)." >&2; exit 1;
}
command -v cargo-audit >/dev/null 2>&1 || cargo audit --version >/dev/null 2>&1 || {
  echo "ERROR: cargo-audit is required (cargo install cargo-audit --locked)." >&2; exit 1;
}

"$PYTHON" scripts/release/check-release-inputs.py --locks-only
GHOST_TALK_NO_PAUSE=1 GHOST_TALK_IN_TERMINAL=1 bash scripts/run-all-tests.sh

APP_MANIFEST="$ROOT/examples/ghost-talk-app/Cargo.toml"
WASM_MANIFEST="$ROOT/examples/ghost-talk-app/crates/ghost-wasm/Cargo.toml"
APP_LOCK="$ROOT/examples/ghost-talk-app/Cargo.lock"
WASM_LOCK="$ROOT/examples/ghost-talk-app/crates/ghost-wasm/Cargo.lock"
EVIDENCE="$ROOT/target/release-evidence"
rm -rf "$EVIDENCE"
mkdir -p "$EVIDENCE"

cargo deny check --manifest-path "$APP_MANIFEST"
cargo deny check --manifest-path "$WASM_MANIFEST"
cargo audit --file "$APP_LOCK"
cargo audit --file "$WASM_LOCK"
cargo metadata --manifest-path "$APP_MANIFEST" --locked --format-version 1 > "$EVIDENCE/app-metadata.json"
cargo metadata --manifest-path "$WASM_MANIFEST" --locked --format-version 1 > "$EVIDENCE/wasm-metadata.json"
"$PYTHON" scripts/release/generate-sbom.py \
  "$EVIDENCE/app-metadata.json" "$EVIDENCE/wasm-metadata.json" \
  --output "$EVIDENCE/ghost-talk.cdx.json"

export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1}"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$ROOT=/workspace"
A="$EVIDENCE/build-a"
B="$EVIDENCE/build-b"
for target in "$A" "$B"; do
  cargo build --manifest-path "$APP_MANIFEST" -p ghost-talk-native --release --locked --target-dir "$target/app"
  cargo build --manifest-path "$WASM_MANIFEST" --target wasm32-unknown-unknown --release --locked --target-dir "$target/wasm"
done
"$PYTHON" scripts/release/hash-artifacts.py \
  --pair ghost-talk-native "$A/app/release/ghost-talk-native" "$B/app/release/ghost-talk-native" \
  --pair ghost-wasm.wasm "$A/wasm/wasm32-unknown-unknown/release/ghost_wasm.wasm" "$B/wasm/wasm32-unknown-unknown/release/ghost_wasm.wasm" \
  --manifest "$EVIDENCE/SHA256SUMS"

cargo package -p ghost-talk --allow-dirty
SOURCE_ZIP="$EVIDENCE/ghost-talk-$(cat REVISION)-source.zip"
"$PYTHON" scripts/release/package-source.py --output "$SOURCE_ZIP"
"$PYTHON" scripts/release/check-source-package.py "$SOURCE_ZIP"
"$PYTHON" scripts/release/check-release-inputs.py
printf '\nCOMMERCIAL RELEASE GATES PASSED for %s\n' "$(cat REVISION)"
