# Ghost Talk roadmap

## M0 Architecture — complete
Pinned upstream boundaries, protocol ownership, platform/license/security rules.

## M1 Cross-platform shell — implemented, host validation pending
Responsive React/Tauri app, native/WASM workspace, platform-separated double-click Devuan/Windows/Android/iOS/WASM build/run scripts; iOS minimum 15.0/Xcode 16.2 policy.

## M2 Accounts/vault — foundation implemented
Account registry, derivation presets/custom validation and encrypted vault are implemented. Manual KAS send/consolidation can optionally use the pinned KasSigner 2.0.0 Rust SDK over QR while Ghost Talk retains wallet policy and broadcast ownership. KasSigner-backed ID creation requires a one-time domain-separated Sign Message proof-of-control verified against the imported account kpub, so public kpub knowledge alone is not accepted as ownership. Remaining: production OS keystore/biometric adapters and funded hardware-signer HIL.

## M3 Direct chat — protocol/UI path implemented; funded E2E pending
HYDRA public stego feature `a8b4b317cef85edce9d0f0528bb53125ebf6333b` is pinned: deterministic compact-envelope cover is active; model-backed profiles remain capability-gated until a deterministic local model backend is configured. ChatEvent/GHST, HYDRA send/receive/stego facade, direct unlisted Kaspa KKTP v2 Discovery/Response consent bootstrap to any valid address/KNS, optional GTCD public profiles, unknown-request review/auto-ignore, restart-safe bootstrap acknowledgement/re-handshake, exact-SID replay isolation, and carrier reassembly are implemented. Private mailbox live delivery now consumes full `BlockAdded` notifications directly from the same single Kaspa Portal wRPC connection used for wallet RPCs and transaction submission; Ghost/KKTP payloads are processed from the included block without REST `GetBlocks` or output-address metadata filtering. REST address history with overlap/deduplication is restricted to archival carriers proven older than 48 hours. Current/recent carrier and wallet truth comes from the one Kaspa Portal wRPC connection; a future node-native gap-recovery cursor can extend reconnect catch-up without widening REST authority. Public-node candidates are attempted sequentially by the sole gateway, and ambiguous transaction submissions are never automatically repeated. Portal exact payload planning/mass/fee adjustment is implemented; successful Kaspa broadcast marks ordinary durable messages Delivered, selected threads auto-follow newly appended messages, chat rows open directly, and archive/leave lifecycle is implemented; remaining: funded multi-instance E2E across restart/reorg/failure cases and production notification/background behavior.

## M4 History/DAG/KNS — foundation implemented
KNS resolution, sync state, index DTO facade and BIP340 GTCD verification are implemented. Current GTCD records are learned from the same live Kaspa BlockAdded stream and cached locally; REST is not consulted for current discoverability. Discover maintains a best-effort cache of up to 100 verified public profiles observed by the client, each with a direct Start Chat action. Private address/KNS bootstrap does not require a public profile. Arbitrary global metadata search, complete historical enumeration, or a true popularity-ranked global top 100 still requires a dedicated chain index/directory service rather than full-DAG scanning from every client. Remaining: funded live node/reorg E2E and production discovery indexing.

## M5 Groups — room policy implemented; cryptographic lifecycle pending
Presenter/radio UX/domain rules exist; full hydra-group commits/snapshots/role transitions remain release-blocking.

## M6 Voice/radio — transport/security foundation implemented
Adaptive route policy, encrypted media frames, replay/jitter/spend controls and UI; 1:1 Auto now negotiates WebRTC with Kaspa signaling/fallback, while remaining work is device/audio HIL validation, group voice integration, and optional future direct mechanisms hidden behind Auto.

## M7 Pong — deterministic primitive implemented
Remaining: challenge UI/direct carrier/reconnect/result flow.

## M8 Platform hardening
Devuan, Windows, Android, Sonoma/Xcode 16.2/iOS and browser lifecycle/device testing.

## M9 Release hardening
The exhaustive runner now sequences first-party/upstream QA, E2E, mutation and all known fuzz targets with fuzzing last. Remaining release work is executing those host-dependent gates on real build hosts plus audit, interoperability, reproducible builds, signing and SBOM.
