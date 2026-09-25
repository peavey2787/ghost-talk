use crate::model::{ContactStore, Profile, RoomWire};
use ghost_domain::identity::PeerBinding;

mod state;
use state::{handle_room_state, RoomStateUpdate};

pub(crate) fn sync_contact_route(
    contacts: &mut ContactStore,
    peer_hydra_id: &str,
    address: Option<&str>,
) {
    let Some(address) = address.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let Ok(peer) = PeerBinding::new(address.to_owned(), peer_hydra_id.to_owned()) else {
        return;
    };
    let _ = ghost_contacts::ContactService::bind_peer(contacts, &peer);
}

pub(crate) async fn handle_room_wire(
    mut profile: Profile,
    password: &str,
    from: &str,
    wire: RoomWire,
) -> Result<Profile, String> {
    match wire {
        RoomWire::State {
            room_id,
            name,
            owner_hydra_id,
            owner_kaspa_address,
            owner_label,
            access,
            text_policy,
            audio_policy,
            broadcast_enabled,
            revision,
            members,
            bans,
        } => {
            handle_room_state(
                &mut profile.rooms,
                &mut profile.room_tombstones,
                &mut profile.chats,
                from,
                RoomStateUpdate {
                    room_id,
                    name,
                    owner_hydra_id,
                    owner_kaspa_address,
                    owner_label,
                    access,
                    text_policy,
                    audio_policy,
                    broadcast_enabled,
                    revision,
                    members,
                    bans,
                },
            );
            Ok(profile)
        }
        other => handle_room_event(profile, password, from, other).await,
    }
}

async fn handle_room_event(
    profile: Profile,
    password: &str,
    from: &str,
    wire: RoomWire,
) -> Result<Profile, String> {
    match wire {
        RoomWire::Message { room_id, message } => {
            handle_room_message(profile, password, from, room_id, message).await
        }
        RoomWire::Reaction {
            room_id,
            target_message_id,
            actor_hydra_id,
            reaction,
        } => {
            handle_room_reaction(
                profile,
                password,
                from,
                room_id,
                target_message_id,
                actor_hydra_id,
                reaction,
            )
            .await
        }
        other => handle_room_control_event(profile, password, from, other).await,
    }
}

async fn handle_room_control_event(
    profile: Profile,
    password: &str,
    from: &str,
    wire: RoomWire,
) -> Result<Profile, String> {
    match wire {
        RoomWire::Leave {
            room_id,
            member_hydra_id,
            member_kaspa_address,
        } => {
            handle_room_leave(
                profile,
                password,
                from,
                room_id,
                member_hydra_id,
                member_kaspa_address,
            )
            .await
        }
        removal @ (RoomWire::Kick { .. } | RoomWire::Ban { .. }) => {
            handle_room_removal_event(profile, from, removal).await
        }
        RoomWire::Disband { room_id, revision } => {
            Ok(handle_room_disband(profile, from, &room_id, revision).await)
        }
        RoomWire::State { .. } => unreachable!("room state is handled before event dispatch"),
        RoomWire::Message { .. } | RoomWire::Reaction { .. } => {
            unreachable!("room content is handled before control dispatch")
        }
    }
}

async fn handle_room_removal_event(
    profile: Profile,
    from: &str,
    wire: RoomWire,
) -> Result<Profile, String> {
    let (room_id, revision, target_hydra_id, target_kaspa_address) = match wire {
        RoomWire::Kick {
            room_id,
            revision,
            target_hydra_id,
            target_kaspa_address,
        }
        | RoomWire::Ban {
            room_id,
            revision,
            target_hydra_id,
            target_kaspa_address,
            ..
        } => (room_id, revision, target_hydra_id, target_kaspa_address),
        _ => unreachable!("only room removal events reach removal dispatch"),
    };
    Ok(handle_room_removal(
        profile,
        from,
        &room_id,
        revision,
        target_hydra_id.as_deref(),
        &target_kaspa_address,
    )
    .await)
}

mod room_messages;
pub(crate) use room_messages::{
    handle_room_disband, handle_room_leave, handle_room_message, handle_room_reaction,
    handle_room_removal,
};
