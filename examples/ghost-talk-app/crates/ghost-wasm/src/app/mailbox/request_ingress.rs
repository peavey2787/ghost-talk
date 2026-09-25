use crate::app::state::random_id;
use crate::model::{
    Chat, ChatStore, Contact, ContactStore, HydraIncomingRequestProjection, IncomingRequest,
    Settings,
};
use ghost_contacts::ContactService;
use ghost_domain::identity::{optional_binding_matches, PeerBinding};
pub(super) fn upsert_incoming_request(
    contacts: &ContactStore,
    chats: &mut ChatStore,
    settings: &Settings,
    incoming: HydraIncomingRequestProjection,
) {
    let existing_contact = matching_request_contact(contacts, &incoming);
    if refresh_existing_request(chats, &incoming) {
        return;
    }
    if is_direct_chat_request(&incoming) && reuse_pending_direct_chat(chats, &incoming) {
        return;
    }
    if is_call_bootstrap_request(&incoming) && reuse_call_chat(chats, &incoming) {
        return;
    }
    insert_incoming_request(chats, settings, &incoming, existing_contact.as_ref());
}

fn matching_request_contact(
    contacts: &ContactStore,
    incoming: &HydraIncomingRequestProjection,
) -> Option<Contact> {
    let peer = incoming_peer(incoming)?;
    ContactService::resolve_peer(contacts, &peer).cloned()
}

fn refresh_existing_request(
    chats: &mut ChatStore,
    incoming: &HydraIncomingRequestProjection,
) -> bool {
    let Some(chat) = chats.iter().find(|chat| {
        chat.incoming_request()
            .is_some_and(|request| same_incoming_request(request, incoming))
    }) else {
        return false;
    };
    let chat_id = chat.id.clone();
    let state = request_state(chat);
    let _ = ghost_chat::ChatService::set_incoming_request(
        chats,
        &chat_id,
        project_incoming_request(incoming, state),
        incoming.room_invite.is_some(),
    );
    let _ = ghost_chat::HydraSessionManager::set_role(chats, &chat_id, "responder".into());
    true
}

fn same_incoming_request(
    request: &IncomingRequest,
    incoming: &HydraIncomingRequestProjection,
) -> bool {
    request.request_id == incoming.request_id
        && request.peer_hydra_id == incoming.peer_hydra_id
        && request
            .peer_address
            .eq_ignore_ascii_case(&incoming.peer_address)
}

fn request_state(chat: &Chat) -> String {
    chat.incoming_request()
        .map(|request| request.state.clone())
        .unwrap_or_else(|| "pending".into())
}

fn is_direct_chat_request(incoming: &HydraIncomingRequestProjection) -> bool {
    incoming.call_id.is_none() && incoming.room_invite.is_none()
}

fn is_call_bootstrap_request(incoming: &HydraIncomingRequestProjection) -> bool {
    incoming.call_id.is_some() && incoming.call_action.as_deref().unwrap_or("request") == "request"
}

fn incoming_peer(incoming: &HydraIncomingRequestProjection) -> Option<PeerBinding> {
    PeerBinding::new(
        incoming.peer_address.clone(),
        incoming.peer_hydra_id.clone(),
    )
    .ok()
}

fn peer_matches_request(chat: &Chat, incoming: &HydraIncomingRequestProjection) -> bool {
    incoming_peer(incoming).is_some_and(|peer| {
        optional_binding_matches(chat.peer_kaspa_address(), chat.peer_hydra_handle(), &peer)
    })
}

fn reuse_pending_direct_chat(
    chats: &mut ChatStore,
    incoming: &HydraIncomingRequestProjection,
) -> bool {
    let Some(chat_id) = chats
        .iter()
        .find(|chat| {
            chat.reusable_direct()
                && !chat.bootstrap_complete()
                && peer_matches_request(chat, incoming)
        })
        .map(|chat| chat.id.clone())
    else {
        return false;
    };
    bind_incoming_request(chats, &chat_id, incoming);
    let _ = ghost_chat::HydraSessionManager::begin(
        chats,
        &chat_id,
        Some(incoming.request_id.clone()),
        Some("responder".into()),
    );
    true
}

