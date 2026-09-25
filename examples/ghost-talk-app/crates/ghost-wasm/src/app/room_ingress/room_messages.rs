mod reactions;
pub(crate) use reactions::handle_room_reaction;

use super::super::state::{native, room_state_wire};
use crate::model::{ChatStore, Profile, Room, RoomMember, RoomMessage, RoomWire};
use ghost_rooms::RoomStore;

pub(crate) async fn handle_room_message(
    mut profile: Profile,
    password: &str,
    from: &str,
    room_id: String,
    message: RoomMessage,
) -> Result<Profile, String> {
    let Some(index) = receivable_room_index(&profile.rooms, &room_id, &message.id) else {
        return Ok(profile);
    };
    if local_owns_room(&profile, index) {
        profile = relay_owner_room_message(profile, password, from, index, room_id, message).await;
    } else {
        accept_member_room_message(&mut profile.rooms, from, index, message);
    }
    Ok(profile)
}

pub(crate) fn receivable_room_index(
    rooms: &RoomStore,
    room_id: &str,
    message_id: &str,
) -> Option<usize> {
    rooms.iter().position(|room| {
        room.id == room_id
            && !room.pending_acceptance()
            && !room
                .messages()
                .iter()
                .any(|existing| existing.id == message_id)
    })
}

pub(crate) fn local_owns_room(profile: &Profile, index: usize) -> bool {
    profile.hydra_identity_id.as_deref() == Some(profile.rooms[index].owner_hydra_id.as_str())
}

pub(crate) async fn relay_owner_room_message(
    mut profile: Profile,
    password: &str,
    from: &str,
    index: usize,
    room_id: String,
    message: RoomMessage,
) -> Profile {
    if !authorized_room_sender(&profile.rooms[index], from, &message) {
        return profile;
    }
    let _ =
        ghost_rooms::RoomService::add_message_at_index(&mut profile.rooms, index, message.clone());
    let payload = RoomWire::Message { room_id, message };
    let routes = other_room_members(&profile.rooms[index], from);
    let broadcast = crate::controllers::room::broadcast_room_wire_to_members(
        &profile,
        password,
        &routes,
        &payload,
        Some(from),
    )
    .await;
    broadcast.apply_wallet(&mut profile.wallet);
    profile
}

pub(crate) fn authorized_room_sender(room: &Room, from: &str, message: &RoomMessage) -> bool {
    message.sender_hydra_id == from
        && room
            .members()
            .iter()
            .any(|member| member.hydra_handle.as_deref() == Some(from))
}

pub(crate) fn other_room_members(room: &Room, from: &str) -> Vec<RoomMember> {
    room.members()
        .iter()
        .filter(|member| {
            member
                .hydra_handle
                .as_deref()
                .is_some_and(|handle| handle != from)
        })
        .cloned()
        .collect()
}

pub(crate) fn accept_member_room_message(
    rooms: &mut RoomStore,
    from: &str,
    index: usize,
    message: RoomMessage,
) {
    if from == rooms[index].owner_hydra_id {
        let _ = ghost_rooms::RoomService::add_message_at_index(rooms, index, message);
    }
}

pub(crate) async fn handle_room_leave(
    mut profile: Profile,
    password: &str,
    from: &str,
    room_id: String,
    member_hydra_id: String,
    member_kaspa_address: String,
) -> Result<Profile, String> {
    let Some(index) = profile.rooms.iter().position(|room| room.id == room_id) else {
        return Ok(profile);
    };
    if !valid_room_leave(
        &profile,
        index,
        from,
        &member_hydra_id,
        &member_kaspa_address,
    ) {
        return Ok(profile);
    }
    let Some(updated_room) = remove_departed_member(&mut profile.rooms, index, &member_hydra_id)
    else {
        return Ok(profile);
    };
    let state = room_state_wire(&updated_room);
    let members = updated_room.members().to_vec();
    let broadcast = crate::controllers::room::broadcast_room_wire_to_members(
        &profile,
        password,
        &members,
        &state,
        Some(from),
    )
    .await;
    broadcast.apply_wallet(&mut profile.wallet);
    retire_room_peer_if_unused(
        &profile.id,
        &profile.rooms,
        profile.hydra_identity_id.as_deref(),
        &mut profile.chats,
        &member_hydra_id,
    )
    .await;
    Ok(profile)
}

