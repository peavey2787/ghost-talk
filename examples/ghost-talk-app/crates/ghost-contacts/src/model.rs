use ghost_domain::identity::PeerBinding;
use ghost_media::MediaReference;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Relationship {
    CloseFriend,
    Friend,
    Acquaintance,
    Anonymous,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContactProfileUpdate {
    pub label: String,
    pub kns_name: Option<String>,
    pub dotk_name: Option<String>,
    pub verified_public: bool,
    pub public_username: Option<String>,
    pub avatar: Option<MediaReference>,
    pub capabilities: Vec<String>,
}

/// Canonical durable Ghost Talk contact model.
///
/// The two serialized peer fields remain separate for storage compatibility,
/// while authenticated association is exposed through `PeerBinding` only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: String,
    pub label: String,
    #[serde(default)]
    kaspa_address: String,
    #[serde(default)]
    pub kns_name: Option<String>,
    #[serde(default)]
    pub dotk_name: Option<String>,
    #[serde(default)]
    hydra_handle: Option<String>,
    #[serde(default)]
    pub verified_public: bool,
    #[serde(default)]
    pub public_username: Option<String>,
    #[serde(default)]
    pub avatar: Option<MediaReference>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub blocked: bool,
}

impl Contact {
    pub fn pending(id: String, label: String, kaspa_address: String) -> Self {
        Self {
            id,
            label,
            kaspa_address,
            ..Self::default()
        }
    }

    pub fn authenticated(id: String, label: String, peer: &PeerBinding) -> Self {
        let mut contact = Self {
            id,
            label,
            ..Self::default()
        };
        contact.set_peer_binding(peer);
        contact
    }

    pub fn kaspa_address(&self) -> &str {
        &self.kaspa_address
    }
    pub fn hydra_handle(&self) -> Option<&str> {
        self.hydra_handle.as_deref()
    }

    pub fn peer_binding(&self) -> Option<PeerBinding> {
        PeerBinding::new(
            self.kaspa_address.clone(),
            self.hydra_handle.as_deref()?.to_owned(),
        )
        .ok()
    }

    pub fn matches_peer(&self, peer: &PeerBinding) -> bool {
        self.peer_binding()
            .as_ref()
            .is_some_and(|stored| stored.matches(&peer.kaspa_address, &peer.hydra_id))
    }

    pub(crate) fn set_peer_binding(&mut self, peer: &PeerBinding) {
        self.kaspa_address = peer.kaspa_address.clone();
        self.hydra_handle = Some(peer.hydra_id.clone());
    }

    pub(crate) fn apply_public_profile(&mut self, update: ContactProfileUpdate) {
        self.label = update.label;
        self.kns_name = update.kns_name;
        self.dotk_name = update.dotk_name;
        self.verified_public = update.verified_public;
        self.public_username = update.public_username;
        self.avatar = update.avatar;
        self.capabilities = update.capabilities;
    }

    pub(crate) fn restore_missing_identity(&mut self, label: String, hydra_handle: Option<String>) {
        if self.label.is_empty() {
            self.label = label;
        }
        if self.hydra_handle.is_none() {
            self.hydra_handle = hydra_handle;
        }
    }

    pub(crate) fn reconcile_identity_fields(&mut self, before: &Self, after: &Self) {
        if before.kaspa_address != after.kaspa_address {
            self.kaspa_address = after.kaspa_address.clone();
        }
        if before.hydra_handle != after.hydra_handle {
            self.hydra_handle = after.hydra_handle.clone();
        }
    }

    #[cfg(test)]
    pub(crate) fn clear_hydra_binding_for_test(&mut self) {
        self.hydra_handle = None;
    }
}
