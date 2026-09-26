//! Ghost Talk's p2p-net node policy, shared by every host that starts a node
//! for Ghost Talk conversations.

use p2p_net::NodeConfig;

use crate::P2pInfrastructure;

/// Discovery namespace application id; keeps Ghost Talk peers in their own
/// hashed discovery namespace instead of p2p-net's default one.
pub const GHOST_TALK_DISCOVERY_APP_ID: &str = "ghost-talk";

/// Stable libp2p network discriminator for a Kaspa network name. Peers on
/// different Kaspa networks never share a libp2p network.
pub fn p2p_network_id(kaspa_network: &str) -> u32 {
    let normalized = kaspa_network.trim().to_ascii_lowercase();
    let hash = blake3::hash(normalized.as_bytes());
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&hash.as_bytes()[..4]);
    u32::from_le_bytes(prefix)
}

/// Per-profile persistence namespace so each Ghost Talk profile keeps one
/// durable transport identity across restarts and Kaspa network switches.
pub fn storage_namespace(profile_id: &str) -> String {
    format!("ghost-talk:{profile_id}")
}

/// p2p-net configuration for a Ghost Talk node. Paths are relative to the
/// caller's storage namespace (IndexedDB in browsers).
pub fn ghost_node_config(kaspa_network: &str, infrastructure: &P2pInfrastructure) -> NodeConfig {
    let mut config = NodeConfig {
        network_id: p2p_network_id(kaspa_network),
        identity_key_path: "identity.key".into(),
        bootstrap_peers: infrastructure.bootstrap_peers.clone(),
        relay_peers: infrastructure.relay_peers.clone(),
        ..NodeConfig::default()
    };
    config.discovery.peer_cache_path = "peer-cache.json".into();
    config.discovery.namespace.app_id = GHOST_TALK_DISCOVERY_APP_ID.into();
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_ids_are_stable_and_network_specific() {
        assert_eq!(p2p_network_id("testnet-10"), p2p_network_id(" Testnet-10 "));
        assert_ne!(p2p_network_id("testnet-10"), p2p_network_id("mainnet"));
    }

    #[test]
    fn node_config_applies_ghost_policy_and_operator_infrastructure() {
        let infrastructure = P2pInfrastructure {
            bootstrap_peers: vec!["/dns4/boot.example/tcp/443/wss/p2p/boot".into()],
            relay_peers: vec!["/dns4/relay.example/tcp/443/wss/p2p/relay".into()],
        };
        let config = ghost_node_config("testnet-10", &infrastructure);
        assert_eq!(config.network_id, p2p_network_id("testnet-10"));
        assert_eq!(config.identity_key_path, "identity.key");
        assert_eq!(config.discovery.peer_cache_path, "peer-cache.json");
        assert_eq!(
            config.discovery.namespace.app_id,
            GHOST_TALK_DISCOVERY_APP_ID
        );
        assert_eq!(config.bootstrap_peers, infrastructure.bootstrap_peers);
        assert_eq!(config.relay_peers, infrastructure.relay_peers);
        assert_eq!(storage_namespace("abc"), "ghost-talk:abc");
    }

    /// Pins the p2p-net browser event/binding schema mirrored by the Ghost
    /// Talk browser adapter (`ghost-p2p`), which reads it through JavaScript.
    #[test]
    fn browser_contract() {
        use p2p_net::{LocalNodeBinding, NodeEvent};
        let binding = LocalNodeBinding {
            peer_id: "me".into(),
            dial_addresses: vec!["/a".into()],
        };
        assert_eq!(
            serde_json::to_string(&binding).unwrap(),
            r#"{"peer_id":"me","dial_addresses":["/a"]}"#
        );
        let cases = [
            (
                NodeEvent::LocalBindingChanged(binding),
                r#"{"event":"local_binding_changed","peer_id":"me","dial_addresses":["/a"]}"#,
            ),
            (
                NodeEvent::PeerConnected {
                    peer_id: "p".into(),
                },
                r#"{"event":"peer_connected","peer_id":"p"}"#,
            ),
            (
                NodeEvent::PeerDisconnected {
                    peer_id: "p".into(),
                },
                r#"{"event":"peer_disconnected","peer_id":"p"}"#,
            ),
            (NodeEvent::Online, r#"{"event":"online"}"#),
            (NodeEvent::Offline, r#"{"event":"offline"}"#),
        ];
        for (event, json) in cases {
            assert_eq!(serde_json::to_string(&event).unwrap(), json);
        }
        let peer =
            serde_json::to_value(p2p_net::PeerInfo::connected(p2p_net::PeerId::random())).unwrap();
        assert!(peer["peer_id"].is_string());
        assert_eq!(peer["connected"], serde_json::Value::Bool(true));
    }
}
