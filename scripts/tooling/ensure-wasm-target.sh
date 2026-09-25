#!/usr/bin/env bash
set -euo pipefail

TARGET="wasm32-unknown-unknown"
TOOLCHAIN="${1:-}"

command -v rustup >/dev/null 2>&1 || {
  echo "ERROR: rustup is required to prepare the ${TARGET} target." >&2
  exit 1
}

if [[ -z "$TOOLCHAIN" ]]; then
  read -r TOOLCHAIN _ < <(rustup show active-toolchain 2>/dev/null || true)
fi
if [[ -z "$TOOLCHAIN" ]]; then
  echo "ERROR: rustup could not resolve the active Rust toolchain for this checkout." >&2
  exit 1
fi

probe_target() {
  local probe_dir probe_source probe_output status
  probe_dir="$(mktemp -d)"
  probe_source="${probe_dir}/probe.rs"
  probe_output="${probe_dir}/probe.rmeta"
  printf '#![no_std]\npub fn ghost_talk_wasm_target_probe() {}\n' >"$probe_source"
  if rustup run "$TOOLCHAIN" rustc \
    --target "$TARGET" \
    --crate-name ghost_talk_wasm_target_probe \
    --crate-type lib \
    --emit metadata \
    "$probe_source" \
    -o "$probe_output" >/dev/null 2>&1; then
    status=0
  else
    status=$?
  fi
  rm -rf "$probe_dir"
  return "$status"
}

if probe_target; then
  exit 0
fi

echo "Installing ${TARGET} for Rust ${TOOLCHAIN}..."
if ! rustup target add --toolchain "$TOOLCHAIN" "$TARGET"; then
  echo "ERROR: failed to install ${TARGET} for Rust ${TOOLCHAIN}." >&2
  exit 1
fi

if ! probe_target; then
  echo "ERROR: rustup reported success, but Rust ${TOOLCHAIN} still cannot compile for ${TARGET}." >&2
  echo "ERROR: run 'rustup target add --toolchain ${TOOLCHAIN} ${TARGET}' and 'rustup show' for diagnostics." >&2
  exit 1
fi
