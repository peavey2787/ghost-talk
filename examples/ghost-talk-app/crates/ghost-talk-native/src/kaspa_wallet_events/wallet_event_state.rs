pub(crate) use kaspa_portal::primitives::utxo::UtxoEntry;
pub(crate) use std::collections::{BTreeMap, HashMap};
pub(crate) use tokio::sync::{broadcast, oneshot};

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

pub(crate) fn outpoint_key(entry: &UtxoEntry) -> String {
    format!("{}:{}", entry.tx_id, entry.index)
}

#[derive(Clone, Debug)]
pub(crate) struct TrackedUtxo {
    pub(crate) amount: u64,
    pub(crate) address: Option<String>,
}

/// Wallet UTXO set keyed by outpoint. Addresses are derived from the locking
/// script because Kaspa notifications identify outputs by script.
pub(crate) struct UtxoTracker {
    prefix: &'static str,
    pub(crate) set: HashMap<String, TrackedUtxo>,
}

impl UtxoTracker {
    pub(crate) fn new(network: &str) -> Self {
        let prefix = if network.trim().eq_ignore_ascii_case("mainnet") {
            "kaspa"
        } else {
            "kaspatest"
        };
        Self {
            prefix,
            set: HashMap::new(),
        }
    }

    pub(crate) fn address(&self, entry: &UtxoEntry) -> Option<String> {
        script_address(&entry.script_public_key, self.prefix)
    }

    pub(crate) fn apply(&mut self, entry: &UtxoEntry) {
        let tracked = TrackedUtxo {
            amount: entry.amount,
            address: self.address(entry),
        };
        self.set.insert(outpoint_key(entry), tracked);
    }

    pub(crate) fn remove(&mut self, entry: &UtxoEntry) {
        self.set.remove(&outpoint_key(entry));
    }
}

fn script_address(script: &[u8], prefix: &str) -> Option<String> {
    use kaspa_portal::primitives::address::{encode_p2pk_address, encode_p2sh_address};
    match script {
        [0x20, key @ .., 0xac] if key.len() == 32 => {
            Some(encode_p2pk_address(key.try_into().ok()?, prefix))
        }
        [0xaa, 0x20, hash @ .., 0x87] if hash.len() == 32 => {
            Some(encode_p2sh_address(hash.try_into().ok()?, prefix))
        }
        _ => None,
    }
}

pub(crate) fn observed_transactions(
    tracker: &UtxoTracker,
    entries: &[UtxoEntry],
) -> Vec<ObservedWalletTransaction> {
    let mut observed = BTreeMap::<String, ObservedWalletTransaction>::new();
    for entry in entries {
        let item =
            observed
                .entry(entry.tx_id.clone())
                .or_insert_with(|| ObservedWalletTransaction {
                    transaction_id: entry.tx_id.clone(),
                    block_daa_score: entry.block_daa_score,
                    addresses: Vec::new(),
                });
        item.block_daa_score = item.block_daa_score.max(entry.block_daa_score);
        if let Some(address) = tracker.address(entry) {
            if !item.addresses.contains(&address) {
                item.addresses.push(address);
            }
        }
    }
    observed.into_values().collect()
}

pub(crate) fn changed_transaction_ids(entries: &[UtxoEntry]) -> Vec<String> {
    let mut ids = entries
        .iter()
        .map(|entry| entry.tx_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

pub(crate) fn emit_utxo_set(
    events: &broadcast::Sender<WalletNodeEvent>,
    tracker: &UtxoTracker,
    changed: &[UtxoEntry],
) {
    let set = &tracker.set;
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
        changed_transaction_ids: changed_transaction_ids(changed),
        observed_transactions: observed_transactions(tracker, changed),
    });
}

mod connection;
pub(crate) use connection::{announce_reconnecting, connect_and_subscribe};

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(script: Vec<u8>, amount: u64) -> UtxoEntry {
        UtxoEntry {
            tx_id: "ab".repeat(32),
            index: 1,
            amount,
            script_public_key: script,
            block_daa_score: 9,
            covenant_id: None,
        }
    }

    #[test]
    fn tracker_derives_p2pk_addresses_and_balances() {
        let mut script = vec![0x20];
        script.extend_from_slice(&[7; 32]);
        script.push(0xac);
        let mut tracker = UtxoTracker::new("testnet-10");
        let utxo = entry(script, 42);
        tracker.apply(&utxo);
        let address = tracker
            .set
            .values()
            .next()
            .unwrap()
            .address
            .clone()
            .unwrap();
        assert!(address.starts_with("kaspatest:q"));
        let observed = observed_transactions(&tracker, std::slice::from_ref(&utxo));
        assert_eq!(observed[0].addresses, vec![address]);
        tracker.remove(&utxo);
        assert!(tracker.set.is_empty());
    }

    #[test]
    fn unknown_scripts_have_no_address() {
        let tracker = UtxoTracker::new("mainnet");
        assert!(tracker.address(&entry(vec![0x51], 1)).is_none());
    }
}
