#!/usr/bin/env bash
set -euo pipefail
[[ "$(uname -s)" == "Darwin" ]] || { echo "ERROR: Ghost Talk iOS builds require macOS." >&2; exit 2; }
source "$(dirname "$0")/../mobile/_common.sh"

resolve_xcode() {
  if [[ -d "/Applications/Xcode.app/Contents/Developer" ]]; then
    export DEVELOPER_DIR="/Applications/Xcode.app/Contents/Developer"
  fi
  if ! command -v xcodebuild >/dev/null 2>&1; then
    open "macappstore://itunes.apple.com/app/id497799835" >/dev/null 2>&1 || true
    echo "ERROR: Full Xcode is required. The App Store page has been opened when possible." >&2
    return 1
  fi
  local xcode_version macos_version xcode_major macos_major
  xcode_version="$(xcodebuild -version | awk '/^Xcode /{print $2; exit}')"
  macos_version="$(sw_vers -productVersion)"
  xcode_major="${xcode_version%%.*}"
  macos_major="${macos_version%%.*}"
  if (( xcode_major < 15 )); then
    if (( macos_major >= 14 )); then
      echo "ERROR: Xcode $xcode_version is not supported on macOS Sonoma $macos_version. Use Xcode 15 or newer." >&2
    else
      echo "ERROR: Ghost Talk mobile requires Xcode 15 or newer; found $xcode_version." >&2
    fi
    return 1
  fi
  if ! xcodebuild -checkFirstLaunchStatus >/dev/null 2>&1; then
    echo "Completing Xcode first-launch setup..."
    sudo xcodebuild -runFirstLaunch
  fi
  printf 'Xcode %s on macOS %s\n' "$xcode_version" "$macos_version"
}

ensure_homebrew() {
  command -v brew >/dev/null 2>&1 && return
  echo "Installing Homebrew for Ghost Talk iOS tooling..."
  NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  if [[ -x /opt/homebrew/bin/brew ]]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
  if [[ -x /usr/local/bin/brew ]]; then eval "$(/usr/local/bin/brew shellenv)"; fi
}

ensure_cocoapods() { command -v pod >/dev/null 2>&1 || brew install cocoapods; }
ensure_python() { command -v python3 >/dev/null 2>&1 || brew install python; }

resolve_xcode
if ! command -v python3 >/dev/null 2>&1 || ! command -v pod >/dev/null 2>&1; then ensure_homebrew; fi
ensure_python
mobile_ensure_rust
mobile_ensure_python
ensure_cocoapods
rustup target add --toolchain "$GHOST_RUST_VERSION" aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
export IPHONEOS_DEPLOYMENT_TARGET=15.0
mobile_build_frontend
native="$(mobile_native_root)"
(cd "$native" && [[ -d gen/apple ]] || cargo tauri ios init)
mobile_configure_native ios
printf 'Ghost Talk iOS toolchain and native project are ready.\n'
