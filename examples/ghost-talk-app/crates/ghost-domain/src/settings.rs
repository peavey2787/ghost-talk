use serde::{Deserialize, Serialize};

/// Canonical durable user settings shared by every frontend/runtime surface.
/// Keep this model free of UI framework types and process-local state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_route")]
    pub route: String,
    #[serde(default = "default_stego")]
    pub stego: String,
    /// Chat text carrier: "Kaspa" (durable, default), "P2P preferred", or
    /// "P2P only". p2p-net text is never stored on Kaspa.
    #[serde(default = "default_text_route")]
    pub text_route: String,
    #[serde(default = "yes")]
    pub contacts_backup_kaspa: bool,
    #[serde(default)]
    pub backup_messages_kaspa: bool,
    #[serde(default)]
    pub require_send_password: bool,
    #[serde(default)]
    pub auto_ignore_unknown_chats: bool,
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default)]
    pub public_username: String,
    #[serde(default)]
    pub public_description: String,
    #[serde(default)]
    pub public_interests: String,
    /// Operator-owned p2p-net bootstrap peers (full `/p2p/<PeerId>` multiaddrs).
    /// Empty keeps p2p-net's public bootstrap policy.
    #[serde(default)]
    pub p2p_bootstrap_peers: Vec<String>,
    /// Operator-owned p2p-net relays that make browser peers reachable.
    #[serde(default)]
    pub p2p_relay_peers: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            route: default_route(),
            stego: default_stego(),
            text_route: default_text_route(),
            contacts_backup_kaspa: true,
            backup_messages_kaspa: false,
            require_send_password: false,
            auto_ignore_unknown_chats: false,
            debug_logging: false,
            public_username: String::new(),
            public_description: String::new(),
            public_interests: String::new(),
            p2p_bootstrap_peers: Vec::new(),
            p2p_relay_peers: Vec::new(),
        }
    }
}

pub fn default_route() -> String {
    "Auto".to_string()
}
pub fn default_text_route() -> String {
    "Kaspa".to_string()
}
pub fn default_stego() -> String {
    "Off".to_string()
}
pub fn yes() -> bool {
    true
}
