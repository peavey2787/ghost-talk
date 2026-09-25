use super::super::form::{prevent_submit, status_view, text_input};
use super::identity_create::{add_id_callback, render_create_gate};
use crate::model::{Profile, ProfilePatch};
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement};
use yew::prelude::*;
mod passwordless;
use passwordless::render_passwordless_toggle;

#[derive(Clone, PartialEq)]
pub struct ActivatedProfile {
    pub profile: Profile,
    pub password: String,
}

#[derive(Properties, PartialEq)]
pub struct IdentityGateProps {
    pub profiles: Vec<Profile>,
    pub initial_status: String,
    pub on_activate: Callback<ActivatedProfile>,
    pub on_update: Callback<ProfilePatch>,
}

#[derive(Clone)]
pub(super) struct IdentityState {
    pub(super) creating: UseStateHandle<bool>,
    pub(super) mode: UseStateHandle<String>,
    pub(super) selected: UseStateHandle<String>,
    pub(super) label: UseStateHandle<String>,
    pub(super) password: UseStateHandle<String>,
    pub(super) mnemonic: UseStateHandle<String>,
    pub(super) passphrase: UseStateHandle<String>,
    pub(super) network: UseStateHandle<String>,
    pub(super) account_path: UseStateHandle<String>,
    pub(super) status: UseStateHandle<String>,
    pub(super) busy: UseStateHandle<bool>,
    pub(super) backup: UseStateHandle<Option<(Profile, String, String)>>,
    pub(super) password_input: NodeRef,
}

#[component(IdentityGate)]
pub fn identity_gate(props: &IdentityGateProps) -> Html {
    let state = IdentityState {
        creating: use_state(|| props.profiles.is_empty()),
        mode: use_state(|| "create".to_string()),
        selected: use_state(|| single_profile_id(&props.profiles)),
        label: use_state(String::new),
        password: use_state(String::new),
        mnemonic: use_state(String::new),
        passphrase: use_state(String::new),
        network: use_state(|| "testnet-10".to_string()),
        account_path: use_state(|| "m/44'/111111'/0'".to_string()),
        status: use_state(|| props.initial_status.clone()),
        busy: use_state(|| false),
        backup: use_state(|| None::<(Profile, String, String)>),
        password_input: use_node_ref(),
    };
    if let Some(backup) = (*state.backup).clone() {
        return render_backup_gate(props, &state, backup);
    }
    if !*state.creating && !props.profiles.is_empty() {
        return render_unlock_gate(props, &state);
    }
    render_create_gate(props, &state)
}

fn single_profile_id(profiles: &[Profile]) -> String {
    if profiles.len() == 1 {
        profiles[0].id.clone()
    } else {
        String::new()
    }
}

fn render_backup_gate(
    props: &IdentityGateProps,
    state: &IdentityState,
    backup: (Profile, String, String),
) -> Html {
    let (profile, password, words) = backup;
    html! {
        <main class="gate-shell"><section class="gate-card backup-card">
            <img class="brand-mark" src={crate::ui_assets::ghost_talk_logo_src()} alt="Ghost Talk" />
            <h1>{"Back up your Ghost Talk ID"}</h1>
            <p class="warning">{"Write down these 24 recovery words before continuing. They restore both the local Kaspa wallet and the deterministic HYDRA messaging identity."}</p>
            <div class="recovery-grid">{for words.split_whitespace().enumerate().map(render_recovery_word)}</div>
            <button class="primary wide" onclick={confirm_backup_callback(props, state, profile, password)}>{"I saved my recovery words"}</button>
        </section></main>
    }
}

fn render_recovery_word((index, word): (usize, &str)) -> Html {
    html! {<span><b>{format!("{}.", index + 1)}</b>{word.to_string()}</span>}
}

fn confirm_backup_callback(
    props: &IdentityGateProps,
    state: &IdentityState,
    profile: Profile,
    password: String,
) -> Callback<MouseEvent> {
    let backup = state.backup.clone();
    let on_activate = props.on_activate.clone();
    Callback::from(move |_| {
        backup.set(None);
        let activated = crate::controllers::account::confirm_recovery_backup(&profile);
        on_activate.emit(ActivatedProfile {
            profile: activated,
            password: password.clone(),
        });
    })
}

