use crate::model::{Chat, Profile, ProfilePatch, WalletProjection};

pub(crate) struct LeaveChatPlan {
    pub(crate) local_profile: Profile,
    pub(crate) peer: Option<String>,
    pub(crate) keep_transport: bool,
    pub(crate) patch: ProfilePatch,
}

pub(crate) fn delete_patch(profile: &Profile, chat: &Chat) -> ProfilePatch {
    let mut patch = ProfilePatch::new(&profile.id);
    patch.chat_remove(chat.clone());
    patch
}

pub(crate) fn prepare_leave(profile: &Profile, chat: &Chat) -> Result<LeaveChatPlan, String> {
    let peer = chat.peer_hydra_handle().map(str::to_owned);
    let mut local = profile.clone();
    let mut patch = ProfilePatch::new(&profile.id);
    let keep_transport = peer
        .as_deref()
        .map(|peer| prepare_peer_transport(profile, &mut local.chats, chat, peer, &mut patch))
        .transpose()?
        .unwrap_or(false);
    let _ = ghost_chat::ChatService::leave(&mut local.chats, &chat.id);
    if let Some(after) = ghost_chat::ChatService::by_id(&local.chats, &chat.id).cloned() {
        patch.chat_upsert(Some(chat.clone()), after);
    }
    Ok(LeaveChatPlan {
        local_profile: local,
        peer,
        keep_transport,
        patch,
    })
}

fn prepare_peer_transport(
    source: &Profile,
    local_chats: &mut crate::model::ChatStore,
    chat: &Chat,
    peer: &str,
    patch: &mut ProfilePatch,
) -> Result<bool, String> {
    let other_live = has_other_live_chat(source, chat, peer);
    let room_needs_peer = ghost_rooms::RoomService::peer_needed_by_room(
        &source.rooms,
        source.hydra_identity_id.as_deref(),
        peer,
    );
    if room_needs_peer && !other_live {
        let transport =
            ghost_chat::ChatService::hidden_room_transport_from(chat, crate::random_id()?);
        ghost_chat::ChatService::push(local_chats, transport.clone());
        patch.chat_upsert(None, transport);
    }
    Ok(other_live || room_needs_peer)
}

fn has_other_live_chat(profile: &Profile, chat: &Chat, peer: &str) -> bool {
    profile.chats.iter().any(|thread| {
        thread.id != chat.id
            && thread.peer_hydra_handle() == Some(peer)
            && !thread.left()
            && !thread.peer_left()
    })
}

pub(crate) async fn notify_leave(
    profile: &Profile,
    chat: &Chat,
    peer: &str,
    password: &str,
) -> Result<Option<WalletProjection>, String> {
    let wallet = notify_peer_if_needed(profile, chat, peer, password).await?;
    cleanup_local_peer(profile, chat, peer).await;
    Ok(wallet)
}

async fn notify_peer_if_needed(
    profile: &Profile,
    chat: &Chat,
    peer: &str,
    password: &str,
) -> Result<Option<WalletProjection>, String> {
    if chat.peer_left() {
        return Ok(None);
    }
    let Some(destination) = chat.peer_kaspa_address() else {
        return Ok(None);
    };
    match crate::native::send_session_end(
        profile, password, peer, destination, chat.session_sid(),
    ).await {
        Ok(sent) => Ok(Some(sent.public)),
        Err(error) if error.contains("stale chat leave") => Ok(None),
        Err(_) => Err(
            "Chat left. Peer notification was not sent because the secure transport was unavailable.".into(),
        ),
    }
}

async fn cleanup_local_peer(profile: &Profile, chat: &Chat, peer: &str) {
    if let Err(error) =
        crate::native::leave_peer_session(&profile.id, peer, chat.session_sid()).await
    {
        web_sys::console::warn_1(
            &format!("Ghost Talk local peer cleanup after Leave failed: {error}").into(),
        );
    }
}

pub(crate) async fn rejoin(
    profile: &Profile,
    chat: &Chat,
) -> Result<(ProfilePatch, &'static str), String> {
    let peer = chat
        .peer_hydra_handle()
        .ok_or_else(|| "This chat has no authenticated HYDRA peer.".to_string())?;
    if let Some(shared) = shared_active_chat(profile, chat, peer) {
        let mut chats = profile.chats.clone();
        let _ = ghost_chat::HydraSessionManager::rejoin(
            &mut chats,
            &chat.id,
            shared.session_sid().map(str::to_owned),
            shared.session_role().map(str::to_owned),
            shared.transport_restore_pending(),
            shared.transport_restore_message_id().map(str::to_owned),
        );
        return Ok((
            super::chat_store_transition(profile, &chats, &chat.id),
            "Rejoined using the existing secure HYDRA peer session.",
        ));
    }
    crate::native::rejoin_peer(&profile.id, peer).await?;
    let mut chats = profile.chats.clone();
    let _ = ghost_chat::HydraSessionManager::rejoin_without_session(&mut chats, &chat.id);
    Ok((
        super::chat_store_transition(profile, &chats, &chat.id),
        "Rejoined locally. Send a message to establish a fresh secure session.",
    ))
}

fn shared_active_chat(profile: &Profile, chat: &Chat, peer: &str) -> Option<Chat> {
    profile
        .chats
        .iter()
        .find(|thread| {
            thread.id != chat.id
                && thread.peer_hydra_handle() == Some(peer)
                && !thread.left()
                && !thread.peer_left()
                && thread.bootstrap_complete()
        })
        .cloned()
}
