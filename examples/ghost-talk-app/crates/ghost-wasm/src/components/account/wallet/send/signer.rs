//! KasKold external-signer send flow.

use super::super::{WalletProps, WalletUiState};
use crate::{
    components::form::textarea_input,
    model::{Profile, ProfilePatch},
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::prepare_transfer;

pub(super) fn render_signer_panel(props: &WalletProps, state: &WalletUiState) -> Html {
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

pub(super) fn use_signer_callback(
    props: &WalletProps,
    state: &WalletUiState,
) -> Callback<MouseEvent> {
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
            Err(error) => {
                status.set(error);
                return;
            }
        };
        if *busy {
            return;
        }
        busy.set(true);
        status.set("Preparing transaction for KasKold signer…".into());
        let task = SignerPrepareTask {
            profile: profile.clone(),
            password: password.clone(),
            destination: destination_value,
            sompi,
            pskt: pskt.clone(),
            request: signing_request.clone(),
            response: signed_response.clone(),
            status: status.clone(),
            busy: busy.clone(),
        };
        spawn_local(async move {
            prepare_for_signer(task).await;
        });
    })
}

struct SignerPrepareTask {
    profile: Profile,
    password: String,
    destination: String,
    sompi: String,
    pskt: UseStateHandle<String>,
    request: UseStateHandle<String>,
    response: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
}

async fn prepare_for_signer(task: SignerPrepareTask) {
    let result = async {
        let unsigned_pskt = crate::controllers::account::prepare_signer_send(
            &task.profile,
            &task.password,
            &task.destination,
            &task.sompi,
        )
        .await?;
        let network_name = task
            .profile
            .wallet
            .as_ref()
            .ok_or("No wallet configured")?
            .public
            .network
            .as_str();
        let network = kaskold_sdk::Network::parse(network_name)
            .map_err(|error| format!("KasKold network: {error}"))?;
        let request = kaskold_sdk::prepare(&unsigned_pskt, network)
            .map_err(|error| format!("KasKold signer request: {error}"))?;
        Ok::<_, String>((unsigned_pskt, request.kspt_hex))
    }
    .await;
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
        if *busy {
            return;
        }
        let signed_pskt = match complete_signer_response(&profile, pskt.as_str(), response.as_str())
        {
            Ok(value) => value,
            Err(error) => {
                status.set(error);
                return;
            }
        };
        busy.set(true);
        status.set("Validating and broadcasting KasKold-signed transaction…".into());
        let task = SignerBroadcastTask {
            profile: profile.clone(),
            signed_pskt,
            destination: destination.clone(),
            amount: amount.clone(),
            password: send_password.clone(),
            request: signing_request.clone(),
            response: response.clone(),
            pskt: pskt.clone(),
            status: status.clone(),
            busy: busy.clone(),
            on_update: on_update.clone(),
        };
        spawn_local(async move {
            broadcast_signer(task).await;
        });
    })
}

fn complete_signer_response(
    profile: &Profile,
    pskt: &str,
    response: &str,
) -> Result<String, String> {
    let network_name = profile
        .wallet
        .as_ref()
        .ok_or("No wallet configured")?
        .public
        .network
        .as_str();
    let network = kaskold_sdk::Network::parse(network_name)
        .map_err(|error| format!("KasKold network: {error}"))?;
    let request = kaskold_sdk::prepare(pskt, network)
        .map_err(|error| format!("KasKold signer request: {error}"))?;
    let signed = kaskold_sdk::complete(&request, response.trim())
        .map_err(|error| format!("KasKold signed response: {error}"))?;
    Ok(signed.pskt_hex)
}

struct SignerBroadcastTask {
    profile: Profile,
    signed_pskt: String,
    destination: UseStateHandle<String>,
    amount: UseStateHandle<String>,
    password: UseStateHandle<String>,
    request: UseStateHandle<String>,
    response: UseStateHandle<String>,
    pskt: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
    on_update: Callback<ProfilePatch>,
}

async fn broadcast_signer(task: SignerBroadcastTask) {
    match crate::controllers::account::broadcast_signer_send_and_patch(
        &task.profile,
        &task.signed_pskt,
    )
    .await
    {
        Ok(result) => {
            task.on_update.emit(result.patch);
            task.destination.set(String::new());
            task.amount.set(String::new());
            task.password.set(String::new());
            task.request.set(String::new());
            task.response.set(String::new());
            task.pskt.set(String::new());
            task.status
                .set(format!("Broadcast {}", result.transaction_id));
        }
        Err(error) => task.status.set(error),
    }
    task.busy.set(false);
}

fn copy_request_callback(state: &WalletUiState) -> Callback<MouseEvent> {
    let request = state.signer_request.clone();
    let status = state.status.clone();
    Callback::from(move |_| {
        if let Some(window) = web_sys::window() {
            let _ = window
                .navigator()
                .clipboard()
                .write_text((*request).as_str());
        }
        status.set("KasKold signing request copied.".into());
    })
}

fn clear_signer_callback(state: &WalletUiState) -> Callback<MouseEvent> {
    let request = state.signer_request.clone();
    let response = state.signer_response.clone();
    let pskt = state.signer_pskt.clone();
    Callback::from(move |_| {
        request.set(String::new());
        response.set(String::new());
        pskt.set(String::new());
    })
}
