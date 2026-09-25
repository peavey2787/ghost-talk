use serde::{Deserialize, Serialize};

/// Human-readable namespace supported by Ghost identity verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GhostNameNamespace {
    Kns,
    DotK,
}

/// A user-selected human-readable name. This is a claim until verified.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileName {
    pub namespace: GhostNameNamespace,
    pub name: String,
}

/// Resolver-verified binding between a human-readable name and Kaspa identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedName {
    pub namespace: GhostNameNamespace,
    pub name: String,
    pub kaspa_address: String,
    pub verified_at_ms: u64,
    pub expires_at_ms: u64,
}

impl VerifiedName {
    pub fn matches_address(&self, address: &str) -> bool {
        self.kaspa_address.eq_ignore_ascii_case(address)
    }

    pub fn is_fresh_at(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms
    }

    pub fn as_profile_name(&self) -> ProfileName {
        ProfileName {
            namespace: self.namespace,
            name: self.name.clone(),
        }
    }
}

/// Authenticated peer identity. Both identifiers are required; an address-only
/// or HYDRA-only historical record is not an authenticated peer binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerBinding {
    pub kaspa_address: String,
    pub hydra_id: String,
}

impl PeerBinding {
    pub fn new(
        kaspa_address: impl Into<String>,
        hydra_id: impl Into<String>,
    ) -> Result<Self, String> {
        let binding = Self {
            kaspa_address: kaspa_address.into().trim().to_string(),
            hydra_id: hydra_id.into().trim().to_string(),
        };
        if binding.kaspa_address.is_empty() || binding.hydra_id.is_empty() {
            return Err(
                "authenticated peer binding requires both Kaspa address and HYDRA identity".into(),
            );
        }
        Ok(binding)
    }

    pub fn from_optional(kaspa_address: Option<&str>, hydra_id: Option<&str>) -> Option<Self> {
        Self::new(kaspa_address?, hydra_id?).ok()
    }

    pub fn matches(&self, kaspa_address: &str, hydra_id: &str) -> bool {
        self.kaspa_address.eq_ignore_ascii_case(kaspa_address) && self.hydra_id == hydra_id
    }

    pub fn same_peer(&self, other: &Self) -> bool {
        self.matches(&other.kaspa_address, &other.hydra_id)
    }
}

/// Exact binding match for non-contact records such as persisted chat/room peer
/// projections. Contact resolution itself belongs to `ghost_contacts::ContactService`.
pub fn optional_binding_matches(
    stored_address: Option<&str>,
    stored_hydra_id: Option<&str>,
    peer: &PeerBinding,
) -> bool {
    PeerBinding::from_optional(stored_address, stored_hydra_id)
        .is_some_and(|stored| stored.same_peer(peer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_binding_is_required_and_address_matching_is_case_insensitive() {
        let peer = PeerBinding::new("kaspatest:abc", "hydra-a").unwrap();
        assert!(peer.matches("KASPATEST:ABC", "hydra-a"));
        assert!(!peer.matches("kaspatest:def", "hydra-a"));
        assert!(!peer.matches("kaspatest:abc", "hydra-b"));
        assert!(PeerBinding::from_optional(Some("kaspatest:abc"), None).is_none());
    }
}
