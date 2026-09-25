use super::super::{
    delivery_ack::{next_monitor_generation, remember_monitor_public, WalletMonitorWorker},
    send_state::{
        AppHandle, HashMap, MailboxSendResult, MonitorState, NetworkStatusEvent, PortalFacade,
        State,
    },
};
#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub fn wallet_monitor_start(
    app: State<'_, crate::NativeAppState>,
    state: State<'_, MonitorState>,
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    directory: State<'_, crate::peer_commands::PublicDirectoryState>,
    profile_id: String,
    public: WalletPublic,
    checkpoint: String,
    history: Vec<crate::wallet_commands::WalletHistoryEntry>,
    wrpc_endpoint: Option<String>,
) -> Result<(), String> {
    let generation = next_monitor_generation(&state, &profile_id)?;
    remember_monitor_public(&state, &profile_id, public.clone())?;
    let worker = WalletMonitorWorker {
        generations: state.generations.clone(),
        app: app.handle().clone(),
        profile_id,
        public,
        publics: state.publics.clone(),
        wrpc_endpoint,
        gateway: gateway.inner().clone(),
        directory: directory.inner().clone(),
        generation,
        checkpoint,
        persisted_history: history,
        snapshot: None,
        wallet_events: None,
        latest_daa_score: None,
        live_stream: None,
        live_reconnect: None,
        reconnect_attempts: 0,
        next_live_retry: Instant::now(),
        last_network_status: "connecting",
        carrier_connected: false,
        wallet_subscriptions_connected: false,
    };
    tauri::async_runtime::spawn(worker.run());
    Ok(())
}

pub(crate) fn monitor_generation_is_active(
    generations: &Arc<Mutex<HashMap<String, u64>>>,
    profile_id: &str,
    generation: u64,
) -> bool {
    generations
        .lock()
        .map(|active| active.get(profile_id).copied() == Some(generation))
        .unwrap_or(false)
}

#[tauri::command]
pub fn wallet_monitor_update_public(
    state: State<'_, MonitorState>,
    profile_id: String,
    public: WalletPublic,
) -> Result<(), String> {
    state
        .publics
        .lock()
        .map_err(|_| "wallet monitor public state is poisoned".to_string())?
        .insert(profile_id, public);
    Ok(())
}

#[tauri::command]
pub fn wallet_monitor_stop(
    state: State<'_, MonitorState>,
    profile_id: String,
) -> Result<(), String> {
    state
        .generations
        .lock()
        .map_err(|_| "wallet monitor state is poisoned".to_string())?
        .remove(&profile_id);
    state
        .publics
        .lock()
        .map_err(|_| "wallet monitor public state is poisoned".to_string())?
        .remove(&profile_id);
    Ok(())
}

pub(crate) fn live_retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    Duration::from_secs((1u64 << exponent).min(30))
}

pub(crate) fn sync_network_status(
    app: &AppHandle,
    profile_id: &str,
    carrier_connected: bool,
    wallet_subscriptions_connected: bool,
    reconnect_attempts: u32,
    last_status: &mut &'static str,
) {
    let status = if carrier_connected && wallet_subscriptions_connected {
        "connected"
    } else if *last_status == "connecting" && reconnect_attempts <= 1 {
        "connecting"
    } else {
        "reconnecting"
    };
    if *last_status != status {
        emit_network_status(app, profile_id, status, reconnect_attempts);
        *last_status = status;
    }
}

pub(crate) fn emit_network_status(
    app: &AppHandle,
    profile_id: &str,
    status: &'static str,
    reconnect_attempts: u32,
) {
    let _ = app.emit(
        "ghost://network-status",
        NetworkStatusEvent {
            profile_id: profile_id.to_owned(),
            status: status.to_string(),
            reconnect_attempts,
        },
    );
}

pub(crate) async fn outbound_portal(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    public: &WalletPublic,
    wrpc_override: Option<&str>,
) -> Result<PortalFacade, String> {
    gateway.portal(public, wrpc_override).await
}

pub(crate) struct MailboxSubmitContext<'a> {
    pub(crate) gateway: &'a crate::kaspa_gateway::KaspaGatewayState,
    pub(crate) wallet_state: &'a crate::wallet_commands::WalletRuntimeState,
    pub(crate) profile_id: &'a str,
    pub(crate) secret: &'a ghost_kaspa::wallet::WalletSecret,
    pub(crate) public: &'a WalletPublic,
    pub(crate) wrpc_override: Option<&'a str>,
}

impl<'a> MailboxSubmitContext<'a> {
    pub(crate) fn new(
        gateway: &'a crate::kaspa_gateway::KaspaGatewayState,
        wallet_state: &'a crate::wallet_commands::WalletRuntimeState,
        profile_id: &'a str,
        secret: &'a ghost_kaspa::wallet::WalletSecret,
        public: &'a WalletPublic,
        wrpc_override: Option<&'a str>,
    ) -> Self {
        Self {
            gateway,
            wallet_state,
            profile_id,
            secret,
            public,
            wrpc_override,
        }
    }

