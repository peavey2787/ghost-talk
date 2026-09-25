use crate::model::{Profile, ProfilePatch};
use ghost_api::{
    select_conversation_mode, BroadcastResult, ConversationMode, ProtocolAvailability,
};
use ghost_kasia::{KasiaContactMapping, KasiaMessage, KasiaReceivedHandshake};

pub(crate) fn protocol_availability(profile: &Profile, contact_id: &str) -> ProtocolAvailability {
    let Some(contact) = profile.contacts.iter().find(|item| item.id == contact_id) else {
        return ProtocolAvailability::default();
    };
    let mut availability = ProtocolAvailability::from_capabilities(&contact.capabilities);
    availability.ghost_pq |= contact.hydra_handle().is_some();
    availability.kasia |= profile
        .kasia_contacts()
        .by_contact(contact_id)
        .is_some_and(|mapping| mapping.established);
    availability
}

pub(crate) fn preferred_new_mode(
    profile: &Profile,
    contact_id: &str,
) -> Result<ConversationMode, &'static str> {
    select_conversation_mode(None, protocol_availability(profile, contact_id))
}

pub(crate) fn pending_mapping(profile: &Profile, contact_id: &str) -> Result<ProfilePatch, String> {
    let contact = profile
        .contacts
        .iter()
        .find(|item| item.id == contact_id)
        .ok_or_else(|| "Contact no longer exists".to_string())?;
    if contact.kaspa_address().trim().is_empty() {
        return Err("Contact has no Kaspa address".into());
    }
    let mapping =
        KasiaContactMapping::pending(contact.id.clone(), contact.kaspa_address().to_owned());
    let mut patch = ProfilePatch::new(&profile.id);
    patch.kasia_contact(mapping);
    Ok(patch)
}

pub(crate) async fn send_handshake(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
    response: bool,
) -> Result<BroadcastResult, String> {
    crate::native::send_kasia_handshake(profile, password, mapping, response).await
}

pub(crate) async fn send_message(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
    text: &str,
) -> Result<BroadcastResult, String> {
    crate::native::send_kasia_message(profile, password, mapping, text).await
}

pub(crate) async fn history(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
    indexer_url: &str,
    block_time: u64,
) -> Result<Vec<KasiaMessage>, String> {
    crate::native::kasia_history(profile, password, mapping, indexer_url, block_time).await
}

pub(crate) struct KasiaSyncResult {
    pub(crate) patch: Option<ProfilePatch>,
    pub(crate) messages: Vec<KasiaMessage>,
}

pub(crate) async fn sync(
    profile: &Profile,
    password: &str,
    mapping: &KasiaContactMapping,
    indexer_url: &str,
    block_time: u64,
) -> Result<KasiaSyncResult, String> {
    let received = received_handshakes(profile, password, indexer_url, block_time).await?;
    let mut updated = mapping.clone();
    let peer_address = updated.kaspa_address.clone();
    let mut changed = false;
    for item in received
        .into_iter()
        .filter(|item| item.sender_address == peer_address)
    {
        updated.apply_peer_handshake(
            item.handshake.alias.as_deref(),
            item.handshake.conversation_id.as_deref(),
        );
        changed = true;
        if item.handshake.is_response != Some(true) {
            let _ = send_handshake(profile, password, &updated, true).await;
        }
    }
    let messages = if updated.established {
        history(profile, password, &updated, indexer_url, block_time).await?
    } else {
        Vec::new()
    };
    let patch = changed.then(|| {
        let mut patch = ProfilePatch::new(&profile.id);
        patch.kasia_contact(updated);
        patch
    });
    Ok(KasiaSyncResult { patch, messages })
}

pub(crate) async fn received_handshakes(
    profile: &Profile,
    password: &str,
    indexer_url: &str,
    block_time: u64,
) -> Result<Vec<KasiaReceivedHandshake>, String> {
    crate::native::received_kasia_handshakes(profile, password, indexer_url, block_time).await
}
