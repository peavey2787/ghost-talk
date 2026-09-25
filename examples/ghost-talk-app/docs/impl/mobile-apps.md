# Android and iOS applications

Ghost Talk mobile is one product surface, not a separate rewrite. Android and iOS are Tauri 2 shells around the same `ghost-wasm` Yew frontend and `ghost-talk-native` Rust command surface used by the desktop application. That keeps navigation, Rooms, chat/reactions, Mask behavior, KasKold compatibility, wallet operations, HYDRA messaging, media, and Studio behavior aligned across platforms.

## Supported baselines

- Android: API 26 minimum, API 36 build platform, Android Build Tools 36.0.0, NDK 30.0.16248370.
- iOS/iPadOS: iOS 15.0 minimum.
- macOS host for iOS: full Xcode 15 or newer. Xcode 14.2 is not a Sonoma-supported Xcode release; Sonoma hosts must use Xcode 15+.
- Rust: repository-pinned 1.98.0.
- Tauri CLI: 2.11.4.
- Trunk: 0.21.14.

The generated platform projects remain under `crates/ghost-talk-native/gen/`. `cargo tauri android init` and `cargo tauri ios init` create them on first use. `scripts/mobile/configure.py` then applies Ghost Talk's required privacy strings and Android permissions idempotently after generation.

## Android

Windows users can double-click `scripts/android/run.cmd` or `scripts/android/build.cmd`. The scripts bootstrap Java 17, the Android SDK command-line tools, API/build tools, NDK, Rust Android targets, Trunk, the Tauri CLI, and a WebAssembly-capable LLVM/Clang toolchain when missing. The shared Yew frontend includes Rusty-Kaspa secp256k1 support; its `secp256k1-sys` dependency compiles bundled C for `wasm32-unknown-unknown`, so the bootstrap validates Clang WebAssembly code generation and binds the matching `llvm-ar` only for that WASM target before Trunk runs. No global Android Studio installation is required for command-line builds. Tauri links the compiled Android Rust library into the generated Gradle project with a symbolic link, so build/run performs a real symbolic-link probe before compiling. If Windows denies unprivileged symlink creation, Ghost Talk opens one normal, visible Administrator PowerShell window through UAC and runs the exact Android build/run script there. The live Android output stays in that Administrator window while Project Repo Manager waits for the final exit code. Every elevated operation also writes a persistent transcript under `target/logs/android`; on failure the Administrator window stays open until acknowledged and Project Repo Manager prints the final 160 log lines automatically. Ghost Talk does not mutate Developer Mode, request another reboot, depend on gsudo, or use a hidden log/TTY bridge. If symlink creation still fails in the elevated process, the script reports a Windows policy/filesystem problem immediately. Release builds intentionally target `arm64-v8a` and `armeabi-v7a` only. Tauri otherwise builds all Android ABIs, and Rusty-Kaspa v2.0.1's `kaspa-hashes` build script panics before compilation on `x86_64-linux-android`. This is an upstream build-script limitation, not an Android SDK limitation. The run scripts therefore select a connected ARM Android device and reject x86/x86_64-only emulators immediately instead of spending several minutes on a build that cannot succeed. A connected ARM64/ARMv7 Android device is required to run the app with this Rusty-Kaspa pin. Finished Android packages are copied to the repository root at `target/android/release/` (or `target/android/debug/` when using `build-debug.cmd` / `--debug`), with a compatibility mirror under `target/dist/android/<profile>/`. Android build wrappers pass Tauri `--ci` so release packaging cannot stop on an interactive prompt after Gradle has already produced the APK/AAB, and the build prints every staged package path before exiting. Gradle build/cache directories created inside Tauri's generated `gen/android` project are removed after each build attempt so transient build output does not remain mixed into source directories.

Linux/macOS users can run `scripts/android/run.sh` or `scripts/android/build.sh`. `bootstrap.sh` installs/configures the same pinned toolchain where the host package manager permits it.

Android native permissions are limited to networking/LAN discovery, microphone, and camera. Camera and microphone hardware are marked optional so devices without either can still install Ghost Talk.

## iOS

On macOS, double-click `scripts/ios/run.command` or `scripts/ios/build.command`, or invoke the corresponding `.sh` file in Terminal. The bootstrap installs the pinned Rust/Tauri/Trunk toolchain, iOS Rust targets, and CocoaPods. Full Xcode is detected separately because Apple distributes it through the App Store and requires its own license/first-launch setup.

The iOS project receives explicit microphone, camera, photo-library, and local-network usage descriptions. The shared UI uses `viewport-fit=cover`, native safe-area insets, 44-point coarse-pointer touch targets, 16px mobile form controls to avoid focus zoom, and dynamic viewport height where the WebView supports it.

## Source of truth

Do not create Android-only or iOS-only copies of Ghost Talk screens. Product behavior belongs in the shared Rust/Yew application unless a platform genuinely requires native Swift/Kotlin integration. Native project files are treated as generated shells; mobile-specific permissions/configuration belong in `scripts/mobile/configure.py` and Tauri platform configuration so regeneration remains deterministic.


## Windows Android elevation

Checked installer/tool commands stream console output without returning it as PowerShell pipeline data. If Tauri's required `jniLibs` symlink cannot be created by the normal Project Repo Manager process, Ghost Talk opens one visible Administrator PowerShell window and runs the Android entry script there. The elevated operation records a persistent PowerShell transcript under `target/logs/android`; on failure the window remains open until acknowledged, and Project Repo Manager prints the final 160 log lines after the elevated process exits. No hidden stdin/TTY bridge is used.

## Kaspa hashing on mobile

Ghost Talk keeps Rusty-Kaspa v2.0.1 pinned to commit `cfafeb4c093fa37a303f1b9f19c58f986b870ce3`. Android and iOS still enable `kaspa-hashes/no-asm` so the Rust library selects its portable Keccak implementation. However, Rusty-Kaspa v2.0.1's `kaspa-hashes/build.rs` selects/compiles x86_64 assembly before that Rust feature can take effect and panics on the Android target OS. Ghost Talk therefore does not build Android x86/x86_64 ABIs with this pinned upstream release. Android release artifacts cover ARM64 and ARMv7 real devices; desktop builds retain the normal optimized assembly behavior. x86/x86_64 Android emulator support can be restored once the pinned Rusty-Kaspa build script itself supports those mobile OS targets.