    pub(crate) async fn send(
        &self,
        destination: &str,
        fee: u64,
        payloads: &[Vec<u8>],
        reuse_change: bool,
    ) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
        let portal = outbound_portal(self.gateway, self.public, self.wrpc_override).await?;
        send_mailbox_payloads_via_gateway(
            self.gateway,
            self.wallet_state,
            self.profile_id,
            &portal,
            self.secret,
            self.public,
            destination,
            fee,
            payloads,
            reuse_change,
        )
        .await
    }
}

pub(crate) fn mailbox_send_result(
    result: ghost_kaspa::wallet::KaspaBroadcastResult,
    pending_handshake: bool,
    pending_id: Option<String>,
) -> MailboxSendResult {
    MailboxSendResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        mailbox_output_sompi: MAILBOX_OUTPUT_SOMPI.to_string(),
        public: crate::wallet_commands::wallet_projection(&result.public),
        pending_handshake,
        pending_id,
    }
}

pub(crate) fn decode_mailbox_payloads(payloads_hex: &[String]) -> Result<Vec<Vec<u8>>, String> {
    if payloads_hex.is_empty() || payloads_hex.len() > ghost_core::MAX_FRAGMENTS {
        return Err("prepared Ghost Talk carrier has an invalid physical fragment count".into());
    }
    payloads_hex
        .iter()
        .map(|payload_hex| {
            let payload = hex::decode(payload_hex)
                .map_err(|_| "prepared mailbox payload is not valid hex".to_string())?;
            if payload.is_empty() || payload.len() > ghost_core::MAX_KSPT_V1_PAYLOAD_BYTES {
                return Err(
                    "prepared Ghost Talk physical fragment exceeds the KSPT v1 payload boundary"
                        .into(),
                );
            }
            ghost_protocol::CarrierFrame::decode(&payload)?;
            Ok(payload)
        })
        .collect()
}

pub(crate) fn reassemble_mailbox_payloads(payloads: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    validate_fragment_count(payloads)?;
    let mut decoded = payloads
        .iter()
        .map(|payload| ghost_protocol::CarrierFrame::decode(payload))
        .collect::<Result<Vec<_>, _>>()?;
    let packet = decoded[0].packet;
    let count = decoded[0].count;
    validate_packet_membership(&decoded, packet, count)?;
    decoded.sort_by_key(|frame| frame.index);
    validate_fragment_order(&decoded)?;
    let total = decoded
        .iter()
        .try_fold(0usize, |total, frame| {
            total.checked_add(frame.payload.len())
        })
        .ok_or_else(|| "Ghost Talk reassembled carrier length overflowed usize".to_string())?;
    let max_reassembled = ghost_core::MAX_FRAGMENTS.saturating_mul(ghost_protocol::GHST_DATA_MAX);
    validate_reassembled_size(total, max_reassembled)?;
    Ok(decoded
        .into_iter()
        .flat_map(|frame| frame.payload)
        .collect())
}

fn validate_fragment_count(payloads: &[Vec<u8>]) -> Result<(), String> {
    if payloads.is_empty() || payloads.len() > ghost_core::MAX_FRAGMENTS {
        Err("Ghost Talk carrier has an invalid fragment count".into())
    } else {
        Ok(())
    }
}

fn validate_packet_membership(
    decoded: &[ghost_protocol::CarrierFrame],
    packet: ghost_core::Id128,
    count: u16,
) -> Result<(), String> {
    let complete = usize::from(count) == decoded.len()
        && decoded
            .iter()
            .all(|frame| frame.packet == packet && frame.count == count);
    complete
        .then_some(())
        .ok_or_else(|| "Ghost Talk carrier fragments do not form one complete packet".into())
}

fn validate_fragment_order(decoded: &[ghost_protocol::CarrierFrame]) -> Result<(), String> {
    decoded
        .iter()
        .enumerate()
        .all(|(expected, frame)| usize::from(frame.index) == expected)
        .then_some(())
        .ok_or_else(|| "Ghost Talk carrier fragments are missing or duplicated".into())
}

fn validate_reassembled_size(total: usize, maximum: usize) -> Result<(), String> {
    (total <= maximum).then_some(()).ok_or_else(|| {
        "Ghost Talk reassembled carrier exceeds the bounded GHST fragment window".into()
    })
}

use super::super::payload_send::send_mailbox_payloads_via_gateway;
use ghost_kaspa::wallet::{WalletPublic, MAILBOX_OUTPUT_SOMPI};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Emitter;
