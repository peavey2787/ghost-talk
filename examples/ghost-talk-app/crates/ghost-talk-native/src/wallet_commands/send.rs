use super::{
    resolvers::{
        absorb_resolver_result, finish_resolver_discovery, resolve_wrpc_override,
        resolver_http_client, resolver_tasks,
    },
    wallet_lifecycle::{
        broadcast_projection, rest_base_for_network, BroadcastResult, Instant, RestHistory, State,
        WalletHistoryEntry, WalletHistoryResult, WalletNetworkOptions, WalletPublic,
        WalletRuntimeState, WalletSecret,
    },
};
#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn wallet_send(
    state: State<'_, WalletRuntimeState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    profile_id: String,
    reuse_unlocked: bool,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    amount_sompi: String,
    fee_sompi: String,
    options: WalletNetworkOptions,
) -> Result<BroadcastResult, String> {
    let outbound_lock = state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    let started = Instant::now();
    let wallet_ready_started = Instant::now();
    let secret = outbound_secret(
        &state,
        &profile_id,
        reuse_unlocked,
        &password,
        &sealed,
        &public,
    )?;
    let wallet_ready_ms = wallet_ready_started.elapsed().as_millis();
    let amount = parse_u64_decimal(&amount_sompi, "amount")?;
    let fee = parse_u64_decimal(&fee_sompi, "fee")?;
    log_send_start(
        &profile_id,
        &destination,
        amount,
        fee,
        wallet_ready_ms,
        reuse_unlocked,
    );
    let portal = timed_outbound_portal(
        &gateway,
        &public,
        options.wrpc_endpoint.as_deref(),
        &profile_id,
        started,
    )
    .await?;
    let result =
        ghost_kaspa::wallet::send(&portal, &secret, &public, &destination, amount, fee).await;
    record_send_result(&gateway, &profile_id, started, &result).await;
    result.map(broadcast_projection)
}

fn outbound_secret(
    state: &WalletRuntimeState,
    profile_id: &str,
    reuse_unlocked: bool,
    password: &str,
    sealed: &[u8],
    public: &WalletPublic,
) -> Result<WalletSecret, String> {
    if reuse_unlocked {
        return state.secret_or_open(profile_id, password, sealed, public);
    }
    let secret = open_secret(password, sealed)?;
    validate_public_projection(&secret, public)?;
    state.insert(profile_id.to_owned(), secret.clone())?;
    Ok(secret)
}

fn log_send_start(
    profile_id: &str,
    destination: &str,
    amount: u64,
    fee: u64,
    wallet_ready_ms: u128,
    reuse_unlocked: bool,
) {
    crate::debug_log::record(
        "info", "wallet", "kaspa-send-start",
        format!("profile={profile_id} destination={destination} amount_sompi={amount} requested_fee_sompi={fee} wallet_ready_ms={wallet_ready_ms} reuse_unlocked={reuse_unlocked}"),
    );
}

async fn timed_outbound_portal(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    public: &WalletPublic,
    wrpc_endpoint: Option<&str>,
    profile_id: &str,
    started: Instant,
) -> Result<ghost_kaspa::PortalFacade, String> {
    let portal_started = Instant::now();
    let result = crate::mailbox_commands::outbound_portal(gateway, public, wrpc_endpoint).await;
    match result {
        Ok(portal) => {
            crate::debug_log::record(
                "debug",
                "wallet",
                "kaspa-send-portal-ready",
                format!(
                    "profile={profile_id} elapsed_ms={}",
                    portal_started.elapsed().as_millis()
                ),
            );
            Ok(portal)
        }
        Err(error) => {
            crate::debug_log::record(
                "error",
                "wallet",
                "kaspa-send-portal-failed",
                format!(
                    "profile={profile_id} elapsed_ms={} total_elapsed_ms={} error={error}",
                    portal_started.elapsed().as_millis(),
                    started.elapsed().as_millis()
                ),
            );
            Err(error)
        }
    }
}

async fn record_send_result(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    profile_id: &str,
    started: Instant,
    result: &Result<ghost_kaspa::wallet::KaspaBroadcastResult, String>,
) {
    match result {
        Ok(sent) => log_send_success(profile_id, started, sent),
        Err(error) => {
            gateway.note_operation_error(error).await;
            crate::debug_log::record(
                "error",
                "wallet",
                "kaspa-send-failed",
                format!(
                    "profile={profile_id} total_elapsed_ms={} error={error}",
                    started.elapsed().as_millis()
                ),
            );
        }
    }
}

