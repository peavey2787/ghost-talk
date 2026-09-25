use super::super::form::{prevent_submit, status_view, text_input};
use super::settings_backup::{backup_callback, backup_fingerprint, restore_backup_callback};
use crate::model::{Profile, ProfilePatch, WalletRecovery};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct SettingsProps {
    pub profile: Profile,
    pub password: String,
    pub on_update: Callback<ProfilePatch>,
    pub on_open_debug: Callback<()>,
}

#[derive(Clone)]
pub(super) struct SettingsState {
    pub(super) recovery: UseStateHandle<Option<WalletRecovery>>,
    pub(super) recovery_password: UseStateHandle<String>,
    pub(super) status: UseStateHandle<String>,
    pub(super) busy: UseStateHandle<bool>,
}

#[component(SettingsView)]
pub fn settings_view(props: &SettingsProps) -> Html {
    let state = SettingsState {
        recovery: use_state(|| None::<WalletRecovery>),
        recovery_password: use_state(String::new),
        status: use_state(String::new),
        busy: use_state(|| false),
    };
    html! {
      <section class="page">
        <div class="page-head"><div><h2>{"Settings"}</h2><p>{"Privacy, transport, wallet security, recovery, and Kaspa endpoints."}</p></div></div>
        {render_voice_settings(props, &state)}
        {render_security_settings(props, &state)}
        {render_recovery(props, &state)}
        {render_backup_settings(props, &state)}
        {render_endpoint_settings(props)}
        {render_debug_settings(props)}
        {status_view(&state.status)}
      </section>
    }
}

fn render_voice_settings(props: &SettingsProps, state: &SettingsState) -> Html {
    html! {
        <div class="card form-grid"><h3>{"Voice & messaging"}</h3>
          <label>{"Realtime route"}<select value={props.profile.settings.route.clone()} onchange={select_setting_callback(props, "route")}><option value="Auto">{"Auto — direct when possible, Kaspa fallback"}</option><option value="Kaspa only">{"Kaspa only"}</option></select></label>
          <label>{"HYDRA steganography"}<select value={props.profile.settings.stego.clone()} onchange={select_setting_callback(props, "stego")}><option>{"Off"}</option><option>{"Deterministic"}</option><option>{"Fast Unicode"}</option><option>{"Fast Hybrid"}</option><option>{"Arithmetic"}</option></select></label>
          <label class="toggle-row"><input type="checkbox" checked={props.profile.settings.auto_ignore_unknown_chats} onchange={toggle_setting_callback(props, state, "autoIgnoreUnknownChats")}/><span><b>{"Automatically ignore new chats from unknown people"}</b><small>{"Requests remain reviewable instead of interrupting you."}</small></span></label>
        </div>
    }
}

fn render_security_settings(props: &SettingsProps, state: &SettingsState) -> Html {
    html! {
        <div class="card form-grid security-card"><h3>{"Wallet security prompts"}</h3>
          <label class="toggle-row"><input type="checkbox" checked={props.profile.auto_login()} disabled={*state.busy} onchange={toggle_setting_callback(props, state, "passwordlessLogin")}/><span><b>{"Log in automatically without entering a password"}</b><small>{"When enabled, Ghost Talk stores this ID's unlock credential encrypted for this device and opens it automatically at startup."}</small></span></label>
          <label class="toggle-row"><input type="checkbox" checked={props.profile.settings.require_send_password} onchange={toggle_setting_callback(props, state, "requireSendPassword")}/><span><b>{"Require password for each KAS send"}</b><small>{"When disabled, the already-unlocked wallet session authorizes sends."}</small></span></label>
        </div>
    }
}

