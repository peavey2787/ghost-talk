use super::{amount::kas_to_sompi, WalletProps, WalletUiState};
use crate::{
    components::{
        form::{prevent_submit, text_input, textarea_input},
        recipient_input::{resolve_recipient_target, RecipientInput},
    },
    model::{Profile, ProfilePatch},
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

pub(super) fn render_send_card(props: &WalletProps, state: &WalletUiState) -> Html {
    html! {
        <form class="card send-card" onsubmit={prevent_submit()}>
            <h3>{"Send KAS"}</h3>
            <label>{"Destination"}
                <RecipientInput profile={props.profile.clone()} value={(*state.destination).clone()}
                    on_change={{let destination=state.destination.clone(); Callback::from(move|value:String|destination.set(value))}}
                    placeholder="Contact, alice.kas, alice.k, or kaspa:…" />
            </label>
            <label>{"Amount (KAS)"}<input value={(*state.amount).clone()} oninput={text_input(state.amount.clone())} placeholder="0.00000000" /></label>
            {render_send_password(props, state)}
            <div class="button-row send-actions">
                <button type="submit" class="primary" disabled={send_disabled(state)} onclick={send_callback(props, state)}>
                    {if *state.busy {"Working…"} else {"Send"}}
                </button>
                <button type="button" disabled={send_disabled(state)} onclick={use_signer_callback(props, state)}>{"Use Signer"}</button>
            </div>
            {render_signer_panel(props, state)}
        </form>
    }
}

fn send_disabled(state: &WalletUiState) -> bool {
    *state.busy || state.destination.trim().is_empty() || state.amount.trim().is_empty()
}

fn render_send_password(props: &WalletProps, state: &WalletUiState) -> Html {
    if !props.profile.settings.require_send_password {
        return Html::default();
    }
    html! {<label>{"Password"}<input type="password" value={(*state.send_password).clone()} oninput={text_input(state.send_password.clone())}/></label>}
}

fn render_signer_panel(props: &WalletProps, state: &WalletUiState) -> Html {
    if state.signer_request.is_empty() {
        return Html::default();
    }
    html! {
        <section class="signer-panel">
            <div class="signer-panel-head">
                <div><b>{"KasKold signer"}</b><small>{"Sign this KSPT request in KasKold, then paste the signed response below."}</small></div>
                <span class="compat-badge">{"KasKold"}</span>
            </div>
            <label>{"Signing request"}
                <textarea class="signer-payload" rows="5" readonly=true value={(*state.signer_request).clone()} />
            </label>
            <button type="button" onclick={copy_request_callback(state)}>{"Copy request"}</button>
            <label>{"Signed KasKold response"}
                <textarea class="signer-payload" rows="5" value={(*state.signer_response).clone()} oninput={textarea_input(state.signer_response.clone())} placeholder="Paste signed KSPT response hex" />
            </label>
            <div class="button-row">
                <button type="button" class="primary" disabled={*state.busy || state.signer_response.trim().is_empty()} onclick={broadcast_signer_callback(props, state)}>
                    {if *state.busy {"Working…"} else {"Broadcast signed transaction"}}
                </button>
                <button type="button" disabled={*state.busy} onclick={clear_signer_callback(state)}>{"Cancel"}</button>
            </div>
        </section>
    }
}

fn send_callback(props: &WalletProps, state: &WalletUiState) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let session_password = props.password.clone();
    let destination = state.destination.clone();
    let amount = state.amount.clone();
    let send_password = state.send_password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        let request = match prepare_send_request(&profile, &session_password, &destination, &amount, &send_password) {
            Ok(request) => request,
            Err(error) => { status.set(error); return; }
        };
        if *busy { return; }
        busy.set(true);
        status.set("Signing and broadcasting transaction…".into());
        send_async(profile.clone(), request, SendUi {
            destination: destination.clone(), amount: amount.clone(), password: send_password.clone(),
            status: status.clone(), busy: busy.clone(), on_update: on_update.clone(),
        });
    })
}

fn use_signer_callback(props: &WalletProps, state: &WalletUiState) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let destination = state.destination.clone();
    let amount = state.amount.clone();
    let pskt = state.signer_pskt.clone();
    let signing_request = state.signer_request.clone();
    let signed_response = state.signer_response.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    Callback::from(move |_| {
        let (destination_value, sompi) = match prepare_transfer(&profile, &destination, &amount) {
            Ok(request) => request,
            Err(error) => { status.set(error); return; }
        };
        if *busy { return; }
        busy.set(true);
        status.set("Preparing transaction for KasKold signer…".into());
        let task = SignerPrepareTask {
            profile: profile.clone(), password: password.clone(), destination: destination_value,
            sompi, pskt: pskt.clone(), request: signing_request.clone(), response: signed_response.clone(),
            status: status.clone(), busy: busy.clone(),
        };
        spawn_local(async move { prepare_for_signer(task).await; });
    })
}

struct SendRequest { destination: String, sompi: String, password: String }
struct SendUi {
    destination: UseStateHandle<String>, amount: UseStateHandle<String>, password: UseStateHandle<String>,
    status: UseStateHandle<String>, busy: UseStateHandle<bool>, on_update: Callback<ProfilePatch>,
}

struct SignerPrepareTask {
    profile: Profile, password: String, destination: String, sompi: String,
    pskt: UseStateHandle<String>, request: UseStateHandle<String>, response: UseStateHandle<String>,
    status: UseStateHandle<String>, busy: UseStateHandle<bool>,
}

fn prepare_send_request(
    profile: &Profile, session_password: &str, destination: &str, amount: &str, send_password: &str,
) -> Result<SendRequest, String> {
    let (destination, sompi) = prepare_transfer(profile, destination, amount)?;
    let password = if profile.settings.require_send_password { send_password } else { session_password };
    if password.len() < 8 { return Err("Enter the wallet password to authorize this send.".into()); }
    Ok(SendRequest { destination, sompi, password: password.to_string() })
}

