use super::{Chat, Contact, Room, RoomTombstone, Settings, WalletRecord};
use ghost_broadcast::{CreatorProfile, PodcastEpisode, PodcastShow, StationProfile};
use ghost_kasia::KasiaContactMapping;

/// Explicit durable chat transition emitted by the owning application command.
#[derive(Clone, Debug)]
pub(crate) enum ChatDelta {
    Upsert {
        before: Option<Chat>,
        after: Box<Chat>,
    },
    Remove {
        before: Chat,
    },
}

/// Explicit durable contact transition emitted by the owning application command.
#[derive(Clone, Debug)]
pub(crate) enum ContactDelta {
    Upsert {
        before: Option<Contact>,
        after: Contact,
    },
    Remove {
        before: Contact,
    },
}

/// Explicit durable room transition emitted by the owning application command.
#[derive(Clone, Debug)]
pub(crate) enum RoomDelta {
    Upsert { before: Option<Room>, after: Room },
    Remove { before: Room },
}

#[derive(Clone, Debug)]
pub(crate) enum RoomTombstoneDelta {
    Upsert { after: RoomTombstone },
    Remove { before: RoomTombstone },
}

#[derive(Clone, Debug)]
pub(crate) struct WalletDelta {
    pub(crate) before: Option<WalletRecord>,
    pub(crate) after: Option<WalletRecord>,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsDelta {
    pub(crate) before: Settings,
    pub(crate) after: Settings,
}

#[derive(Clone, Debug)]
pub(crate) enum BroadcastDelta {
    Creator(CreatorProfile),
    Station(StationProfile),
    Show(PodcastShow),
    Episode(PodcastEpisode),
}

/// One precisely-scoped application mutation. No variant contains a Profile or
/// a whole domain collection, so persistence never reconstructs intent by
/// diffing complete snapshots.
#[derive(Clone, Debug)]
pub(crate) enum ProfileDelta {
    AutoLogin(bool),
    PublicAvatar(Option<ghost_media::MediaReference>),
    KasiaContact(KasiaContactMapping),
    Broadcast(BroadcastDelta),
    Contact(ContactDelta),
    Chat(ChatDelta),
    Room(RoomDelta),
    RoomTombstone(RoomTombstoneDelta),
    SeenCallSignal(String),
    Wallet(WalletDelta),
    Settings(SettingsDelta),
}

/// Typed mutation envelope published by controllers/application services.
///
/// A patch is built at the moment semantic domain commands run. There is
/// deliberately no `between(Profile, Profile)` API: whole-profile snapshots are
/// persistence data, not an application coordination or diff mechanism.
#[derive(Clone, Debug)]
pub struct ProfilePatch {
    profile_id: String,
    deltas: Vec<ProfileDelta>,
}

impl ProfilePatch {
    pub fn new(profile_id: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
            deltas: Vec::new(),
        }
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub(crate) fn into_deltas(self) -> Vec<ProfileDelta> {
        self.deltas
    }

    pub(crate) fn auto_login(&mut self, value: bool) {
        self.deltas.push(ProfileDelta::AutoLogin(value));
    }
    pub(crate) fn public_avatar(&mut self, value: Option<ghost_media::MediaReference>) {
        self.deltas.push(ProfileDelta::PublicAvatar(value));
    }
    pub(crate) fn kasia_contact(&mut self, value: KasiaContactMapping) {
        self.deltas.push(ProfileDelta::KasiaContact(value));
    }
    pub(crate) fn creator(&mut self, value: CreatorProfile) {
        self.deltas
            .push(ProfileDelta::Broadcast(BroadcastDelta::Creator(value)));
    }
    pub(crate) fn station(&mut self, value: StationProfile) {
        self.deltas
            .push(ProfileDelta::Broadcast(BroadcastDelta::Station(value)));
    }
    pub(crate) fn show(&mut self, value: PodcastShow) {
        self.deltas
            .push(ProfileDelta::Broadcast(BroadcastDelta::Show(value)));
    }
    pub(crate) fn episode(&mut self, value: PodcastEpisode) {
        self.deltas
            .push(ProfileDelta::Broadcast(BroadcastDelta::Episode(value)));
    }
    pub(crate) fn chat_upsert(&mut self, before: Option<Chat>, after: Chat) {
        if before.as_ref() != Some(&after) {
            self.deltas.push(ProfileDelta::Chat(ChatDelta::Upsert {
                before,
                after: Box::new(after),
            }));
        }
    }
    pub(crate) fn chat_remove(&mut self, before: Chat) {
        self.deltas
            .push(ProfileDelta::Chat(ChatDelta::Remove { before }));
    }

    pub(crate) fn contact_upsert(&mut self, before: Option<Contact>, after: Contact) {
        if before.as_ref() != Some(&after) {
            self.deltas
                .push(ProfileDelta::Contact(ContactDelta::Upsert {
                    before,
                    after,
                }));
        }
    }
    pub(crate) fn contact_remove(&mut self, before: Contact) {
        self.deltas
            .push(ProfileDelta::Contact(ContactDelta::Remove { before }));
    }

    pub(crate) fn room_upsert(&mut self, before: Option<Room>, after: Room) {
        if before.as_ref() != Some(&after) {
            self.deltas
                .push(ProfileDelta::Room(RoomDelta::Upsert { before, after }));
        }
    }
    pub(crate) fn room_remove(&mut self, before: Room) {
        self.deltas
            .push(ProfileDelta::Room(RoomDelta::Remove { before }));
    }

    pub(crate) fn room_tombstone_upsert(
        &mut self,
        before: Option<RoomTombstone>,
        after: RoomTombstone,
    ) {
        if before.as_ref() != Some(&after) {
            self.deltas
                .push(ProfileDelta::RoomTombstone(RoomTombstoneDelta::Upsert {
                    after,
                }));
        }
    }
    pub(crate) fn room_tombstone_remove(&mut self, before: RoomTombstone) {
        self.deltas
            .push(ProfileDelta::RoomTombstone(RoomTombstoneDelta::Remove {
                before,
            }));
    }

    pub(crate) fn seen_call_signal(&mut self, id: String) {
        self.deltas.push(ProfileDelta::SeenCallSignal(id));
    }

    pub(crate) fn wallet(&mut self, before: Option<WalletRecord>, after: Option<WalletRecord>) {
        if before != after {
            self.deltas
                .push(ProfileDelta::Wallet(WalletDelta { before, after }));
        }
    }

    pub(crate) fn settings(&mut self, before: Settings, after: Settings) {
        if before != after {
            self.deltas
                .push(ProfileDelta::Settings(SettingsDelta { before, after }));
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }
}
