# Source inventory

- **Ghost Talk voice/media SDK:** `crates/ghost-talk/`.
- **Public SDK WebAssembly bindings:** `crates/ghost-talk-wasm/`.
- **Reference application:** `examples/ghost-talk-app/`, a nested Rust workspace.
- **Reference frontend:** `examples/ghost-talk-app/crates/ghost-wasm/`, compiled from Rust to WebAssembly.
- **Native host/platform adapter:** `examples/ghost-talk-app/crates/ghost-talk-native/`.
- **HYDRA/PQ integration:** `examples/ghost-talk-app/crates/ghost-hydra/`, using one exact upstream Git pin.
- **Kaspa transaction/network integration:** `examples/ghost-talk-app/crates/ghost-kaspa/`, using external Kaspa Portal.
- **Kasia/KaChat compatibility:** `examples/ghost-talk-app/crates/ghost-kasia/`, owning compatibility codecs/crypto/indexer history/contact mapping.
- **Ghost lightweight L1 index:** `examples/ghost-talk-app/crates/ghost-indexer/`, owning rebuildable verified Ghost-native discovery state.
- **Human-readable identity:** `examples/ghost-talk-app/crates/ghost-names/`, with `GhostNameResolver` as the KNS/`.k` normalization/verification/cache facade.
- **Shared content-addressed media:** `examples/ghost-talk-app/crates/ghost-media/`, owning content-addressed references and integrity verification.
- **Room authority:** `examples/ghost-talk-app/crates/ghost-rooms/`, owning Private/Public access, membership, and roles.
- **Repository quality scripts:** top-level `scripts/run-all-tests.sh` and `scripts/run-all-tests.cmd`; these gate the SDK workspace and invoke the reference-application quality runner.
- **Application platform/QA scripts:** `examples/ghost-talk-app/scripts/`.

There is no browser-language SDK directory, React/Vite tree, vendored HYDRA source, standalone QA tree, second application-example hierarchy, second Room model, or consumer-specific avatar content store.

Automated validation is driven from `scripts/run-all-tests.sh` / `.cmd`: architecture/SRP/ownership guardrails, committed-lock consistency, `cargo fmt --check`, Clippy with warnings denied, default/all-feature all-target tests and doctests, deterministic ownership regressions, SDK/application real-browser WASM tests, mandatory LCOV surfaces, and measured CRAP. Real independent-process funded/network/device/platform scenarios remain separate commercial release evidence documented in `docs/release/RELEASE.md` and `docs/validation/release-gates.md`.
