#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/../mobile/_common.sh"

ANDROID_API="36"
ANDROID_BUILD_TOOLS="36.0.0"
ANDROID_NDK="30.0.16248370"
CMDLINE_TOOLS="15859902"
WIN_SHA="90ae805d20434428bffcb699c290860f19bb5f66a67e6b330067e3de801fb04a"
MAC_X64_SHA="c5a6378ab5cf7e0d5701921405115befff13e9ff7417fb588389338f8bd050f3"
MAC_ARM_SHA="835b62a26162b229b441d1f6d4680383815a270809eb33522c0d480fa5002c4e"
LINUX_SHA="4e4c464f145a7512b57d088ac6c278c03c9eea610886b35a5e0804e74eedf583"

java_major() { "$1" -version 2>&1 | awk -F'"' '/version/{split($2,v,"."); print v[1]; exit}'; }

ensure_java() {
  if [[ -n "${JAVA_HOME:-}" && -x "$JAVA_HOME/bin/java" && "$(java_major "$JAVA_HOME/bin/java")" -ge 17 ]]; then return; fi
  if [[ "$(uname -s)" == "Darwin" && -x "/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin/java" ]]; then
    export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"; return
  fi
  if [[ -x "/opt/android-studio/jbr/bin/java" ]]; then export JAVA_HOME="/opt/android-studio/jbr"; return; fi
  if command -v java >/dev/null 2>&1 && [[ "$(java_major "$(command -v java)")" -ge 17 ]]; then
    local java_bin
    java_bin="$(command -v java)"
    java_bin="$(readlink -f "$java_bin" 2>/dev/null || printf '%s' "$java_bin")"
    export JAVA_HOME="$(cd "$(dirname "$java_bin")/.." && pwd)"
    return
  fi
  if [[ "$(uname -s)" == "Darwin" ]]; then
    if ! command -v brew >/dev/null 2>&1; then
      NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
      if [[ -x /opt/homebrew/bin/brew ]]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
      if [[ -x /usr/local/bin/brew ]]; then eval "$(/usr/local/bin/brew shellenv)"; fi
    fi
    brew install openjdk@17
    export JAVA_HOME="$(brew --prefix openjdk@17)/libexec/openjdk.jdk/Contents/Home"
  else
    sudo apt-get update
    sudo apt-get install -y openjdk-17-jdk curl unzip
    export JAVA_HOME="$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")"
  fi
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi
}

ensure_android_sdk() {
  export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
  export ANDROID_SDK_ROOT="$ANDROID_HOME"
  local manager="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
  if [[ ! -x "$manager" ]]; then
    mobile_ensure_command curl
    mobile_ensure_command unzip
    local os arch package expected
    os="$(uname -s)"; arch="$(uname -m)"
    if [[ "$os" == "Darwin" && "$arch" == "arm64" ]]; then package="commandlinetools-mac_arm64-${CMDLINE_TOOLS}_latest.zip"; expected="$MAC_ARM_SHA"
    elif [[ "$os" == "Darwin" ]]; then package="commandlinetools-mac_x86_64-${CMDLINE_TOOLS}_latest.zip"; expected="$MAC_X64_SHA"
    elif [[ "$os" == "Linux" ]]; then package="commandlinetools-linux-${CMDLINE_TOOLS}_latest.zip"; expected="$LINUX_SHA"
    else echo "ERROR: unsupported Android bootstrap host: $os/$arch" >&2; return 1; fi
    local zip temp actual
    zip="${TMPDIR:-/tmp}/$package"; temp="${TMPDIR:-/tmp}/ghost-talk-android-cli"
    curl -fL "https://dl.google.com/android/repository/$package" -o "$zip"
    actual="$(sha256_file "$zip")"
    [[ "$actual" == "$expected" ]] || { echo "ERROR: Android command-line tools checksum mismatch: $actual" >&2; return 1; }
    rm -rf "$temp" "$ANDROID_HOME/cmdline-tools/latest"
    mkdir -p "$temp" "$ANDROID_HOME/cmdline-tools/latest"
    unzip -q "$zip" -d "$temp"
    cp -a "$temp/cmdline-tools/." "$ANDROID_HOME/cmdline-tools/latest/"
  fi
  set +o pipefail
  yes | "$manager" --licenses >/dev/null || true
  set -o pipefail
  "$manager" --install platform-tools "platforms;android-$ANDROID_API" "build-tools;$ANDROID_BUILD_TOOLS" "ndk;$ANDROID_NDK"
  export NDK_HOME="$ANDROID_HOME/ndk/$ANDROID_NDK"
  export PATH="$ANDROID_HOME/platform-tools:$PATH"
}

if [[ "$(uname -s)" == "Linux" ]] && { ! command -v curl >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; }; then
  sudo apt-get update
  sudo apt-get install -y build-essential curl unzip python3
elif [[ "$(uname -s)" == "Darwin" ]] && ! command -v python3 >/dev/null 2>&1; then
  if ! command -v brew >/dev/null 2>&1; then
    NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    if [[ -x /opt/homebrew/bin/brew ]]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
    if [[ -x /usr/local/bin/brew ]]; then eval "$(/usr/local/bin/brew shellenv)"; fi
  fi
  brew install python
fi
mobile_ensure_rust
mobile_ensure_python
ensure_java
ensure_android_sdk
rustup target add --toolchain "$GHOST_RUST_VERSION" aarch64-linux-android armv7-linux-androideabi
mobile_build_frontend
native="$(mobile_native_root)"
(cd "$native" && [[ -d gen/android ]] || cargo tauri android init)
mobile_configure_native android
printf 'Ghost Talk Android toolchain and native project are ready.\n'


ghost_android_arm_device() {
  local serial abi
  while read -r serial state; do
    [[ "$state" == "device" ]] || continue
    abi="$(adb -s "$serial" shell getprop ro.product.cpu.abi 2>/dev/null | tr -d '\r\n')"
    if [[ "$abi" == "arm64-v8a" || "$abi" == "armeabi-v7a" ]]; then
      printf '%s\n' "$serial"
      return 0
    fi
  done < <(adb devices | tail -n +2)
  if adb devices | tail -n +2 | grep -qE '[[:space:]]device$'; then
    echo 'ERROR: connected Android targets are x86/x86_64 only. Rusty-Kaspa v2.0.1 cannot build that Android ABI; connect an ARM64/ARMv7 Android device.' >&2
  else
    echo 'ERROR: no Android device is connected. Connect an ARM64/ARMv7 Android device.' >&2
  fi
  return 1
}
