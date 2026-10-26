# Communications and Media Expansion

This document defines the implemented ownership contract for Ghost Talk's communications and media expansion. Source implementation and commercial qualification are deliberately separate: the architecture below is implemented, while external funded/device/platform evidence remains a release gate.

## Identity

Public Ghost profiles retain Kaspa/HYDRA identity as the cryptographic fallback and may expose a display name, one primary KNS or `.k` name, verified-name observations, avatar media, description/interests, capabilities, and a profile signature. A profile name claim is never trusted by itself. `GhostNameResolver` normalizes the claim, resolves the correct namespace, verifies current ownership against the profile identity/address, and owns bounded cache expiration/revalidation/invalidation.

The same verified identity projection is used by profiles, contacts, chat, calls, Rooms, and discovery. Users without KNS/`.k` remain fully supported through raw Kaspa/HYDRA identity.

## Shared media

`ghost-media` is the common media owner for avatars and later chat/Room attachments. It owns the `MediaReference` content-addressed identity and location metadata and content hashing/verification. No consumer-specific content store is allowed. Caches and mirrors are never trusted for integrity.

## Kasia and KaChat interoperability

`ghost-kasia` owns Kasia/KaChat compatibility for supported 1:1 text messaging. It contains codec, ECIES crypto, handshake modeling, Kasia-indexer access/history, and durable Ghost-contact mapping. Current payload writes use `kchat:1:handshake`, `kchat:1:comm`, and `kchat:1:pay`; the documented `ciph_msg:1:*` root is accepted on read for compatibility, including older `hs`/`msg` kind aliases. Contextual messages use the self-stash transaction pattern; handshakes are recipient-addressed.

Ghost-native and Kasia presentation can share UI components, but their protocol/domain/indexing implementations do not merge. The UI labels `🛡 Ghost PQ` and `🔒 Kasia` as distinct modes.

Capability discovery advertises `kasia-v1` in addition to Ghost-native capabilities. New conversations prefer Ghost PQ when the peer advertises both. An established conversation retains its explicit mode; a transport/indexer failure returns an unavailable state rather than silently switching protocols.

Live Ghost↔KaChat execution against an independently built KaChat client is release-qualification evidence, not a substitute for this source-level boundary.

## Hard indexing boundary

Ghost-native state remains on-chain authoritative:

```text
Ghost protocol -> Kaspa L1 -> ghost-indexer -> Ghost Talk
```

`ghost-indexer` is a disposable, rebuildable accelerator over verified Ghost-native L1 descriptors. Anything it reports must remain reconstructible/verifiable from Kaspa.

Kasia discovery/history remains:

```text
Kasia protocol -> Kaspa L1 -> Kasia Indexer -> ghost-kasia -> Ghost Talk
```

`ghost-indexer` never parses Kasia payloads. `ghost-kasia` never depends on `ghost-indexer`. `ghost-protocol`, `ghost-hydra`, and `ghost-chat` never depend on `ghost-kasia`.

## Rooms, Room voice, and Room video

`ghost-rooms` is authoritative for Room membership, access, roles, moderation, invitations, revisions, Room messages, and their reactions. `RoomAccess` is exactly `Private` (invitation only, never listed) or `Public` (announced to the swarm, optionally on Kaspa, and listed in Discover → Rooms). Every participant may send text, react, and join Room voice and video; there are no mode presets, presenter/audience roles, or per-Room text/audio policies.

Room voice and video reuse the shared `BrowserVoiceSender`/receiver and `BrowserVideoSender` from the `ghost-talk-wasm` SDK over the encrypted realtime routes. A member sends to the Room owner, which relays to every other member over each member's authenticated session. Incoming media is checked against Room membership and the hop (sender or owner) before playback. Video is p2p-net only: frames are never carried on Kaspa, are dropped when p2p-net is not connected, and only the newest frame per peer waits in the realtime queue.

## Security-state UX

The implemented presentation distinguishes the relevant security/egress states, including `🛡 Ghost PQ`, `🔒 Kasia`, and private/public Room state. Kasia is never represented as equivalent to Ghost PQ.

## Subsystem ownership

```text
ghost-protocol    Ghost-native wire formats
ghost-hydra       HYDRA/PQ cryptography
ghost-chat        Ghost-native conversation domain
ghost-kasia       Kasia codec/crypto/indexer/contact compatibility
ghost-contacts    contacts/profile domain
ghost-names       KNS/.k resolution and verification
ghost-rooms       membership/access/roles
ghost-talk        reusable voice/video/session SDK
ghost-media       shared content-addressed media identity

ghost-indexer     rebuildable verified Ghost L1 discovery index
ghost-runtime     authoritative durable application state
ghost-wasm        Web presentation/platform adapter
ghost-talk-native native/Tauri platform adapter
```

Simple hashing, verification, codecs, validators, and value objects stay direct functions/structs. Focused facades/services exist only where they own a real complex subsystem boundary.

## Qualification

The source implementation is covered by architecture and automated regression surfaces. The final commercial gate still requires exact-revision evidence for funded independent-process interoperability, device/audio HIL, supported-platform behavior, production background delivery, keystore/biometrics, group cryptographic lifecycle, UI/platform parity, and release signing. Those external requirements must not be marked complete by source tests alone.
