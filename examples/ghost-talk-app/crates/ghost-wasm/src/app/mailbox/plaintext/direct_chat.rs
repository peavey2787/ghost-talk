use super::super::super::{room_ingress::sync_contact_route, state::random_id};
use crate::model::{Chat, ChatStore, Contact, ContactStore};
use ghost_contacts::ContactService;
use ghost_domain::identity::PeerBinding;

pub(crate) fn ensure_visible_direct_chat(
    chats: &mut ChatStore,
    contacts: &mut ContactStore,
    peer_hydra_id: &str,
    sid: &str,
    authenticated_peer_address: Option<&str>,
) -> String {
    if let Some(index) = visible_direct_chat_index(chats, peer_hydra_id, sid) {
        return update_visible_direct_chat(
            chats,
            contacts,
            index,
            peer_hydra_id,
            sid,
            authenticated_peer_address,
        );
    }
    create_visible_direct_chat(
        chats,
        contacts,
        peer_hydra_id,
        sid,
        authenticated_peer_address,
    )
}

pub(crate) fn visible_direct_chat_index(
    chats: &ChatStore,
    peer_hydra_id: &str,
    sid: &str,
) -> Option<usize> {
    chats
        .iter()
        .position(|chat| is_visible_direct_session(chat, peer_hydra_id, sid))
        .or_else(|| {
            chats
                .iter()
                .position(|chat| is_unbound_visible_chat(chat, peer_hydra_id))
        })
}

pub(crate) fn is_visible_direct_session(chat: &Chat, peer_hydra_id: &str, sid: &str) -> bool {
    !chat.room_transport_only()
        && !chat.left()
        && !chat.peer_left()
        && chat.peer_hydra_handle() == Some(peer_hydra_id)
        && chat.session_sid() == Some(sid)
}

pub(crate) fn is_unbound_visible_chat(chat: &Chat, peer_hydra_id: &str) -> bool {
    !chat.room_transport_only()
        && !chat.left()
        && !chat.peer_left()
        && chat.peer_hydra_handle() == Some(peer_hydra_id)
        && chat.bootstrap_complete()
        && chat.session_sid().is_none()
}

pub(crate) fn update_visible_direct_chat(
    chats: &mut ChatStore,
    contacts: &mut ContactStore,
    index: usize,
    peer_hydra_id: &str,
    sid: &str,
    authenticated_peer_address: Option<&str>,
) -> String {
    let address = nonempty_address(authenticated_peer_address);
    if let Some(chat) = ghost_chat::ChatService::by_index(chats, index) {
        let chat_id = chat.id.clone();
        let contact_id = chat.contact_id().map(str::to_owned);
        let _ = ghost_chat::HydraSessionManager::complete_bootstrap(
            chats,
            &chat_id,
            Some(sid.to_string()),
        );
        if let Some(address) = address.as_ref() {
            if let Ok(peer) = PeerBinding::new(address.clone(), peer_hydra_id.to_string()) {
                let _ = ghost_chat::ChatService::bind_peer(chats, &chat_id, &peer);
            }
        }
        sync_contact_by_id(
            contacts,
            contact_id.as_deref(),
            peer_hydra_id,
            address.as_deref(),
        );
        return chat_id;
    }
    create_visible_direct_chat(
        chats,
        contacts,
        peer_hydra_id,
        sid,
        authenticated_peer_address,
    )
}

pub(crate) fn create_visible_direct_chat(
    chats: &mut ChatStore,
    contacts: &mut ContactStore,
    peer_hydra_id: &str,
    sid: &str,
    authenticated_peer_address: Option<&str>,
) -> String {
    let transport = matching_room_transport(chats, peer_hydra_id, sid);
    let contact = matching_peer_contact(contacts, peer_hydra_id, authenticated_peer_address);
    let address = preferred_peer_address(
        authenticated_peer_address,
        transport.as_ref(),
        contact.as_ref(),
    );
    let chat_id = random_id().unwrap_or_else(|_| format!("direct-{sid}"));
    let chat = direct_chat_from_sources(
        &chat_id,
        peer_hydra_id,
        sid,
        address.clone(),
        contact.as_ref(),
        transport.as_ref(),
    );
    ghost_chat::ChatService::insert_front(chats, chat);
    sync_contact_route(contacts, peer_hydra_id, address.as_deref());
    chat_id
}