fn reuse_call_chat(chats: &mut ChatStore, incoming: &HydraIncomingRequestProjection) -> bool {
    let Some(chat_id) = chats
        .iter()
        .find(|chat| chat.reusable_direct() && peer_matches_request(chat, incoming))
        .map(|chat| chat.id.clone())
    else {
        return false;
    };
    bind_incoming_request(chats, &chat_id, incoming);
    true
}

fn bind_incoming_request(
    chats: &mut ghost_chat::ChatStore,
    chat_id: &str,
    incoming: &HydraIncomingRequestProjection,
) {
    if let Ok(peer) = PeerBinding::new(
        incoming.peer_address.clone(),
        incoming.peer_hydra_id.clone(),
    ) {
        let _ = ghost_chat::ChatService::bind_peer(chats, chat_id, &peer);
    }
    let _ = ghost_chat::HydraSessionManager::set_role(chats, chat_id, "responder".into());
    let _ = ghost_chat::ChatService::set_incoming_request(
        chats,
        chat_id,
        project_incoming_request(incoming, "pending".into()),
        incoming.room_invite.is_some(),
    );
}

fn insert_incoming_request(
    chats: &mut ChatStore,
    settings: &Settings,
    incoming: &HydraIncomingRequestProjection,
    existing_contact: Option<&Contact>,
) {
    let is_room_invite = incoming.room_invite.is_some();
    let state = if request_should_be_ignored(settings, incoming, existing_contact, is_room_invite) {
        "ignored"
    } else {
        "pending"
    };
    let id = random_id().unwrap_or_else(|_| incoming.request_id.clone());
    let request = project_incoming_request(incoming, state.into());
    let chat = ghost_chat::ChatService::new_incoming_request(ghost_chat::IncomingRequestChatSpec {
        direct: ghost_chat::DirectChatSpec {
            id,
            label: request_label(incoming, existing_contact),
            contact_id: existing_contact.map(|contact| contact.id.clone()),
            kaspa_address: incoming.peer_address.clone(),
            hydra_handle: Some(incoming.peer_hydra_id.clone()),
            kns_name: existing_contact.and_then(|contact| contact.kns_name.clone()),
            dotk_name: existing_contact.and_then(|contact| contact.dotk_name.clone()),
            verified_public: false,
        },
        request,
        room_transport_only: is_room_invite,
    });
    ghost_chat::ChatService::insert_front(chats, chat);
}

fn request_should_be_ignored(
    settings: &Settings,
    incoming: &HydraIncomingRequestProjection,
    existing_contact: Option<&Contact>,
    is_room_invite: bool,
) -> bool {
    !is_room_invite
        && !is_call_bootstrap_request(incoming)
        && settings.auto_ignore_unknown_chats
        && existing_contact.is_none()
}

fn request_label(incoming: &HydraIncomingRequestProjection, contact: Option<&Contact>) -> String {
    contact
        .map(|contact| contact.label.clone())
        .filter(|label| !label.is_empty())
        // An unknown peer controls its own display name. Never let that
        // self-asserted label visually impersonate a saved contact before the
        // authenticated Kaspa-address + HYDRA binding has actually matched.
        .unwrap_or_else(|| incoming.peer_address.clone())
}

fn project_incoming_request(
    incoming: &HydraIncomingRequestProjection,
    state: String,
) -> IncomingRequest {
    IncomingRequest {
        request_id: incoming.request_id.clone(),
        peer_hydra_id: incoming.peer_hydra_id.clone(),
        peer_address: incoming.peer_address.clone(),
        local_address: incoming.local_address.clone(),
        signed_request_hex: incoming.signed_request_hex.clone(),
        room_invite: incoming.room_invite.clone(),
        call_id: incoming.call_id.clone(),
        call_action: incoming.call_action.clone(),
        state,
    }
}

#[cfg(test)]
#[cfg(test)]
mod tests;
