# Ghost Talk reference application

This Rust workspace demonstrates the Ghost Talk SDK in a complete application. Application concerns such as HYDRA, Kaspa Portal, messaging, rooms, signaling, transport policy, persistence, native Tauri integration, and the Rust/WASM frontend live here; reusable voice/media behavior stays in the repository's `crates/ghost-talk` SDK crate.

All application components are Cargo crates under `crates/`. The native Tauri host is `crates/ghost-talk-native`, and the Rust/WASM frontend is `crates/ghost-wasm`.

Recipient entry accepts direct Kaspa addresses, KNS `.kas`, and verified DotK `.k` names. DotK directory rows are treated only as candidate-owner hints: Ghost Talk locally derives the pinned deed and directly queries the active Toccata-capable Kaspa wRPC node for a fresh live UTXO carrying the pinned registry covenant before using the address.

Run the complete reference-application quality gates with:

```bash
./scripts/run-all-tests.sh
```

On Windows:

```bat
scripts\run-all-tests.cmd
```

The gate runner accepts no test filters and checks architecture/SRP boundaries, formatting, warnings-as-errors Clippy, default-feature and all-feature all-target Cargo tests, both doctest configurations, the actual WASM target, rendered real-browser SDK/application WASM tests, six first-party LCOV reports, and combined measured CRAP<=25 with a separate unmeasured-function audit. The matrix guard inventories every first-party Cargo manifest, forbids ignored/disabled tests, and requires every coverage report to remain wired into CRAP. A deterministic A/B/C two-instance ownership regression runs in the workspace; real independent-process Kaspa/HYDRA scenarios remain a separate production-release gate. See `docs/validation/release-gates.md` for the exact commands.

Calling uses one application-scoped lifecycle owner and one call-domain state machine. A signed standalone Kaspa ring is received before any chat/session bootstrap; only an explicit acceptance creates or reuses the chat and establishes/reuses authenticated HYDRA/KKTP transport. This keeps calls started from an established chat and calls started by pasting a Kaspa address on the same signaling path.

Platform build/run helpers remain under this application's `scripts/` directory. Generated build output is rooted at the repository-level `target/` directory. Finished runnable/installable artifacts use stable repository-level target folders. Android release APK/AAB files are in `target/android/release/` (mirrored to `target/dist/android/release/` for compatibility), Windows release executables/installers are in `target/dist/windows/release/`, and the complete static Web release is in `target/dist/web/release/`. Use `scripts\windows\build.cmd` / `scripts\windows\build-debug.cmd` for Windows release/debug builds, `scripts\android\build.cmd` / `scripts\android\build-debug.cmd` for Android release/debug builds, and double-click `build-web-release.cmd` for a production Web bundle.

The standalone Web bundle does not require the Tauri JavaScript global for local account bootstrap. Profile metadata uses origin-local browser storage, wallet secrets remain password-sealed with the shared Rust vault format, and HYDRA state uses encrypted IndexedDB persistence. After an ID unlocks, Web establishes and retains a Kaspa wRPC connection for the active profile, resolving a public TLS endpoint when no explicit node is configured; Discover publication and external-signer transaction work reuse that browser-held node session. Native-host-only operations that have not yet been given a browser transport return an explicit standalone-Web capability error rather than attempting Tauri IPC.
