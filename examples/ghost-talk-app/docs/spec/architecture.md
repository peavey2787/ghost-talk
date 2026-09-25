# Architecture

Ghost Talk uses typed domain owners rather than a shared mutable application snapshot as its runtime coordination mechanism.

```text
Rust/WASM presentation                         Tauri native host
        |                                             |
        v                                             v
controllers / view state                       native adapters
        |                                             |
        +---------------- typed commands/events ------+
                              |
                              v
         +---------------------------------------------+
         | canonical domain/service boundaries        |
         |                                             |
         | ContactService     ChatService              |
         | CallManager        RoomService              |
         | WalletStateService Mailbox dispositions    |
         +---------------------------------------------+
                 |              |             |
                 v              v             v
             ghost-hydra    ghost-kaspa   persistence
```

`crates/ghost-talk` remains the reusable voice/media SDK. The reference application owns composition around complete `VoiceChunk` values. HYDRA implementation details stay below the HYDRA boundary and Kaspa Portal details stay below the Kaspa boundary. Kaspa remains the durable/offline carrier and authenticated signaling/fallback layer; direct realtime policy is application/runtime behavior rather than part of the root voice SDK.

The frontend is Rust compiled to WebAssembly. The native Tauri host is Rust. Native/WASM serialized contracts use canonical DTO definitions from `ghost-api` rather than parallel copies on each side.

## Mutable-state ownership

Every mutable domain has one write boundary:

| State | Authoritative writer | Durable |
| --- | --- | --- |
| authenticated contact bindings | `ContactService` | yes |
| chat threads/messages/reactions | `ChatService` | yes |
| room state/messages/reactions | `RoomService` / `RoomTombstoneService` | yes |
| Kasia contact routing | `KasiaContactMap` through typed profile deltas | yes |
| creator/station/show/episode catalog | `BroadcastCatalog` through typed profile deltas | yes |
| active calls | `ghost-domain::CallManager` | no |
| wallet state in the WASM application | `WalletStateService` | yes |
| mailbox frame/envelope lifecycle | `MailboxService` + mailbox pipeline using `PacketDisposition` | cursor/dedup only |
| HYDRA session/runtime state | HYDRA runtime/facade + `HydraSessionManager` durable projection | selectively |
| native profile/storage lifetime | native storage root + HYDRA profile lease | process lifetime |

The persisted `Profile` remains a storage aggregate for durable application data, but production subsystems do not use whole-profile replacement as an asynchronous conflict-resolution mechanism. `ProfilePatch` records domain deltas, and reconciliation applies only the changed domain to the latest durable profile. Contacts, chats, rooms, and wallet mutation are routed through their owners; the reconciliation module is the only deliberate durable-collection replacement boundary.


## Application interaction graph

Presentation does not coordinate native, network, or persistence work directly. The supported command paths are:

```text
ChatComponent
  -> ChatController
  -> ChatService
       -> ContactService (immutable peer resolution)
       -> HydraSessionManager
       -> transport adapter
       -> typed persistence delta

CallComponent
  -> CallController
  -> CallManager
       -> media adapter
       -> signaling adapter

KasiaPanel / selected-chat Advanced
  -> KasiaController
  -> ghost-kasia + native Kaspa submission/indexer adapters
       -> typed KasiaContactMapping persistence delta

Studio / Room voice
  -> Studio/Room controllers
  -> ghost-rooms + ghost-broadcast + ghost-media
       -> typed creator/station/show/episode deltas
       -> native broadcast/archive adapters

MailboxService
  -> typed envelope result / ApplicationEvent
  -> ApplicationRouter
       -> ContactService
       -> ChatService / HydraSessionManager
       -> RoomService
       -> CallManager
       -> WalletStateService
```

A domain may return a transition/result that causes another owner to receive a command, but it never receives mutable access to another domain's store. Controllers are orchestration-only and own no durable state.

## Authenticated peer identity

