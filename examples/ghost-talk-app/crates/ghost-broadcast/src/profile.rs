use ghost_media::MediaReference;
use serde::{Deserialize, Serialize};

/// Durable creator identity backed by the owning Ghost/Kaspa profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorProfile {
    pub id: String,
    pub display_name: String,
    pub owner_kaspa_address: String,
    pub verified_name: Option<String>,
    pub avatar: Option<MediaReference>,
    pub description: String,
}

/// A station references its creator instead of embedding another creator snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationProfile {
    pub id: String,
    pub name: String,
    pub artwork: Option<MediaReference>,
    pub description: String,
    pub owner: String,
    #[serde(default)]
    pub shows: Vec<String>,
    /// Authoritative presenter roles and live state are projected from this Room.
    pub live_room_id: Option<String>,
}
