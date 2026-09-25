use crate::model::{Chat, Contact, Profile, ResolvedGhostPeer};
use crate::random_id;
use ghost_contacts::ContactService;
use ghost_domain::identity::{optional_binding_matches, PeerBinding};

use super::super::state::{AppRuntime, AppState};
use super::navigation::select_chat;

pub(crate) fn open_resolved_existing_chat(
    chats: &mut crate::model::ChatStore,
    index: usize,
    state: &AppState,
    live: &AppRuntime,
) -> bool {
    let Some((id, was_archived)) = ghost_chat::ChatService::reopen_at_index(chats, index) else {
        return false;
    };
    select_chat(state, live, id);
    was_archived
}

pub(crate) fn new_resolved_chat(
    profile: &Profile,
    peer: ResolvedGhostPeer,
) -> Result<Chat, String> {
    let id = random_id()?;
    let contact = matching_resolved_contact(profile, &peer);
    let resolved_hydra = contact
        .as_ref()
        .and_then(|contact| contact.hydra_handle().map(str::to_string))
        .or(peer.hydra_handle.clone());
    let transport = matching_shared_transport(profile, &peer, resolved_hydra.as_deref());
    let label = resolved_chat_label(contact.as_ref(), &peer);
    let contact_id = contact.as_ref().map(|contact| contact.id.clone());
    let address = resolved_route_address(transport.as_ref(), &peer.kaspa_address);
    if let (Some(hydra), Some(shared)) = (resolved_hydra.clone(), transport.as_ref()) {
        if let Some(sid) = shared.session_sid() {
            return Ok(ghost_chat::ChatService::new_session_chat(
                ghost_chat::SessionChatSpec {
                    restored: ghost_chat::RestoredChatSpec {
                        id,
                        label,
                        contact_id,
                        kaspa_address: Some(address),
                        hydra_handle: None,
                        kns_name: peer.kns_name,
                        dotk_name: peer.dotk_name,
                    },
                    hydra_handle: hydra,
                    verified_public: peer.verified_public,
                    sid: sid.to_string(),
                    role: shared.session_role().map(str::to_owned),
                    restore_pending: false,
                },
            ));
        }
    }
    Ok(ghost_chat::ChatService::new_direct_chat(
        ghost_chat::DirectChatSpec {
            id,
            label,
            contact_id,
            kaspa_address: address,
            hydra_handle: resolved_hydra,
            kns_name: peer.kns_name,
            dotk_name: peer.dotk_name,
            verified_public: peer.verified_public,
        },
    ))
}

pub(crate) fn matching_resolved_contact(
    profile: &Profile,
    peer: &ResolvedGhostPeer,
) -> Option<Contact> {
    let binding = PeerBinding::new(
        peer.kaspa_address.clone(),
        peer.hydra_handle.as_deref()?.to_owned(),
    )
    .ok()?;
    ContactService::resolve_peer(&profile.contacts, &binding).cloned()
}

pub(crate) fn matching_shared_transport(
    profile: &Profile,
    peer: &ResolvedGhostPeer,
    hydra: Option<&str>,
) -> Option<Chat> {
    let binding = PeerBinding::new(peer.kaspa_address.clone(), hydra?.to_owned()).ok()?;
    profile
        .chats
        .iter()
        .find(|chat| {
            chat.room_transport_only()
                && !chat.left()
                && !chat.peer_left()
                && chat.bootstrap_complete()
                && chat.session_sid().is_some()
                && !chat.transport_restore_pending()
                && optional_binding_matches(
                    chat.peer_kaspa_address(),
                    chat.peer_hydra_handle(),
                    &binding,
                )
        })
        .cloned()
}

pub(crate) fn resolved_chat_label(contact: Option<&Contact>, peer: &ResolvedGhostPeer) -> String {
    contact
        .map(|contact| contact.label.clone())
        .filter(|value| !value.is_empty())
        .or_else(|| (!peer.username.is_empty()).then_some(peer.username.clone()))
        .or_else(|| (!peer.display_name.is_empty()).then_some(peer.display_name.clone()))
        .or_else(|| peer.kns_name.clone())
        .or_else(|| peer.dotk_name.clone())
        .unwrap_or_else(|| peer.kaspa_address.clone())
}

pub(crate) fn resolved_route_address(transport: Option<&Chat>, peer_address: &str) -> String {
    transport
        .and_then(|chat| chat.peer_kaspa_address().map(str::to_owned))
        .filter(|address| !address.is_empty())
        .unwrap_or_else(|| peer_address.to_string())
}
