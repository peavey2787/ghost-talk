use crate::model::{Profile, ProfilePatch, WalletSnapshot};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::wallet::{WalletProps, WalletUiState};

pub(super) fn gather_history_callback(
    props: &WalletProps,
    state: &WalletUiState,
) -> Callback<MouseEvent> {
    let profile = props.profile.clone();
    let existing = props.snapshot.clone();
    let on_snapshot = props.on_snapshot.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    let busy = state.busy.clone();
    Callback::from(move |_| {
        if *busy {
            return;
        }
        busy.set(true);
        status.set("Gathering old transactions…".into());
        gather_history_async(
            profile.clone(),
            existing.clone(),
            on_snapshot.clone(),
            on_update.clone(),
            status.clone(),
            busy.clone(),
        );
    })
}

fn gather_history_async(
    profile: Profile,
    existing: Option<WalletSnapshot>,
    on_snapshot: Callback<WalletSnapshot>,
    on_update: Callback<ProfilePatch>,
    status: UseStateHandle<String>,
    busy: UseStateHandle<bool>,
) {
    spawn_local(async move {
        let base = existing.unwrap_or_default();
        let priority = priority_addresses(&profile, &base);
        match crate::controllers::account::gather_wallet_history(&profile, &priority).await {
            Ok(result) => {
                apply_history_result(profile, base, result, on_snapshot, on_update, status)
            }
            Err(error) => status.set(error),
        }
        busy.set(false);
    });
}

fn priority_addresses(profile: &Profile, snapshot: &WalletSnapshot) -> Vec<String> {
    let mut priority = profile
        .wallet
        .as_ref()
        .map(|wallet| wallet.used_addresses.clone())
        .unwrap_or_default();
    for address in &snapshot.active_addresses {
        if !priority.contains(address) {
            priority.push(address.clone());
        }
    }
    priority
}

fn apply_history_result(
    profile: Profile,
    snapshot: WalletSnapshot,
    result: crate::model::WalletHistoryResult,
    on_snapshot: Callback<WalletSnapshot>,
    on_update: Callback<ProfilePatch>,
    status: UseStateHandle<String>,
) {
    let (snapshot, patch, warnings) =
        crate::controllers::account::apply_history_result(&profile, snapshot, result);
    on_snapshot.emit(snapshot);
    on_update.emit(patch);
    status.set(history_status(warnings));
}

fn history_status(warnings: usize) -> String {
    if warnings == 0 {
        "Old transaction history gathered.".into()
    } else {
        format!("Old transaction history gathered; {warnings} derived address request(s) failed.")
    }
}
