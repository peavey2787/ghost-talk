use crate::model::{Profile, ProfilePatch, Room, RoomMessage, RoomWire};

use super::{broadcast_room_wire_to_members, send_room_wire_to_route, wallet_patch};

pub(crate) struct PreparedRoomSend {
    pub(crate) patch: ProfilePatch,
    working: Profile,
    room: Room,
    sender_hydra_id: String,
    wire: RoomWire,
}

pub(crate) fn prepare_send(
    profile: &Profile,
    room: &Room,
    body: String,
) -> Result<PreparedRoomSend, String> {
    let sender_hydra_id = profile
        .hydra_identity_id
        .clone()
        .ok_or_else(|| "HYDRA identity is unavailable.".to_string())?;
    let message = RoomMessage {
        id: crate::random_id()?,
        sender_hydra_id: sender_hydra_id.clone(),
        sender_label: profile.label.clone(),
        body,
        created_at: crate::now_ms(),
        reactions: Vec::new(),
    };
    let wire = RoomWire::Message {
        room_id: room.id.clone(),
        message: message.clone(),
    };
    let mut working = profile.clone();
    let _ = ghost_rooms::RoomService::add_message(&mut working.rooms, &room.id, message);
    let patch = super::room_patch(profile, &working, &room.id);
    Ok(PreparedRoomSend {
        patch,
        working,
        room: room.clone(),
        sender_hydra_id,
        wire,
    })
}

pub(crate) async fn complete_send(
    mut plan: PreparedRoomSend,
    password: &str,
) -> (ProfilePatch, Result<(), String>) {
    let baseline = plan.working.clone();
    let (progress, result) = if plan.sender_hydra_id != plan.room.owner_hydra_id {
        match send_room_wire_to_route(
            &plan.working,
            password,
            &plan.room.owner_label,
            &plan.room.owner_hydra_id,
            &plan.room.owner_kaspa_address,
            &plan.wire,
        )
        .await
        {
            Ok(progress) => (Some(progress), Ok(())),
            Err(error) => (None, Err(error)),
        }
    } else {
        let broadcast = broadcast_room_wire_to_members(
            &plan.working,
            password,
            plan.room.members(),
            &plan.wire,
            None,
        )
        .await;
        let result = if broadcast.failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Room message could not be sent to: {}",
                broadcast.failures.join(", ")
            ))
        };
        (broadcast.wallet_progress, result)
    };
    if let Some(progress) = progress {
        crate::model::WalletStateService::merge_progress(&mut plan.working.wallet, progress);
    }
    (wallet_patch(&baseline, &plan.working), result)
}
