use crate::model::{ChatStore, Profile};
use crate::native::transport_restore::{
    existing_restore_ids, peers_needing_restore_ids, restore_candidate,
};

pub(crate) fn allocate_restore_ids(profile: &Profile) -> std::collections::HashMap<String, String> {
    let mut ids = existing_restore_ids(profile);
    for peer in peers_needing_restore_ids(profile) {
        if ids.contains_key(&peer) {
            continue;
        }
        match crate::random_id() {
            Ok(id) => {
                ids.insert(peer, id);
            }
            Err(error) => web_sys::console::warn_1(
                &format!("Ghost Talk could not allocate secure-session restore id: {error}").into(),
            ),
        }
    }
    ids
}

pub(crate) fn assign_restore_ids(
    chats: &mut ChatStore,
    ids: &std::collections::HashMap<String, String>,
) {
    let assignments: Vec<(String, String)> = chats
        .iter()
        .filter(|chat| restore_candidate(chat))
        .filter_map(|chat| {
            let peer = chat.peer_hydra_handle()?;
            ids.get(peer).map(|id| (chat.id.clone(), id.clone()))
        })
        .collect();
    for (chat_id, id) in assignments {
        let _ = ghost_chat::HydraSessionManager::set_restore_message_id(chats, &chat_id, Some(id));
    }
}

pub(crate) fn restore_jobs(profile: &Profile) -> Vec<(String, String, String)> {
    let mut jobs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for chat in profile.chats.iter().filter(|chat| restore_candidate(chat)) {
        let (Some(peer), Some(destination), Some(message_id)) = (
            chat.peer_hydra_handle().map(str::to_owned),
            chat.peer_kaspa_address().map(str::to_owned),
            chat.transport_restore_message_id().map(str::to_owned),
        ) else {
            continue;
        };
        if seen.insert(peer.clone()) {
            jobs.push((peer, destination, message_id));
        }
    }
    jobs
}
