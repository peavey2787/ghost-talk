# Voice composition in the reference application

Ghost Talk's reusable media engine is the `crates/ghost-talk` SDK. This document describes only how the reference application composes that SDK with its communication stack.

The application exposes two realtime carrier policies:

- **Auto** — after the authenticated KKTP/HYDRA session is active, use p2p-net when an authenticated p2p binding is connected and fall back to Kaspa when p2p is unavailable or a p2p send fails.
- **Kaspa only** — send call control and voice through the authenticated Kaspa carrier without attempting p2p delivery.

Physical transport selection is not a Ghost Talk concern. WebRTC-direct, WebSocket, Circuit Relay, TCP, QUIC, and any browser/native transport mechanics are internal to p2p-net/libp2p.

## Media boundary

`BrowserVoiceSender` owns microphone acquisition, finalized independently decodable Opus windows, stream IDs, and media sequence numbers. For every finalized window it emits one complete `VoiceChunk`. The application serializes that chunk, wraps it in call signaling, protects it with the active HYDRA/KKTP realtime keys, and sends the resulting GTR1 carrier over the selected carrier.

The receiver reverses those application/network layers first. Only after one complete Ghost Talk chunk has been reconstructed does the app call `BrowserVoiceReceiver.receiveChunk()`. The SDK adapter owns duplicate/stale handling, reorder buffering, bounded gap advancement, decode, and complete decoded-duration `AudioContext` scheduling.

Transport fragmentation and transport ordering are never exposed as Ghost Talk media fragments. Media sequence numbers belong to complete `VoiceChunk`s only.

## Call signaling

Request / Accept / Decline / Hang-up are application concepts owned by the Rust reference application. The audio variant contains opaque base64 of exactly one complete encoded `VoiceChunk`; it does not duplicate SDK sequence/codec fields.

The caller does not enter connected state until the matching Accept arrives. The callee does not send microphone audio before accepting. Under Auto, later complete chunks can fall back from p2p-net to Kaspa without changing SDK media state.

## Security and carrier behavior

Each realtime body is sealed once into the carrier-neutral GTR1 envelope. If Auto selects p2p-net, Ghost sends those exact GTR1 bytes to the authenticated transport PeerId on the SID-derived topic. If that send fails, Kaspa receives the same GTR1 bytes; Ghost does not reseal the body or advance HYDRA a second time.

The transport PeerId is bound to the already-authenticated HYDRA identity and exact active SID before incoming p2p bytes are opened. Cross-carrier replay tracking ensures the same logical GTR1 packet arriving through both p2p-net and Kaspa is processed once.

Live call media is deliberately outside the durable KKTP text sequence so a lost realtime chunk cannot block durable chat. The Ghost Talk SDK has its own bounded media gap policy and does not consume the application's durable KKTP directional sequence numbers.
