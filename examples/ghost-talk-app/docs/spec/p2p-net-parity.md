# p2p-net integration boundary

Ghost Talk has one realtime transport contract: `ghost-p2p::GhostP2pTransport`. Application crates consume Ghost concepts only: start a profile-scoped node, bind an authenticated SID/HYDRA identity to a transport PeerId, connect announced addresses, send the exact sealed GTR1 bytes, subscribe to the SID-derived topic, observe coarse lifecycle events, and shut down.

The browser/PWA implementation delegates to `p2p_net::wasm::WasmNode`. Persistent browser identity, IndexedDB durability, same-profile tab exclusion, browser lifecycle handling, address filtering, WebRTC/WebSocket/relay transports, discovery, and reconnect behavior belong to p2p-net. Ghost does not import libp2p or browser peer-connection APIs.

Transport announcements are sent only over the already-authenticated KKTP/HYDRA/Kaspa path. An announcement contains a p2p PeerId, bounded dial addresses, and capabilities. Before an incoming p2p packet is decrypted, Ghost requires the packet's GTR1 SID and HYDRA sender to match the active authenticated session and the source PeerId to match that session's transport binding.

Realtime fallback is exact-byte preserving: a logical realtime body is sealed once into GTR1, p2p-net receives those exact bytes, and Kaspa fallback fragments and publishes the same carrier if p2p delivery fails. The fallback must never reseal the body or advance HYDRA again.

Architecture gates forbid application-owned `RTCPeerConnection`, SDP, STUN/ICE configuration, negotiation identifiers, direct-carrier code, libp2p imports, and transport-selection implementation outside `ghost-p2p`. UI exposes only **Auto** and **Kaspa only**.
