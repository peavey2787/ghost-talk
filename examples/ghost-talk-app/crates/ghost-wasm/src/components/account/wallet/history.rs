use super::WalletProps;
use crate::model::WalletHistoryEntry;
use yew::prelude::*;

pub(super) fn render_history_card(props: &WalletProps) -> Html {
    html! {<div class="card history-card"><h3>{"History"}</h3>{render_history(wallet_history(props))}</div>}
}

fn wallet_history(props: &WalletProps) -> Option<&[WalletHistoryEntry]> {
    props
        .snapshot
        .as_ref()
        .map(|snapshot| snapshot.history.as_slice())
        .filter(|entries| !entries.is_empty())
        .or_else(|| {
            props
                .profile
                .wallet
                .as_ref()
                .map(|wallet| wallet.history.as_slice())
                .filter(|entries| !entries.is_empty())
        })
}

fn render_history(history: Option<&[WalletHistoryEntry]>) -> Html {
    let Some(history) = history else {
        return html! {<p class="muted">{"No transactions observed yet."}</p>};
    };
    html! {<div class="history-list">{for history.iter().map(render_history_entry)}</div>}
}

fn render_history_entry(entry: &WalletHistoryEntry) -> Html {
    html! {<div><code>{entry.transaction_id.clone()}</code><small>{format!("blue score {}{}",entry.blue_score,if entry.ghost_payload{" · Ghost payload"}else{""})}</small>{render_history_addresses(&entry.addresses)}</div>}
}

fn render_history_addresses(addresses: &[String]) -> Html {
    if addresses.is_empty() {
        Html::default()
    } else {
        html! {<small>{addresses.join(" · ")}</small>}
    }
}
