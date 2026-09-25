use super::super::state::{AppRuntime, AppState};
use crate::{model::Profile, native, storage};
use wasm_bindgen_futures::spawn_local;

pub(crate) fn replace_and_persist(state: &AppState, runtime: &AppRuntime, profile: Profile) {
    let mut next = runtime.profiles.borrow().clone();
    upsert_profile(&mut next, profile);
    let _ = persist_profiles(state, runtime, next);
}

pub(crate) fn persist_profiles(
    state: &AppState,
    runtime: &AppRuntime,
    mut next: Vec<Profile>,
) -> Vec<Profile> {
    let previous = runtime.profiles.borrow().clone();
    let changed_profile_ids = update_profile_revisions(&previous, &mut next);
    let serialized = storage::serialize_profiles(&next);
    runtime.replace_profiles(state, next.clone());
    schedule_profile_save(serialized, changed_profile_ids);
    next
}

fn update_profile_revisions(previous: &[Profile], next: &mut [Profile]) -> Vec<String> {
    let mut changed = Vec::new();
    for profile in next {
        let old = previous.iter().find(|candidate| candidate.id == profile.id);
        if advance_revision(profile, old) {
            changed.push(profile.id.clone());
        }
    }
    changed
}

fn advance_revision(profile: &mut Profile, previous: Option<&Profile>) -> bool {
    let Some(old) = previous else {
        profile.ensure_state_revision_at_least(1);
        return true;
    };
    profile.ensure_state_revision_at_least(old.state_revision());
    if old == profile {
        return false;
    }
    profile.advance_state_revision_after(old.state_revision());
    true
}

fn schedule_profile_save(serialized: String, changed_profile_ids: Vec<String>) {
    if changed_profile_ids.is_empty() {
        return;
    }
    spawn_local(async move {
        if let Err(error) = native::save_profile_state(&serialized, &changed_profile_ids).await {
            web_sys::console::error_1(
                &format!("Ghost Talk profile persistence failed: {error}").into(),
            );
        }
    });
}

pub(crate) fn upsert_profile(profiles: &mut Vec<Profile>, mut profile: Profile) {
    if let Some(slot) = profiles
        .iter_mut()
        .find(|candidate| candidate.id == profile.id)
    {
        profile.ensure_state_revision_at_least(slot.state_revision());
        *slot = profile;
    } else {
        profiles.push(profile);
    }
}
