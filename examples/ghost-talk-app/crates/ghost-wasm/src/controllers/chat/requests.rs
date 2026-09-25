use crate::model::{Chat, IncomingRequest, Profile, ProfilePatch};
use ghost_contacts::{ContactProfileUpdate, ContactService};
use ghost_domain::identity::PeerBinding;

pub(crate) async fn accept_request(
    profile: &Profile,
    password: &str,
    chat: &Chat,
    request: &IncomingRequest,
) -> Result<ProfilePatch, String> {
    let sent =
        crate::native::send_contact_accept(profile, password, &request.signed_request_hex).await?;
    let mut contacts = profile.contacts.clone();
    let mut chats = profile.chats.clone();
    let mut wallet = profile.wallet.clone();
    crate::model::WalletStateService::merge_progress(&mut wallet, sent.public);
    apply_accepted_request(&mut contacts, &mut chats, chat, request);
    Ok(accepted_request_patch(
        profile, &contacts, &chats, wallet, chat, request,
    ))
}

fn apply_accepted_request(
    contacts: &mut ghost_contacts::ContactStore,
    chats: &mut ghost_chat::ChatStore,
    chat: &Chat,
    request: &IncomingRequest,
) {
    let contact_id = existing_request_contact(contacts, request)
        .unwrap_or_else(|| crate::random_id().unwrap_or_else(|_| request.request_id.clone()));
    if !contacts.iter().any(|contact| contact.id == contact_id) {
        let label = ContactService::unique_label(contacts, &chat.label, None);
        ContactService::push(
            contacts,
            ContactService::new_contact(
                contact_id.clone(),
                label,
                request.peer_address.clone(),
                Some(request.peer_hydra_id.clone()),
            ),
        );
    }
    if let Ok(peer) = PeerBinding::new(request.peer_address.clone(), request.peer_hydra_id.clone())
    {
        let _ = ghost_chat::ChatService::accept_incoming_request(
            chats,
            &chat.id,
            Some(contact_id),
            &peer,
            request.request_id.clone(),
            false,
        );
    }
}

fn existing_request_contact(
    contacts: &ghost_contacts::ContactStore,
    request: &IncomingRequest,
) -> Option<String> {
    let peer =
        PeerBinding::new(request.peer_address.clone(), request.peer_hydra_id.clone()).ok()?;
    ContactService::resolve_peer_id(contacts, &peer)
}

fn accepted_request_patch(
    before: &Profile,
    contacts: &ghost_contacts::ContactStore,
    chats: &ghost_chat::ChatStore,
    wallet: Option<crate::model::WalletRecord>,
    chat: &Chat,
    request: &IncomingRequest,
) -> ProfilePatch {
    let mut patch = super::chat_store_transition(before, chats, &chat.id);
    patch.wallet(before.wallet.clone(), wallet);
    if let Ok(peer) = PeerBinding::new(request.peer_address.clone(), request.peer_hydra_id.clone())
    {
        let old = ContactService::resolve_peer(&before.contacts, &peer).cloned();
        if let Some(new) = ContactService::resolve_peer(contacts, &peer).cloned() {
            patch.contact_upsert(old, new);
        }
    }
    patch
}

pub(crate) fn incoming_request_state_patch(
    profile: &Profile,
    chat_id: &str,
    state: &str,
) -> ProfilePatch {
    let mut chats = profile.chats.clone();
    let _ = ghost_chat::ChatService::set_incoming_request_state(&mut chats, chat_id, state);
    super::chat_store_transition(profile, &chats, chat_id)
}

pub(crate) fn toggle_archive_patch(profile: &Profile, chat_id: &str) -> ProfilePatch {
    let mut chats = profile.chats.clone();
    let _ = ghost_chat::ChatService::toggle_archive(&mut chats, chat_id);
    super::chat_store_transition(profile, &chats, chat_id)
}

pub(crate) fn add_contact_patch(profile: &Profile, chat: &Chat) -> Result<ProfilePatch, String> {
    if chat.contact_id().is_some() {
        return Ok(ProfilePatch::new(&profile.id));
    }
    let address = chat
        .peer_kaspa_address()
        .map(str::to_owned)
        .ok_or_else(|| "This chat has no Kaspa address.".to_string())?;
    let id = crate::random_id()?;
    let label = ContactService::unique_label(&profile.contacts, &chat.label, None);
    let mut contacts = profile.contacts.clone();
    let mut chats = profile.chats.clone();
    let contact = ContactService::new_contact(
        id.clone(),
        label.clone(),
        address,
        chat.peer_hydra_handle().map(str::to_owned),
    );
    ContactService::push(&mut contacts, contact.clone());
    let _ = super::super::contact::apply_public_profile_by_id(
        &mut contacts,
        &id,
        ContactProfileUpdate {
            label,
            kns_name: chat.peer_kns_name().map(str::to_owned),
            dotk_name: chat.peer_dotk_name().map(str::to_owned),
            verified_public: chat.verified_public(),
            public_username: None,
            avatar: None,
            capabilities: Vec::new(),
        },
    );
    let _ = ghost_chat::ChatService::set_contact_id(&mut chats, &chat.id, Some(id));
    let mut patch = super::chat_store_transition(profile, &chats, &chat.id);
    patch.contact_upsert(None, contact);
    Ok(patch)
}
