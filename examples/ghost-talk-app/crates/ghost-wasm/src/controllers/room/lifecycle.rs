use crate::model::{Profile, ProfilePatch, Room, RoomWire};

pub(crate) struct PreparedRoomLeave {
    pub(crate) patch: ProfilePatch,
    working: Profile,
    room: Room,
    wire: RoomWire,
}

use super::{
    append_chats, append_room, append_tombstone, broadcast_room_wire_to_members, peer_chat_ids,
    remove_pending_room_payloads, send_room_wire_to_route, union_chat_ids, wallet_patch,
};

pub(crate) fn prepare_leave(profile: &Profile, room: &Room) -> Result<PreparedRoomLeave, String> {
    let own_hydra = profile
        .hydra_identity_id
        .clone()
        .ok_or_else(|| "HYDRA identity is unavailable.".to_string())?;
    if own_hydra == room.owner_hydra_id {
        return Err(
            "The room creator must disband the room instead of leaving it orphaned.".into(),
        );
    }
    let wire = RoomWire::Leave {
        room_id: room.id.clone(),
        member_hydra_id: own_hydra,
        member_kaspa_address: local_room_address(profile, room),
    };
    let mut working = profile.clone();
    ghost_rooms::RoomTombstoneService::record(
        &mut working.room_tombstones,
        &room.id,
        &room.owner_hydra_id,
        room.revision(),
    );
    ghost_rooms::RoomService::remove_by_id(&mut working.rooms, &room.id);
    let mut patch = ProfilePatch::new(&profile.id);
    append_room(&mut patch, profile, &working, &room.id);
    append_tombstone(
        &mut patch,
        profile,
        &working,
        &room.id,
        &room.owner_hydra_id,
    );
    Ok(PreparedRoomLeave {
        patch,
        working,
        room: room.clone(),
        wire,
    })
}

pub(crate) async fn complete_leave(
    mut plan: PreparedRoomLeave,
    password: &str,
) -> (ProfilePatch, String) {
    let baseline = plan.working.clone();
    let tracked = peer_chat_ids(&baseline, &plan.room.owner_hydra_id);
    let result = send_room_wire_to_route(
        &plan.working,
        password,
        &plan.room.owner_label,
        &plan.room.owner_hydra_id,
        &plan.room.owner_kaspa_address,
        &plan.wire,
    )
    .await;
    if let Ok(progress) = result.as_ref() {
        crate::model::WalletStateService::merge_progress(
            &mut plan.working.wallet,
            progress.clone(),
        );
    }
    plan.working = retire_peer_transport(plan.working, &plan.room.owner_hydra_id).await;
    let mut patch = wallet_patch(&baseline, &plan.working);
    let ids = union_chat_ids(
        tracked,
        peer_chat_ids(&plan.working, &plan.room.owner_hydra_id),
    );
    append_chats(&mut patch, &baseline, &plan.working, &ids);
    let status = if result.is_ok() {
        "You left the room.".into()
    } else {
        "You left the room. The owner notification was not sent because the secure transport was unavailable.".into()
    };
    (patch, status)
}

pub(super) async fn retire_peer_transport(mut profile: Profile, peer_hydra_id: &str) -> Profile {
    if ghost_rooms::RoomService::peer_needed_by_room(
        &profile.rooms,
        profile.hydra_identity_id.as_deref(),
        peer_hydra_id,
    ) {
        return profile;
    }
    if ghost_chat::ChatService::retire_room_transports_for_peer(&mut profile.chats, peer_hydra_id) {
        let _ = crate::native::leave_peer(&profile.id, peer_hydra_id).await;
    }
    profile
}

pub(crate) async fn disband(
    profile: &Profile,
    password: &str,
    room: &Room,
) -> Result<(ProfilePatch, Vec<String>), String> {
    let revision = room.revision().saturating_add(1);
    let wire = RoomWire::Disband {
        room_id: room.id.clone(),
        revision,
    };
    let before = profile.clone();
    let mut current = before.clone();
    let mut chat_ids = disband_chat_ids(&before, &mut current.chats, room);
    let broadcast =
        broadcast_room_wire_to_members(&current, password, room.members(), &wire, None).await;
    if let Some(progress) = broadcast.wallet_progress {
        crate::model::WalletStateService::merge_progress(&mut current.wallet, progress);
    }
    ghost_rooms::RoomTombstoneService::record(
        &mut current.room_tombstones,
        &room.id,
        &room.owner_hydra_id,
        revision,
    );
    ghost_rooms::RoomService::remove_by_id(&mut current.rooms, &room.id);
    (current, chat_ids) = retire_disbanded_peers(current, room, chat_ids).await;
    let mut patch = wallet_patch(&before, &current);
    append_room(&mut patch, &before, &current, &room.id);
    append_tombstone(
        &mut patch,
        &before,
        &current,
        &room.id,
        &room.owner_hydra_id,
    );
    append_chats(&mut patch, &before, &current, &chat_ids);
    Ok((patch, broadcast.failures))
}

fn disband_chat_ids(
    before: &Profile,
    chats: &mut crate::model::ChatStore,
    room: &Room,
) -> Vec<String> {
    let mut ids = remove_pending_room_payloads(chats, &room.id, None);
    for peer in room
        .members()
        .iter()
        .filter_map(|member| member.hydra_handle.as_deref())
    {
        ids = union_chat_ids(ids, peer_chat_ids(before, peer));
    }
    ids
}

async fn retire_disbanded_peers(
    mut profile: Profile,
    room: &Room,
    mut chat_ids: Vec<String>,
) -> (Profile, Vec<String>) {
    for peer in room
        .members()
        .iter()
        .filter_map(|member| member.hydra_handle.as_deref())
    {
        profile = retire_peer_transport(profile, peer).await;
        chat_ids = union_chat_ids(chat_ids, peer_chat_ids(&profile, peer));
    }
    (profile, chat_ids)
}

pub(crate) fn local_room_address(profile: &Profile, room: &Room) -> String {
    room.members()
        .iter()
        .find(|member| member.hydra_handle.as_deref() == profile.hydra_identity_id.as_deref())
        .map(|member| member.kaspa_address.clone())
        .or_else(|| {
            profile
                .wallet
                .as_ref()
                .map(|wallet| wallet.public.receive_address().to_owned())
                .filter(|address| !address.is_empty())
        })
        .unwrap_or_default()
}
