"""Static contracts for the first-class Android/iOS application shells."""
from __future__ import annotations

import json
from pathlib import Path

from .common import APP_ROOT, REPO_ROOT, fail, text


def require(source: str, needle: str, label: str) -> None:
    if needle not in source:
        fail(f"mobile contract missing {label}: `{needle}`")


def require_file(path: Path) -> str:
    if not path.is_file():
        fail(f"mobile application file is missing: {path.relative_to(APP_ROOT)}")
        return ""
    return text(path)


def check_mobile_application_contracts() -> None:
    scripts = APP_ROOT / "scripts"
    native = APP_ROOT / "crates" / "ghost-talk-native"
    wasm = APP_ROOT / "crates" / "ghost-wasm"

    configure = require_file(scripts / "mobile" / "configure.py")
    for permission in (
        "android.permission.RECORD_AUDIO",
        "android.permission.CAMERA",
        "android.permission.CHANGE_WIFI_MULTICAST_STATE",
        "NSMicrophoneUsageDescription",
        "NSCameraUsageDescription",
        "NSLocalNetworkUsageDescription",
    ):
        require(configure, permission, "native mobile permission/privacy configuration")

    android_ps = require_file(scripts / "android" / "common.ps1")
    for token in (
        "1.98.0",
        "2.11.4",
        "0.21.14",
        "30.0.16248370",
        "aarch64-linux-android",
        "$GhostAndroidBuildTargets = @('aarch64', 'armv7')",
        "function Get-GhostArmAndroidDevice",
        "Invoke-GhostChecked 'cargo' @('tauri', 'android', 'init')",
        "function Invoke-GhostNativeCapture",
        "& $File @Arguments | Out-Host",
        "Invoke-GhostNativeCapture $Java @('-version')",
    ):
        require(android_ps, token, "Windows Android self-bootstrap")
    if "& $Java -version 2>&1" in android_ps:
        fail("Windows Android JDK probe must not merge native stderr under strict PowerShell mode")
    for token in (
        "function Set-GhostWasmCToolchain",
        "CC_wasm32_unknown_unknown",
        "AR_wasm32_unknown_unknown",
        "ensure-wasm-clang.ps1",
    ):
        require(android_ps, token, "Windows WASM C toolchain bootstrap")
    wasm_clang = require_file(REPO_ROOT / "scripts" / "tooling" / "ensure-wasm-clang.ps1")
    for token in (
        "LLVM.LLVM",
        "--target=wasm32-unknown-unknown",
        "llvm-ar.exe",
        "winget",
    ):
        require(wasm_clang, token, "WASM secp256k1 C compiler bootstrap")

    android_build_ps = require_file(scripts / "android" / "build.ps1")
    require(android_build_ps, "'android', 'build', '--ci', '--apk', '--aab', '--target'", "non-interactive ARM-only Android APK/AAB build")
    require(android_build_ps, "$GhostAndroidBuildTargets", "Android release target list")
    require(android_build_ps, "Stage-GhostBuildArtifacts 'android' $profile", "Android artifact staging")
    require(android_build_ps, "Remove-GhostAndroidPackageOutputs", "stale Android package cleanup before build")
    require(android_build_ps, "Assert-GhostAndroidArtifacts $profile", "post-build canonical Android APK/AAB verification")
    require(android_build_ps, "Ensure-GhostWindowsSymlinkSupport $PSCommandPath $profile", "explicit Android build-profile elevation forwarding")
    require(android_build_ps, "GHOST_TALK_ANDROID_BUILD_PROFILE", "Android debug profile process-state preservation")
    android_run_ps = require_file(scripts / "android" / "run.ps1")
    require(android_run_ps, "Get-GhostArmAndroidDevice", "ARM Android run target selection")
    android_build_sh = require_file(scripts / "android" / "build.sh")
    require(android_build_sh, "--ci --apk --aab --target aarch64 armv7", "POSIX non-interactive ARM-only Android release build")
    android_run_sh = require_file(scripts / "android" / "run.sh")
    require(android_run_sh, "ghost_android_arm_device", "POSIX ARM Android run target selection")
    for forbidden_target in ("i686-linux-android", "x86_64-linux-android"):
        if forbidden_target in android_ps:
            fail(f"Android bootstrap must not provision unsupported Rusty-Kaspa x86 target: {forbidden_target}")

    windows_tools = require_file(scripts / "windows" / "_ensure-tools.cmd")
    for token in ("ensure-wasm-clang.ps1", "CC_wasm32_unknown_unknown", "AR_wasm32_unknown_unknown"):
        require(windows_tools, token, "Windows Web/WASM C toolchain bootstrap")

    windows_host = require_file(scripts / "android" / "windows_host.ps1")
    for token in (
        "function Test-GhostWindowsSymlinkSupport",
        "function Test-GhostWindowsAdministrator",
        "function Invoke-GhostVisibleElevatedAndroidEntry",
        "function Get-GhostAndroidLogPath",
        "function Show-GhostAndroidFailureTail",
        "function Ensure-GhostWindowsSymlinkSupport",
        "[string]$BuildProfile = ''",
        "'-BuildProfile'",
        "Start-Process -FilePath $powershell -Verb RunAs -PassThru -Wait",
        "-WindowStyle Normal",
        "target\\logs\\android",
        "Get-Content -Path $LogPath -Tail 160",
        "Assert-GhostAndroidArtifacts $BuildProfile",
        "exit $exitCode",
    ):
        require(windows_host, token, "Windows Android visible elevation")
    elevated_entry = require_file(scripts / "android" / "elevated_entry.ps1")
    for token in ("Start-Transcript -Path $LogPath -Force", "[string]$BuildProfile = ''", "Unsupported Android build profile", "& $EntryScript -Debug", "Read-Host 'Build failed. Press Enter to close this Administrator window'", "Android build log saved to:"):
        require(elevated_entry, token, "persistent elevated Android diagnostics")

    run_entry = require_file(scripts / "android" / "run.ps1")
    require(run_entry, "Ensure-GhostWindowsSymlinkSupport $PSCommandPath", "pre-run Windows symlink elevation")
    if "AllowDevelopmentWithoutDevLicense" in windows_host or "Restart Windows once" in windows_host:
        fail("Windows Android bootstrap must elevate the operation instead of mutating Developer Mode or requesting a reboot")
    for forbidden in ("gsudo", "Write-GhostElevationBridge", "Show-GhostElevationOutput", "RedirectStandardOutput", "RedirectStandardError", "*>&1"):
        if forbidden.lower() in windows_host.lower():
            fail(f"Windows Android elevation must stay simple and visible; forbidden bridge token: {forbidden}")
    if "Ensure-GhostWindowsSymlinkSupport" in android_ps:
        fail("Windows Android symlink elevation belongs in build/run entry points, not shared environment bootstrap")

    for name in ("bootstrap.cmd", "build.cmd", "build-debug.cmd", "run.cmd", "bootstrap.sh", "build.sh", "run.sh"):
        require_file(scripts / "android" / name)

    artifacts = require_file(scripts / "artifacts" / "stage.py")
    for token in ("TARGET_ROOT / \"android\"", "TARGET_ROOT / \"dist\"", "android", "windows", "linux", "ios", "web", "ARTIFACTS.txt"):
        require(artifacts, token, "target/dist artifact staging")
    for token in ("_android_search_roots", "aarch64-linux-android", "armv7-linux-androideabi", "no matching APK/AAB was found"):
        require(artifacts, token, "robust Android release artifact discovery")

    ios = require_file(scripts / "ios" / "bootstrap.sh")
    for token in (
        "xcode_major < 15",
        "aarch64-apple-ios",
        "aarch64-apple-ios-sim",
        "brew install cocoapods",
        "cargo tauri ios init",
    ):
        require(ios, token, "iOS/Sonoma bootstrap")
    for name in ("bootstrap.command", "build.command", "run.command", "build.sh", "run.sh"):
        require_file(scripts / "ios" / name)

    index = require_file(wasm / "index.html")
    require(index, "viewport-fit=cover", "iOS/Android WebView viewport")
    css = require_file(wasm / "style.css")
    for token in ("safe-area-inset-bottom", "100dvh", "@media(pointer:coarse)", "font-size:16px"):
        require(css, token, "shared mobile-safe responsive UX")

    android_config_path = native / "tauri.android.conf.json"
    ios_config_path = native / "tauri.ios.conf.json"
    android_config = json.loads(require_file(android_config_path) or "{}")
    ios_config = json.loads(require_file(ios_config_path) or "{}")
    android_bundle = android_config.get("bundle", {}).get("android", {})
    ios_bundle = ios_config.get("bundle", {}).get("iOS", {})
    if android_bundle.get("minSdkVersion") != 26:
        fail("Android application minimum SDK must remain API 26")
    if ios_bundle.get("minimumSystemVersion") != "15.0":
        fail("iOS application minimum system version must remain 15.0")
    revision = int(text(REPO_ROOT / "REVISION").strip().removeprefix("r"))
    if android_bundle.get("versionCode") != revision:
        fail("Android versionCode must track the repository revision")
    if ios_bundle.get("bundleVersion") != str(revision):
        fail("iOS bundleVersion must track the repository revision")

    require_file(APP_ROOT / "docs" / "impl" / "mobile-apps.md")