fn render_recovery(props: &SettingsProps, state: &SettingsState) -> Html {
    html! {
        <form class="card form-grid recovery-card" onsubmit={prevent_submit()}><h3>{"Recovery"}</h3><p class="warning">{"Ghost Talk uses one 24-word recovery root for the local Kaspa wallet and deterministic HYDRA identity."}</p><label>{"Re-enter ID / wallet password"}<input type="password" value={(*state.recovery_password).clone()} oninput={text_input(state.recovery_password.clone())}/></label><button type="submit" disabled={state.recovery_password.len()<8} onclick={reveal_callback(props, state)}>{"Reveal recovery"}</button>
          {render_recovery_value((*state.recovery).clone())}
        </form>
    }
}

fn render_recovery_value(recovery: Option<WalletRecovery>) -> Html {
    let Some(value) = recovery else {
        return Html::default();
    };
    html! {<div class="recovery-output"><b>{"24 recovery words"}</b><p class="recovery">{value.mnemonic}</p>{render_passphrase(&value.passphrase)}<small>{format!("{} · {}",value.network,value.account_path)}</small></div>}
}

fn render_passphrase(passphrase: &str) -> Html {
    if passphrase.is_empty() {
        html! {<small>{"No BIP39 passphrase configured."}</small>}
    } else {
        html! {<p class="warning">{format!("BIP39 passphrase: {passphrase}")}</p>}
    }
}

fn render_backup_settings(props: &SettingsProps, state: &SettingsState) -> Html {
    html! {
        <div class="card form-grid"><h3>{"Encrypted Kaspa recovery backup"}</h3><p>{"Back up contacts and, optionally, message history as an encrypted archive recoverable from the same 24-word Ghost Talk root."}</p><label class="toggle-row"><input type="checkbox" checked={props.profile.settings.contacts_backup_kaspa} onchange={toggle_setting_callback(props, state, "contactsBackupKaspa")}/><span><b>{"Back up contacts"}</b><small>{"Recommended after confirming your recovery words."}</small></span></label><label class="toggle-row"><input type="checkbox" checked={props.profile.settings.backup_messages_kaspa} onchange={toggle_setting_callback(props, state, "backupMessagesKaspa")}/><span><b>{"Back up message history"}</b><small>{"Optional; increases encrypted backup size and Kaspa transaction count."}</small></span></label><small>{format!("Last local backup fingerprint: {}",backup_fingerprint(&props.profile))}</small><div class="button-row"><button disabled={*state.busy||props.password.len()<8} onclick={backup_callback(props, state)}>{"Back up now"}</button><button disabled={*state.busy||props.password.len()<8} onclick={restore_backup_callback(props, state)}>{"Restore backup"}</button></div></div>
    }
}

fn render_endpoint_settings(props: &SettingsProps) -> Html {
    html! {
        <div class="card form-grid"><h3>{"Kaspa endpoints"}</h3><label>{"Historical REST endpoint (>48h only)"}<input value={props.profile.wallet.as_ref().and_then(|w|w.rest_endpoint.clone()).unwrap_or_default()} oninput={endpoint_callback(props, "rest")} placeholder="Network default" /></label><label>{"wRPC endpoint"}<input value={props.profile.wallet.as_ref().and_then(|w|w.wrpc_endpoint.clone()).unwrap_or_default()} oninput={endpoint_callback(props, "wrpc")} placeholder="Auto resolver" /></label><small>{"Leave wRPC blank to resolve a public Kaspa node automatically."}</small></div>
    }
}

fn render_debug_settings(props: &SettingsProps) -> Html {
    html! {
        <div class="card form-grid debug-settings-card"><h3>{"Protocol & Kaspa debugging"}</h3><label class="toggle-row"><input type="checkbox" checked={props.profile.settings.debug_logging} onchange={debug_toggle_callback(props)}/><span><b>{"Enable protocol/network debug logging"}</b><small>{"Records redacted protocol and network metadata only. Passwords, recovery words, private keys, message plaintext, and encrypted payloads are excluded."}</small></span></label><div class="button-row"><button disabled={!props.profile.settings.debug_logging} onclick={{let cb=props.on_open_debug.clone();Callback::from(move |_|cb.emit(()))}}>{"Open debug window"}</button></div></div>
    }
}

