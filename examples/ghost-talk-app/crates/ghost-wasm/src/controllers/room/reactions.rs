use crate::model::{Profile, ProfilePatch, ReactionKind, Room, RoomMessage, RoomWire};

use super::{broadcast_room_wire_to_members, room_patch, send_room_wire_to_route, wallet_patch};

pub(crate) struct PreparedRoomReaction {
    pub(crate) patch: ProfilePatch,
    before: Profile,
    working: Profile,
    room: Room,
    sender_hydra_id: String,
    wire: RoomWire,
}

pub(crate) fn prepare_reaction(
    profile: &Profile,
    room: &Room,
    message: &RoomMessage,
    kind: ReactionKind,
) -> Result<PreparedRoomReaction, String> {
    if room.pending_acceptance() {
        return Err("Accept the Room before reacting to messages.".into());
    }
    let sender_hydra_id = profile
        .hydra_identity_id
        .clone()
        .ok_or_else(|| "HYDRA identity is unavailable.".to_string())?;
    if !room.can_react(&sender_hydra_id) {
        return Err("You are not an active member of this Room.".into());
    }
    let current = message.reaction_for(&sender_hydra_id);
    let reaction = (current != Some(kind)).then_some(kind);
    let mut working = profile.clone();
    if !ghost_rooms::RoomService::set_message_reaction(
        &mut working.rooms,
        &room.id,
        &message.id,
        &sender_hydra_id,
        reaction,
    ) {
        return Err("The Room message is no longer available.".into());
    }
    let wire = RoomWire::Reaction {
        room_id: room.id.clone(),
        target_message_id: message.id.clone(),
        actor_hydra_id: sender_hydra_id.clone(),
        reaction,
    };
    let patch = room_patch(profile, &working, &room.id);
    Ok(PreparedRoomReaction {
        patch,
        before: profile.clone(),
        working,
        room: room.clone(),
        sender_hydra_id,
        wire,
    })
}

pub(crate) async fn complete_reaction(
    mut plan: PreparedRoomReaction,
    password: &str,
) -> (ProfilePatch, Result<(), String>) {
    let baseline = plan.working.clone();
    let owner_send = plan.sender_hydra_id == plan.room.owner_hydra_id;
    let (progress, result) = if owner_send {
        broadcast_owner_reaction(&plan, password).await
    } else {
        send_member_reaction(&plan, password).await
    };
    if let Some(progress) = progress {
        crate::model::WalletStateService::merge_progress(&mut plan.working.wallet, progress);
    }
    if result.is_err() && !owner_send {
        return (
            room_patch(&plan.working, &plan.before, &plan.room.id),
            result,
        );
    }
    (wallet_patch(&baseline, &plan.working), result)
}

async fn send_member_reaction(
    plan: &PreparedRoomReaction,
    password: &str,
) -> (Option<crate::model::WalletProjection>, Result<(), String>) {
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
}

async fn broadcast_owner_reaction(
    plan: &PreparedRoomReaction,
    password: &str,
) -> (Option<crate::model::WalletProjection>, Result<(), String>) {
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
            "Room reaction could not be sent to: {}",
            broadcast.failures.join(", ")
        ))
    };
    (broadcast.wallet_progress, result)
}
