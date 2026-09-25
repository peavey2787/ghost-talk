use super::state::ActivatedProfile;
use super::state::IdentityGate;
use super::{
    numeric::decimal_cmp,
    profile_updates::apply_profile_patch,
    shell::{
        new_resolved_chat, open_resolved_existing_chat, persist_profiles, replace_and_persist,
        select_chat, upsert_profile,
    },
    state::{
        live_profile_by_id, native, resolve_recipient_target, spawn_local, AppRuntime, AppState,
    },
};
use crate::model::Profile;
use crate::model::ProfilePatch;
use yew::prelude::*;
mod chat;
mod network;
pub(crate) use chat::start_chat_callback;
pub(crate) use network::{apply_directory_event, launch_wallet_monitor, register_network_listener};

pub(crate) fn update_profile_callback(state: AppState, live: AppRuntime) -> Callback<ProfilePatch> {
    Callback::from(move |patch: ProfilePatch| {
        let profile_id = patch.profile_id().to_string();
        let mut next = live.profiles.borrow().clone();
        let Some(index) = next.iter().position(|candidate| candidate.id == profile_id) else {
            state
                .app_status
                .set("Profile update targeted an ID that is no longer loaded.".into());
            return;
        };
        let merged = match apply_profile_patch(&next[index], patch) {
            Ok(profile) => profile,
            Err(error) => {
                state.app_status.set(format!("Profile update: {error}"));
                return;
            }
        };
        let disabled = enforce_single_auto_login(&mut next, &merged);
        next[index] = merged.clone();
        persist_profiles(&state, &live, next);
        disable_remembered_profiles(disabled);
        update_active_wallet_monitor(&state, merged);
    })
}

pub(crate) fn call_profile_update_callback(
    state: AppState,
    live: AppRuntime,
) -> Callback<ProfilePatch> {
    update_profile_callback(state, live)
}

pub(crate) fn enforce_single_auto_login(
    profiles: &mut [Profile],
    profile: &Profile,
) -> Vec<Profile> {
    if !profile.auto_login() {
        return Vec::new();
    }
    let mut disabled = Vec::new();
    for other in profiles
        .iter_mut()
        .filter(|other| other.id != profile.id && other.auto_login())
    {
        disabled.push(other.clone());
        other.set_auto_login(false);
    }
    disabled
}

pub(crate) fn disable_remembered_profiles(profiles: Vec<Profile>) {
    for profile in profiles {
        spawn_local(async move {
            let _ = native::set_remembered_unlock(&profile, false, "").await;
        });
    }
}

pub(crate) fn update_active_wallet_monitor(state: &AppState, profile: Profile) {
    if state.active_id.as_deref() != Some(profile.id.as_str()) {
        return;
    }
    let Some(wallet) = profile.wallet else {
        return;
    };
    spawn_local(async move {
        let _ = native::update_wallet_monitor_public(&profile.id, &wallet.public).await;
    });
}

pub(crate) fn activate_profile_callback(
    state: AppState,
    live: AppRuntime,
) -> Callback<ActivatedProfile> {
    Callback::from(move |activated: ActivatedProfile| {
        let mut next = live.profiles.borrow().clone();
        upsert_profile(&mut next, activated.profile.clone());
        persist_profiles(&state, &live, next);
        live.activate(&state, activated.profile.id.clone(), activated.password);
        state.snapshot.set(None);
        state.app_status.set(String::new());
        launch_wallet_monitor(state.clone(), live.clone(), activated.profile);
    })
}

pub(crate) fn switch_user_callback(state: AppState, live: AppRuntime) -> Callback<()> {
    Callback::from(move |_| {
        let Some(id) = (*state.active_id).clone() else {
            return;
        };
        let state = state.clone();
        let live = live.clone();
        spawn_local(async move {
            switch_user(id, state, live).await;
        });
    })
}

pub(crate) async fn switch_user(id: String, state: AppState, live: AppRuntime) {
    if let Err(error) = native::lock_profile(&id).await {
        state
            .app_status
            .set(format!("Could not lock Ghost Talk ID: {error}"));
        return;
    }
    live.clear_session(&state);
    live.select_tab(&state, "Chats".into());
    live.select_chat(&state, String::new());
    state.snapshot.set(None);
    state.app_status.set(String::new());
}

pub(crate) fn loading_view() -> Html {
    html! { <main class="gate-shell"><section class="gate-card"><h1>{"Ghost Talk"}</h1><p>{"Loading local IDs…"}</p></section></main> }
}

pub(crate) fn identity_gate_view(
    state: &AppState,
    activate: Callback<ActivatedProfile>,
    update: Callback<ProfilePatch>,
) -> Html {
    html! {
        <IdentityGate profiles={(*state.profiles).clone()} initial_status={(*state.app_status).clone()} on_activate={activate} on_update={update}/>
    }
}

pub(crate) fn active_profile(state: &AppState) -> Option<Profile> {
    state
        .profiles
        .iter()
        .find(|profile| Some(&profile.id) == state.active_id.as_ref())
        .cloned()
}
