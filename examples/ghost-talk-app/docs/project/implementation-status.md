# Implementation status

Ghost Talk is split into a reusable transport-neutral voice SDK and a Rust reference application.

- The public SDK lives in `crates/ghost-talk`; public WebAssembly bindings live in `crates/ghost-talk-wasm`.
- The reference application lives in `examples/ghost-talk-app`; its frontend is Rust/WASM in `crates/ghost-wasm` and its native host is `crates/ghost-talk-native`.
- HYDRA and Kaspa Portal remain external dependencies rather than vendored source. Rusty-Kaspa dependencies are pinned to the stable Toccata v2.0.1 release commit.
- The first-party application package set contains no `example`-named crates. The architecture guard rejects reintroduction of that naming.

## Identity and profiles

`GhostNameResolver` is the single KNS/`.k` normalization, resolution, current-owner verification, cache-expiration, and invalidation facade. Signed public profiles support primary/verified names, raw Kaspa/HYDRA fallback identity, shared-media avatar references, capabilities, description/interests, and signature data. Profile avatar import/retrieval uses the common `ghost-media` store and verifies the signed content hash before display.

## Messaging interoperability

Ghost-native messaging remains HYDRA/PQ and on-chain authoritative. Direct Ghost chats support authenticated message reactions (Like, Love, Haha, Sad, Surprised, Dislike, Disgust, Angry, and Fear) as typed KKTP inner events; reactions update the existing target message rather than creating synthetic chat messages. One actor has at most one current reaction per message, and concurrent reaction/persistence updates are merged per actor. `ghost-kasia` is an isolated compatibility subsystem for supported Kasia/KaChat 1:1 text messaging. It owns the Kasia/KaChat codecs, secp256k1 ECIES framing, handshake data, Kasia-indexer HTTP access/history, and durable contact mapping. Current writes use the `kchat:1:*` family and compatible reads accept the documented `ciph_msg:1:*` root, including older `hs`/`msg` kind aliases on read only. The normal chat surface identifies Ghost PQ and Kasia separately.

Capability selection prefers Ghost PQ for a new peer when both are advertised. An established Ghost PQ conversation never silently falls back to Kasia; an unavailable established mode returns an explicit unavailable state instead. `ghost-chat` does not depend on `ghost-kasia`.

`ghost-indexer` owns the rebuildable verified current-state projection used for Ghost public-profile discovery. It indexes Ghost-native L1 descriptors only. Kasia history/discovery stays behind the Kasia indexer and is never absorbed by the Ghost indexer.

## Rooms, voice, and video

`ghost-rooms` is authoritative for Room membership, access, roles, moderation, invitations, revisions, Room messages, and their reactions. `RoomAccess` is exactly `Private` (invitation only, never listed) or `Public` (announced to the swarm, optionally on Kaspa, and listed in Discover → Rooms). Every participant may send text, react, and join Room voice and video; there are no mode presets, presenter/audience roles, or per-Room text/audio policies.

Room voice and video reuse the shared `BrowserVoiceSender`/receiver and `BrowserVideoSender` from the `ghost-talk-wasm` SDK over the encrypted realtime routes. A member sends to the Room owner, which relays to every other member over each member's authenticated session. Incoming media is checked against Room membership and the hop (sender or owner) before playback. Video is p2p-net only: frames are never carried on Kaspa, are dropped when p2p-net is not connected, and only the newest frame per peer waits in the realtime queue.

## Shared media

`ghost-media` owns content-addressed media references (`MediaReference`, `content_hash`, `verify_content`). Avatars are exposed only after hash verification.

## Quality and release status

The reference-application quality runner enforces architecture/SRP guardrails, package naming, module structure, lockfile/path-dependency consistency, source-size and complexity ceilings, formatting, warnings-as-errors Clippy, default/all-feature all-target tests and doctests, native/WASM browser tests, LCOV surfaces, and measured CRAP. The architecture suite additionally enforces the Ghost/Kasia index boundary, Room/stage/radio convergence, shared Room voice capture, live fan-out, RTMP/recording ownership, resumable archive requirements, and no-silent-downgrade behavior.

The communications/media expansion source implementation is complete for the owned paths above. Commercial qualification is still separate and fail-closed: funded independent-process Ghost↔Ghost and Ghost↔KaChat/Kasia interoperability, device/audio HIL, supported-platform hardening, background behavior, group cryptographic lifecycle, keystore/biometric validation, UI/platform parity, and release-signature evidence must pass against the exact release revision before it can be described as commercially qualified.
