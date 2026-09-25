use crate::model::{Profile, ProfilePatch, ResolvedGhostPeer};
use ghost_contacts::{ContactProfileUpdate, ContactService, ContactStore};
use ghost_domain::identity::PeerBinding;

pub(crate) async fn resolve_peer(
    profile: &Profile,
    password: &str,
    destination: &str,
) -> Result<ResolvedGhostPeer, String> {
    crate::native::resolve_peer(profile, password, destination).await
}

/// Application-level add/update operation. Components provide intent only;
/// contact/chat ownership and the persistence delta are coordinated here.
pub(crate) async fn resolve_and_add(
    profile: &Profile,
    password: &str,
    destination: &str,
    display: &str,
) -> Result<ProfilePatch, String> {
    let peer = crate::native::resolve_peer(profile, password, destination).await?;
    let mut contacts = profile.contacts.clone();
    let mut chats = profile.chats.clone();
    apply_resolved_contact(&mut contacts, &mut chats, &peer, display)?;
    Ok(resolved_contact_patch(profile, &contacts, &chats, &peer))
}

fn apply_resolved_contact(
    contacts: &mut ContactStore,
    chats: &mut ghost_chat::ChatStore,
    peer: &ResolvedGhostPeer,
    display: &str,
) -> Result<(), String> {
    let label = resolved_contact_label(display, peer);
    let existing_id = resolved_peer_binding(peer)
        .and_then(|binding| ContactService::resolve_peer_id(contacts, &binding));
    if ContactService::name_conflicts(contacts, &label, existing_id.as_deref()) {
        return Err("Contact names must be unique. Choose a different name.".into());
    }
    if update_existing_contact(contacts, peer, label.clone()) {
        return Ok(());
    }
    add_new_contact(contacts, chats, peer, label)
}

fn resolved_contact_label(display: &str, peer: &ResolvedGhostPeer) -> String {
    if !display.trim().is_empty() {
        display.trim().to_string()
    } else if !peer.username.is_empty() {
        peer.username.clone()
    } else if !peer.display_name.is_empty() {
        peer.display_name.clone()
    } else {
        peer.kns_name
            .clone()
            .or_else(|| peer.dotk_name.clone())
            .unwrap_or_else(|| peer.kaspa_address.clone())
    }
}

fn resolved_peer_binding(peer: &ResolvedGhostPeer) -> Option<PeerBinding> {
    PeerBinding::from_optional(Some(&peer.kaspa_address), peer.hydra_handle.as_deref())
}

fn update_existing_contact(
    contacts: &mut ContactStore,
    peer: &ResolvedGhostPeer,
    label: String,
) -> bool {
    let Some(binding) = resolved_peer_binding(peer) else {
        return false;
    };
    let Some(contact_id) = ContactService::resolve_peer_id(contacts, &binding) else {
        return false;
    };
    apply_public_profile_by_id(
        contacts,
        &contact_id,
        ContactProfileUpdate {
            label,
            kns_name: peer.kns_name.clone(),
            dotk_name: peer.dotk_name.clone(),
            verified_public: peer.verified_public,
            public_username: (!peer.username.is_empty()).then_some(peer.username.clone()),
            avatar: peer.avatar.clone(),
            capabilities: peer.capabilities.clone(),
        },
    )
}

fn add_new_contact(
    contacts: &mut ContactStore,
    chats: &mut ghost_chat::ChatStore,
    peer: &ResolvedGhostPeer,
    label: String,
) -> Result<(), String> {
    let id = crate::random_id().map_err(|_| "Unable to generate contact ID".to_string())?;
    ContactService::push(
        contacts,
        ContactService::new_contact(
            id.clone(),
            label.clone(),
            peer.kaspa_address.clone(),
            peer.hydra_handle.clone(),
        ),
    );
    let _ = apply_public_profile_by_id(
        contacts,
        &id,
        ContactProfileUpdate {
            label: label.clone(),
            kns_name: peer.kns_name.clone(),
            dotk_name: peer.dotk_name.clone(),
            verified_public: peer.verified_public,
            public_username: (!peer.username.is_empty()).then_some(peer.username.clone()),
            avatar: peer.avatar.clone(),
            capabilities: peer.capabilities.clone(),
        },
    );
    ensure_contact_chat(chats, peer, id, label)
}

fn ensure_contact_chat(
    chats: &mut ghost_chat::ChatStore,
    peer: &ResolvedGhostPeer,
    contact_id: String,
    label: String,
) -> Result<(), String> {
    let exists = resolved_peer_binding(peer)
        .is_some_and(|binding| chats.iter().any(|chat| chat.matches_peer(&binding)));
    if exists {
        return Ok(());
    }
    let chat_id = crate::random_id().map_err(|_| "Unable to generate chat ID".to_string())?;
    ghost_chat::ChatService::push(
        chats,
        ghost_chat::ChatService::new_direct_chat(ghost_chat::DirectChatSpec {
            id: chat_id,
            label,
            contact_id: Some(contact_id),
            kaspa_address: peer.kaspa_address.clone(),
            hydra_handle: peer.hydra_handle.clone(),
            kns_name: peer.kns_name.clone(),
            dotk_name: peer.dotk_name.clone(),
            verified_public: peer.verified_public,
        }),
    );
    Ok(())
}

fn resolved_contact_patch(
    before: &Profile,
    contacts: &ContactStore,
    chats: &ghost_chat::ChatStore,
    peer: &ResolvedGhostPeer,
) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&before.id);
    let Some(binding) = resolved_peer_binding(peer) else {
        return patch;
    };
    let old_contact = ContactService::resolve_peer(&before.contacts, &binding).cloned();
    if let Some(new_contact) = ContactService::resolve_peer(contacts, &binding).cloned() {
        patch.contact_upsert(old_contact, new_contact);
    }
    let old_chat = before
        .chats
        .iter()
        .find(|chat| chat.matches_peer(&binding))
        .cloned();
    if old_chat.is_none() {
        if let Some(new_chat) = chats
            .iter()
            .find(|chat| chat.matches_peer(&binding))
            .cloned()
        {
            patch.chat_upsert(None, new_chat);
        }
    }
    patch
}

pub(crate) fn apply_public_profile_by_id(
    contacts: &mut ContactStore,
    id: &str,
    update: ContactProfileUpdate,
) -> bool {
    let verified = update.verified_public;
    if !ContactService::apply_public_profile_by_id(contacts, id, update) {
        return false;
    }
    crate::app::ApplicationRouter::route_contact(
        contacts,
        crate::app::ApplicationEvent::ContactVerified {
            contact_id: id.to_owned(),
            verified,
        },
    )
}
