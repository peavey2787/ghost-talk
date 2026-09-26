use super::{emit_utxo_set, UtxoTracker, WalletNodeEvent};
use kaspa_portal::{network::NetworkApi, primitives::NetworkId, KaspaPortal};
use tokio::sync::broadcast;

pub(crate) fn announce_reconnecting(events: &broadcast::Sender<WalletNodeEvent>) {
    let _ = events.send(WalletNodeEvent::Reconnecting);
}

/// Connect through Kaspa Portal, subscribe to wallet UTXO and DAA changes,
/// then seed the UTXO baseline. Subscribing first means no change between
/// the baseline query and the first notification is lost.
pub(crate) async fn connect_and_subscribe(
    network: &str,
    endpoint: &str,
    addresses: &[String],
    events: &broadcast::Sender<WalletNodeEvent>,
    tracker: &mut UtxoTracker,
) -> Result<(KaspaPortal, NetworkApi), String> {
    let portal = KaspaPortal::builder()
        .network(NetworkId::parse(network)?)
        .endpoint(endpoint)
        .connect()
        .await
        .map_err(|error| error.to_string())?;
    let seeded = subscribe_and_seed(&portal, addresses).await;
    let (api, entries) = match seeded {
        Ok(value) => value,
        Err(error) => {
            let _ = portal.disconnect();
            return Err(error);
        }
    };
    tracker.set.clear();
    for entry in &entries {
        tracker.apply(entry);
    }
    emit_utxo_set(events, tracker, &entries);
    crate::debug_log::record(
        "info",
        "kaspa",
        "wallet-event-subscriptions-active",
        format!("addresses={} utxos={}", addresses.len(), tracker.set.len()),
    );
    let _ = events.send(WalletNodeEvent::Connected);
    Ok((portal, api))
}

async fn subscribe_and_seed(
    portal: &KaspaPortal,
    addresses: &[String],
) -> Result<(NetworkApi, Vec<kaspa_portal::primitives::utxo::UtxoEntry>), String> {
    let api = portal.network().map_err(|error| error.to_string())?.clone();
    api.subscribe_utxos_changed(addresses)
        .await
        .map_err(|error| format!("UtxosChanged subscribe failed: {error}"))?;
    api.subscribe_virtual_daa_score_changed()
        .await
        .map_err(|error| format!("VirtualDaaScoreChanged subscribe failed: {error}"))?;
    let entries = portal
        .chain()
        .map_err(|error| error.to_string())?
        .utxos_many(addresses)
        .await
        .map_err(|error| format!("wallet UTXO baseline failed: {error}"))?;
    Ok((api, entries))
}
