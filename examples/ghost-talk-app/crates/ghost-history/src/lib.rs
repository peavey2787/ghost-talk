#![forbid(unsafe_code)]

use ghost_core::KaspaAddress;
use serde::{Deserialize, Serialize};

mod fetch;
mod scan;
use fetch::{history_entries, history_next_cursor, HistoryPage};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryTx {
    pub transaction_id: String,
    pub accepting_block_blue_score: Option<u64>,
    pub block_time: Option<u64>,
    pub payload: Vec<u8>,
    #[serde(default)]
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct WalletHistoryScan {
    pub transactions: Vec<HistoryTx>,
    pub failed_addresses: Vec<String>,
}

#[derive(Clone)]
pub struct RestHistory {
    base: String,
    client: reqwest::Client,
    max_pages: usize,
}

impl Default for RestHistory {
    fn default() -> Self {
        Self::new("https://api.kaspa.org", 32)
    }
}

impl RestHistory {
    pub fn new(base: &str, max_pages: usize) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(5))
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base: base.trim_end_matches('/').into(),
            client,
            max_pages: max_pages.max(1),
        }
    }

    // REST is transaction-history-only. Current balance, UTXOs, DAA,
    // active-address state, block walking, and live carrier delivery belong to
    // Kaspa wRPC subscriptions. The raw address-page helper below is used only
    // by explicit/import-time history collection and bounded backup restore.

    async fn address_transactions(&self, address: &KaspaAddress) -> Result<Vec<HistoryTx>, String> {
        let mut out = Vec::new();
        let mut before = None::<u64>;
        for _ in 0..self.max_pages {
            let page = self.fetch_history_page(address, before).await?;
            if page.entries.is_empty() {
                break;
            }
            out.extend(history_entries(address, &page.entries));
            let Some(next_before) = page.next_before else {
                break;
            };
            if before == Some(next_before) {
                return Err("history pagination cursor did not advance".into());
            }
            before = Some(next_before);
        }
        Ok(out)
    }

    async fn fetch_history_page(
        &self,
        address: &KaspaAddress,
        before: Option<u64>,
    ) -> Result<HistoryPage, String> {
        let mut request = self
            .client
            .get(format!(
                "{}/addresses/{}/full-transactions-page",
                self.base,
                address.as_str()
            ))
            .query(&[("limit", "100"), ("acceptance", "accepted")]);
        if let Some(before) = before {
            request = request.query(&[("before", before.to_string())]);
        }
        let response = request
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let next_before = history_next_cursor(response.headers())?;
        let value: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
        let entries = value
            .as_array()
            .ok_or_else(|| "history response is not an array".to_string())?
            .to_vec();
        Ok(HistoryPage {
            entries,
            next_before,
        })
    }
}

pub use ghost_core::kaspa_rest_base as rest_base_for_network;
pub use ghost_protocol::is_ghost_payload;

pub(crate) fn optional_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value {
        Some(serde_json::Value::Number(number)) => number.as_u64(),
        Some(serde_json::Value::String(value)) => value.parse::<u64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
