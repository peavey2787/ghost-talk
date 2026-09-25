use super::super::{
    gateway::{decode_mailbox_payloads, mailbox_send_result},
    send_state::{AppHandle, HashMap, LiveBlockStream, MailboxSendResult, MonitorState, State},
    submission::outbound_send::OutboundMailboxSend,
};
#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_delivery_ack(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    destination_hydra_id: String,
    message_id: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let outbound_lock = wallet_state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    let outbound = OutboundMailboxSend::prepare(
        &gateway,
        wallet_state.inner(),
        &profile_id,
        &password,
        &sealed,
        &public,
        &fee_sompi,
        "mailbox delivery acknowledgement fee",
        wrpc_endpoint.as_deref(),
    )?;
    let payloads = prepare_delivery_ack_payloads(
        &hydra_state,
        &profile_id,
        &identity_id,
        outbound.secret(),
        &public,
        &destination,
        destination_hydra_id,
        message_id,
    )
    .await?;
    let result = outbound.send(&destination, &payloads, false).await?;
    Ok(mailbox_send_result(result, false, None))
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit signed acknowledgement fields"
)]
async fn prepare_delivery_ack_payloads(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    identity_id: &str,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    destination: &str,
    destination_hydra_id: String,
    message_id: String,
) -> Result<Vec<Vec<u8>>, String> {
    validate_ack_identifiers(&message_id, &destination_hydra_id)?;
    ghost_kaspa::validate_destination(destination)?;
    validate_ack_route(
        hydra_state,
        profile_id,
        identity_id,
        &destination_hydra_id,
        destination,
    )
    .await?;
    let ack = signed_delivery_ack(
        secret,
        public,
        identity_id.to_owned(),
        destination_hydra_id,
        message_id,
    )?;
    let prepared = crate::hydra_commands::frame_control(ack.encode()?)?;
    decode_mailbox_payloads(&prepared.payloads_hex)
}

fn validate_ack_identifiers(message_id: &str, destination_hydra_id: &str) -> Result<(), String> {
    if message_id.len() != 32 || !message_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Ghost Talk message id must be exactly 32 hexadecimal characters".into());
    }
    if destination_hydra_id.len() != 64
        || !destination_hydra_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("HYDRA destination id must be exactly 64 hexadecimal characters".into());
    }
    Ok(())
}

async fn validate_ack_route(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    identity_id: &str,
    destination_hydra_id: &str,
    destination: &str,
) -> Result<(), String> {
    let runtime = hydra_state.runtime(profile_id)?;
    let runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    if !runtime.hydra.has_contact(destination_hydra_id)? {
        return Err("delivery acknowledgement destination is not a known HYDRA peer".into());
    }
    let route = runtime
        .peer_routes
        .get(destination_hydra_id)
        .ok_or_else(|| {
            "delivery acknowledgement destination has no verified Kaspa peer route".to_string()
        })?;
    if route.kaspa_address != destination {
        return Err(
            "delivery acknowledgement destination does not match the verified peer route".into(),
        );
    }
    Ok(())
}

fn signed_delivery_ack(
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
    identity_id: String,
    destination_hydra_id: String,
    message_id: String,
) -> Result<ghost_protocol::GhostDeliveryAck, String> {
    let signer_kaspa_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let mut ack = ghost_protocol::GhostDeliveryAck {
        version: 1,
        signer_kaspa_address,
        signer_hydra_id: identity_id,
        destination_hydra_id,
        message_id,
        signature_hex: String::new(),
    };
    let mut private_key = ghost_kaspa::wallet::receive_private_key(secret, 0)?;
    let signed = ghost_kaspa::sign_delivery_ack(&mut ack, &private_key);
    zeroize::Zeroize::zeroize(&mut private_key);
    signed?;
    Ok(ack)
}

pub(crate) fn next_monitor_generation(
    state: &MonitorState,
    profile_id: &str,
) -> Result<u64, String> {
    let mut generations = state
        .generations
        .lock()
        .map_err(|_| "wallet monitor state is poisoned".to_string())?;
    let generation = generations
        .get(profile_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    generations.insert(profile_id.to_owned(), generation);
    Ok(generation)
}

pub(crate) fn remember_monitor_public(
    state: &MonitorState,
    profile_id: &str,
    public: WalletPublic,
) -> Result<(), String> {
    state
        .publics
        .lock()
        .map_err(|_| "wallet monitor public state is poisoned".to_string())?
        .insert(profile_id.to_owned(), public);
    Ok(())
}

pub(crate) struct WalletMonitorWorker {
    pub(crate) generations: Arc<Mutex<HashMap<String, u64>>>,
    pub(crate) app: AppHandle,
    pub(crate) profile_id: String,
    pub(crate) public: WalletPublic,
    pub(crate) publics: Arc<Mutex<HashMap<String, WalletPublic>>>,
    pub(crate) wrpc_endpoint: Option<String>,
    pub(crate) gateway: crate::kaspa_gateway::KaspaGatewayState,
    pub(crate) directory: crate::peer_commands::PublicDirectoryState,
    pub(crate) generation: u64,
    pub(crate) checkpoint: String,
    pub(crate) persisted_history: Vec<crate::wallet_commands::WalletHistoryEntry>,
    pub(crate) snapshot: Option<crate::wallet_commands::WalletSnapshot>,
    pub(crate) wallet_events: Option<crate::kaspa_wallet_events::WalletEventStream>,
    pub(crate) latest_daa_score: Option<u64>,
    pub(crate) live_stream: Option<LiveBlockStream>,
    pub(crate) live_reconnect: Option<tokio::task::JoinHandle<Result<LiveBlockStream, String>>>,
    pub(crate) reconnect_attempts: u32,
    pub(crate) next_live_retry: Instant,
    pub(crate) last_network_status: &'static str,
    pub(crate) carrier_connected: bool,
    pub(crate) wallet_subscriptions_connected: bool,
}
use ghost_kaspa::wallet::WalletPublic;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
