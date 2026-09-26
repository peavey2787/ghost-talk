use super::wallet_event_state::{
    announce_reconnecting, broadcast, connect_and_subscribe, emit_utxo_set, oneshot, UtxoTracker,
    WalletEventStream, WalletNodeEvent, EVENT_FANOUT,
};
use kaspa_portal::network::{wrpc::notification::Notification, NetworkApi};
use std::time::Duration;

const RECONNECT_DELAY: Duration = Duration::from_secs(3);

struct WalletWatch {
    network: String,
    endpoint: String,
    addresses: Vec<String>,
    events: broadcast::Sender<WalletNodeEvent>,
}

pub(crate) fn apply_notification(
    notification: Notification,
    events: &broadcast::Sender<WalletNodeEvent>,
    tracker: &mut UtxoTracker,
) {
    match notification {
        Notification::UtxosChanged(change) => {
            for entry in &change.removed {
                tracker.remove(entry);
            }
            for entry in &change.added {
                tracker.apply(entry);
            }
            emit_utxo_set(events, tracker, &change.added);
        }
        Notification::VirtualDaaScoreChanged(score) => {
            let _ = events.send(WalletNodeEvent::VirtualDaaScoreChanged(score));
        }
        Notification::BlockAdded(_) => {}
    }
}

/// Apply one stream result; false when the stream failed and must reconnect.
pub(crate) fn deliver<E: std::fmt::Display>(
    result: Result<Notification, E>,
    events: &broadcast::Sender<WalletNodeEvent>,
    tracker: &mut UtxoTracker,
) -> bool {
    match result {
        Ok(notification) => {
            apply_notification(notification, events, tracker);
            true
        }
        Err(error) => {
            crate::debug_log::record(
                "warn",
                "kaspa",
                "wallet-event-notification-closed",
                error.to_string(),
            );
            false
        }
    }
}

/// Next stream result, or `None` once shutdown is requested.
async fn next_or_shutdown(
    api: &NetworkApi,
    shutdown: &mut oneshot::Receiver<()>,
) -> Option<kaspa_portal::error::Result<Notification>> {
    tokio::select! {
        _ = shutdown => None,
        result = api.next_notification() => Some(result),
    }
}

/// Pump notifications until shutdown (false) or a stream failure (true).
async fn pump(
    api: &NetworkApi,
    watch: &WalletWatch,
    tracker: &mut UtxoTracker,
    shutdown: &mut oneshot::Receiver<()>,
) -> bool {
    while let Some(result) = next_or_shutdown(api, shutdown).await {
        if !deliver(result, &watch.events, tracker) {
            return true;
        }
    }
    false
}

/// One connect/subscribe/pump cycle; true when the loop should reconnect.
async fn run_cycle(
    watch: &WalletWatch,
    tracker: &mut UtxoTracker,
    shutdown: &mut oneshot::Receiver<()>,
) -> bool {
    let connected = connect_and_subscribe(
        &watch.network,
        &watch.endpoint,
        &watch.addresses,
        &watch.events,
        tracker,
    )
    .await;
    match connected {
        Ok((portal, api)) => {
            let retry = pump(&api, watch, tracker, shutdown).await;
            let _ = portal.disconnect();
            retry
        }
        Err(error) => {
            crate::debug_log::record("warn", "kaspa", "wallet-event-connect-failed", error);
            true
        }
    }
}

async fn run_wallet_event_loop(watch: WalletWatch, mut shutdown: oneshot::Receiver<()>) {
    let mut tracker = UtxoTracker::new(&watch.network);
    while run_cycle(&watch, &mut tracker, &mut shutdown).await {
        tracker.set.clear();
        announce_reconnecting(&watch.events);
        tokio::select! {
            _ = &mut shutdown => return,
            _ = tokio::time::sleep(RECONNECT_DELAY) => {}
        }
    }
}

pub fn start(
    network: &str,
    endpoint: Option<&str>,
    addresses: &[String],
) -> Result<WalletEventStream, String> {
    let endpoint = endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("wallet events require a concrete Kaspa wRPC endpoint")?;
    kaspa_portal::primitives::NetworkId::parse(network)?;
    let (events, receiver) = broadcast::channel(EVENT_FANOUT);
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let watch = WalletWatch {
        network: network.to_owned(),
        endpoint: endpoint.to_owned(),
        addresses: addresses.to_vec(),
        events,
    };
    tauri::async_runtime::spawn(run_wallet_event_loop(watch, shutdown_rx));
    Ok(WalletEventStream {
        events: receiver,
        shutdown: Some(shutdown),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_portal::{
        network::wrpc::{
            block_added::OwnedBlockAddedNotification, notification::UtxosChangedNotification,
        },
        primitives::utxo::UtxoEntry,
    };

    fn utxo(index: u32, amount: u64) -> UtxoEntry {
        let mut script = vec![0x20];
        script.extend_from_slice(&[5; 32]);
        script.push(0xac);
        UtxoEntry {
            tx_id: "cd".repeat(32),
            index,
            amount,
            script_public_key: script,
            block_daa_score: 3,
            covenant_id: None,
        }
    }

    fn empty_block() -> OwnedBlockAddedNotification {
        OwnedBlockAddedNotification {
            block_hash: "00".repeat(32),
            daa_score: 1,
            transactions: Vec::new(),
        }
    }

    #[test]
    fn notifications_update_the_tracked_set_and_fan_out() {
        let (events, mut receiver) = broadcast::channel(EVENT_FANOUT);
        let mut tracker = UtxoTracker::new("testnet-10");
        let added = UtxosChangedNotification {
            added: vec![utxo(0, 7), utxo(1, 5)],
            removed: Vec::new(),
        };
        assert!(deliver::<String>(
            Ok(Notification::UtxosChanged(added)),
            &events,
            &mut tracker
        ));
        let removed = UtxosChangedNotification {
            added: Vec::new(),
            removed: vec![utxo(0, 7)],
        };
        apply_notification(Notification::UtxosChanged(removed), &events, &mut tracker);
        apply_notification(
            Notification::VirtualDaaScoreChanged(99),
            &events,
            &mut tracker,
        );
        apply_notification(
            Notification::BlockAdded(empty_block()),
            &events,
            &mut tracker,
        );
        assert!(matches!(
            receiver.try_recv(),
            Ok(WalletNodeEvent::UtxoSet {
                balance_sompi: 12,
                utxo_count: 2,
                ..
            })
        ));
        assert!(matches!(
            receiver.try_recv(),
            Ok(WalletNodeEvent::UtxoSet {
                balance_sompi: 5,
                utxo_count: 1,
                ..
            })
        ));
        assert!(matches!(
            receiver.try_recv(),
            Ok(WalletNodeEvent::VirtualDaaScoreChanged(99))
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn stream_failures_request_a_reconnect() {
        let (events, _receiver) = broadcast::channel(EVENT_FANOUT);
        let mut tracker = UtxoTracker::new("mainnet");
        assert!(!deliver(Err("socket closed"), &events, &mut tracker));
    }
}
