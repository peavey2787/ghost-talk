use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_kaspa::wallet::WalletPublic;

use super::{
    gateway::{decode_mailbox_payloads, mailbox_send_result},
    send_state::{authenticated_destination, MailboxSendResult, State},
    submission::outbound_send::OutboundMailboxSend,
};

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_realtime_carrier(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    contact_id: String,
    destination: String,
    carrier_b64: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let carrier = BASE64
        .decode(carrier_b64.as_bytes())
        .map_err(|_| "GTR1 realtime carrier is not valid base64".to_string())?;
    let decoded = ghost_protocol::Gtr1Envelope::decode(&carrier).map_err(|error| error.to_string())?;

    let expected_destination = {
        let runtime = hydra_state.runtime(&profile_id)?;
        let runtime = runtime.lock().await;
        if decoded.sender_hex() != runtime.identity_id {
            return Err("GTR1 realtime carrier sender is not the unlocked local HYDRA identity".into());
        }
        let binding = runtime
            .kktp_sessions
            .get(&contact_id)
            .ok_or_else(|| "realtime carrier has no active KKTP session".to_string())?;
        if binding.state != crate::hydra_commands::KktpSessionState::Active
            || binding.sid != decoded.sid_hex()
        {
            return Err("GTR1 realtime carrier SID does not match the active KKTP session".into());
        }
        authenticated_destination(
            &runtime,
            &contact_id,
            "realtime carrier has no authenticated Kaspa peer route",
        )?
    };
    if destination != expected_destination {
        return Err("realtime carrier destination does not match the authenticated peer route".into());
    }

    let prepared = crate::hydra_commands::fragment_realtime_carrier(&carrier)?;
    let payloads = decode_mailbox_payloads(&prepared.payloads_hex)?;
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
        "mailbox realtime fee",
        wrpc_endpoint.as_deref(),
    )?;
    let result = outbound.send(&destination, &payloads, true).await?;
    Ok(mailbox_send_result(result, false, None))
}
