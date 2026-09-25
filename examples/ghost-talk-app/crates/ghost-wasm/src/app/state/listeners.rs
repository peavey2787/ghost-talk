use super::{live_voice, native, spawn_local, AppRuntime, AppState};
use crate::{
    app::{
        apply_directory_event, apply_profile_patch, mark_chat_read, persist_profiles,
        process_ready_mailbox, register_network_listener, upsert_profile,
    },
    model::{CallRuntimeEvent, DirectoryLiveEvent, Profile, RealtimeControl, WalletLiveEvent},
};
use yew::prelude::*;

#[hook]
pub(crate) fn use_native_event_listeners(state: AppState, runtime: AppRuntime) {
    use_effect_with((), move |_| {
        register_wallet_listener(state.clone(), runtime.clone());
        register_directory_listener(state.clone(), runtime.clone());
        register_network_listener(state.clone(), runtime.clone());
        || ()
    });
}

pub(crate) fn register_wallet_listener(state: AppState, runtime: AppRuntime) {
    native::listen::<WalletLiveEvent, _>("ghost://wallet-live", move |event| {
        handle_wallet_live_event(event, state.clone(), runtime.clone());
    });
}

pub(crate) fn handle_wallet_live_event(
    event: WalletLiveEvent,
    state: AppState,
    runtime: AppRuntime,
) {
    if runtime.active_id.borrow().as_deref() == Some(event.profile_id.as_str()) {
        if let Some(snapshot) = event.snapshot.clone() {
            state.snapshot.set(Some(snapshot));
        }
    }
    let profile_id = event.profile_id.clone();
    let mut current = runtime.profiles.borrow().clone();
    let Some(index) = current.iter().position(|profile| profile.id == profile_id) else {
        return;
    };
    apply_wallet_live_payload(&mut current[index].wallet, &event);
    persist_profiles(&state, &runtime, current);
    runtime
        .mailbox_dirty
        .borrow_mut()
        .insert(profile_id.clone());
    if !runtime.mailbox_busy.borrow_mut().insert(profile_id.clone()) {
        return;
    }
    spawn_local(drain_mailbox_profile(profile_id, state, runtime));
}

pub(crate) fn apply_wallet_live_payload(
    wallet: &mut Option<crate::model::WalletRecord>,
    event: &WalletLiveEvent,
) {
    crate::model::WalletStateService::apply_live_update(
        wallet,
        event.checkpoint.clone(),
        &event.mailbox,
        event.snapshot.as_ref(),
    );
}

pub(crate) async fn drain_mailbox_profile(
    profile_id: String,
    state: AppState,
    runtime: AppRuntime,
) {
    loop {
        runtime.mailbox_dirty.borrow_mut().remove(&profile_id);
        let Some(start_profile) = live_profile_by_id(&runtime, &profile_id) else {
            runtime.mailbox_busy.borrow_mut().remove(&profile_id);
            break;
        };
        let password = runtime.password.borrow().clone();
        match process_ready_mailbox(start_profile, &password).await {
            Ok((_updated, patches, controls, call_events)) => {
                commit_mailbox_result(
                    &profile_id,
                    patches,
                    controls,
                    call_events,
                    &state,
                    &runtime,
                );
            }
            Err(error) => state.app_status.set(format!("Mailbox processing: {error}")),
        }
        if !runtime.mailbox_dirty.borrow().contains(&profile_id) {
            runtime.mailbox_busy.borrow_mut().remove(&profile_id);
            break;
        }
    }
}

pub(crate) fn live_profile_by_id(runtime: &AppRuntime, profile_id: &str) -> Option<Profile> {
    runtime
        .profiles
        .borrow()
        .iter()
        .find(|profile| profile.id == profile_id)
        .cloned()
}

pub(crate) fn commit_mailbox_result(
    profile_id: &str,
    patches: Vec<crate::model::ProfilePatch>,
    controls: Vec<RealtimeControl>,
    call_events: Vec<CallRuntimeEvent>,
    state: &AppState,
    runtime: &AppRuntime,
) {
    let mut next = runtime.profiles.borrow().clone();
    let Some(mut merged) = next
        .iter()
        .find(|profile| profile.id == profile_id)
        .cloned()
    else {
        state
            .app_status
            .set("Mailbox result targeted an ID that is no longer loaded.".into());
        return;
    };
    for patch in patches {
        merged = match apply_profile_patch(&merged, patch) {
            Ok(profile) => profile,
            Err(error) => {
                state.app_status.set(format!("Mailbox processing: {error}"));
                return;
            }
        };
    }
    clear_viewed_chat_unread(profile_id, &mut merged.chats, runtime);
    upsert_profile(&mut next, merged);
    persist_profiles(state, runtime, next);
    merge_realtime_controls(controls, state, runtime);
    merge_call_events(call_events, state, runtime);
    if state.app_status.starts_with("Mailbox processing:") {
        state.app_status.set(String::new());
    }
}

fn clear_viewed_chat_unread(
    profile_id: &str,
    chats: &mut crate::model::ChatStore,
    runtime: &AppRuntime,
) {
    if runtime.active_id.borrow().as_deref() != Some(profile_id)
        || runtime.tab.borrow().as_str() != "Chats"
    {
        return;
    }
    let selected = runtime.selected_chat_id.borrow().clone();
    if selected.is_empty() {
        return;
    }
    let _ = ghost_chat::ChatService::mark_read(chats, &selected);
}

pub(crate) fn merge_realtime_controls(
    controls: Vec<RealtimeControl>,
    state: &AppState,
    runtime: &AppRuntime,
) {
    if controls.is_empty() {
        return;
    }
    let mut next = runtime.realtime.borrow().clone();
    let mut incoming_call_chat = None;
    for control in controls {
        if next.iter().any(|existing| existing.id == control.id) {
            continue;
        }
        if is_incoming_call_request(&control) {
            incoming_call_chat = Some(control.chat_id.clone());
        }
        next.push(control);
    }
    runtime.replace_realtime(state, next);
    if let Some(chat_id) = incoming_call_chat {
        mark_chat_read(state, runtime, &chat_id);
        runtime.select_chat(state, chat_id);
        runtime.select_tab(state, "Chats".into());
    }
}

fn is_incoming_call_request(control: &RealtimeControl) -> bool {
    live_voice::decode_live(&control.body).is_some_and(|packet| packet.kind == "request")
}

pub(crate) fn merge_call_events(
    events: Vec<CallRuntimeEvent>,
    state: &AppState,
    runtime: &AppRuntime,
) {
    if events.is_empty() {
        return;
    }
    let mut next = runtime.call_events.borrow().clone();
    for event in events {
        let id = event.id();
        if !next.iter().any(|existing| existing.id() == id) {
            next.push(event);
        }
    }
    runtime.replace_call_events(state, next);
}

pub(crate) fn register_directory_listener(state: AppState, runtime: AppRuntime) {
    native::listen::<DirectoryLiveEvent, _>("ghost://directory-live", move |event| {
        let mut current = runtime.profiles.borrow().clone();
        if let Some(profile) = current
            .iter_mut()
            .find(|profile| profile.id == event.profile_id)
        {
            apply_directory_event(&mut profile.wallet, &mut profile.public_directory, event);
        }
        persist_profiles(&state, &runtime, current);
    });
}
