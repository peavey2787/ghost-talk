use crate::model::{Profile, ProfilePatch, Room, RoomBan, RoomMember, RoomWire};

use super::lifecycle::retire_peer_transport;
use super::{
    append_chats, append_room, broadcast_room_wire_to_members, peer_chat_ids,
    remove_pending_room_payloads, room_member_session_established, room_state_wire,
    send_room_wire_to_member, union_chat_ids, wallet_patch,
};

pub(crate) async fn remove_member(
    profile: &Profile,
    password: &str,
    room: &Room,
    address: &str,
) -> Result<(String, ProfilePatch), String> {
    moderate_member(profile, password, room, address, Moderation::Kick).await
}

pub(crate) async fn ban_member_command(
    profile: &Profile,
    password: &str,
    room: &Room,
    address: &str,
    expires_at: Option<f64>,
) -> Result<(String, ProfilePatch), String> {
    moderate_member(
        profile,
        password,
        room,
        address,
        Moderation::Ban(expires_at),
    )
    .await
}

enum Moderation {
    Kick,
    Ban(Option<f64>),
}

async fn moderate_member(
    profile: &Profile,
    password: &str,
    room: &Room,
    address: &str,
    action: Moderation,
) -> Result<(String, ProfilePatch), String> {
    let member = find_member(room, address)?;
    let before = profile.clone();
    let mut current = before.clone();
    let (changed, direct) = apply_moderation(&mut current.rooms, room, &member, action)?;
    let state = room_state_wire(&changed);
    let mut affected = affected_chat_ids(&mut current.chats, &before, room, &member);
    current = notify_removed_member(current, password, room, &member, &direct, &state).await;
    if let Some(peer) = member.hydra_handle.as_deref() {
        current = retire_peer_transport(current, peer).await;
        affected = union_chat_ids(affected, peer_chat_ids(&current, peer));
    }
    let patch = moderation_patch(&before, &current, &room.id, &affected);
    Ok((member.label, patch))
}

fn find_member(room: &Room, address: &str) -> Result<RoomMember, String> {
    room.members()
        .iter()
        .find(|member| member.kaspa_address.eq_ignore_ascii_case(address))
        .cloned()
        .ok_or_else(|| "Room member is no longer present.".to_string())
}

fn apply_moderation(
    rooms: &mut crate::model::RoomStore,
    room: &Room,
    member: &RoomMember,
    action: Moderation,
) -> Result<(Room, RoomWire), String> {
    let changed = match action {
        Moderation::Kick => ghost_rooms::RoomService::remove_member_by_address(
            rooms,
            &room.id,
            &member.kaspa_address,
        ),
        Moderation::Ban(expires_at) => ghost_rooms::RoomService::ban_member(
            rooms,
            &room.id,
            RoomBan {
                label: member.label.clone(),
                kaspa_address: member.kaspa_address.clone(),
                hydra_handle: member.hydra_handle.clone(),
                expires_at,
            },
        ),
    }
    .ok_or_else(|| "Room no longer exists.".to_string())?;
    let revision = changed.revision();
    let direct = match action {
        Moderation::Kick => RoomWire::Kick {
            room_id: room.id.clone(),
            revision,
            target_hydra_id: member.hydra_handle.clone(),
            target_kaspa_address: member.kaspa_address.clone(),
        },
        Moderation::Ban(expires_at) => RoomWire::Ban {
            room_id: room.id.clone(),
            revision,
            target_hydra_id: member.hydra_handle.clone(),
            target_kaspa_address: member.kaspa_address.clone(),
            expires_at,
        },
    };
    Ok((changed, direct))
}

fn affected_chat_ids(
    chats: &mut crate::model::ChatStore,
    before: &Profile,
    room: &Room,
    member: &RoomMember,
) -> Vec<String> {
    let mut ids = remove_pending_room_payloads(chats, &room.id, Some(&member.kaspa_address));
    if let Some(peer) = member.hydra_handle.as_deref() {
        ids = union_chat_ids(ids, peer_chat_ids(before, peer));
    }
    ids
}

async fn notify_removed_member(
    mut current: Profile,
    password: &str,
    room: &Room,
    member: &RoomMember,
    direct: &RoomWire,
    state: &RoomWire,
) -> Profile {
    if room_member_session_established(&current, member) {
        if let Ok(progress) = send_room_wire_to_member(&current, password, member, direct).await {
            crate::model::WalletStateService::merge_progress(&mut current.wallet, progress);
        }
    }
    let remaining = current
        .rooms
        .iter()
        .find(|candidate| candidate.id == room.id)
        .map(|value| value.members().to_vec())
        .unwrap_or_default();
    let broadcast =
        broadcast_room_wire_to_members(&current, password, &remaining, state, None).await;
    if let Some(progress) = broadcast.wallet_progress {
        crate::model::WalletStateService::merge_progress(&mut current.wallet, progress);
    }
    current
}

fn moderation_patch(
    before: &Profile,
    current: &Profile,
    room_id: &str,
    affected: &[String],
) -> ProfilePatch {
    let mut patch = wallet_patch(before, current);
    append_room(&mut patch, before, current, room_id);
    append_chats(&mut patch, before, current, affected);
    patch
}

pub(crate) async fn unban_member(
    profile: &Profile,
    password: &str,
    room: &Room,
    address: &str,
) -> Result<ProfilePatch, String> {
    let before = profile.clone();
    let mut current = before.clone();
    let changed = ghost_rooms::RoomService::unban(&mut current.rooms, &room.id, address)
        .ok_or_else(|| "Room no longer exists.".to_string())?;
    let state = room_state_wire(&changed);
    let broadcast =
        broadcast_room_wire_to_members(&current, password, changed.members(), &state, None).await;
    if let Some(progress) = broadcast.wallet_progress {
        crate::model::WalletStateService::merge_progress(&mut current.wallet, progress);
    }
    let mut patch = wallet_patch(&before, &current);
    append_room(&mut patch, &before, &current, &room.id);
    Ok(patch)
}
