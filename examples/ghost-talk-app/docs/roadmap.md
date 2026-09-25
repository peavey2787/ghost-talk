# Ghost Talk roadmap

## M0 Architecture — complete
Pinned upstream boundaries, protocol ownership, platform/license/security rules.

## M1 Cross-platform shell — implemented, host validation pending
Rust/Tauri application with a Rust/WASM frontend, platform-separated Devuan/Windows/Android/iOS/WASM build/run scripts, and iOS minimum 15.0/Xcode 16.2 policy.

## M2 Accounts/vault — foundation implemented
Account registry, derivation presets/custom validation and encrypted vault are implemented. Ghost Talk owns local Kaspa transaction signing from the unlocked software wallet. Remaining: production OS keystore/biometric adapters and funded wallet/platform validation.

## M3 Direct chat — protocol/UI path implemented; funded E2E pending
HYDRA public stego feature `a8b4b317cef85edce9d0f0528bb53125ebf6333b` is pinned: deterministic compact-envelope cover is active; model-backed profiles remain capability-gated until a deterministic local model backend is configured. ChatEvent/GHST, HYDRA send/receive/stego facade, direct unlisted Kaspa KKTP v2 Discovery/Response consent bootstrap to any valid address/KNS/dot.k, optional GTCD public profiles, unknown-request review/auto-ignore, restart-safe bootstrap acknowledgement/re-handshake, exact-SID replay isolation, and carrier reassembly are implemented. Private mailbox live delivery consumes full `BlockAdded` notifications directly from the Kaspa Portal wRPC connection used for transaction planning/submission; Ghost/KKTP payloads are processed from the included block without REST `GetBlocks` or output-address metadata filtering. There is no scheduled REST mailbox scan: current carrier truth comes from the live Kaspa `BlockAdded` stream. REST address history is used only by the explicit/import-time transaction-history operation, never as a live carrier or wallet-state source. Wallet live state is separate and node-native: address-scoped `UtxosChanged` notifications maintain the UTXO set/balance and `VirtualDaaScoreChanged` maintains DAA state, with one UTXO baseline only when the subscription starts or reconnects. A future node-native gap-recovery cursor can extend carrier reconnect catch-up without widening REST authority. Public-node candidates are attempted sequentially by the sole gateway, and ambiguous transaction submissions are never automatically repeated. Portal exact payload planning/mass/fee adjustment is implemented; successful Kaspa broadcast marks ordinary durable messages Delivered, selected threads auto-follow newly appended messages, chat rows open directly, and archive/leave lifecycle is implemented; remaining: funded multi-instance E2E across restart/reorg/failure cases and production notification/background behavior.

## M4 History/DAG/KNS — implementation complete; production qualification pending
KNS resolution, verified DotK resolution, sync state, BIP340 GTCD verification, and the dedicated `ghost-indexer` owner are implemented. Current GTCD records are learned from the live Kaspa BlockAdded stream, verified, and projected through a disposable/rebuildable Ghost-native index; REST is not authority for current Ghost protocol state. Discover maintains a bounded best-effort set of verified profiles observed by the client with direct Start Chat actions. Private address/KNS/dot.k bootstrap does not require a public profile. Global metadata search/popularity ranking still requires a separately deployed Ghost-native indexing service if that product feature is desired; any such service must remain reconstructible from L1. Remaining release evidence: funded live-node/reorg/failure E2E.

## M5 Groups — Room/stage/radio convergence implemented; group cryptographic lifecycle pending
Private / InviteOnly / Unlisted / Public Room access, authoritative membership/roles/moderation, Private Group / Community Voice / Stage / Radio policy presets, and Room voice authorization are implemented on the one Room model. Full hydra-group commits/snapshots/role-transition qualification remains a separate release requirement.

## M6 Voice/radio — shared voice/broadcast implementation complete; HIL pending
Application-owned adaptive route policy and HYDRA/GTR1/p2p-net/Kaspa composition reuse the root Ghost Talk voice SDK. Room voice reuses the same BrowserVoiceSender/receiver and encrypted realtime routes; the same encoded voice units can feed isolated RTMP/RTMPS and local-recording sinks through the shared broadcast pipeline. Remaining release evidence is device/audio HIL and supported-platform lifecycle validation, not another group-voice implementation.

## M7 Pong — deterministic primitive implemented
Remaining: challenge UI/direct carrier/reconnect/result flow.

## M8 Platform hardening
Devuan, Windows, Android, Sonoma/Xcode 16.2/iOS and browser lifecycle/device testing.

## M9 Release hardening — automated gate implemented; external qualification pending
The mandatory repository runner covers native/default/all-feature compilation, warnings-as-errors Clippy, tests, doctests, real-browser WASM tests, six LCOV surfaces, and a combined CRAP<=25 gate. `scripts/release/run-release-gates.*` adds committed-lockfile enforcement, dependency policy/advisory auditing, locked release builds, deterministic CycloneDX SBOM generation, two-build reproducibility comparison, artifact hashing, packaging, and fail-closed external qualification evidence. Commercial qualification remains blocked until the exact release revision has passing funded multi-instance/restart/reorg/failure E2E, production keystore/biometric validation, group cryptographic lifecycle validation, device/audio HIL, supported-platform hardening, interoperability, production background/notification behavior, UI/platform parity, and signed-release evidence. See `docs/release/RELEASE.md`.
## M10 Communications/media interoperability — source implementation complete; external E2E pending
Unified KNS/.k identity/profile names, signed content-addressed avatars, `ghost-kasia` 1:1 Kasia/KaChat compatibility, explicit Ghost-PQ/Kasia protocol selection with no silent downgrade, shared `ghost-media`, Room/stage/radio convergence, shared live fan-out, RTMP/RTMPS, local recording, resumable/cost-bounded permanent Kaspa archival, podcast publishing, creator/station/show profiles, and security-state UX are implemented. Architecture tests enforce the hard Ghost/Kasia index boundary and shared-owner rules. Remaining release evidence includes independently built Ghost↔KaChat/Kasia interoperability, device/audio HIL, funded permanent-archive validation, background behavior, and supported-platform/UI qualification.

