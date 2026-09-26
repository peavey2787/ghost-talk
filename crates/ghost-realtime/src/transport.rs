use std::{future::Future, pin::Pin};

use serde::{Deserialize, Serialize};

use crate::{LocalP2pBinding, SessionPeerBinding};

pub type P2pFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + 'a>>;

/// Coarse route state shown to users; never names transport mechanics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum P2pRouteState {
    Unavailable,
    Starting,
    Connecting,
    Connected,
    KaspaFallback,
}

/// Additional operator-owned p2p-net infrastructure. Empty lists keep
/// p2p-net's own public bootstrap policy.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct P2pInfrastructure {
    pub bootstrap_peers: Vec<String>,
    pub relay_peers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct P2pStartConfig {
    pub network_id: String,
    pub profile_id: String,
    pub infrastructure: P2pInfrastructure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct P2pIncoming {
    pub source_peer_id: String,
    pub topic: String,
    pub packet: Vec<u8>,
}

pub trait P2pSubscription {
    fn next<'a>(&'a mut self) -> P2pFuture<'a, Option<P2pIncoming>>;
}

/// The single Ghost Talk realtime transport contract. Implementations deliver
/// exact sealed GTR1 bytes on SID-derived topics and verify the authenticated
/// SID/HYDRA/PeerId binding before a packet reaches the application.
pub trait GhostP2pTransport {
    type Subscription: P2pSubscription;

    fn start<'a>(&'a mut self, config: P2pStartConfig) -> P2pFuture<'a, LocalP2pBinding>;
    fn local_binding<'a>(&'a self) -> P2pFuture<'a, LocalP2pBinding>;
    fn bind_session<'a>(&'a mut self, binding: SessionPeerBinding) -> P2pFuture<'a, ()>;
    fn connect<'a>(&'a mut self, address: &'a str) -> P2pFuture<'a, ()>;
    fn send<'a>(&'a self, sid: [u8; 16], packet: &'a [u8]) -> P2pFuture<'a, ()>;
    fn subscribe<'a>(&'a self, sid: [u8; 16]) -> P2pFuture<'a, Self::Subscription>;
    fn shutdown<'a>(&'a mut self) -> P2pFuture<'a, ()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_states_do_not_expose_transport_mechanics() {
        for state in [
            P2pRouteState::Unavailable,
            P2pRouteState::Starting,
            P2pRouteState::Connecting,
            P2pRouteState::Connected,
            P2pRouteState::KaspaFallback,
        ] {
            let label = serde_json::to_string(&state).unwrap().to_ascii_lowercase();
            assert!(
                !label.contains("webrtc") && !label.contains("dcutr") && !label.contains("quic")
            );
        }
    }

    #[test]
    fn infrastructure_defaults_to_p2p_net_public_policy() {
        let parsed: P2pInfrastructure = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, P2pInfrastructure::default());
        let parsed: P2pInfrastructure =
            serde_json::from_str(r#"{"relayPeers":["/ip4/1.2.3.4/udp/1/webrtc-direct"]}"#).unwrap();
        assert!(parsed.bootstrap_peers.is_empty());
        assert_eq!(parsed.relay_peers.len(), 1);
    }
}
