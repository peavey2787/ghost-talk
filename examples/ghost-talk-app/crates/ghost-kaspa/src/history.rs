use ghost_api::WalletHistoryEntry;

pub fn project_wallet_history_entry(
    transaction_id: String,
    accepting_block_blue_score: Option<u64>,
    block_time: Option<u64>,
    payload: &[u8],
    addresses: Vec<String>,
) -> WalletHistoryEntry {
    WalletHistoryEntry {
        transaction_id,
        blue_score: accepting_block_blue_score.unwrap_or_default().to_string(),
        block_time,
        ghost_payload: ghost_protocol::is_ghost_payload(payload),
        addresses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_plain_transaction() {
        let entry = project_wallet_history_entry(
            "tx".into(),
            Some(42),
            Some(123),
            b"not-ghost",
            vec!["kaspa:test".into()],
        );
        assert_eq!(entry.transaction_id, "tx");
        assert_eq!(entry.blue_score, "42");
        assert_eq!(entry.block_time, Some(123));
        assert!(!entry.ghost_payload);
        assert_eq!(entry.addresses, vec!["kaspa:test"]);
    }
}
