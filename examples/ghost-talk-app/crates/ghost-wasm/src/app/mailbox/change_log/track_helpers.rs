use ghost_domain::identity::{optional_binding_matches, PeerBinding};

use crate::model::{HydraIncomingRequestProjection, Profile, ReceivedProjection, RoomWire};

pub(super) fn same_request(
    chat: &crate::model::Chat,
    incoming: &HydraIncomingRequestProjection,
) -> bool {
    chat.incoming_request().is_some_and(|request| {
        request.request_id == incoming.request_id
            && request.peer_hydra_id == incoming.peer_hydra_id
            && request
                .peer_address
                .eq_ignore_ascii_case(&incoming.peer_address)
    })
}

pub(super) fn reusable_request_chat(chat: &crate::model::Chat, peer: Option<&PeerBinding>) -> bool {
    !chat.room_transport_only()
        && !chat.archived()
        && !chat.left()
        && !chat.peer_left()
        && peer.is_some_and(|peer| {
            optional_binding_matches(chat.peer_kaspa_address(), chat.peer_hydra_handle(), peer)
        })
}

pub(super) fn delivery_chat_matches(
    chat: &crate::model::Chat,
    peer: &str,
    message_id: &str,
) -> bool {
    chat.peer_hydra_handle() == Some(peer)
        && chat.messages().iter().any(|message| {
            message.wire_id.as_deref() == Some(message_id) || message.id == message_id
        })
}

pub(super) fn contact_acceptance_chat_ids(profile: &Profile, request_id: &str) -> Vec<String> {
    profile
        .chats
        .iter()
        .filter(|chat| {
            chat.messages()
                .iter()
                .any(|message| message.contact_request_id.as_deref() == Some(request_id))
        })
        .map(|chat| chat.id.clone())
        .collect()
}

pub(super) fn room_contains_accepted_peer(
    room: &crate::model::Room,
    prior_addresses: &[String],
    accepted_peer: Option<&PeerBinding>,
) -> bool {
    room.members().iter().any(|member| {
        prior_addresses
            .iter()
            .any(|address| member.kaspa_address.eq_ignore_ascii_case(address))
            || accepted_peer.is_some_and(|peer| {
                optional_binding_matches(
                    Some(member.kaspa_address.as_str()),
                    member.hydra_handle.as_deref(),
                    peer,
                )
            })
    })
}

pub(super) fn direct_received_chat_matches(
    chat: &crate::model::Chat,
    received: &ReceivedProjection,
    sid: &str,
) -> bool {
    !chat.room_transport_only()
        && !chat.left()
        && !chat.peer_left()
        && chat.peer_hydra_handle() == Some(received.from.as_str())
        && (chat.session_sid() == Some(sid)
            || (chat.bootstrap_complete() && chat.session_sid().is_none()))
}

pub(super) fn room_wire_id(wire: &RoomWire) -> &str {
    match wire {
        RoomWire::State { room_id, .. }
        | RoomWire::Message { room_id, .. }
        | RoomWire::Reaction { room_id, .. }
        | RoomWire::Leave { room_id, .. }
        | RoomWire::Kick { room_id, .. }
        | RoomWire::Ban { room_id, .. }
        | RoomWire::Disband { room_id, .. } => room_id,
    }
}

pub(super) fn room_transport_chat_ids(profile: &Profile, peer: &str) -> Vec<String> {
    profile
        .chats
        .iter()
        .filter(|chat| chat.room_transport_only() && chat.peer_hydra_handle() == Some(peer))
        .map(|chat| chat.id.clone())
        .collect()
}

pub(super) fn peer_chat_ids(profile: &Profile, peer: &str) -> Vec<String> {
    profile
        .chats
        .iter()
        .filter(|chat| chat.peer_hydra_handle() == Some(peer) && !chat.left() && !chat.peer_left())
        .map(|chat| chat.id.clone())
        .collect()
}
