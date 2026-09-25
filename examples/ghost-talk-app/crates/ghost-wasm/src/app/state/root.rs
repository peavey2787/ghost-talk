use super::{use_app_runtime, use_app_state, use_native_event_listeners};
use crate::app::{
    activate_profile_callback, active_profile, app_shell, identity_gate_view, loading_view,
    start_chat_callback, switch_user_callback, update_profile_callback, use_initial_profile_load,
};
use yew::prelude::*;

#[component(App)]
pub fn app() -> Html {
    let state = use_app_state();
    let runtime = use_app_runtime();
    use_initial_profile_load(state.clone(), runtime.clone());
    use_native_event_listeners(state.clone(), runtime.clone());

    let update_profile = update_profile_callback(state.clone(), runtime.clone());
    let activate = activate_profile_callback(state.clone(), runtime.clone());
    if !*state.loaded {
        return loading_view();
    }
    if state.active_id.is_none() {
        return identity_gate_view(&state, activate, update_profile);
    }
    let Some(active) = active_profile(&state) else {
        return identity_gate_view(&state, activate, update_profile);
    };

    let switch_user = switch_user_callback(state.clone(), runtime.clone());
    let start_chat = start_chat_callback(state.clone(), runtime.clone());
    app_shell(
        &state,
        &runtime,
        active,
        update_profile,
        switch_user,
        start_chat,
    )
}
