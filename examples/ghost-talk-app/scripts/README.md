# Ghost Talk reference-application scripts

Platform launchers are grouped by host target:

```text
scripts/
├── windows/       # self-setup Rust/Tauri build/run
├── linux/         # Devuan Rust/Tauri bootstrap/build/run
├── android/       # self-bootstrap Rust/Tauri Android build/run
├── ios/           # self-bootstrap Rust/Tauri iOS build/run + .command wrappers
├── mobile/        # shared generated-native-project configuration
├── wasm/          # Rust/WASM browser build/run via Trunk
├── architecture/     # focused source/ownership/runtime/integration guard modules
├── check-architecture.py  # thin architecture/SRP facade
├── check-test-matrix.py
├── check-crap.py
├── run-all-tests.cmd
└── run-all-tests.sh
```

## Mobile applications

Android and iOS use the same Rust/Yew frontend and Tauri command surface as desktop, so product UX and behavior do not fork by platform. Both mobile runners build/stage `ghost-wasm`, initialize the generated Tauri native project when absent, then apply `scripts/mobile/configure.py` to restore Ghost Talk privacy/permission settings deterministically.

On Windows, `android\build.cmd` and `android\run.cmd` automatically provision pinned Rust 1.98.0, Trunk 0.21.14, Tauri CLI 2.11.4, Java 17, Android command-line tools, API 36/build-tools 36.0.0, NDK 30.0.16248370, and all four Rust Android targets. Linux/macOS Android scripts provide the same command-line bootstrap. A device or configured emulator is still required for `android dev`.

On macOS, `ios/build.command` and `ios/run.command` provision Rust/Tauri/Trunk, the iOS Rust targets, and CocoaPods, then initialize/configure the generated Xcode project. Full Xcode is the one prerequisite that cannot be silently installed because Apple distributes it through the App Store and requires license/first-launch setup. Ghost Talk accepts Xcode 15+; Xcode 14.2 is not supported on macOS Sonoma. The iOS deployment target remains 15.0.

The repository has no Node/npm frontend toolchain. Browser code is Rust compiled to WebAssembly from `crates/ghost-wasm/`. Trunk writes intermediate frontend output only to the repository-level `target/build/frontend/`, and the release builder stages the complete directly-servable site under `target/dist/web/release/`. From `examples/ghost-talk-app`, `build-web-release.cmd` is the double-clickable Windows release entry point. The resulting standalone site uses the browser-native account runtime and does not require `window.__TAURI__` for profile/wallet/HYDRA bootstrap.

The root `scripts/run-all-tests` wrapper is the single authoritative full-suite entry point. It rejects test-filter arguments so a caller cannot accidentally turn the complete gate into a partial run. Before compilation it runs the test-matrix and architecture guards. It then runs root SDK formatting plus default/all-feature warnings-as-errors linting, default-feature and all-feature all-target tests, both doctest configurations, wasm32 Clippy, a real-browser `ghost-talk-wasm` SDK test/build surface, and both root LCOV feature configurations before invoking this reference application's complete runner. The application runner repeats the architecture/test-matrix guard, runs default/all-feature application linting, tests and doctests, the intentionally standalone `ghost-wasm` native/wasm32/browser surfaces, and the combined CRAP gate. The test-matrix guard inventories every first-party Cargo manifest, rejects disabled/ignored tests, and fails if any mandatory command or report disappears.

The complete coverage/CRAP pass consumes **six LCOV reports**: root SDK default features, root SDK all features, reference-application default features, reference-application all features, standalone `ghost-wasm` default features, and standalone `ghost-wasm` all features. Running both feature configurations prevents `cfg(feature = ...)` or `cfg(not(feature = ...))` production paths from silently disappearing from coverage. `check-crap.py` writes `target/coverage/crap-report.tsv` for every first-party production function and `target/coverage/crap-unmeasured.tsv` for functions that received no host LCOV lines. Ownership/security-critical host-executable functions must be measured and all measured functions must have CRAP <=25. Browser-only code is validated by mandatory real-browser WASM tests and remains explicitly visible as unmeasured rather than receiving invented host coverage.

Prerequisites are Rust/Cargo, Python 3, and Firefox or Chrome for the browser harness. The Windows runner provisions its isolated pinned Rust toolchain, `llvm-tools-preview`, `wasm32-unknown-unknown`, pinned `cargo-llvm-cov 0.8.7`, and pinned `wasm-pack 0.14.0` automatically when missing. Other runners require `cargo-llvm-cov` and `wasm-pack` to be installed. Missing tools or installation failures fail closed and are never treated as skipped gates.

`check-architecture.py` intentionally uses only the Python standard library. It protects repository-wide ownership boundaries, controller/UI separation, typed persistence deltas, call lifecycle ownership, peer identity binding, mailbox isolation, NodeSession and wallet ownership, schema duplication, the repository-wide `<300` production-file target (350 hard ceiling), `<300` first-party QA-script target, function/complexity limits, and removal of obsolete production paths before Cargo compilation begins. `check-test-matrix.py` protects the runner itself: default/all-feature all-target tests, integration tests, doctests, SDK/application browser tests, all six LCOV reports, and the combined CRAP gate must remain wired and ignored tests/`--lib` narrowing are forbidden.
