use super::MailboxChangeLog;
use crate::model::{Profile, ProfilePatch};

impl MailboxChangeLog {
    pub(crate) fn finish(self, profile: &Profile) -> ProfilePatch {
        let mut patch = ProfilePatch::new(self.profile_id.clone());
        self.append_tracked_chats(profile, &mut patch);
        self.append_tracked_contacts(profile, &mut patch);
        self.append_tracked_rooms(profile, &mut patch);
        self.append_tracked_tombstones(profile, &mut patch);
        self.append_scalar_domains(profile, &mut patch);
        patch
    }

    fn append_tracked_chats(&self, profile: &Profile, patch: &mut ProfilePatch) {
        for (id, before) in &self.chats {
            let after = profile.chats.iter().find(|chat| chat.id == *id).cloned();
            append_chat_delta(patch, before.clone(), after);
        }
    }

    fn append_tracked_contacts(&self, profile: &Profile, patch: &mut ProfilePatch) {
        for (id, before) in &self.contacts {
            let after = profile
                .contacts
                .iter()
                .find(|contact| contact.id == *id)
                .cloned();
            append_contact_delta(patch, before.clone(), after);
        }
    }

    fn append_tracked_rooms(&self, profile: &Profile, patch: &mut ProfilePatch) {
        for (id, before) in &self.rooms {
            let after = profile.rooms.iter().find(|room| room.id == *id).cloned();
            append_room_delta(patch, before.clone(), after);
        }
    }

    fn append_tracked_tombstones(&self, profile: &Profile, patch: &mut ProfilePatch) {
        for ((room_id, owner_hydra_id), before) in &self.tombstones {
            let after = profile
                .room_tombstones
                .iter()
                .find(|item| item.room_id == *room_id && item.owner_hydra_id == *owner_hydra_id)
                .cloned();
            append_tombstone_delta(patch, before.clone(), after);
        }
    }

    fn append_scalar_domains(&self, profile: &Profile, patch: &mut ProfilePatch) {
        if let Some(before) = &self.wallet_baseline {
            patch.wallet(before.clone(), profile.wallet.clone());
        }
        for signal_id in &self.seen_call_signals {
            patch.seen_call_signal(signal_id.clone());
        }
    }
}

fn append_chat_delta(
    patch: &mut ProfilePatch,
    before: Option<crate::model::Chat>,
    after: Option<crate::model::Chat>,
) {
    match (before, after) {
        (Some(before), Some(after)) => patch.chat_upsert(Some(before), after),
        (None, Some(after)) => patch.chat_upsert(None, after),
        (Some(before), None) => patch.chat_remove(before),
        (None, None) => {}
    }
}

fn append_contact_delta(
    patch: &mut ProfilePatch,
    before: Option<crate::model::Contact>,
    after: Option<crate::model::Contact>,
) {
    match (before, after) {
        (Some(before), Some(after)) => patch.contact_upsert(Some(before), after),
        (None, Some(after)) => patch.contact_upsert(None, after),
        (Some(before), None) => patch.contact_remove(before),
        (None, None) => {}
    }
}

fn append_room_delta(
    patch: &mut ProfilePatch,
    before: Option<crate::model::Room>,
    after: Option<crate::model::Room>,
) {
    match (before, after) {
        (Some(before), Some(after)) => patch.room_upsert(Some(before), after),
        (None, Some(after)) => patch.room_upsert(None, after),
        (Some(before), None) => patch.room_remove(before),
        (None, None) => {}
    }
}

fn append_tombstone_delta(
    patch: &mut ProfilePatch,
    before: Option<crate::model::RoomTombstone>,
    after: Option<crate::model::RoomTombstone>,
) {
    match (before, after) {
        (Some(before), Some(after)) => patch.room_tombstone_upsert(Some(before), after),
        (None, Some(after)) => patch.room_tombstone_upsert(None, after),
        (Some(before), None) => patch.room_tombstone_remove(before),
        (None, None) => {}
    }
}
