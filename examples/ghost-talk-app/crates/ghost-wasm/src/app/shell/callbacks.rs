use crate::model::{WalletProjection, WalletSnapshot};
use yew::prelude::*;

use super::super::state::{AppRuntime, AppState};
use super::navigation::{mark_chat_read, select_chat};
use super::persistence::persist_profiles;

pub(crate) fn realtime_handled_callback(state: AppState, runtime: AppRuntime) -> Callback<String> {
    Callback::from(move |id: String| {
        let mut next = runtime.realtime.borrow().clone();
        next.retain(|item| item.id != id);
        runtime.replace_realtime(&state, next);
    })
}

pub(crate) fn call_event_handled_callback(
    state: AppState,
    runtime: AppRuntime,
) -> Callback<String> {
    Callback::from(move |id: String| {
        let mut next = runtime.call_events.borrow().clone();
        next.retain(|event| event.id() != id);
        runtime.replace_call_events(&state, next);
    })
}

pub(crate) fn app_error_callback(status: UseStateHandle<String>) -> Callback<String> {
    Callback::from(move |message: String| status.set(message))
}

pub(crate) fn call_chat_select_callback(state: AppState, live: AppRuntime) -> Callback<String> {
    Callback::from(move |id: String| {
        if !id.is_empty() {
            select_chat(&state, &live, id);
        }
    })
}

pub(crate) fn chat_select_callback(state: AppState, live: AppRuntime) -> Callback<String> {
    Callback::from(move |id: String| {
        if !id.is_empty() {
            mark_chat_read(&state, &live, &id);
        }
        live.select_chat(&state, id);
    })
}

pub(crate) fn snapshot_callback(
    snapshot: UseStateHandle<Option<WalletSnapshot>>,
) -> Callback<WalletSnapshot> {
    Callback::from(move |value| snapshot.set(Some(value)))
}

pub(crate) fn room_add_contact_callback(state: AppState, live: AppRuntime) -> Callback<String> {
    Callback::from(move |address: String| {
        if address.trim().is_empty() {
            return;
        }
        state.contact_prefill.set(address);
        live.select_tab(&state, "Contacts".into());
    })
}

pub(crate) fn clear_contact_prefill_callback(prefill: UseStateHandle<String>) -> Callback<()> {
    Callback::from(move |_| prefill.set(String::new()))
}

pub(crate) fn wallet_public_callback(
    state: AppState,
    live: AppRuntime,
) -> Callback<WalletProjection> {
    Callback::from(move |public: WalletProjection| {
        let Some(profile_id) = state.active_id.as_ref() else {
            return;
        };
        let mut next = live.profiles.borrow().clone();
        let Some(profile) = next.iter_mut().find(|profile| &profile.id == profile_id) else {
            return;
        };
        if !crate::model::WalletStateService::merge_progress(&mut profile.wallet, public) {
            return;
        }
        persist_profiles(&state, &live, next);
    })
}

pub(crate) fn tab_callback(state: AppState, live: AppRuntime) -> Callback<String> {
    Callback::from(move |value: String| {
        if value == "Chats" {
            let selected = live.selected_chat_id.borrow().clone();
            mark_chat_read(&state, &live, &selected);
        }
        live.select_tab(&state, value);
    })
}

pub(crate) fn open_debug_callback(open: UseStateHandle<bool>) -> Callback<()> {
    Callback::from(move |_| open.set(true))
}

pub(crate) fn close_debug_callback(open: UseStateHandle<bool>) -> Callback<()> {
    Callback::from(move |_| open.set(false))
}
