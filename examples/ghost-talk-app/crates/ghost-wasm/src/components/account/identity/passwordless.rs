use super::{selected_profile_credentials, ActivatedProfile, IdentityGateProps, IdentityState};
use crate::model::{Profile, ProfilePatch};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

pub(super) fn render_passwordless_toggle(
    props: &IdentityGateProps,
    state: &IdentityState,
    selected: Option<&Profile>,
) -> Html {
    let Some(profile) = selected else {
        return Html::default();
    };
    html! {
        <label class="toggle-row">
          <input type="checkbox" checked={profile.auto_login()} disabled={*state.busy} onchange={passwordless_callback(props, state)}/>
          <span><b>{"Log in automatically without entering a password"}</b><small>{"When enabled, Ghost Talk stores this ID's unlock credential encrypted for this device and opens it automatically at startup."}</small></span>
        </label>
    }
}

fn passwordless_callback(props: &IdentityGateProps, state: &IdentityState) -> Callback<Event> {
    let profiles = props.profiles.clone();
    let selected = state.selected.clone();
    let password = state.password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    let on_activate = props.on_activate.clone();
    Callback::from(move |event: Event| {
        let enabled = event.target_unchecked_into::<HtmlInputElement>().checked();
        let Some((profile, pass)) = selected_profile_credentials(&profiles, &selected, &password)
        else {
            return;
        };
        if enabled && pass.len() < 8 {
            status.set("Enter this ID's password before enabling automatic login.".into());
            return;
        }
        if *busy {
            return;
        }
        busy.set(true);
        status.set(
            if enabled {
                "Enabling automatic login…"
            } else {
                "Disabling automatic login…"
            }
            .into(),
        );
        persist_passwordless(PasswordlessTask {
            profile,
            password: pass,
            enabled,
            busy: busy.clone(),
            status: status.clone(),
            on_update: on_update.clone(),
            on_activate: on_activate.clone(),
        });
    })
}

struct PasswordlessTask {
    profile: Profile,
    password: String,
    enabled: bool,
    busy: UseStateHandle<bool>,
    status: UseStateHandle<String>,
    on_update: Callback<ProfilePatch>,
    on_activate: Callback<ActivatedProfile>,
}

fn persist_passwordless(task: PasswordlessTask) {
    spawn_local(async move {
        match crate::controllers::account::set_passwordless_and_maybe_unlock(
            &task.profile,
            &task.password,
            task.enabled,
        )
        .await
        {
            Ok(result) => {
                task.on_update.emit(result.patch);
                if let Some(profile) = result.activated {
                    task.on_activate.emit(ActivatedProfile {
                        profile,
                        password: task.password,
                    });
                } else {
                    task.status.set(
                        "Automatic login disabled. This ID will require its password next time."
                            .into(),
                    );
                }
            }
            Err(error) => task.status.set(error),
        }
        task.busy.set(false);
    });
}
