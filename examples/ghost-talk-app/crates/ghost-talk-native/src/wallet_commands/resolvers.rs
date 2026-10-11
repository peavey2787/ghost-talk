pub(crate) fn resolve_wrpc_override(
    override_endpoint: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(endpoint) = override_endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(Some(endpoint.to_owned()));
    }
    Err("custom Kaspa RPC endpoint must use ws:// or wss://".into())
}

#[cfg(test)]
mod tests {
    use super::super::recommended_receive_index;
    use super::super::send::supported_network;
    use ghost_kaspa::wallet::WalletPublic;

    #[test]
    fn receive_cursor_advances_past_highest_observed_receive_address() {
        let public = WalletPublic {
            network: "testnet-10".into(),
            account_path: "m/44'/111111'/0'".into(),
            receive_addresses: vec!["r0".into(), "r1".into(), "r2".into(), "r3".into()],
            change_addresses: vec!["c0".into()],
            next_receive_index: 0,
            next_change_index: 0,
        };
        assert_eq!(recommended_receive_index(&public, &["r0".into()]), 1);
        assert_eq!(recommended_receive_index(&public, &["r2".into()]), 3);
        assert_eq!(recommended_receive_index(&public, &["c0".into()]), 0);
    }

    #[test]
    fn wallet_creation_accepts_only_current_ui_networks() {
        assert_eq!(supported_network("mainnet").unwrap(), "mainnet");
        assert_eq!(supported_network("testnet-10").unwrap(), "testnet-10");
        assert!(supported_network("testnet-11").is_err());
        assert!(supported_network("devnet").is_err());
    }
}