fn render_unlock_gate(props: &IdentityGateProps, state: &IdentityState) -> Html {
    let selected_profile = props
        .profiles
        .iter()
        .find(|profile| profile.id == *state.selected);
    html! {
      <main class="gate-shell"><section class="gate-card">
        <img class="brand-mark" src={crate::ui_assets::ghost_talk_logo_src()} alt="Ghost Talk" />
        <h1>{"Ghost Talk"}</h1>
        <p class="muted">{"Choose a local ID and unlock its wallet + encrypted mailbox."}</p>
        <div class="identity-list">{for props.profiles.iter().map(|profile| render_identity_row(profile, state))}</div>
        <form class="gate-unlock-form" onsubmit={prevent_submit()}>
          <label>{"Password"}<input ref={state.password_input.clone()} type="password" value={(*state.password).clone()} oninput={text_input(state.password.clone())} placeholder="ID / wallet password" /></label>
          {render_passwordless_toggle(props, state, selected_profile)}
          <div class="button-row gate-actions">
            <button type="submit" class="primary" disabled={state.selected.is_empty() || state.password.len() < 8 || *state.busy} onclick={unlock_callback(props, state)}>{if *state.busy {"Working…"} else {"Unlock"}}</button>
            <button type="button" disabled={*state.busy} onclick={add_id_callback(state)}>{"Add ID"}</button>
          </div>
          {status_view(&state.status)}
        </form>
      </section></main>
    }
}

fn render_identity_row(profile: &Profile, state: &IdentityState) -> Html {
    let id = profile.id.clone();
    let selected = state.selected.clone();
    let password = state.password.clone();
    let status = state.status.clone();
    let password_input = state.password_input.clone();
    let active = *state.selected == profile.id;
    let network = profile
        .wallet
        .as_ref()
        .map(|wallet| wallet.public.network.clone())
        .unwrap_or_else(|| "No wallet".into());
    html! {
      <button class={classes!("identity-row", active.then_some("active"))} onclick={Callback::from(move |_| {
          selected.set(id.clone());
          password.set(String::new());
          status.set(String::new());
          if let Some(input) = password_input.cast::<HtmlInputElement>() {
              let _ = input.focus();
          }
      })}>
        <span class="avatar">{profile.label.chars().next().unwrap_or('G').to_ascii_uppercase()}</span>
        <span><b>{profile.label.clone()}</b><small>{network}</small></span>
        <span class="arrow">{"›"}</span>
      </button>
    }
}

fn unlock_callback(props: &IdentityGateProps, state: &IdentityState) -> Callback<MouseEvent> {
    let profiles = props.profiles.clone();
    let selected = state.selected.clone();
    let password = state.password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_activate = props.on_activate.clone();
    Callback::from(move |_| {
        let Some((profile, pass)) = selected_profile_credentials(&profiles, &selected, &password)
        else {
            return;
        };
        if *busy {
            return;
        }
        busy.set(true);
        status.set("Unlocking wallet + HYDRA mailbox…".into());
        unlock_profile_async(
            profile,
            pass,
            busy.clone(),
            status.clone(),
            on_activate.clone(),
        );
    })
}

pub(super) fn selected_profile_credentials(
    profiles: &[Profile],
    selected: &UseStateHandle<String>,
    password: &UseStateHandle<String>,
) -> Option<(Profile, String)> {
    let id = selected.as_str();
    if id.is_empty() || password.len() < 8 {
        return None;
    }
    profiles
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .map(|profile| (profile, (**password).clone()))
}

fn unlock_profile_async(
    profile: Profile,
    password: String,
    busy: UseStateHandle<bool>,
    status: UseStateHandle<String>,
    on_activate: Callback<ActivatedProfile>,
) {
    spawn_local(async move {
        match crate::controllers::account::unlock_profile_runtime(profile, &password).await {
            Ok(profile) => on_activate.emit(ActivatedProfile { profile, password }),
            Err(error) => status.set(error),
        }
        busy.set(false);
    });
}

pub(super) fn on_textarea(state: UseStateHandle<String>) -> Callback<InputEvent> {
    Callback::from(move |event: InputEvent| {
        state.set(event.target_unchecked_into::<HtmlTextAreaElement>().value())
    })
}
pub(super) fn on_select(state: UseStateHandle<String>) -> Callback<Event> {
    Callback::from(move |event: Event| {
        state.set(event.target_unchecked_into::<HtmlSelectElement>().value())
    })
}
