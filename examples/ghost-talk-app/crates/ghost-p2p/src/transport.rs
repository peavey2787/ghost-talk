use std::{future::Future, pin::Pin};

use serde::{Deserialize, Serialize};

use crate::{LocalP2pBinding, SessionPeerBinding};

pub type P2pFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + 'a>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum P2pRouteState {
    Unavailable,
    Starting,
    Connecting,
    Connected,
    KaspaFallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct P2pStartConfig {
    pub network_id: String,
    pub profile_id: String,
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
        let labels = [
            format!("{:?}", P2pRouteState::Unavailable),
            format!("{:?}", P2pRouteState::Connected),
            format!("{:?}", P2pRouteState::KaspaFallback),
        ];
        for label in labels {
            let lower = label.to_ascii_lowercase();
            assert!(!lower.contains("webrtc"));
            assert!(!lower.contains("dcutr"));
            assert!(!lower.contains("quic"));
        }
    }
}