fn remove_departed_member(
    rooms: &mut RoomStore,
    index: usize,
    member_hydra_id: &str,
) -> Option<Room> {
    let room_id = rooms[index].id.clone();
    super::super::ApplicationRouter::route_room(
        rooms,
        super::super::ApplicationEvent::RoomMemberRemoved {
            room_id,
            peer_hydra_id: member_hydra_id.to_owned(),
        },
    )
}

pub(crate) fn valid_room_leave(
    profile: &Profile,
    index: usize,
    from: &str,
    member_hydra_id: &str,
    member_address: &str,
) -> bool {
    let room = &profile.rooms[index];
    let local_is_owner = profile.hydra_identity_id.as_deref() == Some(room.owner_hydra_id.as_str());
    local_is_owner
        && from == member_hydra_id
        && room.members().iter().any(|member| {
            member.hydra_handle.as_deref() == Some(member_hydra_id)
                && member.kaspa_address.eq_ignore_ascii_case(member_address)
        })
}

pub(crate) async fn handle_room_removal(
    mut profile: Profile,
    from: &str,
    room_id: &str,
    revision: u64,
    target_hydra_id: Option<&str>,
    target_kaspa_address: &str,
) -> Profile {
    let Some(room) = authoritative_room_control(&profile, from, room_id, revision) else {
        return profile;
    };
    if room_control_targets_local(&profile, target_hydra_id, target_kaspa_address) {
        profile = remove_local_room(profile, &room, revision).await;
    }
    profile
}

pub(crate) async fn handle_room_disband(
    profile: Profile,
    from: &str,
    room_id: &str,
    revision: u64,
) -> Profile {
    let Some(room) = authoritative_room_control(&profile, from, room_id, revision) else {
        return profile;
    };
    remove_local_room(profile, &room, revision).await
}

pub(crate) fn authoritative_room_control(
    profile: &Profile,
    from: &str,
    room_id: &str,
    revision: u64,
) -> Option<Room> {
    profile
        .rooms
        .iter()
        .find(|room| {
            room.id == room_id && from == room.owner_hydra_id && revision >= room.revision()
        })
        .cloned()
}

pub(crate) async fn remove_local_room(mut profile: Profile, room: &Room, revision: u64) -> Profile {
    ghost_rooms::RoomTombstoneService::record(
        &mut profile.room_tombstones,
        &room.id,
        &room.owner_hydra_id,
        revision,
    );
    ghost_rooms::RoomService::remove_by_id(&mut profile.rooms, &room.id);
    retire_room_peer_if_unused(
        &profile.id,
        &profile.rooms,
        profile.hydra_identity_id.as_deref(),
        &mut profile.chats,
        &room.owner_hydra_id,
    )
    .await;
    profile
}

pub(crate) async fn retire_room_peer_if_unused(
    profile_id: &str,
    rooms: &RoomStore,
    local_hydra_id: Option<&str>,
    chats: &mut ChatStore,
    peer_hydra_id: &str,
) {
    if ghost_rooms::RoomService::peer_needed_by_room(rooms, local_hydra_id, peer_hydra_id) {
        return;
    }
    if ghost_chat::ChatService::retire_room_transports_for_peer(chats, peer_hydra_id) {
        let _ = native::leave_peer(profile_id, peer_hydra_id).await;
    }
}

pub(crate) fn room_control_targets_local(
    profile: &Profile,
    target_hydra_id: Option<&str>,
    target_kaspa_address: &str,
) -> bool {
    if target_hydra_id.is_some_and(|target| profile.hydra_identity_id.as_deref() == Some(target)) {
        return true;
    }
    profile.wallet.as_ref().is_some_and(|wallet| {
        wallet
            .public
            .receive_addresses
            .iter()
            .chain(wallet.public.change_addresses.iter())
            .any(|address| address.eq_ignore_ascii_case(target_kaspa_address))
    })
}
