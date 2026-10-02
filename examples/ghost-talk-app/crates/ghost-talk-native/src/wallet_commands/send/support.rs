use super::{
    absorb_resolver_result, finish_resolver_discovery, resolve_wrpc_override, resolver_http_client,
    resolver_tasks, WalletPublic, WalletSecret,
};

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

/// Select one or more concrete Kaspa wRPC endpoints for the published
/// `KaspaPortal::builder().network(...).endpoint(...).connect().await` API.
///
/// Resolver HTTPS is endpoint metadata discovery only. It never supplies
/// wallet state and never opens a Kaspa wRPC session; the selected WebSocket is
/// opened exclusively by Kaspa Portal.
pub(crate) async fn resolve_wrpc_endpoints(
    network: &str,
    override_endpoint: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(endpoint) = resolve_wrpc_override(override_endpoint)? {
        return Ok(vec![endpoint]);
    }
    let network =
        ghost_kaspa::upstream::portal::primitives::NetworkId::parse(network)?.canonical_name();
    let client = resolver_http_client()?;
    let mut tasks = resolver_tasks(&client, &network);
    let mut endpoints = Vec::new();
    let mut errors = Vec::new();
    while let Some(result) = tasks.join_next().await {
        absorb_resolver_result(result, &mut endpoints, &mut errors);
    }
    finish_resolver_discovery(endpoints, errors)
}
