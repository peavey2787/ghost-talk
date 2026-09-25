use super::state::{spawn_local, AppRuntime, AppState};
use crate::{model::Profile, native, storage};
use yew::prelude::*;

#[hook]
pub(crate) fn use_initial_profile_load(state: AppState, live: AppRuntime) {
    use_effect_with((), move |_| {
        spawn_local(load_initial_profiles(state.clone(), live.clone()));
        || ()
    });
}

pub(crate) async fn load_initial_profiles(state: AppState, live: AppRuntime) {
    let mut resolved = load_profiles_or_report(&state).await;
    try_automatic_unlock(&mut resolved, &state, &live).await;
    live.replace_profiles(&state, resolved);
    state.loaded.set(true);
}

pub(crate) async fn load_profiles_or_report(state: &AppState) -> Vec<Profile> {
    match native::load_profile_state().await {
        Ok(Some(raw)) => storage::parse_profiles(&raw),
        Ok(None) => Vec::new(),
        Err(error) => {
            state
                .app_status
                .set(format!("Unable to load local IDs: {error}"));
            Vec::new()
        }
    }
}

pub(crate) async fn try_automatic_unlock(
    resolved: &mut [Profile],
    state: &AppState,
    live: &AppRuntime,
) {
    let candidates = auto_login_candidates(resolved);
    if candidates.len() > 1 {
        state.app_status.set("More than one local ID is configured for automatic login. Choose an ID and leave automatic login enabled for only one of them.".into());
        return;
    }
    let Some((index, profile)) = candidates.into_iter().next() else {
        return;
    };
    attempt_automatic_unlock(resolved, index, profile, state, live).await;
}

pub(crate) fn auto_login_candidates(profiles: &[Profile]) -> Vec<(usize, Profile)> {
    profiles
        .iter()
        .enumerate()
        .filter(|(_, profile)| profile.auto_login())
        .map(|(index, profile)| (index, profile.clone()))
        .collect()
}

pub(crate) async fn attempt_automatic_unlock(
    resolved: &mut [Profile],
    index: usize,
    profile: Profile,
    state: &AppState,
    live: &AppRuntime,
) {
    let password = match native::load_remembered_unlock(&profile.id).await {
        Ok(Some(password)) => password,
        Ok(None) => {
            state.app_status.set("Automatic login is enabled, but this device no longer has the remembered unlock credential. Enter the ID password once to re-enable it.".into());
            return;
        }
        Err(error) => {
            state
                .app_status
                .set(format!("Automatic unlock unavailable: {error}"));
            return;
        }
    };
    let persisted = profile.clone();
    match native::unlock_profile_runtime(profile, &password).await {
        Ok(mut profile) => {
            let changed = profile != persisted;
            if changed {
                profile.ensure_state_revision_at_least(persisted.state_revision());
                profile.advance_state_revision_after(persisted.state_revision());
            }
            finish_automatic_unlock(resolved, index, profile, password, changed, state, live).await;
        }
        Err(error) => state
            .app_status
            .set(format!("Automatic unlock failed: {error}")),
    }
}

pub(crate) async fn finish_automatic_unlock(
    resolved: &mut [Profile],
    index: usize,
    profile: Profile,
    password: String,
    changed: bool,
    state: &AppState,
    live: &AppRuntime,
) {
    resolved[index] = profile.clone();
    if changed {
        let serialized = storage::serialize_profiles(resolved);
        if let Err(error) =
            native::save_profile_state(&serialized, std::slice::from_ref(&profile.id)).await
        {
            web_sys::console::error_1(
                &format!("Ghost Talk automatic-unlock state persistence failed: {error}").into(),
            );
        }
    }
    live.replace_profiles(state, resolved.to_vec());
    live.activate(state, profile.id.clone(), password);
    state.app_status.set(String::new());
    super::profile_runtime::launch_wallet_monitor(state.clone(), live.clone(), profile);
}
