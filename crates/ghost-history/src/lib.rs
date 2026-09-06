#![forbid(unsafe_code)]

use ghost_core::KaspaAddress;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryTx {
    pub transaction_id: String,
    pub accepting_block_blue_score: Option<u64>,
    pub block_time: Option<u64>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DagObservation {
    pub transaction_id: String,
    pub blue_score: u64,
    pub block_time: Option<u64>,
    pub payload: Vec<u8>,
    pub addresses: Vec<String>,
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
            max_pages: max_pages.clamp(1, 128),
        }
    }

    // REST is intentionally archival-only. Current balance, UTXOs, DAA,
    // active-address state, block walking, and live carrier delivery belong to
    // the single public Kaspa wRPC node owned by Kaspa Portal. The private raw
    // address-page helpers below exist only to implement the cutoff-enforcing
    // public archival methods.

    async fn address_transactions(
        &self,
        address: &KaspaAddress,
    ) -> Result<Vec<HistoryTx>, String> {
        let mut out = Vec::new();
        let mut before = None::<u64>;
        for _ in 0..self.max_pages {
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
            let next_before = response
                .headers()
                .get("x-next-page-before")
                .map(|value| {
                    value
                        .to_str()
                        .map_err(|error| format!("invalid history pagination header: {error}"))?
                        .parse::<u64>()
                        .map_err(|error| format!("invalid history pagination cursor: {error}"))
                })
                .transpose()?;
            let value: serde_json::Value = response
                .json()
                .await
                .map_err(|error| error.to_string())?;
            let entries = value
                .as_array()
                .ok_or_else(|| "history response is not an array".to_string())?;
            if entries.is_empty() {
                break;
            }
            for transaction in entries {
                if let Some(id) = transaction
                    .get("transaction_id")
                    .or_else(|| transaction.get("transactionId"))
                    .and_then(serde_json::Value::as_str)
                {
                    let payload = transaction
                        .get("payload")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|payload| hex::decode(payload).ok())
                        .unwrap_or_default();
                    let blue_score = optional_u64(
                        transaction
                            .get("accepting_block_blue_score")
                            .or_else(|| transaction.get("acceptingBlockBlueScore")),
                    );
                    let block_time = optional_u64(
                        transaction
                            .get("block_time")
                            .or_else(|| transaction.get("blockTime")),
                    );
                    out.push(HistoryTx {
                        transaction_id: id.to_owned(),
                        accepting_block_blue_score: blue_score,
                        block_time,
                        payload,
                    });
                }
            }
            let Some(next_before) = next_before else {
                break;
            };
            if before == Some(next_before) {
                return Err("history pagination cursor did not advance".into());
            }
            before = Some(next_before);
        }
        Ok(out)
    }

    async fn address_transactions_after(
        &self,
        address: &KaspaAddress,
        after_blue_score: u64,
    ) -> Result<Vec<HistoryTx>, String> {
        let mut out = Vec::new();
        let mut before = None::<u64>;
        for _ in 0..self.max_pages {
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
            let next_before = response
                .headers()
                .get("x-next-page-before")
                .map(|value| {
                    value
                        .to_str()
                        .map_err(|error| format!("invalid history pagination header: {error}"))?
                        .parse::<u64>()
                        .map_err(|error| format!("invalid history pagination cursor: {error}"))
                })
                .transpose()?;
            let value: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
            let entries = value
                .as_array()
                .ok_or_else(|| "history response is not an array".to_string())?;
            if entries.is_empty() {
                break;
            }
            let mut page_has_newer = false;
            for transaction in entries {
                let Some(id) = transaction
                    .get("transaction_id")
                    .or_else(|| transaction.get("transactionId"))
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let blue_score = optional_u64(
                    transaction
                        .get("accepting_block_blue_score")
                        .or_else(|| transaction.get("acceptingBlockBlueScore")),
                );
                if blue_score.unwrap_or_default() <= after_blue_score {
                    continue;
                }
                page_has_newer = true;
                let payload = transaction
                    .get("payload")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|payload| hex::decode(payload).ok())
                    .unwrap_or_default();
                let block_time = optional_u64(
                    transaction
                        .get("block_time")
                        .or_else(|| transaction.get("blockTime")),
                );
                out.push(HistoryTx {
                    transaction_id: id.to_owned(),
                    accepting_block_blue_score: blue_score,
                    block_time,
                    payload,
                });
            }
            // Address history pages are newest-first. Once an entire page is at
            // or before the overlap checkpoint, older pages cannot contribute.
            if !page_has_newer {
                break;
            }
            let Some(next_before) = next_before else { break; };
            if before == Some(next_before) {
                return Err("history pagination cursor did not advance".into());
            }
            before = Some(next_before);
        }
        Ok(out)
    }

    /// REST is reserved for genuinely historical transactions. Entries whose
    /// block time is missing or newer than `cutoff_ms` are excluded because the
    /// REST service must never become an authority for current wallet state.
    pub async fn wallet_history_before_ms(
        &self,
        addresses: &[String],
        cutoff_ms: u64,
    ) -> Result<Vec<HistoryTx>, String> {
        let mut unique = BTreeMap::<String, HistoryTx>::new();
        for address in addresses {
            let address = KaspaAddress::parse(address)?;
            for transaction in self.address_transactions(&address).await? {
                if !transaction.block_time.is_some_and(|time| time <= cutoff_ms) {
                    continue;
                }
                unique
                    .entry(transaction.transaction_id.clone())
                    .or_insert(transaction);
            }
        }
        let mut transactions = unique.into_values().collect::<Vec<_>>();
        transactions.sort_by_key(|transaction| {
            std::cmp::Reverse(transaction.accepting_block_blue_score.unwrap_or_default())
        });
        Ok(transactions)
    }

    /// Historical mailbox recovery with a hard time boundary. Current/recent
    /// carriers are never sourced from REST; they belong to the Kaspa wRPC live
    /// stream (or wRPC DAG recovery).
    pub async fn historical_mailbox_before_ms(
        &self,
        addresses: &[String],
        after_blue_score: u64,
        cutoff_ms: u64,
    ) -> Result<Vec<DagObservation>, String> {
        let mut seen = BTreeSet::new();
        let mut observations = Vec::new();
        for address in addresses {
            let address = KaspaAddress::parse(address)?;
            for transaction in self.address_transactions_after(&address, after_blue_score).await? {
                if !transaction.block_time.is_some_and(|time| time <= cutoff_ms) {
                    continue;
                }
                let score = transaction.accepting_block_blue_score.unwrap_or_default();
                if !is_ghost_payload(&transaction.payload)
                    || !seen.insert(transaction.transaction_id.clone())
                {
                    continue;
                }
                observations.push(DagObservation {
                    transaction_id: transaction.transaction_id,
                    blue_score: score,
                    block_time: transaction.block_time,
                    payload: transaction.payload,
                    addresses: Vec::new(),
                });
            }
        }
        observations.sort_by_key(|observation| observation.blue_score);
        Ok(observations)
    }
}

