use super::{WalletPublic, WalletSecret};

pub(crate) fn recommended_receive_index(
    public: &WalletPublic,
    observed_addresses: &[String],
) -> usize {
    if public.receive_addresses.is_empty() {
        return 0;
    }
    let observed = observed_addresses
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let highest_used = public
        .receive_addresses
        .iter()
        .enumerate()
        .filter_map(|(index, address)| observed.contains(address.as_str()).then_some(index))
        .max();
    let last = public.receive_addresses.len() - 1;
    match highest_used {
        Some(index) if index >= public.next_receive_index => index.saturating_add(1).min(last),
        _ => public.next_receive_index.min(last),
    }
}

pub fn open_secret(password: &str, sealed: &[u8]) -> Result<WalletSecret, String> {
    ghost_storage::open_json(password, sealed, "wallet vault")
}

pub(crate) fn seal_secret(password: &str, secret: &WalletSecret) -> Result<Vec<u8>, String> {
    ghost_storage::seal_json(password, secret, "wallet vault")
}

pub(crate) fn validate_public_projection(
    secret: &WalletSecret,
    public: &WalletPublic,
) -> Result<(), String> {
    ghost_kaspa::wallet::validate_public_projection(secret, public)
}

pub(crate) fn supported_network(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mainnet" => Ok("mainnet".to_string()),
        "testnet-10" => Ok("testnet-10".to_string()),
        _ => Err("Kaspa network must be mainnet or testnet-10".into()),
    }
}

pub(crate) fn validate_requested_network(
    requested: &str,
    secret: &WalletSecret,
    public: &WalletPublic,
) -> Result<(), String> {
    if secret.network != requested || public.network != requested {
        return Err("wallet network projection does not match the selected Kaspa network".into());
    }
    Ok(())
}

pub(crate) fn parse_u64_decimal(value: &str, label: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{label} must be an unsigned decimal integer"));
    }
    value
        .parse::<u64>()
        .map_err(|_| format!("{label} is outside the supported range"))
}
