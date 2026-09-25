use super::super::{
    gateway::{decode_mailbox_payloads, mailbox_send_result},
    send_state::{MailboxSendResult, State},
    submission::outbound_send::OutboundMailboxSend,
};
use ghost_kaspa::wallet::WalletPublic;

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_session_end(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    contact_id: String,
    destination: String,
    expected_session_sid: Option<String>,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let outbound_lock = wallet_state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    ghost_kaspa::validate_destination(&destination)?;
    let outbound = OutboundMailboxSend::prepare(
        &gateway,
        wallet_state.inner(),
        &profile_id,
        &password,
        &sealed,
        &public,
        &fee_sompi,
        "mailbox session_end fee",
        wrpc_endpoint.as_deref(),
    )?;
    let payloads = prepare_session_end_payloads(
        &hydra_state,
        &profile_id,
        &identity_id,
        &contact_id,
        &destination,
        expected_session_sid.as_deref(),
        outbound.secret(),
        &public,
    )
    .await?;
    let result = outbound.send(&destination, &payloads, false).await?;
    Ok(mailbox_send_result(result, false, None))
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit signed session-end fields"
)]
async fn prepare_session_end_payloads(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    identity_id: &str,
    contact_id: &str,
    destination: &str,
    expected_session_sid: Option<&str>,
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
) -> Result<Vec<Vec<u8>>, String> {
    let sender = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let runtime = hydra_state.runtime(profile_id)?;
    let mut end = {
        let runtime = runtime.lock().await;
        if runtime.identity_id != identity_id {
            return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
        }
        crate::hydra_commands::prepare_kktp_session_end(
            &runtime,
            contact_id,
            &sender,
            destination,
            expected_session_sid,
            "left",
        )?
    };
    let mut signing_key = ghost_kaspa::wallet::receive_private_key(secret, 0)?;
    let signed = ghost_kaspa::sign_kktp_session_end(&mut end, &signing_key);
    zeroize::Zeroize::zeroize(&mut signing_key);
    signed?;
    let prepared = crate::hydra_commands::frame_control(end.encode()?)?;
    decode_mailbox_payloads(&prepared.payloads_hex)
}
