use super::WalletUiState;
use crate::model::WalletSnapshot;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

pub(super) fn render_receive_card(
    view: Option<&crate::view_models::WalletViewModel>,
    state: &WalletUiState,
) -> Html {
    let Some(view) = view else {
        return Html::default();
    };
    let receive = view.receive_address.clone();
    html! {
        <div class="card receive-card"><h3>{"Receive"}</h3><code>{receive.clone()}</code><small>{format!("{} · receive index {}", view.network, view.next_receive_index)}</small><div class="button-row"><button class={(*state.copied).then_some("primary")} onclick={copy_receive_callback(receive, state.copied.clone())}>{if *state.copied {"Copied!"} else {"Copy address"}}</button></div></div>
    }
}

pub(super) fn render_funded_addresses(
    snapshot: Option<&WalletSnapshot>,
    copied: UseStateHandle<Option<String>>,
) -> Html {
    let Some(snapshot) = snapshot else {
        return html! {<p class="muted">{"Waiting for wallet state…"}</p>};
    };
    if snapshot.active_addresses.is_empty() {
        return html! {<p class="muted">{"No funded addresses."}</p>};
    }
    html! {<div class="funded-address-list">{for snapshot.active_addresses.iter().map(|address| render_funded_address(address, copied.clone()))}</div>}
}

fn render_funded_address(address: &str, copied: UseStateHandle<Option<String>>) -> Html {
    let address_value = address.to_string();
    let is_copied = (*copied).as_deref() == Some(address);
    html! {
        <button type="button" class={classes!("funded-address-row", is_copied.then_some("copied"))} onclick={copy_address_callback(address_value.clone(), copied)}>
            <code>{address_value}</code><small>{if is_copied {"Copied!"} else {"Click to copy"}}</small>
        </button>
    }
}

fn copy_receive_callback(receive: String, copied: UseStateHandle<bool>) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        if receive.is_empty() {
            return;
        }
        copy_to_clipboard(&receive);
        copied.set(true);
        spawn_local(clear_boolean_flag(copied.clone()));
    })
}

fn copy_address_callback(
    address: String,
    copied: UseStateHandle<Option<String>>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        copy_to_clipboard(&address);
        copied.set(Some(address.clone()));
        let copied = copied.clone();
        spawn_local(async move {
            TimeoutFuture::new(1_500).await;
            copied.set(None);
        });
    })
}

fn copy_to_clipboard(value: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(value);
    }
}

async fn clear_boolean_flag(copied: UseStateHandle<bool>) {
    TimeoutFuture::new(1_500).await;
    copied.set(false);
}
