use crate::model::{Message, Profile, ProfilePatch, ResolvedGhostPeer, Room, RoomMember, RoomWire};

use super::{
    address_chat_ids, append_chats, broadcast_room_wire_to_members,
    room_member_session_established, room_state_wire, room_wallet_patch, send_room_wire_to_member,
    union_chat_ids,
};
mod bootstrap;
use bootstrap::{start_room_bootstrap, RoomBootstrap};

pub(crate) async fn invite_member(
    profile: &Profile,
    password: &str,
    room: &Room,
    raw_label: &str,
    destination: &str,
) -> Result<(String, ProfilePatch), String> {
    let baseline = profile.clone();
    let peer = crate::controllers::contact::resolve_peer(profile, password, destination).await?;
    validate_room_invitee(room, &peer)?;
    let member = member_from_peer(profile, &peer);
    let label = member.label.clone();
    let mut current = profile.clone();
    let state = add_member_state(&mut current.rooms, &room.id, member.clone())?;
    if let Some(progress) =
        broadcast_roster(&current, password, &room.id, &peer.kaspa_address, &state).await
    {
        crate::model::WalletStateService::merge_progress(&mut current.wallet, progress);
    }
    let message = if room_member_session_established(&current, &member) {
        let progress = send_room_wire_to_member(&current, password, &member, &state).await?;
        crate::model::WalletStateService::merge_progress(&mut current.wallet, progress);
        format!("Room invitation sent to {label}.")
    } else {
        let (next, message) =
            queue_or_start_bootstrap(current, password, room, &member, &peer, &state).await?;
        current = next;
        message
    };
    let patch = room_invite_patch(&baseline, &current, &room.id, &peer.kaspa_address);
    let _ = raw_label;
    Ok((message, patch))
}

fn member_from_peer(profile: &Profile, peer: &ResolvedGhostPeer) -> RoomMember {
    let existing = profile.contacts.iter().find(|contact| {
        let Some(hydra_id) = peer.hydra_handle.as_deref() else {
            return false;
        };
        let Ok(binding) = ghost_domain::identity::PeerBinding::new(
            peer.kaspa_address.clone(),
            hydra_id.to_owned(),
        ) else {
            return false;
        };
        contact.matches_peer(&binding)
    });
    RoomMember {
        contact_id: existing.map(|contact| contact.id.clone()),
        label: existing
            .map(|contact| contact.label.clone())
            .or_else(|| (!peer.username.trim().is_empty()).then(|| peer.username.clone()))
            .or_else(|| (!peer.display_name.trim().is_empty()).then(|| peer.display_name.clone()))
            .or_else(|| peer.kns_name.clone())
            .or_else(|| peer.dotk_name.clone())
            .unwrap_or_else(|| peer.kaspa_address.clone()),
        kaspa_address: peer.kaspa_address.clone(),
        hydra_handle: peer.hydra_handle.clone(),
        role: ghost_rooms::Role::Audience,
    }
}

fn validate_room_invitee(room: &Room, peer: &ResolvedGhostPeer) -> Result<(), String> {
    if room.members().iter().any(|member| {
        member
            .kaspa_address
            .eq_ignore_ascii_case(&peer.kaspa_address)
    }) {
        return Err("That Kaspa address is already in this room.".into());
    }
    let now = crate::now_ms();
    if let Some(ban) = room.bans().iter().find(|ban| {
        ban.is_active(now)
            && (ban.kaspa_address.eq_ignore_ascii_case(&peer.kaspa_address)
                || matches!((&ban.hydra_handle, &peer.hydra_handle), (Some(a), Some(b)) if a == b))
    }) {
        return Err(format!("{} is currently banned from this room.", ban.label));
    }
    Ok(())
}

fn add_member_state(
    rooms: &mut crate::model::RoomStore,
    room_id: &str,
    member: RoomMember,
) -> Result<RoomWire, String> {
    let room = ghost_rooms::RoomService::add_member(rooms, room_id, member, crate::now_ms())
        .ok_or_else(|| "Room no longer exists.".to_string())?;
    Ok(room_state_wire(&room))
}

