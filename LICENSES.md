# Licensing inventory

- Ghost Talk first-party SDK and reference-application code: GPL-2.0-or-later; see `LICENSE`.
- HYDRA is not vendored; the reference application consumes its required crates from one pinned upstream Git commit.
- Kaspa Portal is not vendored; the reference application consumes the published package dependency.
- DotK interoperability in `examples/ghost-talk-app/crates/ghost-names` was implemented from the reviewed Kaspire DotK v4 integration and pinned deployment data. Kaspire is Apache-2.0, Copyright 2026 Kaspire contributors; the corresponding license and NOTICE are retained under `licenses/kaspire/`. Ghost Talk does not use Kaspire branding or wallet code outside this interoperability implementation.
- KasKold interoperability vendors the user-supplied KasKold 2.0.0 compatibility crates under `external/kaskold/`. The SDK/protocol/shared-signer crates retain their MIT/Apache-2.0 files; vault-runtime/hot-wallet/offline-signer are covered by the retained upstream GPL license in `external/kaskold/LICENSE`.
- p2p-net (BUSL-1.1, same author) is consumed from one pinned Git revision by `ghost-realtime` (default `p2p-net` feature), the reference application's `ghost-p2p` browser transport, and the E2E harness. It is not vendored.
