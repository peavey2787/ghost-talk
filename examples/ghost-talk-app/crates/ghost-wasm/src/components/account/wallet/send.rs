use super::{amount::kas_to_sompi, WalletProps, WalletUiState};
use crate::{
    components::{
        form::{prevent_submit, text_input},
        recipient_input::{resolve_recipient_target, RecipientInput},
    },
    model::{Profile, ProfilePatch},
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

mod signer;

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
                <button type="button" disabled={send_disabled(state)} onclick={signer::use_signer_callback(props, state)}>{"Use Signer"}</button>
            </div>
            {signer::render_signer_panel(props, state)}
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
        let request = match prepare_send_request(
            &profile,
            &session_password,
            &destination,
            &amount,
            &send_password,
        ) {
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
        status.set("Signing and broadcasting transaction…".into());
        send_async(
            profile.clone(),
            request,
            SendUi {
                destination: destination.clone(),
                amount: amount.clone(),
                password: send_password.clone(),
                status: status.clone(),
                busy: busy.clone(),
                on_update: on_update.clone(),
            },
        );
    })
}

struct SendRequest {
    destination: String,
    sompi: String,
    password: String,
}
struct SendUi {
    destination: UseStateHandle<String>,
    amount: UseStateHandle<String>,
    password: UseStateHandle<String>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
    on_update: Callback<ProfilePatch>,
}

fn prepare_send_request(
    profile: &Profile,
    session_password: &str,
    destination: &str,
    amount: &str,
    send_password: &str,
) -> Result<SendRequest, String> {
    let (destination, sompi) = prepare_transfer(profile, destination, amount)?;
    let password = if profile.settings.require_send_password {
        send_password
    } else {
        session_password
    };
    if password.len() < 8 {
        return Err("Enter the wallet password to authorize this send.".into());
    }
    Ok(SendRequest {
        destination,
        sompi,
        password: password.to_string(),
    })
}

fn prepare_transfer(
    profile: &Profile,
    destination: &str,
    amount: &str,
) -> Result<(String, String), String> {
    let raw_destination = destination.trim();
    let amount_text = amount.trim();
    if raw_destination.is_empty() || amount_text.is_empty() {
        return Err("Enter a destination and amount.".into());
    }
    let target = resolve_recipient_target(profile, raw_destination)?;
    let sompi = kas_to_sompi(amount_text)
        .map_err(|_| "Enter a valid KAS amount with at most 8 decimal places.".to_string())?;
    Ok((target, sompi))
}

fn send_async(profile: Profile, request: SendRequest, ui: SendUi) {
    spawn_local(async move {
        let result = crate::controllers::account::send_kaspa_and_patch(
            &profile,
            &request.password,
            &request.destination,
            &request.sompi,
            !profile.settings.require_send_password,
        )
        .await;
        match result {
            Ok(result) => {
                ui.on_update.emit(result.patch);
                ui.destination.set(String::new());
                ui.amount.set(String::new());
                ui.password.set(String::new());
                ui.status
                    .set(format!("Broadcast {}", result.transaction_id));
            }
            Err(error) => ui.status.set(error),
        }
        ui.busy.set(false);
    });
}
