use serde::{Deserialize, Serialize};

/// Who pays for Ghost Talk traffic. `Kaspa` anchors mailbox traffic on chain
/// (fees, durable history). `NoSpend` sends the same signed/sealed mailbox
/// frames over the p2p-net swarm only: no Kaspa fees, nothing stored on chain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpendMode {
    #[default]
    Kaspa,
    NoSpend,
}

impl SpendMode {
    pub const ALL: [Self; 2] = [Self::Kaspa, Self::NoSpend];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Kaspa => "Kaspa",
            Self::NoSpend => "No-spend",
        }
    }
}

/// Canonical durable user settings shared by every frontend/runtime surface.
/// Keep this model free of UI framework types and process-local state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_route")]
    pub route: String,
    #[serde(default)]
    pub spend_mode: SpendMode,
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
            spend_mode: SpendMode::Kaspa,
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

impl Settings {
    /// The p2p-net node runs for the Auto realtime route and whenever the
    /// profile sends without spending (the swarm is then its only carrier).
    pub fn runs_swarm(&self) -> bool {
        self.route.eq_ignore_ascii_case("auto") || self.spend_mode == SpendMode::NoSpend
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

#[cfg(test)]
mod tests {
    use super::{Settings, SpendMode};

    #[test]
    fn spend_mode_defaults_to_kaspa() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.spend_mode, SpendMode::Kaspa);
        let labels = SpendMode::ALL.map(SpendMode::label);
        assert_eq!(labels, ["Kaspa", "No-spend"]);
        let wire = serde_json::to_string(&SpendMode::NoSpend).unwrap();
        assert_eq!(wire, "\"noSpend\"");
    }

    #[test]
    fn no_spend_runs_the_swarm_even_on_the_kaspa_only_route() {
        let mut settings = Settings {
            route: "Kaspa only".into(),
            ..Settings::default()
        };
        assert!(!settings.runs_swarm());
        settings.spend_mode = SpendMode::NoSpend;
        assert!(settings.runs_swarm());
        assert!(Settings::default().runs_swarm());
    }
}
