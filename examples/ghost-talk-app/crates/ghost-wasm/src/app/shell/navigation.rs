use super::super::state::{AppRuntime, AppState};
use super::persistence::persist_profiles;

pub(crate) fn select_chat(state: &AppState, live: &AppRuntime, id: String) {
    mark_chat_read(state, live, &id);
    live.select_chat(state, id);
    live.select_tab(state, "Chats".into());
}

pub(crate) fn mark_chat_read(state: &AppState, live: &AppRuntime, chat_id: &str) {
    if chat_id.is_empty() {
        return;
    }
    let Some(profile_id) = live.active_id.borrow().clone() else {
        return;
    };
    let mut next = live.profiles.borrow().clone();
    let Some(profile) = next
        .iter_mut()
        .find(|profile| profile.id.as_str() == profile_id.as_str())
    else {
        return;
    };
    let Some(was_unread) = ghost_chat::ChatService::mark_read(&mut profile.chats, chat_id) else {
        return;
    };
    if !was_unread {
        return;
    }
    persist_profiles(state, live, next);
}
