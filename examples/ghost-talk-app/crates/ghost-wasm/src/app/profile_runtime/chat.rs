use super::{
    live_profile_by_id, native, new_resolved_chat, open_resolved_existing_chat,
    replace_and_persist, resolve_recipient_target, select_chat, spawn_local, AppRuntime, AppState,
};
use crate::model::{Chat, Contact, Profile, ResolvedGhostPeer};
use ghost_domain::identity::{optional_binding_matches, PeerBinding};
use yew::prelude::*;

pub(crate) fn start_chat_callback(state: AppState, live: AppRuntime) -> Callback<String> {
    Callback::from(move |target: String| {
        let Some(profile) = current_active_profile(&state, &live) else {
            return;
        };
        let password = (*state.session_password).clone();
        if password.is_empty() {
            state
                .app_status
                .set("Unlock the wallet + mailbox before starting a chat.".into());
            return;
        }
        if open_known_contact_chat(&profile, &target, &state, &live) {
            return;
        }
        let target = match resolve_recipient_target(&profile, &target) {
            Ok(value) => value,
            Err(error) => {
                state.app_status.set(error);
                return;
            }
        };
        spawn_local(resolve_and_open_chat(
            profile,
            password,
            target,
            state.clone(),
            live.clone(),
        ));
    })
}

pub(crate) fn current_active_profile(state: &AppState, live: &AppRuntime) -> Option<Profile> {
    live.profiles
        .borrow()
        .iter()
        .find(|profile| Some(&profile.id) == state.active_id.as_ref())
        .cloned()
}

pub(crate) fn open_known_contact_chat(
    profile: &Profile,
    target: &str,
    state: &AppState,
    live: &AppRuntime,
) -> bool {
    // Explicit dot.k syntax always receives a fresh deed proof before an address is trusted.
    if target.trim().to_ascii_lowercase().ends_with(".k") {
        return false;
    }
    let Some(contact) = known_contact(profile, target) else {
        return false;
    };
    let Some(index) = profile
        .chats
        .iter()
        .position(|chat| chat_matches_contact(chat, &contact))
    else {
        return false;
    };
    let mut updated = profile.clone();
    let Some((id, was_archived)) =
        ghost_chat::ChatService::reopen_at_index(&mut updated.chats, index)
    else {
        return false;
    };
    if was_archived {
        replace_and_persist(state, live, updated);
    }
    select_chat(state, live, id);
    true
}

pub(crate) fn known_contact(profile: &Profile, target: &str) -> Option<Contact> {
    let target = target.trim();
    profile
        .contacts
        .iter()
        .find(|contact| {
            contact.label.eq_ignore_ascii_case(target)
                || contact.kaspa_address().eq_ignore_ascii_case(target)
                || contact
                    .kns_name
                    .as_deref()
                    .is_some_and(|kns| kns.eq_ignore_ascii_case(target))
                || contact
                    .dotk_name
                    .as_deref()
                    .is_some_and(|dotk| dotk.eq_ignore_ascii_case(target))
        })
        .cloned()
}

pub(crate) fn chat_matches_contact(chat: &Chat, contact: &Contact) -> bool {
    visible_chat_candidate(chat)
        && (chat.contact_id() == Some(contact.id.as_str())
            || contact.peer_binding().as_ref().is_some_and(|peer| {
                optional_binding_matches(chat.peer_kaspa_address(), chat.peer_hydra_handle(), peer)
            }))
}

pub(crate) fn visible_chat_candidate(chat: &Chat) -> bool {
    !chat.room_transport_only()
        && !chat.left()
        && !chat.peer_left()
        && !chat
            .incoming_request()
            .is_some_and(|request| request.call_id.is_none() && request.state == "ignored")
}

async fn resolve_peer_or_report(
    profile: &Profile,
    password: &str,
    target: &str,
    state: &AppState,
) -> Option<ResolvedGhostPeer> {
    match native::resolve_peer(profile, password, target).await {
        Ok(peer) => Some(peer),
        Err(error) => {
            state.app_status.set(error);
            None
        }
    }
}

fn reopen_resolved_chat(
    chats: &mut crate::model::ChatStore,
    index: usize,
    peer: &ResolvedGhostPeer,
    state: &AppState,
    live: &AppRuntime,
) -> bool {
    let alias_changed = ghost_chat::ChatService::set_peer_name_aliases_at_index(
        chats,
        index,
        peer.kns_name.clone(),
        peer.dotk_name.clone(),
    );
    let reopened = open_resolved_existing_chat(chats, index, state, live);
    alias_changed || reopened
}

pub(crate) async fn resolve_and_open_chat(
    profile: Profile,
    password: String,
    target: String,
    state: AppState,
    live: AppRuntime,
) {
    let Some(peer) = resolve_peer_or_report(&profile, &password, &target, &state).await else {
        return;
    };
    let mut updated = live_profile_by_id(&live, &profile.id).unwrap_or(profile);
    if let Some(index) = resolved_peer_chat_index(&updated, &peer) {
        let changed = reopen_resolved_chat(&mut updated.chats, index, &peer, &state, &live);
        if changed {
            replace_and_persist(&state, &live, updated);
        }
        return;
    }
    let chat = match new_resolved_chat(&updated, peer) {
        Ok(chat) => chat,
        Err(error) => {
            state.app_status.set(error);
            return;
        }
    };
    let id = chat.id.clone();
    ghost_chat::ChatService::insert_front(&mut updated.chats, chat);
    replace_and_persist(&state, &live, updated);
    select_chat(&state, &live, id);
}

pub(crate) fn resolved_peer_chat_index(
    profile: &Profile,
    peer: &ResolvedGhostPeer,
) -> Option<usize> {
    let binding = PeerBinding::new(
        peer.kaspa_address.clone(),
        peer.hydra_handle.as_deref()?.to_owned(),
    )
    .ok()?;
    profile.chats.iter().position(|chat| {
        visible_chat_candidate(chat)
            && optional_binding_matches(
                chat.peer_kaspa_address(),
                chat.peer_hydra_handle(),
                &binding,
            )
    })
}
