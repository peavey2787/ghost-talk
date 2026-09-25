use super::super::form::{prevent_submit, status_view, text_input};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::identity::{on_select, on_textarea, ActivatedProfile, IdentityGateProps, IdentityState};
use super::kaskold_restore::KasKoldRestoreImport;

pub(super) fn add_id_callback(state: &IdentityState) -> Callback<MouseEvent> {
    let state = state.clone();
    Callback::from(move |_| reset_create_form(&state, true))
}

fn reset_create_form(state: &IdentityState, creating: bool) {
    state.mode.set("create".into());
    state.selected.set(String::new());
    state.label.set(String::new());
    state.password.set(String::new());
    state.mnemonic.set(String::new());
    state.passphrase.set(String::new());
    state.network.set("testnet-10".into());
    state.account_path.set("m/44'/111111'/0'".into());
    state.status.set(String::new());
    state.creating.set(creating);
}

pub(super) fn render_create_gate(props: &IdentityGateProps, state: &IdentityState) -> Html {
    html! {
      <main class="gate-shell"><section class="gate-card">
        <img class="brand-mark" src={crate::ui_assets::ghost_talk_logo_src()} alt="Ghost Talk" />
        <h1>{"Ghost Talk"}</h1>
        <p class="muted">{"Create or restore a private Ghost Talk ID."}</p>
        {render_mode_switch(state)}
        <form class="create-id" onsubmit={prevent_submit()}>
          <label>{"ID name"}<input value={(*state.label).clone()} oninput={text_input(state.label.clone())} placeholder="Your local display name" /></label>
          <label>{"Password"}<input type="password" value={(*state.password).clone()} oninput={text_input(state.password.clone())} placeholder="At least 8 characters" /></label>
          <label>{"Kaspa network"}<select value={(*state.network).clone()} onchange={on_select(state.network.clone())}><option value="mainnet">{"Mainnet"}</option><option value="testnet-10">{"Testnet 10"}</option></select></label>
          <label>{"Kaspa account path"}<input value={(*state.account_path).clone()} oninput={text_input(state.account_path.clone())} /></label>
          <label>{"BIP39 passphrase (optional)"}<input type="password" value={(*state.passphrase).clone()} oninput={text_input(state.passphrase.clone())} /></label>
          {render_kaskold_restore(state)}
          {render_restore_words(state)}
          <button type="submit" class="primary wide" disabled={state.label.trim().is_empty() || state.password.len() < 8 || *state.busy} onclick={submit_profile_callback(props, state)}>{submit_button_text(state)}</button>
          {render_back_to_ids(props, state)}
          {status_view(&state.status)}
        </form>
      </section></main>
    }
}

fn render_mode_switch(state: &IdentityState) -> Html {
    html! {
        <div class="segmented">
          <button class={(*state.mode == "create").then_some("active")} onclick={set_mode_callback(state, "create")}>{"Create"}</button>
          <button class={(*state.mode == "restore").then_some("active")} onclick={set_mode_callback(state, "restore")}>{"Restore"}</button>
        </div>
    }
}

fn set_mode_callback(state: &IdentityState, mode: &'static str) -> Callback<MouseEvent> {
    let current_mode = state.mode.clone();
    let mnemonic = state.mnemonic.clone();
    let status = state.status.clone();
    Callback::from(move |_| {
        current_mode.set(mode.into());
        if mode == "create" {
            mnemonic.set(String::new());
        }
        status.set(String::new());
    })
}

fn render_kaskold_restore(state: &IdentityState) -> Html {
    if *state.mode != "restore" {
        return Html::default();
    }
    html! {
        <KasKoldRestoreImport
            mnemonic={state.mnemonic.clone()}
            passphrase={state.passphrase.clone()}
        />
    }
}

fn render_restore_words(state: &IdentityState) -> Html {
    if *state.mode != "restore" {
        return Html::default();
    }
    html! {<label>{"24 recovery words"}<textarea rows="4" value={(*state.mnemonic).clone()} oninput={on_textarea(state.mnemonic.clone())} placeholder="word1 word2 …" /></label>}
}

fn submit_button_text(state: &IdentityState) -> &'static str {
    if *state.busy {
        "Working…"
    } else if *state.mode == "create" {
        "Create Ghost Talk ID"
    } else {
        "Restore Ghost Talk ID"
    }
}

fn render_back_to_ids(props: &IdentityGateProps, state: &IdentityState) -> Html {
    if props.profiles.is_empty() {
        return Html::default();
    }
    let state = state.clone();
    html! {<button type="button" onclick={Callback::from(move |_| reset_create_form(&state, false))}>{"Back to IDs"}</button>}
}

fn submit_profile_callback(
    props: &IdentityGateProps,
    state: &IdentityState,
) -> Callback<MouseEvent> {
    let state = state.clone();
    let on_activate = props.on_activate.clone();
    Callback::from(move |_| {
        if let Err(error) = validate_profile_form(&state) {
            state.status.set(error);
            return;
        }
        state.busy.set(true);
        state.status.set(if *state.mode == "create" {
            "Creating Ghost Talk ID…".into()
        } else {
            "Restoring Ghost Talk ID…".into()
        });
        create_or_restore_async(state.clone(), on_activate.clone());
    })
}

fn validate_profile_form(state: &IdentityState) -> Result<(), String> {
    if state.label.trim().is_empty() || state.password.len() < 8 || *state.busy {
        return Err("Enter an ID name and password with at least 8 characters.".into());
    }
    if *state.mode == "restore" && state.mnemonic.split_whitespace().count() != 24 {
        return Err("Restore requires exactly 24 Ghost Talk recovery words.".into());
    }
    Ok(())
}

fn create_or_restore_async(state: IdentityState, on_activate: Callback<ActivatedProfile>) {
    let input = identity_setup(&state);
    spawn_local(async move {
        let result = crate::controllers::account::create_or_restore_profile(input.clone()).await;
        state.busy.set(false);
        match result {
            Ok((profile, Some(words))) => state.backup.set(Some((profile, input.password, words))),
            Ok((profile, None)) => on_activate.emit(ActivatedProfile {
                profile,
                password: input.password,
            }),
            Err(error) => state.status.set(error),
        }
    });
}

fn identity_setup(state: &IdentityState) -> crate::controllers::account::IdentitySetup {
    crate::controllers::account::IdentitySetup {
        mode: (*state.mode).clone(),
        label: (*state.label).clone(),
        password: (*state.password).clone(),
        mnemonic: (*state.mnemonic).clone(),
        passphrase: (*state.passphrase).clone(),
        network: (*state.network).clone(),
        account_path: (*state.account_path).clone(),
    }
}
