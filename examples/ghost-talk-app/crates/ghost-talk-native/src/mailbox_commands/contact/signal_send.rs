use super::super::contact_request_helpers::{
    private_descriptor, signed_call_signal, validate_call_signal_fields,
};
use super::super::{
    gateway::{decode_mailbox_payloads, mailbox_send_result},
    send_state::{MailboxSendResult, State},
    submission::outbound_send::OutboundMailboxSend,
};
use super::logging::log_call_signal;
use ghost_kaspa::wallet::WalletPublic;

struct CallSignalInput {
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    signal_id: String,
    call_id: String,
    action: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_call_signal(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    signal_id: String,
    call_id: String,
    action: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let input = CallSignalInput {
        profile_id,
        password,
        identity_id,
        sender_display_name,
        sealed,
        public,
        destination,
        signal_id,
        call_id,
        action,
        fee_sompi,
        wrpc_endpoint,
    };
    let outbound_lock = wallet_state.outbound_lock(&input.profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    send_call_signal(
        gateway.inner(),
        wallet_state.inner(),
        hydra_state.inner(),
        input,
    )
    .await
}

async fn send_call_signal(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    input: CallSignalInput,
) -> Result<MailboxSendResult, String> {
    validate_call_signal_fields(&input.signal_id, &input.call_id, &input.action)?;
    ghost_kaspa::validate_destination(&input.destination)?;
    let outbound = OutboundMailboxSend::prepare(
        gateway,
        wallet_state,
        &input.profile_id,
        &input.password,
        &input.sealed,
        &input.public,
        &input.fee_sompi,
        "call-signal fee",
        input.wrpc_endpoint.as_deref(),
    )?;
    let runtime = hydra_state.runtime(&input.profile_id)?;
    let descriptor = private_descriptor(
        &runtime,
        &input.identity_id,
        outbound.secret(),
        &input.public,
        &input.sender_display_name,
    )
    .await?;
    let signal = signed_call_signal(
        outbound.secret(),
        descriptor,
        input.signal_id.clone(),
        input.call_id.clone(),
        input.action.clone(),
        input.destination.clone(),
    )?;
    let framed = crate::hydra_commands::frame_control(signal.encode()?)?;
    let payloads = decode_mailbox_payloads(&framed.payloads_hex)?;
    log_signal(&input, None);
    let result = outbound.send(&input.destination, &payloads, true).await?;
    log_signal(&input, Some(&result.transaction_id));
    Ok(mailbox_send_result(result, false, None))
}

fn log_signal(input: &CallSignalInput, transaction_id: Option<&str>) {
    log_call_signal(
        &input.profile_id,
        &input.signal_id,
        &input.call_id,
        &input.action,
        &input.destination,
        transaction_id,
    );
}
