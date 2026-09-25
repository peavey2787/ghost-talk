use crate::{CreatorProfile, PodcastEpisode, PodcastShow, StationProfile};
use serde::{Deserialize, Serialize};

/// Durable creator, station, show, and episode metadata owned by the broadcast domain.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastCatalog {
    #[serde(default)]
    creators: Vec<CreatorProfile>,
    #[serde(default)]
    stations: Vec<StationProfile>,
    #[serde(default)]
    shows: Vec<PodcastShow>,
    #[serde(default)]
    episodes: Vec<PodcastEpisode>,
}

impl BroadcastCatalog {
    pub fn creators(&self) -> &[CreatorProfile] {
        &self.creators
    }
    pub fn stations(&self) -> &[StationProfile] {
        &self.stations
    }
    pub fn shows(&self) -> &[PodcastShow] {
        &self.shows
    }
    pub fn episodes(&self) -> &[PodcastEpisode] {
        &self.episodes
    }

    pub fn upsert_creator(&mut self, value: CreatorProfile) {
        upsert_by(&mut self.creators, value, |item| item.id.as_str());
    }
    pub fn upsert_station(&mut self, value: StationProfile) {
        upsert_by(&mut self.stations, value, |item| item.id.as_str());
    }
    pub fn upsert_show(&mut self, value: PodcastShow) {
        upsert_by(&mut self.shows, value, |item| item.id.as_str());
    }
    pub fn upsert_episode(&mut self, value: PodcastEpisode) {
        upsert_by(&mut self.episodes, value, |item| item.id.as_str());
    }

    pub fn creator(&self, id: &str) -> Option<&CreatorProfile> {
        self.creators.iter().find(|item| item.id == id)
    }
    pub fn station(&self, id: &str) -> Option<&StationProfile> {
        self.stations.iter().find(|item| item.id == id)
    }
    pub fn show(&self, id: &str) -> Option<&PodcastShow> {
        self.shows.iter().find(|item| item.id == id)
    }
    pub fn episode(&self, id: &str) -> Option<&PodcastEpisode> {
        self.episodes.iter().find(|item| item.id == id)
    }
}

fn upsert_by<T, F>(items: &mut Vec<T>, value: T, id: F)
where
    F: Fn(&T) -> &str,
{
    let value_id = id(&value).to_owned();
    if let Some(existing) = items.iter_mut().find(|item| id(item) == value_id) {
        *existing = value;
    } else {
        items.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creator_upsert_replaces_by_identity() {
        let mut catalog = BroadcastCatalog::default();
        let first = CreatorProfile {
            id: "creator".into(),
            display_name: "One".into(),
            ..CreatorProfile::default()
        };
        catalog.upsert_creator(first);
        let replacement = CreatorProfile {
            id: "creator".into(),
            display_name: "Two".into(),
            ..CreatorProfile::default()
        };
        catalog.upsert_creator(replacement);
        assert_eq!(catalog.creators().len(), 1);
        assert_eq!(catalog.creators()[0].display_name, "Two");
    }
}
