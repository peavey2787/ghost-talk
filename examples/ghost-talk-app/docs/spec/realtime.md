# Realtime transport contract

Ghost Talk realtime traffic is defined independently of its carrier. KKTP v2 remains the authenticated session protocol and `GTR1` is the carrier-neutral realtime envelope. The exact same sealed `GTR1` bytes are intended to move over either the direct P2P carrier or Kaspa fallback; a carrier switch must not advance HYDRA or create a second ciphertext for the same logical packet.

## Ownership

- `ghost-protocol` owns `GTR1`, `RealtimeBodyV1`, transport announcements, Opus batches, and deterministic session-topic derivation.
- `ghost-realtime` owns carrier selection and the bounded cross-carrier replay key `(sender_hydra_id, sid, message_id)`.
- `ghost-p2p` owns authenticated SID/HYDRA/P2P-PeerId bindings. A transport PeerId is never a Ghost identity.
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

## Shared realtime bodies

`RealtimeBodyV1` defines the reusable Ghost/Kinesis realtime vocabulary:

- transport announce / acknowledgement;
- call request / accept / decline / hangup;
- `VoiceBatchV1` raw Opus batches;
- room presence;
- room voice.

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

## Implementation status

The browser realtime path now uses the shared p2p-net WASM backend. Ghost no longer owns a browser WebRTC/SDP/ICE stack. Authenticated transport announcements bind p2p PeerId to the active HYDRA identity and SID, and Auto preserves exact GTR1 bytes when falling back from p2p-net to Kaspa.
