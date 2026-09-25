use ghost_api::PublicGhostProfile;
use ghost_broadcast::{
    BroadcastCatalog, CreatorProfile, PodcastEpisode, PodcastShow, StationProfile,
};
use ghost_chat::ChatStore;
use ghost_contacts::ContactStore;
use ghost_domain::{call::MAX_SEEN_CALL_SIGNAL_IDS, settings::Settings};
use ghost_kasia::KasiaContactMap;
use ghost_rooms::{RoomStore, RoomTombstone};
use serde::{Deserialize, Serialize};

use crate::WalletRecord;

/// Canonical durable Ghost Talk profile. Process-local call/media/network-task
/// state is intentionally absent and reconstructed after unlock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub label: String,
    #[serde(default)]
    state_revision: u64,
    #[serde(default)]
    auto_login: bool,
    #[serde(default)]
    recovery_backup_confirmed: bool,
    #[serde(default)]
    pub hydra_identity_id: Option<String>,
    #[serde(default)]
    pub contacts: ContactStore,
    #[serde(default)]
    pub chats: ChatStore,
    #[serde(default)]
    pub rooms: RoomStore,
    #[serde(default)]
    pub room_tombstones: Vec<RoomTombstone>,
    #[serde(default)]
    pub public_directory: Vec<PublicGhostProfile>,
    #[serde(default)]
    public_avatar: Option<ghost_media::MediaReference>,
    #[serde(default)]
    kasia_contacts: KasiaContactMap,
    #[serde(default)]
    broadcast_catalog: BroadcastCatalog,
    #[serde(default)]
    seen_call_signal_ids: Vec<String>,
    #[serde(default)]
    pub wallet: Option<WalletRecord>,
    #[serde(default)]
    pub settings: Settings,
}

impl Profile {
    pub fn new(id: String, label: String) -> Self {
        Self {
            id,
            label: if label.trim().is_empty() {
                "User".into()
            } else {
                label.trim().into()
            },
            state_revision: 0,
            auto_login: false,
            recovery_backup_confirmed: false,
            hydra_identity_id: None,
            contacts: ContactStore::new(),
            chats: ChatStore::new(),
            rooms: RoomStore::new(),
            room_tombstones: Vec::new(),
            public_directory: Vec::new(),
            public_avatar: None,
            kasia_contacts: KasiaContactMap::default(),
            broadcast_catalog: BroadcastCatalog::default(),
            seen_call_signal_ids: Vec::new(),
            wallet: None,
            settings: Settings::default(),
        }
    }
    /// Monotonic durable-state revision used to reconcile persisted snapshots.
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Raises the durable-state revision to at least `minimum`.
    pub fn ensure_state_revision_at_least(&mut self, minimum: u64) {
        self.state_revision = self.state_revision.max(minimum);
    }

    /// Advances the durable-state revision beyond both the current and previous values.
    pub fn advance_state_revision_after(&mut self, previous: u64) {
        self.state_revision = self.state_revision.max(previous).saturating_add(1);
    }

    /// Returns whether this profile is configured for automatic local unlock.
    pub const fn auto_login(&self) -> bool {
        self.auto_login
    }

    /// Changes the automatic local-unlock preference.
    pub fn set_auto_login(&mut self, enabled: bool) {
        self.auto_login = enabled;
    }

    /// Returns whether the user has confirmed recovery-backup creation.
    pub const fn recovery_backup_confirmed(&self) -> bool {
        self.recovery_backup_confirmed
    }

    /// Records whether recovery-backup creation has been confirmed.
    pub fn set_recovery_backup_confirmed(&mut self, confirmed: bool) {
        self.recovery_backup_confirmed = confirmed;
    }

    /// Content-addressed avatar committed by the signed public profile descriptor.
    pub fn public_avatar(&self) -> Option<&ghost_media::MediaReference> {
        self.public_avatar.as_ref()
    }

    /// Replaces the public avatar reference through a typed profile mutation.
    pub fn set_public_avatar(&mut self, avatar: Option<ghost_media::MediaReference>) {
        self.public_avatar = avatar;
    }

    /// Read-only Kasia contact mappings. Mutations stay inside the compatibility domain.
    pub fn kasia_contacts(&self) -> &KasiaContactMap {
        &self.kasia_contacts
    }

    /// Replaces a Kasia mapping through its domain-owned map.
    pub fn upsert_kasia_contact(&mut self, mapping: ghost_kasia::KasiaContactMapping) {
        self.kasia_contacts.upsert(mapping);
    }

    /// Read-only creator/station/show/episode catalog.
    pub fn broadcast_catalog(&self) -> &BroadcastCatalog {
        &self.broadcast_catalog
    }

    /// Adds or replaces one creator profile through the broadcast domain owner.
    pub fn upsert_creator(&mut self, value: CreatorProfile) {
        self.broadcast_catalog.upsert_creator(value);
    }

    /// Adds or replaces one station profile through the broadcast domain owner.
    pub fn upsert_station(&mut self, value: StationProfile) {
        self.broadcast_catalog.upsert_station(value);
    }

    /// Adds or replaces one podcast show through the broadcast domain owner.
    pub fn upsert_show(&mut self, value: PodcastShow) {
        self.broadcast_catalog.upsert_show(value);
    }

    /// Adds or replaces one podcast episode through the broadcast domain owner.
    pub fn upsert_episode(&mut self, value: PodcastEpisode) {
        self.broadcast_catalog.upsert_episode(value);
    }

    /// Returns whether a signed call signal has already been processed.
    pub fn has_seen_call_signal(&self, signal_id: &str) -> bool {
        self.seen_call_signal_ids.iter().any(|id| id == signal_id)
    }

    /// Records a processed call signal while enforcing the bounded replay cache.
    pub fn record_seen_call_signal(&mut self, signal_id: String) -> bool {
        if self.has_seen_call_signal(&signal_id) {
            return false;
        }
        self.seen_call_signal_ids.push(signal_id);
        if self.seen_call_signal_ids.len() > MAX_SEEN_CALL_SIGNAL_IDS {
            let excess = self.seen_call_signal_ids.len() - MAX_SEEN_CALL_SIGNAL_IDS;
            self.seen_call_signal_ids.drain(0..excess);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::Profile;

    #[test]
    fn durable_profile_contains_no_live_call_state() {
        let json = serde_json::to_value(Profile::new("id".into(), "User".into())).unwrap();
        let object = json.as_object().unwrap();
        assert!(!object.contains_key("calls"));
        assert!(!object.contains_key("activeCall"));
        assert!(!object.contains_key("muted"));
    }
}
