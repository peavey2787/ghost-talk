//! Mirror of p2p-net's transport-neutral browser event schema.
//! `ghost-realtime`'s `browser_contract` test pins the upstream side.

use ghost_realtime::LocalP2pBinding;
use serde::Deserialize;

/// Coarse p2p-net lifecycle events relevant to Ghost Talk routing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum P2pNetEvent {
    LocalBindingChanged(LocalP2pBinding),
    PeerConnected(String),
    PeerDisconnected(String),
    Online,
    Offline,
}

/// The subset of p2p-net's `PeerInfo` Ghost Talk needs.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Debug, Deserialize)]
pub(crate) struct PeerInfoWire {
    pub(crate) peer_id: String,
    #[serde(default)]
    pub(crate) connected: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum NodeEventWire {
    LocalBindingChanged(LocalP2pBinding),
    PeerConnected { peer_id: String },
    PeerDisconnected { peer_id: String },
    Online,
    Offline,
}

impl From<NodeEventWire> for P2pNetEvent {
    fn from(event: NodeEventWire) -> Self {
        match event {
            NodeEventWire::LocalBindingChanged(binding) => {
                Self::LocalBindingChanged(binding.with_peer_suffixes())
            }
            NodeEventWire::PeerConnected { peer_id } => Self::PeerConnected(peer_id),
            NodeEventWire::PeerDisconnected { peer_id } => Self::PeerDisconnected(peer_id),
            NodeEventWire::Online => Self::Online,
            NodeEventWire::Offline => Self::Offline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(json: &str) -> P2pNetEvent {
        serde_json::from_str::<NodeEventWire>(json).unwrap().into()
    }

    #[test]
    fn upstream_event_schema_maps_to_ghost_events() {
        assert_eq!(
            event(r#"{"event":"peer_connected","peer_id":"a"}"#),
            P2pNetEvent::PeerConnected("a".into())
        );
        assert_eq!(
            event(r#"{"event":"peer_disconnected","peer_id":"a"}"#),
            P2pNetEvent::PeerDisconnected("a".into())
        );
        assert_eq!(event(r#"{"event":"online"}"#), P2pNetEvent::Online);
        assert_eq!(event(r#"{"event":"offline"}"#), P2pNetEvent::Offline);
        let changed = event(
            r#"{"event":"local_binding_changed","peer_id":"me","dial_addresses":["/ip4/1.2.3.4/tcp/1/p2p/relay/p2p-circuit"]}"#,
        );
        assert_eq!(
            changed,
            P2pNetEvent::LocalBindingChanged(LocalP2pBinding {
                peer_id: "me".into(),
                dial_addresses: vec!["/ip4/1.2.3.4/tcp/1/p2p/relay/p2p-circuit/p2p/me".into()],
            })
        );
    }

    #[test]
    fn peer_info_subset_ignores_unrelated_fields() {
        let peers: Vec<PeerInfoWire> = serde_json::from_str(
            r#"[{"peer_id":"a","connected":true,"addresses":[],"sources":["connected"]},{"peer_id":"b"}]"#,
        )
        .unwrap();
        assert_eq!(peers[0].peer_id, "a");
        assert!(peers[0].connected);
        assert!(!peers[1].connected);
    }
}