pub fn rest_base_for_network(network: &str) -> &'static str {
    match network.trim().to_ascii_lowercase().as_str() {
        "testnet-10" | "testnet10" => "https://api-tn10.kaspa.org",
        "testnet-11" | "testnet11" => "https://api-tn11.kaspa.org",
        _ => "https://api.kaspa.org",
    }
}

pub fn is_ghost_payload(payload: &[u8]) -> bool {
    payload.starts_with(b"KKTP:")
        || payload.starts_with(b"GHST")
        || payload.starts_with(b"GTCD")
        || payload.starts_with(b"GTCR")
        || payload.starts_with(b"GTCA")
        || payload.starts_with(b"GTAK")
        || payload.starts_with(b"GTVA")
        || payload.starts_with(b"GTBK")
}

fn optional_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value {
        Some(serde_json::Value::Number(number)) => number.as_u64(),
        Some(serde_json::Value::String(value)) => value.parse::<u64>().ok(),
        _ => None,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_payload_filter_is_explicit() {
        assert!(is_ghost_payload(b"KKTP:ANCHOR:{\"type\":\"discovery\"}"));
        assert!(is_ghost_payload(b"KKTP:deadbeef:{}"));
        assert!(is_ghost_payload(b"GHSTpayload"));
        assert!(is_ghost_payload(b"GTCDpayload"));
        assert!(is_ghost_payload(b"GTCRpayload"));
        assert!(is_ghost_payload(b"GTCApayload"));
        assert!(is_ghost_payload(b"GTAKpayload"));
        assert!(is_ghost_payload(b"GTVApayload"));
        assert!(is_ghost_payload(b"GTBKpayload"));
        assert!(!is_ghost_payload(b"other"));
    }

    #[test]
    fn history_u64_fields_accept_numbers_and_decimal_strings() {
        assert_eq!(optional_u64(Some(&serde_json::json!(42))), Some(42));
        assert_eq!(optional_u64(Some(&serde_json::json!("42"))), Some(42));
        assert_eq!(optional_u64(Some(&serde_json::json!("not-a-number"))), None);
    }

    #[test]
    fn testnet_rest_endpoints_are_network_specific() {
        assert!(rest_base_for_network("testnet-10").contains("tn10"));
        assert!(rest_base_for_network("testnet-11").contains("tn11"));
        assert_eq!(rest_base_for_network("mainnet"), "https://api.kaspa.org");
    }
}
