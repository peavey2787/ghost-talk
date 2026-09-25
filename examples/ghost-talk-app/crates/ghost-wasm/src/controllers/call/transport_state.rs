use crate::model::{
    HydraContactAcceptedProjection, HydraSessionBindingProjection, Profile, ProfilePatch,
};
use ghost_domain::identity::PeerBinding;

pub(crate) fn prepare_bootstrap(
    profile: &Profile,
    chat_id: &str,
    request_id: &str,
) -> Result<(Profile, ProfilePatch), String> {
    let mut updated = profile.clone();
    if !super::super::chat::begin_session(
        &mut updated.chats,
        chat_id,
        Some(request_id.to_string()),
        Some("initiator".into()),
    ) {
        return Err("Call chat no longer exists.".into());
    }
    let patch = super::super::chat::chat_profile_transition(profile, &updated, chat_id);
    Ok((updated, patch))
}

pub(crate) fn apply_session_binding(
    profile: &Profile,
    chat_id: &str,
    binding: HydraSessionBindingProjection,
    pending_handshake: bool,
) -> Option<(Profile, ProfilePatch)> {
    let mut updated = profile.clone();
    let changed = if pending_handshake {
        super::super::chat::begin_session(
            &mut updated.chats,
            chat_id,
            Some(binding.sid),
            Some(binding.role),
        )
    } else {
        super::super::chat::establish_session(
            &mut updated.chats,
            chat_id,
            binding.sid,
            binding.role,
        )
    };
    changed.then(|| {
        let patch = super::super::chat::chat_profile_transition(profile, &updated, chat_id);
        (updated, patch)
    })
}

pub(crate) async fn complete_contact_bootstrap(
    profile: &Profile,
    password: &str,
    chat_id: &str,
    accepted: &HydraContactAcceptedProjection,
    body: &str,
    message_id: &str,
) -> Result<(Profile, ProfilePatch), String> {
    let mut updated = profile.clone();
    let chat_index = updated
        .chats
        .iter()
        .position(|chat| chat.id == chat_id)
        .ok_or_else(|| "Call chat no longer exists.".to_string())?;
    bind_accepted_call_peer(
        &mut updated.contacts,
        &mut updated.chats,
        chat_index,
        accepted,
    );
    let sent = crate::native::send_mailbox_message(
        &updated,
        password,
        &accepted.peer_hydra_id,
        &accepted.peer_address,
        body,
        message_id,
    )
    .await?;
    crate::model::WalletStateService::merge_progress(&mut updated.wallet, sent.public);
    let mut patch = super::super::chat::chat_profile_transition(profile, &updated, chat_id);
    patch.wallet(profile.wallet.clone(), updated.wallet.clone());
    append_contact_delta(profile, &updated, chat_id, &mut patch);
    Ok((updated, patch))
}

fn bind_accepted_call_peer(
    contacts: &mut crate::model::ContactStore,
    chats: &mut crate::model::ChatStore,
    chat_index: usize,
    accepted: &HydraContactAcceptedProjection,
) {
    let contact_id = chats[chat_index].contact_id().map(str::to_owned);
    if let Ok(peer) = PeerBinding::new(
        accepted.peer_address.clone(),
        accepted.peer_hydra_id.clone(),
    ) {
        if let Some(contact_id) = contact_id.as_deref() {
            let _ = ghost_contacts::ContactService::bind_peer_by_id(contacts, contact_id, &peer);
        }
        let _ = super::super::chat::bind_peer_at_index(chats, chat_index, &peer);
    }
    let _ = super::super::chat::begin_session_at_index(
        chats,
        chat_index,
        Some(accepted.request_id.clone()),
        Some("initiator".into()),
    );
}

fn append_contact_delta(
    before: &Profile,
    after: &Profile,
    chat_id: &str,
    patch: &mut ProfilePatch,
) {
    let Some(contact_id) =
        ghost_chat::ChatService::by_id(&after.chats, chat_id).and_then(|chat| chat.contact_id())
    else {
        return;
    };
    let old = ghost_contacts::ContactService::by_id(&before.contacts, contact_id).cloned();
    if let Some(new) = ghost_contacts::ContactService::by_id(&after.contacts, contact_id).cloned() {
        patch.contact_upsert(old, new);
    }
}