async fn broadcast_roster(
    profile: &Profile,
    password: &str,
    room_id: &str,
    new_address: &str,
    state: &RoomWire,
) -> Option<crate::model::WalletProjection> {
    let members = profile
        .rooms
        .iter()
        .find(|room| room.id == room_id)
        .map(|room| {
            room.members()
                .iter()
                .filter(|member| !member.kaspa_address.eq_ignore_ascii_case(new_address))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    broadcast_room_wire_to_members(profile, password, &members, state, None)
        .await
        .wallet_progress
}

async fn queue_or_start_bootstrap(
    mut profile: Profile,
    password: &str,
    room: &Room,
    member: &RoomMember,
    peer: &ResolvedGhostPeer,
    state: &RoomWire,
) -> Result<(Profile, String), String> {
    let wire_id = crate::random_id()?;
    let payload = super::encode_room_wire(state)?;
    let chat_id = ensure_room_transport_chat(&mut profile.chats, member, peer)?;
    if chat_is_establishing(&profile.chats, &chat_id) {
        queue_room_state(&mut profile.chats, &chat_id, wire_id, payload)?;
        return Ok((
            profile,
            format!(
                "Room invite for {} is queued behind the existing secure-session setup.",
                member.label
            ),
        ));
    }
    start_room_bootstrap(
        profile,
        password,
        RoomBootstrap {
            room,
            member,
            peer,
            chat_id,
            wire_id,
            payload,
        },
    )
    .await
}

fn ensure_room_transport_chat(
    chats: &mut crate::model::ChatStore,
    member: &RoomMember,
    peer: &ResolvedGhostPeer,
) -> Result<String, String> {
    if let Some(chat) = chats.iter().find(|chat| {
        chat.peer_kaspa_address()
            .is_some_and(|address| address.eq_ignore_ascii_case(&peer.kaspa_address))
            && !chat.left()
    }) {
        return Ok(chat.id.clone());
    }
    let chat = ghost_chat::ChatService::new_direct_chat(ghost_chat::DirectChatSpec {
        id: crate::random_id()?,
        label: member.label.clone(),
        contact_id: member.contact_id.clone(),
        kaspa_address: peer.kaspa_address.clone(),
        hydra_handle: peer.hydra_handle.clone(),
        kns_name: peer.kns_name.clone(),
        dotk_name: peer.dotk_name.clone(),
        verified_public: peer.verified_public,
    });
    let id = chat.id.clone();
    ghost_chat::ChatService::push(chats, chat);
    Ok(id)
}

fn chat_is_establishing(chats: &crate::model::ChatStore, chat_id: &str) -> bool {
    ghost_chat::ChatService::by_id(chats, chat_id).is_some_and(|chat| {
        !chat.bootstrap_complete()
            && chat.messages().iter().any(|message| {
                message.pending
                    && matches!(
                        message.pending_stage.as_deref(),
                        Some("request" | "handshake" | "finish")
                    )
                    && (message.pending_id.is_some() || message.txid.is_some())
            })
    })
}

fn queue_room_state(
    chats: &mut crate::model::ChatStore,
    chat_id: &str,
    wire_id: String,
    payload: String,
) -> Result<(), String> {
    let session_sid = ghost_chat::ChatService::by_id(chats, chat_id)
        .ok_or_else(|| "Room transport chat disappeared.".to_string())?
        .session_sid()
        .map(str::to_owned);
    if !ghost_chat::ChatService::record_message(
        chats,
        chat_id,
        Message {
            id: wire_id.clone(),
            wire_id: Some(wire_id),
            session_sid,
            direction: "out".into(),
            body: payload,
            created_at: crate::now_ms(),
            send_state: Some("queued".into()),
            pending: true,
            pending_stage: Some("handshake".into()),
            ..Default::default()
        },
    ) {
        return Err("Room transport chat disappeared.".into());
    }
    Ok(())
}

fn room_invite_patch(
    before: &Profile,
    after: &Profile,
    room_id: &str,
    peer_address: &str,
) -> ProfilePatch {
    let mut patch = room_wallet_patch(before, after, room_id);
    let ids = union_chat_ids(
        address_chat_ids(before, peer_address),
        address_chat_ids(after, peer_address),
    );
    append_chats(&mut patch, before, after, &ids);
    patch
}
