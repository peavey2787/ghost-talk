use ghost_contacts::ContactService;
use ghost_rooms::{RoomService, RoomTombstoneService};

use crate::model::{
    BroadcastDelta, ChatDelta, ContactDelta, Profile, ProfileDelta, ProfilePatch, RoomDelta,
    RoomTombstoneDelta, Settings, WalletStateService,
};

/// Apply one typed command result to the latest authoritative profile.
///
/// Every mutation was already identified by its owning service/controller.
/// Persistence reconciles those explicit transitions; it never compares two
/// complete Profile snapshots to infer application intent.
pub(crate) fn apply_profile_patch(
    latest: &Profile,
    patch: ProfilePatch,
) -> Result<Profile, String> {
    if latest.id != patch.profile_id() {
        return Err("profile update belongs to a different Ghost Talk ID".into());
    }
    let mut merged = latest.clone();
    for delta in patch.into_deltas() {
        apply_delta(&mut merged, delta);
    }
    merged.advance_state_revision_after(latest.state_revision());
    Ok(merged)
}

fn apply_delta(profile: &mut Profile, delta: ProfileDelta) {
    match delta {
        ProfileDelta::AutoLogin(value) => profile.set_auto_login(value),
        ProfileDelta::PublicAvatar(value) => profile.set_public_avatar(value),
        ProfileDelta::Settings(delta) => {
            profile.settings = merge_settings(&delta.before, &delta.after, &profile.settings);
        }
        other => apply_owner_delta(profile, other),
    }
}

fn apply_owner_delta(profile: &mut Profile, delta: ProfileDelta) {
    match delta {
        ProfileDelta::Contact(delta) => apply_contact_delta(profile, delta),
        ProfileDelta::Chat(delta) => apply_chat_delta(profile, delta),
        ProfileDelta::Room(delta) => apply_room_delta(profile, delta),
        ProfileDelta::RoomTombstone(delta) => apply_tombstone_delta(profile, delta),
        other => apply_extension_delta(profile, other),
    }
}

fn apply_extension_delta(profile: &mut Profile, delta: ProfileDelta) {
    match delta {
        ProfileDelta::KasiaContact(value) => profile.upsert_kasia_contact(value),
        ProfileDelta::Broadcast(delta) => apply_broadcast_delta(profile, delta),
        other => apply_runtime_delta(profile, other),
    }
}

fn apply_runtime_delta(profile: &mut Profile, delta: ProfileDelta) {
    match delta {
        ProfileDelta::SeenCallSignal(id) => {
            profile.record_seen_call_signal(id);
        }
        ProfileDelta::Wallet(delta) => {
            WalletStateService::reconcile(&mut profile.wallet, delta.before.as_ref(), delta.after);
        }
        _ => unreachable!("ProfileDelta routed to the wrong persistence owner"),
    }
}

fn apply_broadcast_delta(profile: &mut Profile, delta: BroadcastDelta) {
    match delta {
        BroadcastDelta::Creator(value) => profile.upsert_creator(value),
        BroadcastDelta::Station(value) => profile.upsert_station(value),
        BroadcastDelta::Show(value) => profile.upsert_show(value),
        BroadcastDelta::Episode(value) => profile.upsert_episode(value),
    }
}

fn apply_contact_delta(profile: &mut Profile, delta: ContactDelta) {
    match delta {
        ContactDelta::Upsert { before, after } => {
            ContactService::reconcile_upsert(&mut profile.contacts, before.as_ref(), after)
        }
        ContactDelta::Remove { before } => {
            ContactService::remove_if_unchanged(&mut profile.contacts, &before);
        }
    }
}

fn apply_chat_delta(profile: &mut Profile, delta: ChatDelta) {
    match delta {
        ChatDelta::Upsert { before, after } => {
            ghost_chat::ChatService::reconcile_upsert(&mut profile.chats, before.as_ref(), *after)
        }
        ChatDelta::Remove { before } => {
            ghost_chat::ChatService::remove_if_unchanged(&mut profile.chats, &before);
        }
    }
}

fn apply_room_delta(profile: &mut Profile, delta: RoomDelta) {
    match delta {
        RoomDelta::Upsert { before, after } => {
            let tombstoned = RoomTombstoneService::revision(
                &profile.room_tombstones,
                &after.id,
                &after.owner_hydra_id,
            )
            .is_some_and(|revision| after.revision() <= revision);
            if !tombstoned || profile.rooms.iter().any(|room| room.id == after.id) {
                RoomService::reconcile_upsert(&mut profile.rooms, before.as_ref(), after);
            }
        }
        RoomDelta::Remove { before } => {
            RoomService::remove_if_unchanged(&mut profile.rooms, &before);
        }
    }
}

fn apply_tombstone_delta(profile: &mut Profile, delta: RoomTombstoneDelta) {
    match delta {
        RoomTombstoneDelta::Upsert { after } => RoomTombstoneService::record(
            &mut profile.room_tombstones,
            &after.room_id,
            &after.owner_hydra_id,
            after.revision(),
        ),
        RoomTombstoneDelta::Remove { before } => RoomTombstoneService::clear(
            &mut profile.room_tombstones,
            &before.room_id,
            &before.owner_hydra_id,
        ),
    }
}

fn merge_settings(before: &Settings, after: &Settings, latest: &Settings) -> Settings {
    let mut merged = latest.clone();
    macro_rules! field {
        ($name:ident) => {
            if before.$name != after.$name {
                merged.$name = after.$name.clone();
            }
        };
    }
    field!(route);
    field!(stego);
    field!(text_route);
    field!(contacts_backup_kaspa);
    field!(backup_messages_kaspa);
    field!(require_send_password);
    field!(auto_ignore_unknown_chats);
    field!(debug_logging);
    field!(public_username);
    field!(public_description);
    field!(public_interests);
    field!(p2p_bootstrap_peers);
    field!(p2p_relay_peers);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_patch_does_not_roll_back_concurrent_contact_change() {
        let before = Profile::new("p".into(), "Profile".into());
        let chat = ghost_chat::ChatService::new_basic("chat".into(), "Peer".into());
        let mut patch = ProfilePatch::new(&before.id);
        patch.chat_upsert(None, chat);

        let mut latest = before.clone();
        ContactService::push(
            &mut latest.contacts,
            ContactService::new_contact("contact".into(), "Saved".into(), String::new(), None),
        );
        let merged = apply_profile_patch(&latest, patch).unwrap();
        assert_eq!(merged.contacts.len(), 1);
        assert_eq!(merged.chats.len(), 1);
    }
}
