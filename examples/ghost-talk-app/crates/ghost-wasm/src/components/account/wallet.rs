use super::super::form::status_view;
use super::wallet_history::gather_history_callback;
use crate::{
    model::{Profile, ProfilePatch, WalletSnapshot},
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
mod amount;
mod history;
mod receive;
mod send;
use super::{identity::ActivatedProfile, kaskold::KasKoldBackup};
use amount::format_kas;
use history::render_history_card;
use receive::{render_funded_addresses, render_receive_card};
use send::render_send_card;

#[derive(Properties, PartialEq)]
pub struct WalletProps {
    pub profile: Profile,
    pub password: String,
    pub snapshot: Option<WalletSnapshot>,
    pub on_update: Callback<ProfilePatch>,
    pub on_snapshot: Callback<WalletSnapshot>,
}

#[derive(Clone)]
pub(super) struct WalletUiState {
    pub(super) destination: UseStateHandle<String>,
    pub(super) amount: UseStateHandle<String>,
    pub(super) send_password: UseStateHandle<String>,
    pub(super) status: UseStateHandle<String>,
    pub(super) busy: UseStateHandle<bool>,
    pub(super) copied: UseStateHandle<bool>,
    pub(super) copied_funded: UseStateHandle<Option<String>>,
    pub(super) signer_pskt: UseStateHandle<String>,
    pub(super) signer_request: UseStateHandle<String>,
    pub(super) signer_response: UseStateHandle<String>,
}

#[component(KaspaWallet)]
pub fn kaspa_wallet(props: &WalletProps) -> Html {
    let state = WalletUiState {
        destination: use_state(String::new),
        amount: use_state(String::new),
        send_password: use_state(String::new),
        status: use_state(String::new),
        busy: use_state(|| false),
        copied: use_state(|| false),
        copied_funded: use_state(|| None::<String>),
        signer_pskt: use_state(String::new),
        signer_request: use_state(String::new),
        signer_response: use_state(String::new),
    };
    let wallet = props.profile.wallet.clone();
    html! {
      <section class="page">
        <div class="page-head"><div><h2>{"Kaspa"}</h2><p>{"Ghost Talk's local software wallet funds messages and ordinary KAS sends."}</p></div><button onclick={gather_history_callback(props, &state)} disabled={wallet.is_none()||*state.busy}>{"Gather old transactions"}</button></div>
        {render_wallet_body(props, &state)}
        {status_view(&state.status)}
      </section>
    }
}

fn render_wallet_body(props: &WalletProps, state: &WalletUiState) -> Html {
    if props.profile.wallet.is_none() {
        return html! {<div class="card muted">{"This ID has no Kaspa wallet."}</div>};
    }
    let wallet_view = props
        .profile
        .wallet
        .as_ref()
        .map(crate::view_models::WalletViewModel::from);
    html! {
        <div class="wallet-grid">
          {render_balance_card(props)}
          {render_receive_card(wallet_view.as_ref(), state)}
          <div class="card funded-addresses-card"><h3>{"Funded addresses"}</h3>{render_funded_addresses(props.snapshot.as_ref(), state.copied_funded.clone())}</div>
          {render_send_card(props, state)}
          <div class="card consolidation-card"><h3>{"UTXO consolidation"}</h3><p>{"Combine spendable UTXOs when wallet fragmentation becomes inconvenient."}</p><button disabled={*state.busy} onclick={consolidate_callback(props, state)}>{"Consolidate"}</button></div>
          {render_history_card(props)}
          <KasKoldBackup session={ActivatedProfile { profile: props.profile.clone(), password: props.password.clone() }} />
        </div>
    }
}

fn render_balance_card(props: &WalletProps) -> Html {
    let balance = props
        .snapshot
        .as_ref()
        .map(|snapshot| format_kas(&snapshot.balance_sompi))
        .unwrap_or_else(|| "—".into());
    let utxos = props
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.utxo_count.as_str())
        .unwrap_or("—");
    let funded = props
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.active_addresses.len().to_string())
        .unwrap_or_else(|| "—".into());
    let blue_score = props
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.blue_score.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("—");
    html! {<div class="card balance-card"><small>{"Available balance"}</small><strong>{balance}{" KAS"}</strong><span>{format!("{utxos} UTXOs · {funded} funded address(es) · blue score {blue_score}")}</span></div>}
}

fn consolidate_callback(props: &WalletProps, state: &WalletUiState) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let password = props.password.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    let on_update = props.on_update.clone();
    Callback::from(move |_| {
        if *busy {
            return;
        }
        busy.set(true);
        status.set("Consolidating wallet UTXOs…".into());
        consolidate_async(
            profile.clone(),
            password.clone(),
            status.clone(),
            busy.clone(),
            on_update.clone(),
        );
    })
}

fn consolidate_async(
    profile: Profile,
    password: String,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
    on_update: Callback<ProfilePatch>,
) {
    spawn_local(async move {
        match crate::controllers::account::consolidate_and_patch(&profile, &password).await {
            Ok(result) => {
                on_update.emit(result.patch);
                status.set(format!("Consolidation broadcast {}", result.transaction_id));
            }
            Err(error) => status.set(error),
        }
        busy.set(false);
    });
}
