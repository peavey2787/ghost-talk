use ghost_kaspa::wallet::WalletPublic;

pub(crate) fn stable_address(public: &WalletPublic) -> Result<String, String> {
    public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wallet(receive_addresses: Vec<String>) -> WalletPublic {
        WalletPublic {
            network: "mainnet".into(),
            account_path: "m/44'/111111'/0'".into(),
            receive_addresses,
            change_addresses: vec!["kaspa:qchange".into()],
            next_receive_index: 0,
            next_change_index: 0,
        }
    }

    #[test]
    fn stable_address_uses_first_receive_address() {
        let public = wallet(vec!["kaspa:qstable".into(), "kaspa:qnext".into()]);
        assert_eq!(stable_address(&public).unwrap(), "kaspa:qstable");
    }

    #[test]
    fn stable_address_rejects_wallet_without_receive_address() {
        let error = stable_address(&wallet(Vec::new())).unwrap_err();
        assert_eq!(error, "wallet has no stable Ghost Talk receive address");
    }
}
