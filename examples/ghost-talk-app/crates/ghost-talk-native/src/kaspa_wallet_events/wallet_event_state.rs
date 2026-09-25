pub(crate) use kaspa_wrpc_client::prelude::*;
pub(crate) use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
pub(crate) use tokio::sync::{broadcast, oneshot};
pub(crate) use workflow_core::channel::Channel;

pub(crate) const EVENT_FANOUT: usize = 256;

#[derive(Clone, Debug)]
pub struct ObservedWalletTransaction {
    pub transaction_id: String,
    pub block_daa_score: u64,
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum WalletNodeEvent {
    Connected,
    Reconnecting,
    UtxoSet {
        balance_sompi: u64,
        utxo_count: usize,
        active_addresses: Vec<String>,
        changed_transaction_ids: Vec<String>,
        observed_transactions: Vec<ObservedWalletTransaction>,
    },
    VirtualDaaScoreChanged(u64),
}

pub struct WalletEventStream {
    pub events: broadcast::Receiver<WalletNodeEvent>,
    pub(crate) shutdown: Option<oneshot::Sender<()>>,
}

impl Drop for WalletEventStream {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

pub(crate) fn wallet_subscription_scopes(addresses: &[RpcAddress]) -> (Scope, Scope) {
    (
        Scope::UtxosChanged(UtxosChangedScope::new(addresses.to_vec())),
        Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {}),
    )
}

pub(crate) async fn stop_all_wallet_subscriptions(
    client: &KaspaRpcClient,
    listener_id: &mut Option<ListenerId>,
    addresses: &[RpcAddress],
) {
    let Some(id) = listener_id.take() else {
        return;
    };
    let (utxo_scope, daa_scope) = wallet_subscription_scopes(addresses);
    // Best-effort cleanup: even if the socket is already down, always unregister
    // the listener so a reconnect starts from one clean subscription owner.
    let _ = client.stop_notify(id, utxo_scope).await;
    let _ = client.stop_notify(id, daa_scope).await;
    let _ = client.unregister_listener(id).await;
}

pub(crate) async fn start_all_wallet_subscriptions(
    client: &KaspaRpcClient,
    notifications: &Channel<Notification>,
    listener_id: &mut Option<ListenerId>,
    addresses: &[RpcAddress],
) -> Result<(), String> {
    stop_all_wallet_subscriptions(client, listener_id, addresses).await;
    let id = client.register_new_listener(ChannelConnection::new(
        "ghost-talk-wallet-events",
        notifications.sender.clone(),
        ChannelType::Persistent,
    ));
    let (utxo_scope, daa_scope) = wallet_subscription_scopes(addresses);
    if let Err(error) = client.start_notify(id, utxo_scope.clone()).await {
        let _ = client.unregister_listener(id).await;
        return Err(error.to_string());
    }
    if let Err(error) = client.start_notify(id, daa_scope).await {
        let _ = client.stop_notify(id, utxo_scope).await;
        let _ = client.unregister_listener(id).await;
        return Err(error.to_string());
    }
    *listener_id = Some(id);
    Ok(())
}

pub(crate) fn outpoint_key(entry: &RpcUtxosByAddressesEntry) -> String {
    // RpcTransactionOutpoint's Debug representation is stable within the
    // process and contains both transaction id and output index. We only need
    // a collision-free runtime key for reconciling the startup set with the
    // same typed outpoints arriving on UtxosChanged notifications.
    format!("{:?}", entry.outpoint)
}

#[derive(Clone, Debug)]
pub(crate) struct TrackedUtxo {
    pub(crate) amount: u64,
    pub(crate) address: Option<String>,
}

pub(crate) fn apply_entry(
    set: &mut HashMap<String, TrackedUtxo>,
    entry: &RpcUtxosByAddressesEntry,
) {
    set.insert(
        outpoint_key(entry),
        TrackedUtxo {
            amount: entry.utxo_entry.amount,
            address: entry.address.as_ref().map(ToString::to_string),
        },
    );
}

pub(crate) fn remove_entry(
    set: &mut HashMap<String, TrackedUtxo>,
    entry: &RpcUtxosByAddressesEntry,
) {
    set.remove(&outpoint_key(entry));
}

pub(crate) fn observed_transactions(
    entries: &[RpcUtxosByAddressesEntry],
) -> Vec<ObservedWalletTransaction> {
    let mut observed = BTreeMap::<String, ObservedWalletTransaction>::new();
    for entry in entries {
        let transaction_id = entry.outpoint.transaction_id.to_string();
        let address = entry.address.as_ref().map(ToString::to_string);
        let item =
            observed
                .entry(transaction_id.clone())
                .or_insert_with(|| ObservedWalletTransaction {
                    transaction_id,
                    block_daa_score: entry.utxo_entry.block_daa_score,
                    addresses: Vec::new(),
                });
        item.block_daa_score = item.block_daa_score.max(entry.utxo_entry.block_daa_score);
        if let Some(address) = address {
            if !item.addresses.contains(&address) {
                item.addresses.push(address);
            }
        }
    }
    observed.into_values().collect()
}

pub(crate) fn emit_utxo_set(
    events: &broadcast::Sender<WalletNodeEvent>,
    set: &HashMap<String, TrackedUtxo>,
    changed_transaction_ids: Vec<String>,
    observed_transactions: Vec<ObservedWalletTransaction>,
) {
    let balance_sompi = set
        .values()
        .fold(0u64, |sum, entry| sum.saturating_add(entry.amount));
    let mut active_addresses = set
        .values()
        .filter_map(|entry| entry.address.clone())
        .collect::<Vec<_>>();
    active_addresses.sort();
    active_addresses.dedup();
    let _ = events.send(WalletNodeEvent::UtxoSet {
        balance_sompi,
        utxo_count: set.len(),
        active_addresses,
        changed_transaction_ids,
        observed_transactions,
    });
}

pub(crate) fn parse_event_addresses(addresses: &[String]) -> Result<Vec<RpcAddress>, String> {
    addresses
        .iter()
        .map(|value| RpcAddress::try_from(value.as_str()).map_err(|error| error.to_string()))
        .collect()
}

mod connection;
pub(crate) use connection::{
    announce_reconnecting, changed_transaction_ids, connect_event_client, create_event_client,
    handle_rpc_state,
};
