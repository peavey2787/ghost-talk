use std::collections::BTreeMap;

use crate::model::{Chat, Contact, Profile, Room, RoomTombstone, WalletRecord};

/// Explicit mailbox mutation journal.
///
/// Mailbox handlers mark only the records/domains a decoded command may
/// change. The journal emits typed persistence deltas for those records and
/// never compares whole Profile snapshots.
pub(super) struct MailboxChangeLog {
    profile_id: String,
    chats: BTreeMap<String, Option<Chat>>,
    contacts: BTreeMap<String, Option<Contact>>,
    rooms: BTreeMap<String, Option<Room>>,
    tombstones: BTreeMap<(String, String), Option<RoomTombstone>>,
    wallet_baseline: Option<Option<WalletRecord>>,
    seen_call_signals: Vec<String>,
}

impl MailboxChangeLog {
    pub(super) fn new(profile: &Profile) -> Self {
        Self {
            profile_id: profile.id.clone(),
            chats: BTreeMap::new(),
            contacts: BTreeMap::new(),
            rooms: BTreeMap::new(),
            tombstones: BTreeMap::new(),
            wallet_baseline: None,
            seen_call_signals: Vec::new(),
        }
    }

    pub(super) fn track_chat(&mut self, profile: &Profile, chat_id: &str) {
        self.chats.entry(chat_id.to_string()).or_insert_with(|| {
            profile
                .chats
                .iter()
                .find(|chat| chat.id == chat_id)
                .cloned()
        });
    }

    pub(super) fn track_new_chat(&mut self, chat_id: &str) {
        self.chats.entry(chat_id.to_string()).or_insert(None);
    }

    pub(super) fn track_chats<I>(&mut self, profile: &Profile, ids: I)
    where
        I: IntoIterator<Item = String>,
    {
        for id in ids {
            self.track_chat(profile, &id);
        }
    }

    pub(super) fn track_contact(&mut self, profile: &Profile, contact_id: &str) {
        self.contacts
            .entry(contact_id.to_string())
            .or_insert_with(|| {
                profile
                    .contacts
                    .iter()
                    .find(|contact| contact.id == contact_id)
                    .cloned()
            });
    }

    pub(super) fn track_room(&mut self, profile: &Profile, room_id: &str) {
        self.rooms.entry(room_id.to_string()).or_insert_with(|| {
            profile
                .rooms
                .iter()
                .find(|room| room.id == room_id)
                .cloned()
        });
    }

    pub(super) fn track_tombstone(
        &mut self,
        profile: &Profile,
        room_id: &str,
        owner_hydra_id: &str,
    ) {
        let key = (room_id.to_string(), owner_hydra_id.to_string());
        self.tombstones.entry(key).or_insert_with(|| {
            profile
                .room_tombstones
                .iter()
                .find(|item| item.room_id == room_id && item.owner_hydra_id == owner_hydra_id)
                .cloned()
        });
    }

    pub(super) fn track_wallet(&mut self, profile: &Profile) {
        if self.wallet_baseline.is_none() {
            self.wallet_baseline = Some(profile.wallet.clone());
        }
    }

    pub(super) fn record_seen_call_signal(&mut self, signal_id: &str) {
        if !self.seen_call_signals.iter().any(|id| id == signal_id) {
            self.seen_call_signals.push(signal_id.to_string());
        }
    }
}

mod finish;
mod track;
mod track_helpers;
