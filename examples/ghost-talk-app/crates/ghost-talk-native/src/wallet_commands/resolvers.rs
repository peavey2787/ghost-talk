use super::send::ResolverNodeDescriptor;
type ResolverResponse = (&'static str, Result<(String, String), String>);
type ResolverTaskResult = Result<ResolverResponse, tokio::task::JoinError>;
pub(crate) use ghost_kaspa::PUBLIC_WRPC_RESOLVERS as PUBLIC_RESOLVERS;

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

pub(crate) fn resolver_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(1_500))
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|error| format!("Kaspa public resolver client: {error}"))
}

pub(crate) fn resolver_tasks(
    client: &reqwest::Client,
    network: &str,
) -> tokio::task::JoinSet<ResolverResponse> {
    let mut tasks = tokio::task::JoinSet::new();
    for resolver in PUBLIC_RESOLVERS {
        let client = client.clone();
        let network = network.to_owned();
        tasks.spawn(async move { (resolver, query_resolver(client, resolver, &network).await) });
    }
    tasks
}

pub(crate) async fn query_resolver(
    client: reqwest::Client,
    resolver: &str,
    network: &str,
) -> Result<(String, String), String> {
    let url = ghost_kaspa::resolver_query_url(resolver, network)?;
    let response = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "ghost-talk/0.1.0")
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let descriptor: ResolverNodeDescriptor = response
        .json()
        .await
        .map_err(|error| format!("invalid resolver descriptor: {error}"))?;
    let endpoint = descriptor.url.trim().to_owned();
    if !endpoint.starts_with("wss://") {
        return Err(format!(
            "TLS resolver returned non-WSS endpoint: {}",
            descriptor.url
        ));
    }
    Ok((descriptor.uid.unwrap_or_default(), endpoint))
}

pub(crate) fn absorb_resolver_result(
    result: ResolverTaskResult,
    endpoints: &mut Vec<String>,
    errors: &mut Vec<String>,
) {
    match result {
        Ok((resolver, Ok((uid, endpoint)))) => {
            record_resolver_success(resolver, uid, endpoint, endpoints)
        }
        Ok((resolver, Err(error))) => record_resolver_failure(resolver, error, errors),
        Err(error) => record_resolver_task_failure(error, errors),
    }
}

pub(crate) fn record_resolver_success(
    resolver: &str,
    uid: String,
    endpoint: String,
    endpoints: &mut Vec<String>,
) {
    eprintln!(
        "Ghost Talk: Kaspa resolver selected candidate: resolver={} endpoint={}",
        resolver, endpoint
    );
    crate::debug_log::record(
        "info",
        "kaspa",
        "public-resolver-node",
        format!(
            "resolver={} uid={} endpoint={}",
            resolver,
            uid.chars().take(64).collect::<String>(),
            endpoint
        ),
    );
    if !endpoints.iter().any(|value| value == &endpoint) {
        endpoints.push(endpoint);
    }
}

pub(crate) fn record_resolver_failure(resolver: &str, error: String, errors: &mut Vec<String>) {
    crate::debug_log::record(
        "warn",
        "kaspa",
        "public-resolver-failed",
        format!("resolver={} error={}", resolver, error),
    );
    errors.push(format!("{resolver}: {error}"));
}

pub(crate) fn record_resolver_task_failure(
    error: tokio::task::JoinError,
    errors: &mut Vec<String>,
) {
    crate::debug_log::record(
        "warn",
        "kaspa",
        "public-resolver-task-failed",
        format!("error={error}"),
    );
    errors.push(format!("resolver task: {error}"));
}

pub(crate) fn finish_resolver_discovery(
    endpoints: Vec<String>,
    errors: Vec<String>,
) -> Result<Vec<String>, String> {
    if !endpoints.is_empty() {
        return Ok(endpoints);
    }
    let detail = errors.into_iter().take(4).collect::<Vec<_>>().join(" | ");
    let error = if detail.is_empty() {
        "no Kaspa public wRPC resolver returned a usable WSS endpoint".to_string()
    } else {
        format!("no Kaspa public wRPC resolver returned a usable WSS endpoint: {detail}")
    };
    eprintln!("Ghost Talk: {error}");
    Err(error)
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