fn log_send_success(
    profile_id: &str,
    started: Instant,
    sent: &ghost_kaspa::wallet::KaspaBroadcastResult,
) {
    if let Some(timing) = sent.timings.as_ref() {
        crate::debug_log::record(
            "info", "wallet", "kaspa-send-timing",
            format!("profile={profile_id} txid={} utxo_plan_ms={} signed_analysis_ms={} submit_ms={} core_total_ms={}", sent.transaction_id, timing.utxo_plan_ms, timing.signed_analysis_ms, timing.submit_ms, timing.total_ms),
        );
    }
    crate::debug_log::record(
        "info",
        "wallet",
        "kaspa-send-broadcast-accepted",
        format!(
            "profile={profile_id} txid={} actual_fee_sompi={} total_elapsed_ms={}",
            sent.transaction_id,
            sent.fee_sompi,
            started.elapsed().as_millis()
        ),
    );
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn wallet_consolidate(
    state: State<'_, WalletRuntimeState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    profile_id: String,
    reuse_unlocked: bool,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    options: WalletNetworkOptions,
) -> Result<BroadcastResult, String> {
    let outbound_lock = state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    let secret = if reuse_unlocked {
        state.secret_or_open(&profile_id, &password, &sealed, &public)?
    } else {
        let secret = open_secret(&password, &sealed)?;
        validate_public_projection(&secret, &public)?;
        state.insert(profile_id.clone(), secret.clone())?;
        secret
    };
    let fee = parse_u64_decimal(&fee_sompi, "fee")?;
    let portal = crate::mailbox_commands::outbound_portal(
        &gateway,
        &public,
        options.wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = ghost_kaspa::wallet::consolidate(&portal, &secret, &public, fee).await;
    if let Err(error) = &result {
        gateway.note_operation_error(error).await;
    }
    result.map(broadcast_projection)
}

pub async fn gather_wallet_history(
    public: &WalletPublic,
    rest_override: Option<&str>,
    priority_addresses: &[String],
) -> Result<WalletHistoryResult, String> {
    let addresses = ordered_history_addresses(public, priority_addresses);
    let rest = rest_override
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| rest_base_for_network(&public.network));
    let scan = RestHistory::new(rest, usize::MAX)
        .wallet_history(&addresses)
        .await?;
    Ok(project_history_result(public, scan))
}

fn ordered_history_addresses(public: &WalletPublic, priority_addresses: &[String]) -> Vec<String> {
    let wallet_addresses = public
        .all_addresses()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let mut seen = std::collections::HashSet::<String>::new();
    priority_addresses
        .iter()
        .filter(|address| wallet_addresses.contains(*address))
        .chain(public.all_addresses())
        .filter(|address| seen.insert((*address).clone()))
        .cloned()
        .collect()
}

fn project_history_result(
    public: &WalletPublic,
    scan: ghost_history::WalletHistoryScan,
) -> WalletHistoryResult {
    let mut used_addresses = scan
        .transactions
        .iter()
        .flat_map(|transaction| transaction.addresses.iter().cloned())
        .collect::<Vec<_>>();
    used_addresses.sort();
    used_addresses.dedup();
    let recommended_receive_index = recommended_receive_index(public, &used_addresses);
    let history = scan
        .transactions
        .into_iter()
        .map(project_history_entry)
        .collect();
    WalletHistoryResult {
        history,
        warnings: scan.failed_addresses,
        used_addresses,
        recommended_receive_index,
    }
}

fn project_history_entry(transaction: ghost_history::HistoryTx) -> WalletHistoryEntry {
    ghost_kaspa::project_wallet_history_entry(
        transaction.transaction_id,
        transaction.accepting_block_blue_score,
        transaction.block_time,
        &transaction.payload,
        transaction.addresses,
    )
}

mod signer;
pub(crate) use signer::{wallet_broadcast_signer_send, wallet_prepare_signer_send};
mod support;
pub(crate) use support::{
    open_secret, parse_u64_decimal, recommended_receive_index, resolve_wrpc_endpoints, seal_secret,
    supported_network, validate_public_projection, validate_requested_network,
    ResolverNodeDescriptor,
};