`ghost-domain::PeerBinding` is the canonical authenticated peer identity: a Kaspa address plus HYDRA identity. `ContactService` owns exact saved-contact resolution. A one-sided address or HYDRA match is not sufficient to inherit a saved contact identity. Unknown inbound peers are presented by their authenticated Kaspa address until intentionally associated with a saved contact.

### Name-service boundary

`ghost-names::GhostNameResolver` is the single human-readable-name facade. It owns normalization, KNS/.k dispatch, bounded caching, expiration, explicit invalidation, and verification that the freshly resolved owner matches the claimed Kaspa identity. A signed public profile may select one `ProfileName` as its primary name and carry `VerifiedName` observations, but those observations are display/cache evidence only: the resolver remains authoritative for current ownership. Raw Kaspa/HYDRA identity continues to work when no human-readable name is available.

KNS and DotK remain address-resolution adapters, not identity owners. KNS preserves the existing `.kas` lookup path. DotK is intentionally stricter: an explicit `.k` input bypasses cached contact/public aliases, the directory is only an untrusted candidate-owner index, the pinned v4 deed is derived locally, and a fresh covenant-bound live deed proof through the shared NodeSession-selected Toccata wRPC endpoint must succeed before an address is returned. See `docs/spec/dotk.md`. After resolution, GTCD/HYDRA processing is identical to a direct Kaspa-address bootstrap.

## Shared media ownership

`ghost-media` owns content-addressed media primitives shared by profiles, broadcast recordings, podcasts, and permanent archives. `MediaReference` identifies content by cryptographic hash, size, content type, and location; `GhostMediaManifest` owns canonical signing bytes, chunk metadata, signature metadata, and strict validation. Local media and archive chunks are exposed only after size/hash verification. Profiles, creator/station artwork, recordings, and episodes therefore reference common media rather than maintaining consumer-specific stores.

Permanent Kaspa archival extends the same owner with independently verifiable archive carriers and exact transaction locators. The native archive adapter persists per-media progress, records an in-flight intent before each submission, reconciles an uncertain submission from Kaspa history before any retry, enforces a user-confirmed current cost bound, stores only verified chunks in its persistent cache, and verifies the reconstructed root content hash.

## Room access, voice, and broadcast policy

`ghost-rooms` is the only Room membership/policy owner. `RoomAccess` explicitly models `Private`, `InviteOnly`, `Unlisted`, and `Public`; room membership, bans, ownership, revisions, moderation, invitations, roles, text policy, and audio policy remain authoritative. Private groups, community voice, stages, and radio are policy presets over the same Room model; no separate channel/stage/radio room store exists.

Room voice reuses the existing `ghost-talk` browser voice sender/receiver and encrypted realtime routes. `Off`, `Interactive`, and `PresentersOnly` are checked against Room roles before capture, relay, and playback. The same encoded `VoiceChunk` input can be forwarded to `ghost-broadcast::BroadcastPipeline`; broadcast sinks do not own another microphone or browser capture stack.

`ghost-broadcast` owns live fan-out, sink failure isolation, RTMP/RTMPS adaptation, local recording finalization, and creator/station/show/episode metadata. RTMP stream keys are zeroized/redacted secret material. Local recordings reuse the encoded Ghost voice units and become content-addressed `MediaReference`s that podcast publication can reference directly.

## Ghost/Kasia protocol and indexing boundary

`ghost-kasia` is the isolated Kasia/KaChat interoperability owner. It owns Kasia wire parsing/encoding, secp256k1 ECIES framing, handshake data, transaction payload construction, Kasia-indexer access/history, and durable contact mapping. `ghost-protocol`, `ghost-hydra`, and `ghost-chat` do not depend on it. Presentation can expose an explicit Kasia panel alongside a Ghost chat without moving Kasia protocol types into the Ghost conversation domain.

Current Kasia writes use the `kchat:1:*` family and compatible reads accept the documented `ciph_msg:1:*` root, including older `hs`/`msg` kind aliases on read only. Contextual messages use the self-stash pattern; handshakes are recipient-addressed. Capability selection prefers Ghost PQ for a new peer that supports both modes, while an established mode is retained or reported unavailable rather than silently switched.

