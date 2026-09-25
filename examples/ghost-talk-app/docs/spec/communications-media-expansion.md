# Communications and Media Expansion

This document defines the implemented ownership contract for Ghost Talk's communications and media expansion. Source implementation and commercial qualification are deliberately separate: the architecture below is implemented, while external funded/device/platform evidence remains a release gate.

## Identity

Public Ghost profiles retain Kaspa/HYDRA identity as the cryptographic fallback and may expose a display name, one primary KNS or `.k` name, verified-name observations, avatar media, description/interests, capabilities, and a profile signature. A profile name claim is never trusted by itself. `GhostNameResolver` normalizes the claim, resolves the correct namespace, verifies current ownership against the profile identity/address, and owns bounded cache expiration/revalidation/invalidation.

The same verified identity projection is used by profiles, contacts, chat, calls, Rooms, discovery, creator/station metadata, radio, and podcasts. Users without KNS/`.k` remain fully supported through raw Kaspa/HYDRA identity.

## Shared media

`ghost-media` is the common media owner for avatars, recordings, podcasts, broadcast archives, and later chat/Room attachments. It owns:

- `MediaReference` content-addressed identity and location metadata;
- `GhostMediaManifest`, canonical signing bytes, signature metadata, and strict manifest validation;
- content/chunk hashing and independently verifiable archive carriers;
- local content-addressed storage and verified retrieval;
- persistent verified archive chunk caching and final reconstruction verification.

No profile-, podcast-, radio-, or archive-specific content store is allowed. Caches and mirrors are never trusted for integrity.

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

## Rooms, stages, radio, and Room voice

`ghost-rooms` remains authoritative for membership, access, roles, bans, moderation, invitations, revisions, text policy, and audio policy. `RoomAccess` models Private, InviteOnly, Unlisted, and Public.

Private group, community voice, stage, and radio are `RoomMode`/policy presets over the same Room system. `Off`, `Interactive`, and `PresentersOnly` audio authorization is enforced from Room roles. There is no parallel stage/radio Room implementation.

Room voice uses the existing Ghost `BrowserVoiceSender`/receiver and encrypted realtime path. The Room owner relays authorized speakers to other members where required. Incoming audio is checked against Room membership/policy before playback. No second microphone or Room-specific capture stack exists.

## Shared live-media pipeline

The canonical live path is:

```text
BrowserVoiceSender
      -> encoded Ghost VoiceChunk units
      -> Room encrypted realtime distribution
      -> optional BroadcastPipeline fan-out
             -> RTMP/RTMPS sink
             -> local recording sink
```

`ghost-broadcast::BroadcastPipeline` owns fan-out and sink-failure isolation. Additional sinks consume the same encoded input; they do not open another capture device. Browser-generated independently decodable WebM/Ogg units are retained as the source stream. Local recording finalizes those units through FFmpeg concat/remux. RTMP adapts the units to one persistent AAC/FLV publisher without introducing a second microphone owner.

RTMP stream keys use a redacting/zeroizing `RtmpSecret`. They are never chain data, normal debug output, public configuration, or crash-report fields. RTMP/RTMPS URLs are explicitly validated.

## Permanent Kaspa archive

Permanent archival is optional and explicitly confirmed. The implementation:

1. derives the content-addressed media id;
2. plans the actual archive payload sizes through the existing Kaspa transaction fee planner;
3. shows/accepts a maximum confirmed cost before spending;
4. persists archive progress before every submission;
5. records an in-flight chunk intent before broadcast;
6. reconciles uncertain in-flight submissions from Kaspa history before retrying;
7. commits exact transaction ids and chunk hashes;
8. reconstructs media from those transaction locators;
9. verifies every chunk and the final root content hash;
10. stores only verified chunks/results in the persistent media cache.

An interruption therefore resumes from durable progress rather than restarting the archive or risking duplicate permanent publication.

## Podcasts and creator/station profiles

`ghost-broadcast::BroadcastCatalog` owns durable creator, station, show, and episode metadata. Creator/station records reuse Ghost identity/name/media primitives instead of creating another account/authentication system. A station references its owning creator and authoritative live Room; presenter authority remains in Room roles.

A completed local broadcast recording is already a `MediaReference`. Podcast publishing references that existing asset directly. Episodes may attach a signed `GhostMediaManifest` and may optionally reference a permanent Kaspa archive of the same content.

## Security-state UX

The implemented presentation distinguishes the relevant security/egress states, including `🛡 Ghost PQ`, `🔒 Kasia`, private/public Room state, `📡 External RTMP`, and `💾 Local Recording`. Permanent Kaspa publication requires a separate explicit confirmation and cost bound. Kasia is never represented as equivalent to Ghost PQ; external RTMP is outside Ghost's security boundary.

## Subsystem ownership

```text
ghost-protocol    Ghost-native wire formats
ghost-hydra       HYDRA/PQ cryptography
ghost-chat        Ghost-native conversation domain
ghost-kasia       Kasia codec/crypto/indexer/contact compatibility
ghost-contacts    contacts/profile domain
ghost-names       KNS/.k resolution and verification
ghost-rooms       membership/access/roles/policies
ghost-talk        reusable voice/session SDK
ghost-media       shared media identity/manifests/archive carriers
ghost-broadcast   live fan-out, RTMP, recording, creator/podcast metadata
ghost-indexer     rebuildable verified Ghost L1 discovery index
ghost-runtime     authoritative durable application state
ghost-wasm        Web presentation/platform adapter
ghost-talk-native native/Tauri platform adapter
```

Simple hashing, verification, codecs, validators, and value objects stay direct functions/structs. Focused facades/services exist only where they own a real complex subsystem boundary.

## Qualification

The source implementation covers the 28-stage communications/media plan through architecture and automated regression surfaces. The final commercial gate still requires exact-revision evidence for funded independent-process interoperability, device/audio HIL, supported-platform behavior, production background delivery, keystore/biometrics, group cryptographic lifecycle, UI/platform parity, and release signing. Those external requirements must not be marked complete by source tests alone.