fn select_setting_callback(props: &SettingsProps, field: &'static str) -> Callback<Event> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    super::super::form::select_value(Callback::from(move |value: String| {
        if let Ok(patch) = crate::controllers::account::update_setting(&profile, field, value) {
            on_update.emit(patch);
        }
    }))
}

fn toggle_setting_callback(
    props: &SettingsProps,
    state: &SettingsState,
    field: &'static str,
) -> Callback<Event> {
    if field == "passwordlessLogin" {
        return passwordless_callback(props, state);
    }
    if field == "debugLogging" {
        return debug_toggle_callback(props);
    }
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |event: Event| {
        let checked = event.target_unchecked_into::<HtmlInputElement>().checked();
        if let Ok(patch) =
            crate::controllers::account::update_boolean_setting(&profile, field, checked)
        {
            on_update.emit(patch);
        }
    })
}

fn debug_toggle_callback(props: &SettingsProps) -> Callback<Event> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |event: Event| {
        let checked = event.target_unchecked_into::<HtmlInputElement>().checked();
        if let Ok(patch) =
            crate::controllers::account::update_boolean_setting(&profile, "debugLogging", checked)
        {
            on_update.emit(patch);
        }
        spawn_local(async move {
            let _ = crate::controllers::account::set_debug_logging(checked).await;
        });
    })
}

fn passwordless_callback(props: &SettingsProps, state: &SettingsState) -> Callback<Event> {
    let profile = props.profile.clone();
    let session_password = props.password.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    Callback::from(move |event: Event| {
        let checked = event.target_unchecked_into::<HtmlInputElement>().checked();
        if checked && session_password.len() < 8 {
            status.set("This ID must be unlocked before automatic login can be enabled.".into());
            return;
        }
        if *busy {
            return;
        }
        busy.set(true);
        persist_passwordless(
            profile.clone(),
            session_password.clone(),
            checked,
            on_update.clone(),
            status.clone(),
            busy.clone(),
        );
    })
}

fn persist_passwordless(
    profile: Profile,
    password: String,
    enabled: bool,
    on_update: Callback<ProfilePatch>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) {
    spawn_local(async move {
        let credential = if enabled { password.as_str() } else { "" };
        match crate::controllers::account::set_remembered_unlock(&profile, enabled, credential)
            .await
        {
            Ok(()) => {
                on_update.emit(crate::controllers::account::auto_login_patch(
                    &profile, enabled,
                ));
                status.set(passwordless_status(enabled).into());
            }
            Err(error) => status.set(error),
        }
        busy.set(false);
    });
}

fn passwordless_status(enabled: bool) -> &'static str {
    if enabled {
        "Automatic login enabled for this ID."
    } else {
        "Automatic login disabled; this ID will require its password next time."
    }
}

fn reveal_callback(props: &SettingsProps, state: &SettingsState) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = state.recovery_password.clone();
    let recovery = state.recovery.clone();
    let status = state.status.clone();
    Callback::from(move |_| {
        let pass = password.trim().to_string();
        if pass.len() < 8 {
            return;
        }
        let profile = profile.clone();
        let recovery = recovery.clone();
        let status = status.clone();
        spawn_local(async move {
            match crate::controllers::account::reveal_recovery(&profile, &pass).await {
                Ok(value) => recovery.set(Some(value)),
                Err(error) => status.set(error),
            }
        });
    })
}

fn endpoint_callback(props: &SettingsProps, kind: &'static str) -> Callback<InputEvent> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |event: InputEvent| {
        let value = event.target_unchecked_into::<HtmlInputElement>().value();
        on_update.emit(crate::controllers::account::wallet_endpoint_patch(
            &profile,
            kind,
            nonempty(value),
        ));
    })
}

fn nonempty(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}