```text
Ghost native -> Kaspa L1 -> ghost-indexer -> Ghost Talk
Kasia        -> Kaspa L1 -> Kasia Indexer -> ghost-kasia -> Ghost Talk
```

Kaspa L1 remains authoritative for Ghost-native protocol state. `ghost-indexer::GhostProfileIndex` is a disposable/rebuildable verified current-state projection over Ghost-native descriptors; it never parses Kasia payloads. Kasia discovery/history stays behind the Kasia indexer and never becomes the source of Ghost-native state.

## Call lifecycle ownership

Calling is process-local. `ghost-domain::CallManager` is the only authority over `CallStore`; live call state is not part of the durable profile. UI, mailbox, signaling, transport, and media code issue typed call commands/events instead of mutating call phases.

`LiveCallManager` is mounted once at the application shell so navigation cannot discard an incoming or active call. Hang-up commits the local call transition before media/network cleanup, so remote signaling failure cannot keep the local UI stuck in a live call. Mute is an explicit `SetMuted(bool)` transition bound to the exact rendered `CallRecord`.

The normal lifecycle is:

```text
Call button
  -> signed standalone Kaspa ring
  -> IncomingRinging / OutgoingRinging
  -> receiver explicitly accepts
  -> Answering / Connecting
  -> establish or reuse authenticated HYDRA/KKTP transport
  -> Connected
  -> media + optional p2p-net realtime delivery
  -> local-first Ending / Ended
```

The initial signed ring does not create a receiver chat. A chat/session is created or reused only after acceptance. `components/call/` contains presentation/orchestration modules; none owns a second call-state machine.

## Mailbox processing

`MailboxService` exclusively owns durable frame reassembly, ready-envelope discovery, and envelope removal. Mailbox processing above that storage boundary is per envelope:

```text
fetch -> validate -> deduplicate -> decrypt -> classify -> route -> acknowledge
```

Every envelope receives a `PacketDisposition` such as consumed, retryable, or rejected. Failure of one stale/corrupt envelope does not abort later unrelated envelopes, preventing head-of-line blocking of new chat or call requests.

## Wallet/network progress

`WalletStateService` is the WASM-side mutation boundary for durable wallet state. Address-derivation progress is merged monotonically so delayed asynchronous completion cannot roll a newer receive/change projection backward. The native Kaspa gateway owns connection/broadcast details through one canonical `ghost-kaspa::NodeSession` containing the exact network, endpoint, and gateway generation; transaction planning and network retry logic must not be duplicated in UI/chat/call components.

## Module structure

Production Rust uses real semantic modules. Textual `include!` composition, numeric-order pseudo-modules, compound `_and_` source names, wildcard sibling coupling, stale local re-exports, nontrivial exact function-body duplication, duplicate shared DTO/domain declarations, and oversized flat source directories are architecture violations enforced by `scripts/check-architecture.py`. Every first-party production Rust file must stay below the 300-line normal SRP threshold; 350 remains an absolute safety ceiling. Functions target <50 lines and CC<=6, with hard ceilings of 75/8 and an explicit current review for every normal-target crossing.

Chat lifecycle/session/message/peer-binding fields, Room lifecycle/member/message state, and Contact authenticated identity fields are private outside their owning domain crates. External code receives immutable getters and semantic commands; Rust privacy therefore enforces the ownership matrix in addition to static QA. Yew components do not construct `ProfilePatch`, clone/mutate durable `Profile` state, or coordinate multiple domain stores. Application commands/controllers perform orchestration and emit typed deltas.

Complex subsystems expose focused facades/services/managers. Pure codecs, deterministic validators, hashes, shared primitive browser/native validation helpers, and small single-purpose transformations remain direct functions or structs rather than being hidden behind unnecessary facades. One-line controller-to-adapter forwarding layers are treated as architecture debt unless they establish a genuine semantic boundary.
