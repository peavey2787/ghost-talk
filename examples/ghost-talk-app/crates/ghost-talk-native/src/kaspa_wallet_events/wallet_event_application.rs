use super::wallet_event_state::{
    announce_reconnecting, apply_entry, broadcast, changed_transaction_ids, connect_event_client,
    create_event_client, emit_utxo_set, handle_rpc_state, observed_transactions, oneshot,
    parse_event_addresses, remove_entry, stop_all_wallet_subscriptions, Arc, Channel, HashMap,
    KaspaRpcClient, ListenerId, NetworkId, Notification, RpcAddress, RpcState,
    RpcUtxosByAddressesEntry, TrackedUtxo, WalletEventStream, WalletNodeEvent, EVENT_FANOUT,
};
use std::str::FromStr;
use workflow_core::channel::MultiplexerChannel;
pub(crate) fn apply_utxo_change(
    events: &broadcast::Sender<WalletNodeEvent>,
    utxo_set: &mut HashMap<String, TrackedUtxo>,
    removed: &[RpcUtxosByAddressesEntry],
    added: &[RpcUtxosByAddressesEntry],
) {
    let transaction_ids = changed_transaction_ids(added);
    for entry in removed {
        remove_entry(utxo_set, entry);
    }
    for entry in added {
        apply_entry(utxo_set, entry);
    }
    emit_utxo_set(
        events,
        utxo_set,
        transaction_ids,
        observed_transactions(added),
    );
}

pub(crate) fn handle_wallet_notification<E: std::fmt::Display>(
    notification: Result<Notification, E>,
    events: &broadcast::Sender<WalletNodeEvent>,
    utxo_set: &mut HashMap<String, TrackedUtxo>,
) -> bool {
    let notification = notification
        .map_err(|error| {
            crate::debug_log::record(
                "warn",
                "kaspa",
                "wallet-event-notification-closed",
                error.to_string(),
            );
            announce_reconnecting(events);
        })
        .ok();
    let Some(notification) = notification else {
        return false;
    };
    if let Notification::UtxosChanged(change) = notification {
        apply_utxo_change(
            events,
            utxo_set,
            change.removed.as_ref(),
            change.added.as_ref(),
        );
        return true;
    }
    if let Notification::VirtualDaaScoreChanged(change) = notification {
        let _ = events.send(WalletNodeEvent::VirtualDaaScoreChanged(
            change.virtual_daa_score,
        ));
    }
    true
}

pub(crate) async fn run_wallet_event_loop(
    client: Arc<KaspaRpcClient>,
    notifications: Channel<Notification>,
    events: broadcast::Sender<WalletNodeEvent>,
    addresses: Vec<RpcAddress>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let ctl = client.rpc_ctl().multiplexer().channel();
    let mut listener_id: Option<ListenerId> = None;
    let mut utxo_set = HashMap::<String, TrackedUtxo>::new();
    connect_event_client(&client, &events).await;

    while pump_wallet_event(
        &client,
        &ctl,
        &notifications,
        &events,
        &addresses,
        &mut listener_id,
        &mut utxo_set,
        &mut shutdown_rx,
    )
    .await
    {}
    stop_all_wallet_subscriptions(&client, &mut listener_id, &addresses).await;
    let _ = client.disconnect().await;
}

#[expect(
    clippy::too_many_arguments,
    reason = "wallet event-loop state boundary"
)]
async fn pump_wallet_event(
    client: &KaspaRpcClient,
    ctl: &MultiplexerChannel<RpcState>,
    notifications: &Channel<Notification>,
    events: &broadcast::Sender<WalletNodeEvent>,
    addresses: &[RpcAddress],
    listener_id: &mut Option<ListenerId>,
    utxo_set: &mut HashMap<String, TrackedUtxo>,
    shutdown_rx: &mut oneshot::Receiver<()>,
) -> bool {
    tokio::select! {
        _ = shutdown_rx => false,
        state = ctl.receiver.recv() => handle_rpc_state(
            state,
            client,
            notifications,
            events,
            listener_id,
            addresses,
            utxo_set,
        ).await,
        notification = notifications.receiver.recv() => {
            handle_wallet_notification(notification, events, utxo_set)
        }
    }
}

pub fn start(
    network: &str,
    endpoint: Option<&str>,
    addresses: &[String],
) -> Result<WalletEventStream, String> {
    let network_id = NetworkId::from_str(network).map_err(|error| error.to_string())?;
    let addresses = parse_event_addresses(addresses)?;
    let client = create_event_client(network_id, endpoint)?;
    let notifications = Channel::<Notification>::unbounded();
    let (events, receiver) = broadcast::channel(EVENT_FANOUT);
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let events_task = events.clone();
    tauri::async_runtime::spawn(run_wallet_event_loop(
        client,
        notifications,
        events_task,
        addresses,
        shutdown_rx,
    ));
    Ok(WalletEventStream {
        events: receiver,
        shutdown: Some(shutdown),
    })
}
