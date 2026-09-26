# p2p-net integration boundary

Ghost Talk has one realtime transport contract: `ghost_realtime::GhostP2pTransport`. Application crates consume Ghost concepts only: start a profile-scoped node, bind an authenticated SID/HYDRA identity to a transport PeerId, connect announced addresses, send the exact sealed GTR1 bytes, subscribe to the SID-derived topic, observe coarse lifecycle events, and shut down.

## One WebAssembly module, clean imports

Ghost Talk, Kaspa Portal, HYDRA and p2p-net are separately maintained crates that share one `wasm-bindgen 0.2.108` ABI family, so the application links all of them as ordinary dependencies into one WebAssembly module. There is no copy of p2p-net code and no separately loaded p2p module.

`ghost-p2p` implements `GhostP2pTransport` by starting p2p-net's own `WasmNode` in process with the shared Ghost node policy (`ghost_realtime::ghost_node_config`) in a per-profile IndexedDB namespace (`ghost_realtime::storage_namespace`). Kaspa access goes only through Kaspa Portal (wRPC requests, BlockAdded/UtxosChanged/VirtualDaaScoreChanged subscriptions, address and script derivation); Rusty-Kaspa client crates are not in either graph. Architecture checks enforce both rules and a single p2p-net revision.

Persistent browser identity, IndexedDB durability, same-profile tab exclusion, lifecycle handling, address filtering, WebRTC-direct/WebSocket/relay transports, discovery, and reconnect behavior belong to p2p-net. Ghost does not import libp2p or browser peer-connection APIs.

## Authentication and fallback

Transport announcements are sent only over the already-authenticated KKTP/HYDRA/Kaspa path; Kaspa is the signal layer. An announcement contains a p2p PeerId, bounded dial addresses, and capabilities. Before an incoming p2p packet is decrypted, Ghost requires the packet's GTR1 SID and HYDRA sender to match the active authenticated session and the source PeerId to match that session's transport binding.

Realtime fallback preserves exact bytes. A realtime body is sealed once into GTR1; p2p-net receives those exact bytes; if p2p delivery fails, the Kaspa fallback publishes the same carrier. The fallback never reseals the body or advances HYDRA again.

## Operator infrastructure

Browsers cannot accept inbound connections. They reserve a slot on a p2p-net relay (WebRTC-direct or secure WebSocket) and reach each other through it. Settings → Direct transport lets a profile list operator relays and bootstrap peers; empty lists keep p2p-net's public bootstrap policy. The E2E suite runs a private relay (`examples/ghost-talk-app/e2e/harness`).

Architecture gates forbid application-owned `RTCPeerConnection`, SDP, STUN/ICE configuration, negotiation identifiers, direct-carrier code, libp2p imports, and p2p-net dependencies in application crates. The realtime route UI exposes only **Auto** and **Kaspa only**.