pub(crate) fn nonempty_address(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_string)
}

pub(crate) fn matching_room_transport(
    chats: &ChatStore,
    peer_hydra_id: &str,
    sid: &str,
) -> Option<Chat> {
    chats
        .iter()
        .find(|chat| {
            chat.room_transport_only()
                && !chat.left()
                && !chat.peer_left()
                && chat.peer_hydra_handle() == Some(peer_hydra_id)
                && chat.session_sid() == Some(sid)
        })
        .cloned()
}

pub(crate) fn matching_peer_contact(
    contacts: &ContactStore,
    peer_hydra_id: &str,
    address: Option<&str>,
) -> Option<Contact> {
    let address = address.filter(|value| !value.trim().is_empty())?;
    let peer = PeerBinding::new(address.to_owned(), peer_hydra_id.to_owned()).ok()?;
    ContactService::resolve_peer(contacts, &peer).cloned()
}

pub(crate) fn preferred_peer_address(
    address: Option<&str>,
    transport: Option<&Chat>,
    contact: Option<&Contact>,
) -> Option<String> {
    nonempty_address(address)
        .or_else(|| transport.and_then(|chat| chat.peer_kaspa_address().map(str::to_owned)))
        .or_else(|| contact.map(|contact| contact.kaspa_address().to_string()))
}

pub(crate) fn direct_chat_from_sources(
    chat_id: &str,
    peer_hydra_id: &str,
    sid: &str,
    address: Option<String>,
    contact: Option<&Contact>,
    transport: Option<&Chat>,
) -> Chat {
    ghost_chat::ChatService::new_session_chat(ghost_chat::SessionChatSpec {
        restored: ghost_chat::RestoredChatSpec {
            id: chat_id.to_string(),
            label: direct_chat_label(contact, transport, address.as_deref(), peer_hydra_id),
            contact_id: contact
                .map(|contact| contact.id.clone())
                .or_else(|| transport.and_then(|chat| chat.contact_id().map(str::to_owned))),
            kaspa_address: address,
            hydra_handle: None,
            kns_name: contact
                .and_then(|contact| contact.kns_name.clone())
                .or_else(|| transport.and_then(|chat| chat.peer_kns_name().map(str::to_owned))),
            dotk_name: contact
                .and_then(|contact| contact.dotk_name.clone())
                .or_else(|| transport.and_then(|chat| chat.peer_dotk_name().map(str::to_owned))),
        },
        hydra_handle: peer_hydra_id.to_string(),
        verified_public: contact.is_some_and(|contact| contact.verified_public)
            || transport.is_some_and(|chat| chat.verified_public()),
        sid: sid.to_string(),
        role: transport.and_then(|chat| chat.session_role().map(str::to_owned)),
        restore_pending: false,
    })
}

pub(crate) fn direct_chat_label(
    contact: Option<&Contact>,
    transport: Option<&Chat>,
    address: Option<&str>,
    peer_hydra_id: &str,
) -> String {
    contact
        .map(|contact| contact.label.clone())
        .or_else(|| transport.map(|chat| chat.label.clone()))
        .or_else(|| address.map(str::to_string))
        .unwrap_or_else(|| peer_hydra_id.to_string())
}

pub(crate) fn sync_contact_by_id(
    contacts: &mut ContactStore,
    contact_id: Option<&str>,
    peer_hydra_id: &str,
    address: Option<&str>,
) {
    let Some(contact_id) = contact_id else {
        return;
    };
    if let Some(address) = address {
        if let Ok(peer) = PeerBinding::new(address.to_string(), peer_hydra_id.to_string()) {
            let _ = ghost_contacts::ContactService::bind_peer_by_id(contacts, contact_id, &peer);
        }
    }
}
