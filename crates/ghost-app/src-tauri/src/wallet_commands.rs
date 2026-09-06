use ghost_history::{rest_base_for_network, RestHistory};
use ghost_kaspa::wallet::{BroadcastResult, CreatedWallet, WalletPublic, WalletSecret};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, sync::{Arc, Mutex}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use tauri::State;
use zeroize::Zeroize;


#[derive(Default)]
pub struct WalletRuntimeState {
    secrets: Mutex<HashMap<String, WalletSecret>>,
    // Serialize spend planning/broadcast per profile. Portal uses the live UTXO
    // set, so two concurrent Ghost Talk carriers must never plan against the
    // same still-unspent outpoint while the first accepted transaction is
    // waiting to become visible through the node.
    outbound_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl WalletRuntimeState {
    fn insert(&self, profile_id: String, secret: WalletSecret) -> Result<(), String> {
        self.secrets
            .lock()
            .map_err(|_| "wallet runtime state is poisoned".to_string())?
            .insert(profile_id, secret);
        Ok(())
    }

    pub(super) fn secret_or_open(
        &self,
        profile_id: &str,
        password: &str,
        sealed: &[u8],
        public: &WalletPublic,
    ) -> Result<WalletSecret, String> {
        if let Some(secret) = self
            .secrets
            .lock()
            .map_err(|_| "wallet runtime state is poisoned".to_string())?
            .get(profile_id)
            .cloned()
        {
            validate_public_projection(&secret, public)?;
            return Ok(secret);
        }
        let secret = open_secret(password, sealed)?;
        validate_public_projection(&secret, public)?;
        self.insert(profile_id.to_owned(), secret.clone())?;
        Ok(secret)
    }

    fn remove(&self, profile_id: &str) -> Result<(), String> {
        self.secrets
            .lock()
            .map_err(|_| "wallet runtime state is poisoned".to_string())?
            .remove(profile_id);
        // Keep the per-profile outbound mutex allocated for the process lifetime.
        // Dropping/recreating it while an in-flight send still owns the old Arc
        // could allow a concurrent post-lock send to bypass serialization.
        Ok(())
    }

    pub(super) fn outbound_lock(
        &self,
        profile_id: &str,
    ) -> Result<Arc<tokio::sync::Mutex<()>>, String> {
        let mut locks = self
            .outbound_locks
            .lock()
            .map_err(|_| "wallet outbound state is poisoned".to_string())?;
        Ok(locks
            .entry(profile_id.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DerivationPresetInfo {
    pub label: &'static str,
    pub path: Option<&'static str>,
    pub supported: bool,
    pub note: &'static str,
}

#[derive(Clone, Serialize)]
pub struct WalletCreateResponse {
    pub sealed: Vec<u8>,
    pub mnemonic: String,
    pub public: WalletPublic,
}

#[derive(Clone, Serialize)]
pub struct WalletImportResponse {
    pub sealed: Vec<u8>,
    pub public: WalletPublic,
}

#[derive(Clone, Serialize)]
pub struct WalletRecovery {
    pub mnemonic: String,
    pub passphrase: String,
    pub account_path: String,
    pub network: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WalletHistoryEntry {
    pub transaction_id: String,
    pub blue_score: String,
    pub block_time: Option<u64>,
    pub ghost_payload: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct WalletSnapshot {
    pub balance_sompi: String,
    pub utxo_count: String,
    pub blue_score: String,
    pub history: Vec<WalletHistoryEntry>,
    pub active_addresses: usize,
    pub recommended_receive_index: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WalletNetworkOptions {
    #[serde(default)]
    pub rest_endpoint: Option<String>,
    #[serde(default)]
    pub wrpc_endpoint: Option<String>,
}

#[tauri::command]
pub fn derivation_presets() -> Vec<DerivationPresetInfo> {
    vec![
        DerivationPresetInfo {
            label: "Kaspa Standard (CLI / Kaspium / KasWare / OneKey / Tangem)",
            path: Some("m/44'/111111'/0'"),
            supported: true,
            note: "BIP44 account root; receive /0/index and change /1/index.",
        },
        DerivationPresetInfo {
            label: "Kaspa account #1",
            path: Some("m/44'/111111'/1'"),
            supported: true,
            note: "Second standard Kaspa BIP44 account.",
        },
        DerivationPresetInfo {
            label: "Kaspa account #2",
            path: Some("m/44'/111111'/2'"),
            supported: true,
            note: "Third standard Kaspa BIP44 account.",
        },
        DerivationPresetInfo {
            label: "Custom account path",
            path: None,
            supported: true,
            note: "Enter an account-level BIP32 path. Ghost Talk appends /0/index and /1/index.",
        },
        DerivationPresetInfo {
            label: "Legacy KDX / Kaspanet m/44'/972/0'",
            path: Some("m/44'/972/0'"),
            supported: false,
            note: "Legacy non-BIP32-compatible wallet family; shown for identification only.",
        },
    ]
}

#[tauri::command]
pub async fn wallet_create(
    password: String,
    passphrase: String,
    account_path: String,
    network: String,
) -> Result<WalletCreateResponse, String> {
    super::run_blocking("wallet creation", move || {
        require_wallet_password(&password)?;
        let (secret, CreatedWallet { mnemonic, public }) =
            ghost_kaspa::wallet::generate_wallet(&passphrase, &account_path, &network)?;
        let sealed = seal_secret(&password, &secret)?;
        Ok(WalletCreateResponse {
            sealed,
            mnemonic,
            public,
        })
    })
    .await
}

#[tauri::command]
pub async fn wallet_import(
    password: String,
    mnemonic: String,
    passphrase: String,
    account_path: String,
    network: String,
) -> Result<WalletImportResponse, String> {
    super::run_blocking("wallet restore", move || {
        require_wallet_password(&password)?;
        let (secret, public) =
            ghost_kaspa::wallet::import_wallet(&mnemonic, &passphrase, &account_path, &network)?;
        let sealed = seal_secret(&password, &secret)?;
        Ok(WalletImportResponse { sealed, public })
    })
    .await
}

#[tauri::command]
pub async fn wallet_unlock(
    state: State<'_, WalletRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<WalletPublic, String> {
    let public_for_worker = public.clone();
    let secret = super::run_blocking("wallet unlock", move || {
        let secret = open_secret(&password, &sealed)?;
        validate_public_projection(&secret, &public_for_worker)?;
        Ok(secret)
    })
    .await?;
    state.insert(profile_id, secret)?;
    Ok(public)
}

#[tauri::command]
pub fn wallet_lock(
    state: State<'_, WalletRuntimeState>,
    profile_id: String,
) -> Result<(), String> {
    state.remove(&profile_id)
}

#[tauri::command]
pub async fn wallet_reveal_recovery(
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<WalletRecovery, String> {
    super::run_blocking("wallet recovery reveal", move || {
        require_wallet_password(&password)?;
        let secret = open_secret(&password, &sealed)?;
        validate_public_projection(&secret, &public)?;
        Ok(WalletRecovery {
            mnemonic: secret.mnemonic.clone(),
            passphrase: secret.passphrase.clone(),
            account_path: secret.account_path.clone(),
            network: secret.network.clone(),
        })
    })
    .await
}

#[tauri::command]
pub fn wallet_next_receive(mut public: WalletPublic) -> Result<WalletPublic, String> {
    public.advance_receive()?;
    Ok(public)
}

#[tauri::command]
pub async fn wallet_refresh(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    public: WalletPublic,
    options: WalletNetworkOptions,
) -> Result<WalletSnapshot, String> {
    refresh_wallet(
        gateway.inner(),
        &public,
        options.wrpc_endpoint.as_deref(),
        options.rest_endpoint.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn wallet_send(
    state: State<'_, WalletRuntimeState>,
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
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
    let started = Instant::now();
    let wallet_ready_started = Instant::now();
    let secret = if reuse_unlocked {
        state.secret_or_open(&profile_id, &password, &sealed, &public)?
    } else {
        let secret = open_secret(&password, &sealed)?;
        validate_public_projection(&secret, &public)?;
        state.insert(profile_id.clone(), secret.clone())?;
        secret
    };
    let wallet_ready_ms = wallet_ready_started.elapsed().as_millis();
    let amount = parse_u64_decimal(&amount_sompi, "amount")?;
    let fee = parse_u64_decimal(&fee_sompi, "fee")?;
    crate::debug_log::record(
        "info",
        "wallet",
        "kaspa-send-start",
        format!("profile={} destination={} amount_sompi={} requested_fee_sompi={} wallet_ready_ms={} reuse_unlocked={}", profile_id, destination, amount, fee, wallet_ready_ms, reuse_unlocked),
    );
    let portal_started = Instant::now();
    let portal = match super::mailbox_commands::outbound_portal(
        &gateway,
        &public,
        options.wrpc_endpoint.as_deref(),
    )
    .await
    {
        Ok(portal) => portal,
        Err(error) => {
            crate::debug_log::record(
                "error",
                "wallet",
                "kaspa-send-portal-failed",
                format!("profile={} elapsed_ms={} total_elapsed_ms={} error={}", profile_id, portal_started.elapsed().as_millis(), started.elapsed().as_millis(), error),
            );
            return Err(error);
        }
    };
    crate::debug_log::record(
        "debug",
        "wallet",
        "kaspa-send-portal-ready",
        format!("profile={} elapsed_ms={}", profile_id, portal_started.elapsed().as_millis()),
    );
    let result = ghost_kaspa::wallet::send(&portal, &secret, &public, &destination, amount, fee).await;
    match &result {
        Ok(sent) => {
            if let Some(timing) = sent.timings.as_ref() {
                crate::debug_log::record(
                    "info",
                    "wallet",
                    "kaspa-send-timing",
                    format!(
                        "profile={} txid={} utxo_plan_ms={} signed_analysis_ms={} submit_ms={} core_total_ms={}",
                        profile_id,
                        sent.transaction_id,
                        timing.utxo_plan_ms,
                        timing.signed_analysis_ms,
                        timing.submit_ms,
                        timing.total_ms,
                    ),
                );
            }
            crate::debug_log::record(
                "info",
                "wallet",
                "kaspa-send-broadcast-accepted",
                format!("profile={} txid={} actual_fee_sompi={} total_elapsed_ms={}", profile_id, sent.transaction_id, sent.fee_sompi, started.elapsed().as_millis()),
            );
        }
        Err(error) => {
            gateway.note_operation_error(error).await;
            crate::debug_log::record(
                "error",
                "wallet",
                "kaspa-send-failed",
                format!("profile={} total_elapsed_ms={} error={}", profile_id, started.elapsed().as_millis(), error),
            );
        }
    }
    result
}

#[tauri::command]
pub async fn wallet_consolidate(
    state: State<'_, WalletRuntimeState>,
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    profile_id: String,
    reuse_unlocked: bool,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    options: WalletNetworkOptions,
) -> Result<BroadcastResult, String> {
    let secret = if reuse_unlocked {
        state.secret_or_open(&profile_id, &password, &sealed, &public)?
    } else {
        let secret = open_secret(&password, &sealed)?;
        validate_public_projection(&secret, &public)?;
        state.insert(profile_id.clone(), secret.clone())?;
        secret
    };
    let fee = parse_u64_decimal(&fee_sompi, "fee")?;
    let portal = super::mailbox_commands::outbound_portal(
        &gateway,
        &public,
        options.wrpc_endpoint.as_deref(),
    )
    .await?;
    let result = ghost_kaspa::wallet::consolidate(&portal, &secret, &public, fee).await;
    if let Err(error) = &result {
        gateway.note_operation_error(error).await;
    }
    result
}

pub async fn refresh_wallet(
    gateway: &super::kaspa_gateway::KaspaGatewayState,
    public: &WalletPublic,
    wrpc_override: Option<&str>,
    rest_override: Option<&str>,
) -> Result<WalletSnapshot, String> {
    const HISTORICAL_REST_MIN_AGE: Duration = Duration::from_secs(48 * 60 * 60);
    const HISTORICAL_REST_BUDGET: Duration = Duration::from_millis(750);

    let portal = super::mailbox_commands::outbound_portal(gateway, public, wrpc_override).await?;
    let addresses = public.all_addresses().cloned().collect::<Vec<_>>();

    // Current wallet truth comes only from the selected public Kaspa wRPC node.
    // Sends and live BlockAdded share the managed Portal connection; Ghost Talk only fans out decoded events.
    let utxos = match portal.current_utxos(&addresses).await {
        Ok(utxos) => utxos,
        Err(error) => {
            let error = format!("Kaspa wRPC current UTXO query failed: {error}");
            gateway.note_operation_error(&error).await;
            eprintln!("Ghost Talk: {error}");
            return Err(error);
        }
    };
    let blue_score = match portal.current_virtual_daa_score().await {
        Ok(score) => score,
        Err(error) => {
            let error = format!("Kaspa wRPC DAG-state query failed: {error}");
            gateway.note_operation_error(&error).await;
            eprintln!("Ghost Talk: {error}");
            return Err(error);
        }
    };

    let mut balance = 0u64;
    for utxo in &utxos {
        balance = balance
            .checked_add(utxo.amount)
            .ok_or_else(|| "wallet balance exceeds u64".to_string())?;
    }

    // Determine active derived addresses locally from the node-returned UTXO
    // scripts. This avoids REST /addresses/active and avoids one RPC per address.
    let mut script_to_address = HashMap::<Vec<u8>, String>::new();
    for address in &addresses {
        let script = ghost_kaspa::PortalFacade::script_pubkey_for_address(address)?;
        script_to_address.insert(script, address.clone());
    }
    let mut active_set = HashSet::<String>::new();
    for utxo in &utxos {
        if let Some(address) = script_to_address.get(&utxo.script_public_key) {
            active_set.insert(address.clone());
        }
    }

    let mut recommended_receive_index = public.next_receive_index;
    while recommended_receive_index + 1 < public.receive_addresses.len()
        && active_set.contains(&public.receive_addresses[recommended_receive_index])
    {
        recommended_receive_index += 1;
    }

    // REST is historical archival retrieval only. It is never allowed to
    // influence current balance/UTXOs/DAA and only returns transactions whose
    // on-chain block time proves they are at least 48 hours old. Historical
    // enrichment also has a tight budget so current-node refresh never waits on
    // an indexer service.
    let cutoff_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .saturating_sub(HISTORICAL_REST_MIN_AGE)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let rest = rest_override
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| rest_base_for_network(&public.network));
    let historical = RestHistory::new(rest, 8);
    let historical_transactions = match tokio::time::timeout(
        HISTORICAL_REST_BUDGET,
        historical.wallet_history_before_ms(&addresses, cutoff_ms),
    )
    .await
    {
        Ok(Ok(transactions)) => transactions,
        Ok(Err(error)) => {
            crate::debug_log::record(
                "warn",
                "wallet",
                "historical-rest-refresh-failed",
                format!("source=historical-only min_age_hours=48 error={error}"),
            );
            Vec::new()
        }
        Err(_) => {
            crate::debug_log::record(
                "debug",
                "wallet",
                "historical-rest-refresh-timeout",
                "source=historical-only min_age_hours=48 budget_ms=750",
            );
            Vec::new()
        }
    };
    let history_entries = historical_transactions
        .into_iter()
        .take(200)
        .map(|transaction| WalletHistoryEntry {
            transaction_id: transaction.transaction_id,
            blue_score: transaction
                .accepting_block_blue_score
                .unwrap_or_default()
                .to_string(),
            block_time: transaction.block_time,
            ghost_payload: ghost_history::is_ghost_payload(&transaction.payload),
        })
        .collect();

    let endpoint = portal.endpoint()?;
    crate::debug_log::record(
        "info",
        "wallet",
        "wallet-current-state",
        format!(
            "source=kaspa-wrpc endpoint={} balance_sompi={} utxo_count={} daa_score={} active_addresses={} historical_rest_min_age_hours=48",
            endpoint,
            balance,
            utxos.len(),
            blue_score,
            active_set.len(),
        ),
    );

    Ok(WalletSnapshot {
        balance_sompi: balance.to_string(),
        utxo_count: utxos.len().to_string(),
        blue_score: blue_score.to_string(),
        history: history_entries,
        active_addresses: active_set.len(),
        recommended_receive_index,
    })
}

pub fn open_secret(password: &str, sealed: &[u8]) -> Result<WalletSecret, String> {
    let mut plaintext =
        ghost_storage::open(password, &ghost_storage::SealedVault(sealed.to_vec()))?;
    let result = serde_json::from_slice(&plaintext)
        .map_err(|error| format!("wallet vault decode: {error}"));
    plaintext.zeroize();
    result
}

fn seal_secret(password: &str, secret: &WalletSecret) -> Result<Vec<u8>, String> {
    let mut plaintext =
        serde_json::to_vec(secret).map_err(|error| format!("wallet vault encode: {error}"))?;
    let result = ghost_storage::seal(password, &plaintext).map(|sealed| sealed.0);
    plaintext.zeroize();
    result
}

pub(super) fn validate_public_projection(
    secret: &WalletSecret,
    public: &WalletPublic,
) -> Result<(), String> {
    let expected = ghost_kaspa::wallet::derive_public(secret)?;
    if expected.network != public.network
        || expected.account_path != public.account_path
        || expected.receive_addresses != public.receive_addresses
        || expected.change_addresses != public.change_addresses
        || public.next_receive_index >= public.receive_addresses.len()
        || public.next_change_index >= public.change_addresses.len()
    {
        return Err("wallet public projection does not match encrypted wallet secret".into());
    }
    Ok(())
}

fn require_wallet_password(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("wallet password must be at least 8 characters".into());
    }
    Ok(())
}

pub(super) fn parse_u64_decimal(value: &str, label: &str) -> Result<u64, String> {
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
pub(super) async fn resolve_wrpc_endpoints(
    network: &str,
    override_endpoint: Option<&str>,
) -> Result<Vec<String>, String> {
    #[derive(serde::Deserialize)]
    struct ResolverNodeDescriptor {
        #[serde(default)]
        uid: Option<String>,
        url: String,
    }

    if let Some(endpoint) = override_endpoint.map(str::trim).filter(|value| !value.is_empty()) {
        if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
            return Ok(vec![endpoint.to_owned()]);
        }
        return Err("custom Kaspa RPC endpoint must use ws:// or wss://".into());
    }

    let network = ghost_kaspa::upstream::portal::primitives::NetworkId::parse(network)?
        .canonical_name();

    // Mirror the current resolver groups shipped by rusty-kaspa. These HTTPS
    // services return node metadata; they are not the node connection itself.
    const PUBLIC_RESOLVERS: [&str; 16] = [
        "https://eric.kaspa.stream",
        "https://maxim.kaspa.stream",
        "https://sean.kaspa.stream",
        "https://troy.kaspa.stream",
        "https://john.kaspa.red",
        "https://mike.kaspa.red",
        "https://paul.kaspa.red",
        "https://alex.kaspa.red",
        "https://jake.kaspa.green",
        "https://mark.kaspa.green",
        "https://adam.kaspa.green",
        "https://liam.kaspa.green",
        "https://noah.kaspa.blue",
        "https://ryan.kaspa.blue",
        "https://jack.kaspa.blue",
        "https://luke.kaspa.blue",
    ];

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(1_500))
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|error| format!("Kaspa public resolver client: {error}"))?;

    let mut set = tokio::task::JoinSet::new();
    for resolver in PUBLIC_RESOLVERS {
        let client = client.clone();
        let network = network.clone();
        set.spawn(async move {
            let url = format!("{resolver}/v2/kaspa/{network}/tls/wrpc/borsh");
            let result = async {
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
                Ok::<_, String>((descriptor.uid.unwrap_or_default(), endpoint))
            }
            .await;
            (resolver, result)
        });
    }

    let mut endpoints = Vec::new();
    let mut errors = Vec::new();
    while let Some(result) = set.join_next().await {
        match result {
            Ok((resolver, Ok((uid, endpoint)))) => {
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
            Ok((resolver, Err(error))) => {
                crate::debug_log::record(
                    "warn",
                    "kaspa",
                    "public-resolver-failed",
                    format!("resolver={} error={}", resolver, error),
                );
                errors.push(format!("{resolver}: {error}"));
            }
            Err(error) => {
                crate::debug_log::record(
                    "warn",
                    "kaspa",
                    "public-resolver-task-failed",
                    format!("error={error}"),
                );
                errors.push(format!("resolver task: {error}"));
            }
        }
    }

    if endpoints.is_empty() {
        let detail = errors.into_iter().take(4).collect::<Vec<_>>().join(" | ");
        let error = if detail.is_empty() {
            "no Kaspa public wRPC resolver returned a usable WSS endpoint".to_string()
        } else {
            format!("no Kaspa public wRPC resolver returned a usable WSS endpoint: {detail}")
        };
        eprintln!("Ghost Talk: {error}");
        Err(error)
    } else {
        Ok(endpoints)
    }
}

