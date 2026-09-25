use ghost_media::{GhostMediaManifest, MediaReference};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastShow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub artwork: Option<MediaReference>,
    /// Creator profile id.
    pub owner: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastEpisode {
    pub id: String,
    pub show_id: String,
    pub title: String,
    pub description: String,
    pub media: MediaReference,
    #[serde(default)]
    pub manifest: Option<GhostMediaManifest>,
    #[serde(default)]
    pub tags: Vec<String>,
}
