use crate::model::{Profile, ProfilePatch};

pub(crate) fn room_patch(before: &Profile, after: &Profile, room_id: &str) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&before.id);
    append_room(&mut patch, before, after, room_id);
    patch
}

pub(crate) fn room_wallet_patch(before: &Profile, after: &Profile, room_id: &str) -> ProfilePatch {
    let mut patch = room_patch(before, after, room_id);
    patch.wallet(before.wallet.clone(), after.wallet.clone());
    patch
}

pub(crate) fn wallet_patch(before: &Profile, after: &Profile) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&before.id);
    patch.wallet(before.wallet.clone(), after.wallet.clone());
    patch
}

pub(crate) fn append_room(
    patch: &mut ProfilePatch,
    before: &Profile,
    after: &Profile,
    room_id: &str,
) {
    let old = ghost_rooms::RoomService::by_id(&before.rooms, room_id).cloned();
    match ghost_rooms::RoomService::by_id(&after.rooms, room_id).cloned() {
        Some(new) => patch.room_upsert(old, new),
        None => {
            if let Some(old) = old {
                patch.room_remove(old);
            }
        }
    }
}

pub(crate) fn append_chat(
    patch: &mut ProfilePatch,
    before: &Profile,
    after: &Profile,
    chat_id: &str,
) {
    let old = ghost_chat::ChatService::by_id(&before.chats, chat_id).cloned();
    match ghost_chat::ChatService::by_id(&after.chats, chat_id).cloned() {
        Some(new) => patch.chat_upsert(old, new),
        None => {
            if let Some(old) = old {
                patch.chat_remove(old);
            }
        }
    }
}

pub(crate) fn append_chats(
    patch: &mut ProfilePatch,
    before: &Profile,
    after: &Profile,
    chat_ids: &[String],
) {
    for chat_id in chat_ids {
        append_chat(patch, before, after, chat_id);
    }
}

pub(crate) fn peer_chat_ids(profile: &Profile, peer_hydra_id: &str) -> Vec<String> {
    profile
        .chats
        .iter()
        .filter(|chat| chat.peer_hydra_handle() == Some(peer_hydra_id))
        .map(|chat| chat.id.clone())
        .collect()
}

pub(crate) fn address_chat_ids(profile: &Profile, address: &str) -> Vec<String> {
    profile
        .chats
        .iter()
        .filter(|chat| {
            chat.peer_kaspa_address()
                .is_some_and(|peer| peer.eq_ignore_ascii_case(address))
        })
        .map(|chat| chat.id.clone())
        .collect()
}

pub(crate) fn append_tombstone(
    patch: &mut ProfilePatch,
    before: &Profile,
    after: &Profile,
    room_id: &str,
    owner_hydra_id: &str,
) {
    let old = before
        .room_tombstones
        .iter()
        .find(|item| item.room_id == room_id && item.owner_hydra_id == owner_hydra_id)
        .cloned();
    let new = after
        .room_tombstones
        .iter()
        .find(|item| item.room_id == room_id && item.owner_hydra_id == owner_hydra_id)
        .cloned();
    match (old, new) {
        (before, Some(after)) => patch.room_tombstone_upsert(before, after),
        (Some(before), None) => patch.room_tombstone_remove(before),
        (None, None) => {}
    }
}

pub(crate) fn union_chat_ids(left: Vec<String>, right: Vec<String>) -> Vec<String> {
    let mut ids = left;
    for id in right {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}
