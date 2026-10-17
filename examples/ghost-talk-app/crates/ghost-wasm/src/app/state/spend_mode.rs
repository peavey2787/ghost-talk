//! Keep the host's per-profile mailbox carrier in step with the active
//! profile's spend mode (Kaspa transactions or the no-spend swarm).

use super::{native, spawn_local, AppState};
use crate::app::active_profile;
use yew::prelude::*;

#[hook]
pub(crate) fn use_spend_mode_sync(state: &AppState) {
    let key = active_profile(state).map(|profile| (profile.id, profile.settings.spend_mode));
    let status = state.app_status.clone();
    use_effect_with(key, move |key| {
        if let Some((profile_id, spend_mode)) = key.clone() {
            spawn_local(async move {
                if let Err(error) = native::set_spend_mode(&profile_id, spend_mode).await {
                    status.set(format!(
                        "Could not switch to {} mode: {error}",
                        spend_mode.label()
                    ));
                }
            });
        }
    });
}
