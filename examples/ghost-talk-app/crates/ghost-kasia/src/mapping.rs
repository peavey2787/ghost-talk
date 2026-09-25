use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KasiaContactMapping {
    pub ghost_contact_id: String,
    pub kaspa_address: String,
    pub our_alias: String,
    #[serde(default)]
    pub their_alias: Option<String>,
    pub conversation_id: String,
    #[serde(default)]
    pub established: bool,
}

impl KasiaContactMapping {
    pub fn pending(ghost_contact_id: impl Into<String>, kaspa_address: impl Into<String>) -> Self {
        Self {
            ghost_contact_id: ghost_contact_id.into(),
            kaspa_address: kaspa_address.into(),
            our_alias: random_route_id(),
            their_alias: None,
            conversation_id: random_route_id(),
            established: false,
        }
    }

    pub fn apply_peer_handshake(&mut self, alias: Option<&str>, conversation_id: Option<&str>) {
        if let Some(alias) = normalized(alias) {
            self.their_alias = Some(alias);
        }
        if let Some(conversation_id) = normalized(conversation_id) {
            self.conversation_id = conversation_id;
        }
        self.established = self.their_alias.is_some();
    }
}

/// Durable mapping between Ghost contacts and their Kasia-compatible routing identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KasiaContactMap(BTreeMap<String, KasiaContactMapping>);

impl KasiaContactMap {
    pub fn upsert(&mut self, mapping: KasiaContactMapping) {
        self.0.insert(mapping.ghost_contact_id.clone(), mapping);
    }
    pub fn by_contact(&self, id: &str) -> Option<&KasiaContactMapping> {
        self.0.get(id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &KasiaContactMapping> {
        self.0.values()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

fn random_route_id() -> String {
    let mut bytes = [0u8; 6];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_handshake_establishes_route() {
        let mut mapping = KasiaContactMapping::pending("contact", "kaspa:qpeer");
        assert_eq!(mapping.our_alias.len(), 12);
        mapping.apply_peer_handshake(Some("abc123"), Some("conversation"));
        assert_eq!(mapping.their_alias.as_deref(), Some("abc123"));
        assert_eq!(mapping.conversation_id, "conversation");
        assert!(mapping.established);
    }
}
