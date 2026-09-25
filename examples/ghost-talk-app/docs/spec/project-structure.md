# Project structure

The complete reference application lives under `examples/ghost-talk-app/`. Every first-party application component is a Cargo crate under `crates/`.

```text
examples/ghost-talk-app/
├── Cargo.toml
├── crates/
│   ├── ghost-api/
│   ├── ghost-broadcast/
│   ├── ghost-chat/
│   ├── ghost-contacts/
│   ├── ghost-core/
│   ├── ghost-direct-chat/
│   ├── ghost-domain/
│   ├── ghost-games/
│   ├── ghost-history/
│   ├── ghost-hydra/
│   ├── ghost-indexer/
│   ├── ghost-kasia/
│   ├── ghost-kaspa/
│   ├── ghost-kaspa-voice/
│   ├── ghost-media/
│   ├── ghost-names/
│   ├── ghost-p2p/
│   ├── ghost-pong/
│   ├── ghost-presenter-room/
│   ├── ghost-protocol/
│   ├── ghost-realtime/
│   ├── ghost-radio-room/
│   ├── ghost-rooms/
│   ├── ghost-runtime/
│   ├── ghost-storage/
│   ├── ghost-talk-native/
│   └── ghost-wasm/
├── docs/
└── scripts/
```

The repository-level reusable SDK remains under `crates/ghost-talk` with public WASM bindings under `crates/ghost-talk-wasm`.

## Ownership layout

- `ghost-protocol` owns Ghost-native wire formats, including carrier-neutral GTR1/realtime bodies; `ghost-hydra` owns HYDRA/PQ integration.
- `ghost-realtime` owns carrier-independent replay/routing policy; `ghost-p2p` owns authenticated SID/HYDRA/P2P-PeerId binding and is the sole future p2p-net adapter boundary.
- `ghost-kasia` owns Kasia/KaChat compatibility and Kasia-indexer integration without leaking into Ghost-native domains.
- `ghost-indexer` owns the rebuildable Ghost-native public-profile/discovery projection.
- `ghost-media` owns content identity/manifests/archive carriers; `ghost-broadcast` owns live fan-out, RTMP/recording, and creator/podcast metadata.
- `ghost-rooms` remains the single Room/stage/radio membership/policy owner; Room voice reuses the repository voice SDK.
- `ghost-runtime` owns durable application aggregation and typed persistence deltas.
- `ghost-wasm` is presentation/controller composition; `ghost-talk-native` is the native platform/network/storage adapter.

## Source-tree policy

Production Rust is organized by semantic responsibility rather than numeric fragments or compound `_and_` modules. Every first-party production file must remain below 300 lines; crowded folders are regrouped by subsystem before becoming flat grab bags. Complex subsystems expose focused owners/facades; simple codecs, validators, value objects, hashes, and pure transformations stay direct.

`ghost-wasm/src/components/` is presentation-only: rendering, local form/modal state, and dispatch of typed application commands. Durable coordination lives in controllers/domain owners. Components do not construct persistence deltas directly or mutate durable profile aggregates.

Architecture tests additionally prohibit a second Room/stage/radio model, a second Room microphone stack, Kasia leakage into Ghost-native protocol/chat/indexer owners, generated/build content in source packages, stale path dependencies in committed lockfiles, and reintroduction of first-party `example` crate/package names.