fn prepare_transfer(profile: &Profile, destination: &str, amount: &str) -> Result<(String, String), String> {
    let raw_destination = destination.trim();
    let amount_text = amount.trim();
    if raw_destination.is_empty() || amount_text.is_empty() { return Err("Enter a destination and amount.".into()); }
    let target = resolve_recipient_target(profile, raw_destination)?;
    let sompi = kas_to_sompi(amount_text)
        .map_err(|_| "Enter a valid KAS amount with at most 8 decimal places.".to_string())?;
    Ok((target, sompi))
}

async fn prepare_for_signer(task: SignerPrepareTask) {
    let result = async {
        let unsigned_pskt = crate::controllers::account::prepare_signer_send(
            &task.profile, &task.password, &task.destination, &task.sompi,
        ).await?;
        let network_name = task.profile.wallet.as_ref().ok_or("No wallet configured")?.public.network.as_str();
        let network = kaskold_sdk::Network::parse(network_name).map_err(|error| format!("KasKold network: {error}"))?;
        let request = kaskold_sdk::prepare(&unsigned_pskt, network).map_err(|error| format!("KasKold signer request: {error}"))?;
        Ok::<_, String>((unsigned_pskt, request.kspt_hex))
    }.await;
    match result {
        Ok((unsigned_pskt, request)) => {
            task.pskt.set(unsigned_pskt);
            task.request.set(request);
            task.response.set(String::new());
            task.status.set("KasKold signing request ready.".into());
        }
        Err(error) => task.status.set(error),
    }
    task.busy.set(false);
}

fn broadcast_signer_callback(props: &WalletProps, state: &WalletUiState) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let pskt = state.signer_pskt.clone();
    let response = state.signer_response.clone();
    let destination = state.destination.clone();
    let amount = state.amount.clone();
    let send_password = state.send_password.clone();
    let signing_request = state.signer_request.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        if *busy { return; }
        let signed_pskt = match complete_signer_response(&profile, pskt.as_str(), response.as_str()) {
            Ok(value) => value,
            Err(error) => { status.set(error); return; }
        };
        busy.set(true);
        status.set("Validating and broadcasting KasKold-signed transaction…".into());
        let task = SignerBroadcastTask {
            profile: profile.clone(), signed_pskt, destination: destination.clone(), amount: amount.clone(),
            password: send_password.clone(), request: signing_request.clone(), response: response.clone(),
            pskt: pskt.clone(), status: status.clone(), busy: busy.clone(), on_update: on_update.clone(),
        };
        spawn_local(async move { broadcast_signer(task).await; });
    })
}

fn complete_signer_response(profile: &Profile, pskt: &str, response: &str) -> Result<String, String> {
    let network_name = profile.wallet.as_ref().ok_or("No wallet configured")?.public.network.as_str();
    let network = kaskold_sdk::Network::parse(network_name).map_err(|error| format!("KasKold network: {error}"))?;
    let request = kaskold_sdk::prepare(pskt, network).map_err(|error| format!("KasKold signer request: {error}"))?;
    let signed = kaskold_sdk::complete(&request, response.trim()).map_err(|error| format!("KasKold signed response: {error}"))?;
    Ok(signed.pskt_hex)
}

struct SignerBroadcastTask {
    profile: Profile, signed_pskt: String,
    destination: UseStateHandle<String>, amount: UseStateHandle<String>, password: UseStateHandle<String>,
    request: UseStateHandle<String>, response: UseStateHandle<String>, pskt: UseStateHandle<String>,
    status: UseStateHandle<String>, busy: UseStateHandle<bool>, on_update: Callback<ProfilePatch>,
}

async fn broadcast_signer(task: SignerBroadcastTask) {
    match crate::controllers::account::broadcast_signer_send_and_patch(&task.profile, &task.signed_pskt).await {
        Ok(result) => {
            task.on_update.emit(result.patch);
            task.destination.set(String::new()); task.amount.set(String::new()); task.password.set(String::new());
            task.request.set(String::new()); task.response.set(String::new()); task.pskt.set(String::new());
            task.status.set(format!("Broadcast {}", result.transaction_id));
        }
        Err(error) => task.status.set(error),
    }
    task.busy.set(false);
}

fn send_async(profile: Profile, request: SendRequest, ui: SendUi) {
    spawn_local(async move {
        let result = crate::controllers::account::send_kaspa_and_patch(
            &profile, &request.password, &request.destination, &request.sompi,
            !profile.settings.require_send_password,
        ).await;
        match result {
            Ok(result) => {
                ui.on_update.emit(result.patch); ui.destination.set(String::new()); ui.amount.set(String::new());
                ui.password.set(String::new()); ui.status.set(format!("Broadcast {}", result.transaction_id));
            }
            Err(error) => ui.status.set(error),
        }
        ui.busy.set(false);
    });
}

fn copy_request_callback(state: &WalletUiState) -> Callback<MouseEvent> {
    let request = state.signer_request.clone();
    let status = state.status.clone();
    Callback::from(move |_| {
        if let Some(window) = web_sys::window() { let _ = window.navigator().clipboard().write_text((*request).as_str()); }
        status.set("KasKold signing request copied.".into());
    })
}

fn clear_signer_callback(state: &WalletUiState) -> Callback<MouseEvent> {
    let request = state.signer_request.clone();
    let response = state.signer_response.clone();
    let pskt = state.signer_pskt.clone();
    Callback::from(move |_| { request.set(String::new()); response.set(String::new()); pskt.set(String::new()); })
}
