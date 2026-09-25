use super::{HistoryTx, RestHistory, WalletHistoryScan};
use ghost_core::KaspaAddress;
use std::collections::BTreeMap;

impl RestHistory {
    /// Explicit user-requested archival discovery across the wallet's derived
    /// address set. This does not participate in live balance/UTXO state.
    pub async fn wallet_history(&self, addresses: &[String]) -> Result<WalletHistoryScan, String> {
        self.wallet_history_scan(addresses, None).await
    }

    /// REST is reserved for genuinely historical transactions. Entries whose
    /// block time is missing or newer than `cutoff_ms` are excluded because the
    /// REST service must never become an authority for current wallet state.
    pub async fn wallet_history_before_ms(
        &self,
        addresses: &[String],
        cutoff_ms: u64,
    ) -> Result<WalletHistoryScan, String> {
        self.wallet_history_scan(addresses, Some(cutoff_ms)).await
    }

    async fn wallet_history_scan(
        &self,
        addresses: &[String],
        cutoff_ms: Option<u64>,
    ) -> Result<WalletHistoryScan, String> {
        const CONCURRENCY: usize = 4;

        let mut queue = unique_addresses(addresses).into_iter();
        let mut jobs = tokio::task::JoinSet::new();
        let mut accumulator = HistoryAccumulator::default();

        for _ in 0..CONCURRENCY {
            let Some(address) = queue.next() else {
                break;
            };
            spawn_history_job(&mut jobs, self.clone(), address);
        }

        while let Some(joined) = jobs.join_next().await {
            accumulator.absorb(joined, cutoff_ms);
            if let Some(address) = queue.next() {
                spawn_history_job(&mut jobs, self.clone(), address);
            }
        }

        accumulator.finish()
    }
}

#[derive(Default)]
struct HistoryAccumulator {
    unique: BTreeMap<String, HistoryTx>,
    failed_addresses: Vec<String>,
    successful_addresses: usize,
}

impl HistoryAccumulator {
    fn absorb(
        &mut self,
        joined: Result<(String, Result<Vec<HistoryTx>, String>), tokio::task::JoinError>,
        cutoff_ms: Option<u64>,
    ) {
        match joined {
            Ok((_address, Ok(transactions))) => self.absorb_success(transactions, cutoff_ms),
            Ok((address, Err(error))) => self.failed_addresses.push(format!("{address}: {error}")),
            Err(error) => self
                .failed_addresses
                .push(format!("history worker failed: {error}")),
        }
    }

    fn absorb_success(&mut self, transactions: Vec<HistoryTx>, cutoff_ms: Option<u64>) {
        self.successful_addresses += 1;
        for transaction in transactions {
            if transaction_is_before_cutoff(&transaction, cutoff_ms) {
                merge_history_transaction(&mut self.unique, transaction);
            }
        }
    }

    fn finish(self) -> Result<WalletHistoryScan, String> {
        if self.successful_addresses == 0 && !self.failed_addresses.is_empty() {
            return Err(format!(
                "Historical transaction requests failed for every wallet address; first failure: {}",
                self.failed_addresses[0]
            ));
        }
        let mut transactions = self.unique.into_values().collect::<Vec<_>>();
        transactions.sort_by_key(|transaction| {
            std::cmp::Reverse(transaction.accepting_block_blue_score.unwrap_or_default())
        });
        Ok(WalletHistoryScan {
            transactions,
            failed_addresses: self.failed_addresses,
        })
    }
}

fn unique_addresses(addresses: &[String]) -> Vec<String> {
    let mut ordered = Vec::new();
    for address in addresses {
        if !ordered.contains(address) {
            ordered.push(address.clone());
        }
    }
    ordered
}

fn spawn_history_job(
    jobs: &mut tokio::task::JoinSet<(String, Result<Vec<HistoryTx>, String>)>,
    history: RestHistory,
    address: String,
) {
    jobs.spawn(async move {
        let result = match KaspaAddress::parse(&address) {
            Ok(parsed) => history.address_transactions(&parsed).await,
            Err(error) => Err(error),
        };
        (address, result)
    });
}

fn transaction_is_before_cutoff(transaction: &HistoryTx, cutoff_ms: Option<u64>) -> bool {
    cutoff_ms.is_none_or(|cutoff| transaction.block_time.is_some_and(|time| time <= cutoff))
}

fn merge_history_transaction(unique: &mut BTreeMap<String, HistoryTx>, mut transaction: HistoryTx) {
    if let Some(existing) = unique.get_mut(&transaction.transaction_id) {
        for matched in transaction.addresses.drain(..) {
            if !existing.addresses.contains(&matched) {
                existing.addresses.push(matched);
            }
        }
        return;
    }
    unique.insert(transaction.transaction_id.clone(), transaction);
}
