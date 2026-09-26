//! Ghost Talk realtime carrier contract.
//!
//! This crate is the single owner of Ghost Talk realtime primitives shared by
//! the Ghost Talk application and Kaspa Kinesis:
//!
//! - GTR1 framing ([`Gtr1Envelope`], [`encode_gtr1`], [`decode_gtr1`],
//!   `GTR1_MAGIC`) and the cross-carrier [`ReplayIdentity`];
//! - canonical p2p-net topics (`CONTROL_TOPIC`, `REALTIME_TOPIC`,
//!   `VOICE_TOPIC`, room and per-session topics);
//! - the `Automatic` (p2p-net, then Kaspa) / `KaspaOnly` route policy;
//! - authenticated SID/HYDRA/PeerId session bindings and the
//!   [`GhostP2pTransport`] contract implemented by platform adapters;
//! - with the default `p2p-net` feature: game-peer routing
//!   (`PeerDirectory`, `send_to_game_peer`) and Ghost Talk's p2p-net node
//!   policy.
//!
//! Transport mechanics (WebRTC, relays, NAT traversal) belong to p2p-net and
//! never appear in this API.

#![forbid(unsafe_code)]

mod binding;
mod gtr1;
#[cfg(feature = "p2p-net")]
mod node;
#[cfg(feature = "p2p-net")]
mod peers;
mod replay;
mod route;
mod topics;
mod transport;

pub use binding::{LocalP2pBinding, SessionPeerBinding, SessionPeerBindings, MAX_BOUND_SESSIONS};
pub use gtr1::{
    decode_gtr1, encode_gtr1, parse_hydra_id, parse_session_id, Gtr1Envelope, Gtr1Error,
    ReplayIdentity, GTR1_HEADER_LEN, GTR1_MAGIC, MAX_REALTIME_CIPHERTEXT_BYTES,
};
#[cfg(feature = "p2p-net")]
pub use node::{ghost_node_config, p2p_network_id, storage_namespace, GHOST_TALK_DISCOVERY_APP_ID};
#[cfg(feature = "p2p-net")]
pub use peers::{send_to_game_peer, PeerDirectory};
pub use replay::ReplayWindow;
pub use route::{
    route, route_order, text_route, RealtimeCarrier, RouteDecision, RoutePreference, TextPreference,
};
pub use topics::{
    room_topic, session_topic, session_topic_id, CONTROL_TOPIC, REALTIME_TOPIC, ROOM_TOPIC_PREFIX,
    SESSION_TOPIC_PREFIX, VOICE_TOPIC,
};
pub use transport::{
    GhostP2pTransport, P2pFuture, P2pIncoming, P2pInfrastructure, P2pRouteState, P2pStartConfig,
    P2pSubscription,
};
