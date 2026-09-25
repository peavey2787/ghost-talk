use super::{
    apply_entry, emit_utxo_set, observed_transactions, start_all_wallet_subscriptions,
    stop_all_wallet_subscriptions, TrackedUtxo, WalletNodeEvent,
};
use kaspa_wrpc_client::prelude::*;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::broadcast;
use workflow_core::channel::Channel;

pub(crate) fn create_event_client(
    network_id: NetworkId,
    endpoint: Option<&str>,
) -> Result<Arc<KaspaRpcClient>, String> {
    let resolver = endpoint.is_none().then(Resolver::default);
    KaspaRpcClient::new_with_args(
        WrpcEncoding::Borsh,
        endpoint,
        resolver,
        Some(network_id),
        None,
    )
    .map(Arc::new)
    .map_err(|error| error.to_string())
}

pub(crate) fn announce_reconnecting(events: &broadcast::Sender<WalletNodeEvent>) {
    let _ = events.send(WalletNodeEvent::Reconnecting);
}

pub(crate) async fn connect_event_client(
    client: &KaspaRpcClient,
    events: &broadcast::Sender<WalletNodeEvent>,
) {
    let options = ConnectOptions {
        block_async_connect: false,
        ..Default::default()
    };
    if let Err(error) = client.connect(Some(options)).await {
        crate::debug_log::record(
            "warn",
            "kaspa",
            "wallet-event-connect-start-failed",
            error.to_string(),
        );
        announce_reconnecting(events);
    }
}

pub(crate) async fn seed_wallet_utxos(
    client: &KaspaRpcClient,
    events: &broadcast::Sender<WalletNodeEvent>,
    addresses: &[RpcAddress],
    utxo_set: &mut HashMap<String, TrackedUtxo>,
) -> Result<(), String> {
    let entries = client
        .get_utxos_by_addresses(addresses.to_vec())
        .await
        .map_err(|error| error.to_string())?;
    utxo_set.clear();
    for entry in &entries {
        apply_entry(utxo_set, entry);
    }
    let mut transaction_ids = entries
        .iter()
        .map(|entry| entry.outpoint.transaction_id.to_string())
        .collect::<Vec<_>>();
    transaction_ids.sort();
    transaction_ids.dedup();
    emit_utxo_set(
        events,
        utxo_set,
        transaction_ids,
        observed_transactions(&entries),
    );
    crate::debug_log::record(
        "info",
        "kaspa",
        "wallet-event-subscriptions-active",
        format!("addresses={} utxos={}", addresses.len(), utxo_set.len()),
    );
    let _ = events.send(WalletNodeEvent::Connected);
    Ok(())
}

pub(crate) async fn activate_wallet_subscriptions(
    client: &KaspaRpcClient,
    notifications: &Channel<Notification>,
    events: &broadcast::Sender<WalletNodeEvent>,
    listener_id: &mut Option<ListenerId>,
    addresses: &[RpcAddress],
    utxo_set: &mut HashMap<String, TrackedUtxo>,
) {
    let subscribed =
        start_all_wallet_subscriptions(client, notifications, listener_id, addresses).await;
    if let Err(error) = subscribed {
        crate::debug_log::record("warn", "kaspa", "wallet-event-subscribe-failed", error);
        announce_reconnecting(events);
        return;
    }
    if let Err(error) = seed_wallet_utxos(client, events, addresses, utxo_set).await {
        crate::debug_log::record("warn", "kaspa", "wallet-event-baseline-failed", error);
        stop_all_wallet_subscriptions(client, listener_id, addresses).await;
        announce_reconnecting(events);
    }
}

pub(crate) async fn handle_rpc_state<E: std::fmt::Display>(
    state: Result<RpcState, E>,
    client: &KaspaRpcClient,
    notifications: &Channel<Notification>,
    events: &broadcast::Sender<WalletNodeEvent>,
    listener_id: &mut Option<ListenerId>,
    addresses: &[RpcAddress],
    utxo_set: &mut HashMap<String, TrackedUtxo>,
) -> bool {
    match state {
        Ok(RpcState::Connected) => {
            activate_wallet_subscriptions(
                client,
                notifications,
                events,
                listener_id,
                addresses,
                utxo_set,
            )
            .await;
            true
        }
        Ok(RpcState::Disconnected) => {
            stop_all_wallet_subscriptions(client, listener_id, addresses).await;
            utxo_set.clear();
            announce_reconnecting(events);
            true
        }
        Err(error) => {
            crate::debug_log::record(
                "warn",
                "kaspa",
                "wallet-event-control-closed",
                error.to_string(),
            );
            announce_reconnecting(events);
            false
        }
    }
}

pub(crate) fn changed_transaction_ids(entries: &[RpcUtxosByAddressesEntry]) -> Vec<String> {
    let mut ids = entries
        .iter()
        .map(|entry| entry.outpoint.transaction_id.to_string())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}
