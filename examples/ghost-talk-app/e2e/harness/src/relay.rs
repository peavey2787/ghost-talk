//! A private p2p-net relay for the browser instances. Browsers cannot accept
//! inbound connections; they reserve a slot on this relay over WebRTC-direct
//! and reach each other through it, exactly as with an operator relay.

use std::time::Duration;

use ghost_realtime::{ghost_node_config, P2pInfrastructure};
use p2p_net::{NodeProfile, PublicBootstrapConfig};
use serde_json::json;

pub async fn run(network: &str) -> Result<(), String> {
    let home = crate::state::home();
    std::fs::create_dir_all(&home).map_err(|error| error.to_string())?;
    let mut config = ghost_node_config(network, &P2pInfrastructure::default());
    config.profile = NodeProfile::Relay;
    config.identity_key_path = home.join("relay-identity.key").display().to_string();
    config.webrtc_certificate_path = home.join("relay-webrtc-cert.pem").display().to_string();
    config.discovery.peer_cache_path = home.join("relay-peer-cache.json").display().to_string();
    config.discovery.public_bootstrap = PublicBootstrapConfig::private_infrastructure_only();
    config.listen_addresses = vec![
        "/ip4/0.0.0.0/udp/0/webrtc-direct".into(),
        "/ip4/0.0.0.0/tcp/0".into(),
    ];
    config.relay.enabled = true;
    config.relay.max_circuit_duration_secs = 6 * 60 * 60;
    config.relay.max_circuit_bytes = 1 << 32;
    let node = p2p_net::start_node(config)
        .await
        .map_err(|error| error.to_string())?;
    let addresses = browser_addresses(&node).await;
    if addresses.is_empty() {
        return Err("relay has no browser-dialable WebRTC-direct LAN address".into());
    }
    println!(
        "{}",
        json!({ "peerId": node.peer_id.to_string(), "addresses": addresses })
    );
    tokio::signal::ctrl_c()
        .await
        .map_err(|error| error.to_string())?;
    node.shutdown().await;
    Ok(())
}

/// WebRTC-direct listen addresses on a LAN interface (browsers ignore
/// loopback candidates), each naming the relay PeerId.
async fn browser_addresses(node: &p2p_net::NodeHandle) -> Vec<String> {
    for _ in 0..50 {
        let snapshot = node.snapshot.lock().await;
        let found: Vec<String> = snapshot
            .local_listen_addresses
            .iter()
            .chain(snapshot.public_direct_listen_addresses.iter())
            .filter(|address| address.contains("/webrtc-direct/certhash/"))
            .filter(|address| !address.starts_with("/ip4/127.") && !address.starts_with("/ip4/0."))
            .map(|address| format!("{address}/p2p/{}", node.peer_id))
            .collect();
        drop(snapshot);
        if !found.is_empty() {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Vec::new()
}
