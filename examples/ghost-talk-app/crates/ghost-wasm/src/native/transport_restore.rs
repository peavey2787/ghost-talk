use super::{
    commands::{lock_profile, unlock_wallet},
    invoke::{
        canonical_peer_routes, ensure_hydra, inspect_surviving_bindings, migrate_session_roles,
        register_peer_routes, Chat, HydraSessionBindingProjection, Profile,
    },
    restore::leave_peer,
};
pub(crate) fn apply_transport_restore_state(
    profile: &mut Profile,
    surviving: &std::collections::HashMap<String, HydraSessionBindingProjection>,
    restoring: &std::collections::HashMap<String, (Option<String>, Option<String>)>,
) {
    let jobs: Vec<(String, String)> = profile
        .chats
        .iter()
        .filter(|chat| !chat.left() && !chat.peer_left())
        .filter_map(|chat| {
            chat.peer_hydra_handle()
                .map(|peer| (chat.id.clone(), peer.to_owned()))
        })
        .collect();
    for (chat_id, peer) in jobs {
        if let Some(binding) = surviving.get(&peer) {
            apply_surviving_binding(&mut profile.chats, &chat_id, binding);
        } else if let Some((sid, role)) = restoring.get(&peer) {
            mark_transport_restore_pending(&mut profile.chats, &chat_id, sid, role);
        }
    }
}

pub(crate) fn apply_surviving_binding(
    chats: &mut ghost_chat::ChatStore,
    chat_id: &str,
    binding: &HydraSessionBindingProjection,
) {
    let _ = ghost_chat::HydraSessionManager::establish(
        chats,
        chat_id,
        binding.sid.clone(),
        binding.role.clone(),
    );
}

pub(crate) fn mark_transport_restore_pending(
    chats: &mut ghost_chat::ChatStore,
    chat_id: &str,
    sid: &Option<String>,
    role: &Option<String>,
) {
    let _ =
        ghost_chat::HydraSessionManager::mark_restore(chats, chat_id, sid.clone(), role.clone());

    // Native pending ids belong to the volatile HYDRA runtime. If that runtime
    // disappeared during a handshake, retaining the id across restart makes the
    // frontend believe a now-nonexistent handshake is still in flight forever.
    let message_ids: Vec<String> = ghost_chat::ChatService::by_id(chats, chat_id)
        .map(|chat| {
            chat.messages()
                .iter()
                .filter(|message| {
                    message.direction == "out"
                        && message.pending
                        && message.pending_stage.as_deref() == Some("handshake")
                        && message.txid.is_none()
                })
                .map(|message| message.id.clone())
                .collect()
        })
        .unwrap_or_default();
    for message_id in message_ids {
        let _ = ghost_chat::ChatService::clear_message_pending_id_for_restore(
            chats,
            chat_id,
            &message_id,
        );
    }
}

pub(crate) async fn close_inactive_peers(
    profile_id: &str,
    peer_has_live_thread: std::collections::HashMap<String, bool>,
) {
    for (contact_id, live) in peer_has_live_thread {
        if !live {
            let _ = leave_peer(profile_id, &contact_id).await;
        }
    }
}

pub async fn unlock_profile_runtime(
    mut profile: Profile,
    password: &str,
) -> Result<Profile, String> {
    unlock_wallet(&profile, password).await?;
    let hydra =
        match ensure_hydra(&profile.id, password, profile.hydra_identity_id.as_deref()).await {
            Ok(hydra) => hydra,
            Err(error) => {
                let _ = lock_profile(&profile.id).await;
                return Err(error);
            }
        };
    profile.hydra_identity_id = Some(hydra.identity_id.clone());
    let local_hydra_id = profile.hydra_identity_id.clone().unwrap_or_default();
    migrate_session_roles(&mut profile.chats, &profile.rooms, &local_hydra_id);
    let (routes, live_peers) = canonical_peer_routes(&profile);
    if let Err(error) = register_peer_routes(&profile.id, &hydra.identity_id, &routes).await {
        let _ = lock_profile(&profile.id).await;
        return Err(error);
    }
    let (surviving, restoring) = inspect_surviving_bindings(&profile.id, &routes).await?;
    apply_transport_restore_state(&mut profile, &surviving, &restoring);
    close_inactive_peers(&profile.id, live_peers).await;
    // Unlock must not wait on Kaspa/network session restoration. The app starts
    // the wallet monitor immediately after activation, and the connected-network
    // listener resumes any transports marked pending here. This keeps release
    // unlock latency bounded by local cryptographic/storage work.
    Ok(profile)
}

pub(crate) fn restore_candidate(chat: &Chat) -> bool {
    !chat.left()
        && !chat.peer_left()
        && chat.transport_restore_pending()
        && chat.session_role() == Some("initiator")
}

pub(crate) fn existing_restore_ids(profile: &Profile) -> std::collections::HashMap<String, String> {
    let mut ids = std::collections::HashMap::new();
    for chat in profile.chats.iter().filter(|chat| restore_candidate(chat)) {
        let (Some(peer), Some(id)) = (
            chat.peer_hydra_handle(),
            chat.transport_restore_message_id(),
        ) else {
            continue;
        };
        ids.entry(peer.to_owned()).or_insert_with(|| id.to_owned());
    }
    ids
}

pub(crate) fn peers_needing_restore_ids(profile: &Profile) -> std::collections::HashSet<String> {
    profile
        .chats
        .iter()
        .filter(|chat| restore_candidate(chat))
        .filter_map(|chat| chat.peer_hydra_handle().map(str::to_owned))
        .collect()
}
