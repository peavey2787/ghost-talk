use super::super::{
    gateway::{decode_mailbox_payloads, mailbox_send_result, reassemble_mailbox_payloads},
    send_state::{MailboxSendResult, State},
    submission::outbound_send::OutboundMailboxSend,
};
#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_control(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    payloads_hex: Vec<String>,
    completes_pending_id: Option<String>,
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
        "mailbox control fee",
        wrpc_endpoint.as_deref(),
    )?;
    let payloads = validate_control_payloads(&payloads_hex)?;
    let result = outbound.send(&destination, &payloads, false).await?;
    reconcile_sent_control(
        &hydra_state,
        &profile_id,
        &payloads_hex,
        completes_pending_id,
    )
    .await;
    Ok(mailbox_send_result(result, false, None))
}

fn validate_control_payloads(payloads_hex: &[String]) -> Result<Vec<Vec<u8>>, String> {
    let payloads = decode_mailbox_payloads(payloads_hex)?;
    let carrier = reassemble_mailbox_payloads(&payloads)?;
    let authenticated = carrier.starts_with(ghost_protocol::KKTP_ANCHOR_PREFIX)
        && ghost_protocol::kktp_anchor_type(&carrier)?.as_deref() == Some("ghost_handshake")
        && ghost_protocol::KktpHandshakeControl::decode(&carrier).is_ok();
    authenticated.then_some(payloads).ok_or_else(|| {
        "mailbox control carrier is not an authenticated KKTP/HYDRA control packet".into()
    })
}

async fn reconcile_sent_control(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    payloads_hex: &[String],
    completes_pending_id: Option<String>,
) {
    let Ok(runtime) = hydra_state.runtime(profile_id) else {
        return;
    };
    let mut runtime = runtime.lock().await;
    if completes_pending_id.is_some() {
        return;
    }
    let completed_peer = runtime
        .prepared_recovery_finish
        .iter()
        .find_map(|(peer, value)| (value.payloads_hex == payloads_hex).then(|| peer.clone()));
    if let Some(peer) = completed_peer {
        runtime.pending_recovery.remove(&peer);
        runtime.prepared_recovery_finish.remove(&peer);
    }
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_retry_handshake_finish(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    contact_id: String,
    message_id: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let outbound_lock = wallet_state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    validate_message_id(&message_id)?;
    let outbound = OutboundMailboxSend::prepare(
        &gateway,
        wallet_state.inner(),
        &profile_id,
        &password,
        &sealed,
        &public,
        &fee_sompi,
        "mailbox FINISH retry fee",
        wrpc_endpoint.as_deref(),
    )?;
    let runtime = hydra_state.runtime(&profile_id)?;
    let (destination, payloads) = {
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        let prepared = runtime
            .prepared_completion
            .get(&contact_id)
            .ok_or_else(|| {
                "no peer-unacknowledged KKTP FINISH is retained for retry".to_string()
            })?;
        if prepared.message_id != message_id {
            return Err("retained KKTP FINISH does not match this chat message".into());
        }
        let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
        (prepared.destination.clone(), payloads)
    };
    let result = outbound.send(&destination, &payloads, false).await?;
    Ok(mailbox_send_result(result, true, None))
}

fn validate_message_id(message_id: &str) -> Result<(), String> {
    (message_id.len() == 32 && message_id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(())
        .ok_or_else(|| "Ghost Talk message id must be exactly 32 hexadecimal characters".into())
}

pub(crate) fn decode_recovery_offer(offer_hex: &str) -> Result<Vec<u8>, String> {
    let offer =
        hex::decode(offer_hex).map_err(|_| "HYDRA recovery offer is not valid hex".to_string())?;
    if offer.is_empty() || offer.len() > 64 * 1024 {
        return Err("HYDRA recovery offer size is invalid".into());
    }
    Ok(offer)
}

pub(crate) fn prepare_recovery_payloads(
    runtime: &crate::hydra_commands::HydraProfileRuntime,
    identity_id: &str,
    contact_id: &str,
    destination: &str,
    offer_hex: &str,
    offer: &[u8],
) -> Result<Vec<Vec<u8>>, String> {
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    let pending = runtime
        .pending_recovery
        .get(contact_id)
        .filter(|pending| pending.destination == destination && pending.offer_hex == offer_hex)
        .ok_or_else(|| {
            "KKTP secure-session recovery is no longer pending for this peer".to_string()
        })?;
    let binding = runtime
        .kktp_sessions
        .get(&pending.contact_id)
        .cloned()
        .ok_or_else(|| "KKTP recovery session binding is missing".to_string())?;
    if binding.sid != pending.sid || binding.role != crate::hydra_commands::KktpRole::Initiator {
        return Err("KKTP recovery session identity changed before broadcast".into());
    }
    let control =
        crate::hydra_commands::kktp_handshake_payload(runtime, &binding, "pq_init", offer, None)?;
    let prepared = crate::hydra_commands::frame_control(control)?;
    decode_mailbox_payloads(&prepared.payloads_hex)
}

pub(crate) async fn mark_recovery_offer_broadcast(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    contact_id: &str,
    destination: &str,
    offer_hex: &str,
) -> Result<(), String> {
    let runtime = hydra_state.runtime(profile_id)?;
    let mut runtime = runtime.lock().await;
    let Some(pending) = runtime.pending_recovery.get_mut(contact_id) else {
        return Ok(());
    };
    if pending.destination == destination && pending.offer_hex == offer_hex {
        pending.offer_broadcast = true;
    }
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_recovery_offer(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    contact_id: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    offer_hex: String,
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
        "mailbox recovery fee",
        wrpc_endpoint.as_deref(),
    )?;
    ghost_kaspa::validate_destination(&destination)?;
    let payloads = locked_recovery_payloads(
        &hydra_state,
        &profile_id,
        &identity_id,
        &contact_id,
        &destination,
        &offer_hex,
    )
    .await?;
    let result = outbound.send(&destination, &payloads, false).await?;
    mark_recovery_offer_broadcast(
        hydra_state.inner(),
        &profile_id,
        &contact_id,
        &destination,
        &offer_hex,
    )
    .await?;
    Ok(mailbox_send_result(result, true, None))
}

async fn locked_recovery_payloads(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    identity_id: &str,
    contact_id: &str,
    destination: &str,
    offer_hex: &str,
) -> Result<Vec<Vec<u8>>, String> {
    let offer = decode_recovery_offer(offer_hex)?;
    let runtime = hydra_state.runtime(profile_id)?;
    let runtime = runtime.lock().await;
    prepare_recovery_payloads(
        &runtime,
        identity_id,
        contact_id,
        destination,
        offer_hex,
        &offer,
    )
}
use ghost_kaspa::wallet::WalletPublic;
