# Realtime transport contract

Ghost Talk realtime traffic is defined independently of its carrier. KKTP v2 remains the authenticated session protocol and `GTR1` is the carrier-neutral realtime envelope. The exact same sealed `GTR1` bytes are intended to move over either the direct P2P carrier or Kaspa fallback; a carrier switch must not advance HYDRA or create a second ciphertext for the same logical packet.

## Ownership

- `ghost-realtime` (SDK) owns `GTR1`, the cross-carrier replay identity `(sender, message_id, session_id)`, canonical topics including SID-derived session topics, carrier selection, the text carrier policy, and SID/HYDRA/P2P-PeerId bindings. A transport PeerId is never a Ghost identity.
- `ghost-voice` (SDK) owns the 1:1 call and Room voice packet formats.
- `ghost-protocol` owns the transport announcement body (`RealtimeBodyV1`), direct p2p text (`DirectTextV1`), and the inner SID binding prefix.
- `ghost-p2p` is the browser adapter that drives p2p-net's `WasmNode`.
- `ghost-hydra` remains the cryptographic identity/session authority.
- Kaspa remains the durable carrier and final fallback.
- p2p-net is the target direct/realtime implementation boundary. Ghost application code must not grow a second libp2p, relay, NAT, DCUtR, ICE, STUN, or WebRTC implementation.

## GTR1

The version-1 binary layout is preserved for compatibility:

```text
0..4    magic = GTR1
4..36   sender HYDRA id (32 bytes)
36..52  message id (16 bytes)
52..68  exact KKTP SID (16 bytes)
68..    HYDRA/persistent realtime ciphertext
```

`ghost-protocol::Gtr1Envelope` is the only encoder/decoder. Consumers must reject truncated carriers, wrong magic, empty ciphertext, oversized ciphertext, wrong/retired SID, and mismatched authenticated sender before dispatching application bodies.

## Realtime bodies

Sealed realtime bodies are one of: a transport announcement or acknowledgement (`RealtimeBodyV1`), a live call packet or Room voice packet (`ghost-voice`), or ephemeral direct text (`DirectTextV1`).

Transport announcements carry only a P2P PeerId, bounded dial addresses, and capabilities. They inherit authentication from the already-active KKTP/HYDRA session; they do not introduce a second signature scheme.

## Session topics

A direct session topic is derived with a BLAKE3 domain separator from the binary KKTP SID and is exposed as:

```text
ghost-talk/realtime/v1/<derived-id>
```

The SID is not embedded as readable hexadecimal. Room voice will use an equivalent room/epoch derivation.

## Routing

The user-facing route model remains only:

```text
Auto
Kaspa only
```

The target routing policy is:

```text
Auto
  -> p2p-net when available
  -> Kaspa on P2P send failure/unavailability

Kaspa only
  -> Kaspa
```

Direct transport details such as TCP, QUIC, relay, DCUtR, WebSocket, or WebRTC-direct belong inside p2p-net and must not be exposed as Ghost Talk route choices.

## Text carrier

Chat text is anchored on Kaspa by default: it is durable and recoverable from chain history. A profile may choose **p2p-net preferred** (p2p-net while connected, otherwise Kaspa) or **p2p-net only** (never stored on Kaspa; not sent without a connected route). p2p-net text is sealed by the same authenticated HYDRA session, is excluded from Kaspa backups, and every such message is marked "p2p · not stored on Kaspa". Together with Auto, this keeps Kaspa usage to the signed signalling and fallback traffic.

## Implementation status

The browser realtime path now uses the shared p2p-net WASM backend. Ghost no longer owns a browser WebRTC/SDP/ICE stack. Authenticated transport announcements bind p2p PeerId to the active HYDRA identity and SID, and Auto preserves exact GTR1 bytes when falling back from p2p-net to Kaspa.
