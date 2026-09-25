use super::{optional_u64, HistoryTx, KaspaAddress};

pub(super) struct HistoryPage {
    pub(super) entries: Vec<serde_json::Value>,
    pub(super) next_before: Option<u64>,
}

pub(super) fn history_next_cursor(
    headers: &reqwest::header::HeaderMap,
) -> Result<Option<u64>, String> {
    headers
        .get("x-next-page-before")
        .map(|value| {
            value
                .to_str()
                .map_err(|error| format!("invalid history pagination header: {error}"))?
                .parse::<u64>()
                .map_err(|error| format!("invalid history pagination cursor: {error}"))
        })
        .transpose()
}

pub(super) fn history_entries(
    address: &KaspaAddress,
    entries: &[serde_json::Value],
) -> Vec<HistoryTx> {
    entries
        .iter()
        .filter_map(|transaction| history_entry(address, transaction))
        .collect()
}

fn history_entry(address: &KaspaAddress, transaction: &serde_json::Value) -> Option<HistoryTx> {
    let id = transaction
        .get("transaction_id")
        .or_else(|| transaction.get("transactionId"))?
        .as_str()?;
    let payload = transaction
        .get("payload")
        .and_then(serde_json::Value::as_str)
        .and_then(|payload| hex::decode(payload).ok())
        .unwrap_or_default();
    let accepting_block_blue_score = optional_u64(
        transaction
            .get("accepting_block_blue_score")
            .or_else(|| transaction.get("acceptingBlockBlueScore")),
    );
    let block_time = optional_u64(
        transaction
            .get("block_time")
            .or_else(|| transaction.get("blockTime")),
    );
    Some(HistoryTx {
        transaction_id: id.to_owned(),
        accepting_block_blue_score,
        block_time,
        payload,
        addresses: vec![address.as_str().to_owned()],
    })
}
