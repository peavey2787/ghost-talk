use ghost_domain::identity::{ProfileName, VerifiedName};
use ghost_media::MediaReference;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRouteRegistration {
    pub contact_id: String,
    pub kaspa_address: String,
    pub display_name: String,
    pub session_sid: Option<String>,
    #[serde(default)]
    pub session_role: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub resume_required: bool,
}

impl PeerRouteRegistration {
    pub fn session_sid(&self) -> Option<&str> {
        self.session_sid.as_deref()
    }

    pub fn session_role(&self) -> Option<&str> {
        self.session_role.as_deref()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicGhostProfile {
    #[serde(default)]
    pub primary_name: Option<ProfileName>,
    #[serde(default)]
    pub verified_names: Vec<VerifiedName>,
    #[serde(default)]
    pub avatar: Option<MediaReference>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub signature: String,
    pub kaspa_address: String,
    #[serde(default)]
    pub display_name: String,
    pub hydra_identity_id: String,
    pub descriptor_blue_score: String,
    #[serde(default)]
    pub kns_name: Option<String>,
    #[serde(default)]
    pub dotk_name: Option<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub interests: Vec<String>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub discoverable: bool,
}

impl PublicGhostProfile {
    pub fn verified_primary_name(&self) -> Option<&str> {
        let primary = self.primary_name.as_ref()?;
        self.verified_names
            .iter()
            .any(|verified| {
                verified.namespace == primary.namespace
                    && verified.name.eq_ignore_ascii_case(&primary.name)
                    && verified.matches_address(&self.kaspa_address)
            })
            .then_some(primary.name.as_str())
    }

    pub fn preferred_name(&self) -> Option<&str> {
        self.verified_primary_name()
            .or(self.kns_name.as_deref())
            .or(self.dotk_name.as_deref())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedGhostPeer {
    pub kaspa_address: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub hydra_handle: Option<String>,
    #[serde(default)]
    pub descriptor_blue_score: Option<String>,
    #[serde(default)]
    pub kns_name: Option<String>,
    #[serde(default)]
    pub dotk_name: Option<String>,
    #[serde(default)]
    pub verified_public: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub interests: Vec<String>,
    #[serde(default)]
    pub avatar: Option<MediaReference>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}
