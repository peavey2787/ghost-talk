use crate::model::{Profile, ReactionKind, Room, RoomWire};

pub(crate) async fn handle_room_reaction(
    mut profile: Profile,
    password: &str,
    from: &str,
    room_id: String,
    target_message_id: String,
    actor_hydra_id: String,
    reaction: Option<ReactionKind>,
) -> Result<Profile, String> {
    let Some(room) = ghost_rooms::RoomService::by_id(&profile.rooms, &room_id).cloned() else {
        return Ok(profile);
    };
    if room.pending_acceptance() || !target_exists(&room, &target_message_id) {
        return Ok(profile);
    }
    let local_is_owner = profile.hydra_identity_id.as_deref() == Some(room.owner_hydra_id.as_str());
    if local_is_owner {
        return relay_member_reaction(
            profile,
            password,
            from,
            room,
            target_message_id,
            actor_hydra_id,
            reaction,
        )
        .await;
    }
    accept_owner_reaction(
        &mut profile.rooms,
        from,
        &room,
        &target_message_id,
        &actor_hydra_id,
        reaction,
    );
    Ok(profile)
}

async fn relay_member_reaction(
    mut profile: Profile,
    password: &str,
    from: &str,
    room: Room,
    target_message_id: String,
    actor_hydra_id: String,
    reaction: Option<ReactionKind>,
) -> Result<Profile, String> {
    if actor_hydra_id != from || !room.can_react(from) {
        return Ok(profile);
    }
    let changed = ghost_rooms::RoomService::set_message_reaction(
        &mut profile.rooms,
        &room.id,
        &target_message_id,
        from,
        reaction,
    );
    if !changed {
        return Ok(profile);
    }
    let wire = RoomWire::Reaction {
        room_id: room.id.clone(),
        target_message_id,
        actor_hydra_id,
        reaction,
    };
    let routes = super::other_room_members(&room, from);
    let broadcast = crate::controllers::room::broadcast_room_wire_to_members(
        &profile,
        password,
        &routes,
        &wire,
        Some(from),
    )
    .await;
    broadcast.apply_wallet(&mut profile.wallet);
    Ok(profile)
}

fn accept_owner_reaction(
    rooms: &mut ghost_rooms::RoomStore,
    from: &str,
    room: &Room,
    target_message_id: &str,
    actor_hydra_id: &str,
    reaction: Option<ReactionKind>,
) {
    if from != room.owner_hydra_id || !room.can_react(actor_hydra_id) {
        return;
    }
    let _ = ghost_rooms::RoomService::set_message_reaction(
        rooms,
        &room.id,
        target_message_id,
        actor_hydra_id,
        reaction,
    );
}

fn target_exists(room: &Room, target_message_id: &str) -> bool {
    room.messages()
        .iter()
        .any(|message| message.id == target_message_id)
}

#[cfg(test)]
mod tests {
    use super::target_exists;
    use ghost_rooms::{ChannelPolicy, NewRoom, RoomAccess, RoomService};

    #[test]
    fn reaction_target_must_exist() {
        let room = RoomService::new_room(NewRoom {
            id: "room".into(),
            name: "Room".into(),
            owner_hydra_id: "owner".into(),
            owner_kaspa_address: "kaspa:owner".into(),
            owner_label: "Owner".into(),
            access: RoomAccess::Private,
            text_policy: ChannelPolicy::Interactive,
            audio_policy: ChannelPolicy::Interactive,
            broadcast_enabled: false,
            pending_acceptance: false,
            revision: 0,
            members: Vec::new(),
            bans: Vec::new(),
        });
        assert!(!target_exists(&room, "missing"));
    }
}
