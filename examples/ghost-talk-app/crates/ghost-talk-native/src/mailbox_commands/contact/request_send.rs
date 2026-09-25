use super::super::contact_request_helpers::{
    contact_bootstrap, private_descriptor, set_pending_contact_request, signed_contact_request,
    validate_contact_request_id, ContactBootstrap,
};
use super::super::{
    gateway::{decode_mailbox_payloads, mailbox_send_result},
    send_state::{MailboxSendResult, State},
    submission::outbound_send::OutboundMailboxSend,
};
use ghost_kaspa::wallet::WalletPublic;

struct ContactRequestInput {
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    destination: String,
    request_id: String,
    room_id: Option<String>,
    room_name: Option<String>,
    call_id: Option<String>,
    call_action: Option<String>,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
}

struct PreparedContactRequest {
    payloads: Vec<Vec<u8>>,
    expects_acceptance: bool,
    reuse_change: bool,
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_contact_request(
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
    request_id: String,
    room_id: Option<String>,
    room_name: Option<String>,
    call_id: Option<String>,
    call_action: Option<String>,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let input = ContactRequestInput {
        profile_id,
        password,
        identity_id,
        sender_display_name,
        sealed,
        public,
        destination,
        request_id,
        room_id,
        room_name,
        call_id,
        call_action,
        fee_sompi,
        wrpc_endpoint,
    };
    let outbound_lock = wallet_state.outbound_lock(&input.profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    send_contact_request(
        gateway.inner(),
        wallet_state.inner(),
        hydra_state.inner(),
        input,
    )
    .await
}

async fn send_contact_request(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    input: ContactRequestInput,
) -> Result<MailboxSendResult, String> {
    validate_contact_request_id(&input.request_id)?;
    ghost_kaspa::validate_destination(&input.destination)?;
    let bootstrap = contact_bootstrap(
        input.room_id.clone(),
        input.room_name.clone(),
        input.call_id.clone(),
        input.call_action.clone(),
    )?;
    log_request_start(&input);
    let outbound = OutboundMailboxSend::prepare(
        gateway,
        wallet_state,
        &input.profile_id,
        &input.password,
        &input.sealed,
        &input.public,
        &input.fee_sompi,
        "mailbox contact-request fee",
        input.wrpc_endpoint.as_deref(),
    )?;
    let runtime = hydra_state.runtime(&input.profile_id)?;
    let prepared = prepare_request(&runtime, outbound.secret(), &input, bootstrap).await?;
    let result = submit_request(&outbound, &runtime, &input, &prepared).await?;
    log_request_sent(&input, &result.transaction_id);
    Ok(mailbox_send_result(result, true, Some(input.request_id)))
}

async fn prepare_request(
    runtime: &std::sync::Arc<tokio::sync::Mutex<crate::hydra_commands::HydraProfileRuntime>>,
    secret: &ghost_kaspa::wallet::WalletSecret,
    input: &ContactRequestInput,
    bootstrap: ContactBootstrap,
) -> Result<PreparedContactRequest, String> {
    let descriptor = private_descriptor(
        runtime,
        &input.identity_id,
        secret,
        &input.public,
        &input.sender_display_name,
    )
    .await?;
    let ContactBootstrap {
        room_invite,
        call_invite,
        reuse_change,
    } = bootstrap;
    let request = signed_contact_request(
        secret,
        input.request_id.clone(),
        input.destination.clone(),
        descriptor,
        room_invite,
        call_invite,
    )?;
    let expects_acceptance = request
        .call_invite
        .as_ref()
        .map(|invite| invite.action == "request")
        .unwrap_or(true);
    let framed = crate::hydra_commands::frame_control(request.encode()?)?;
    Ok(PreparedContactRequest {
        payloads: decode_mailbox_payloads(&framed.payloads_hex)?,
        expects_acceptance,
        reuse_change,
    })
}

async fn submit_request(
    outbound: &OutboundMailboxSend<'_>,
    runtime: &std::sync::Arc<tokio::sync::Mutex<crate::hydra_commands::HydraProfileRuntime>>,
    input: &ContactRequestInput,
    prepared: &PreparedContactRequest,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    if prepared.expects_acceptance {
        set_pending_contact_request(runtime, &input.request_id, true).await?;
    }
    match outbound
        .send(
            &input.destination,
            &prepared.payloads,
            prepared.reuse_change,
        )
        .await
    {
        Ok(result) => Ok(result),
        Err(error) => {
            clear_failed_request(
                runtime,
                &input.request_id,
                prepared.expects_acceptance,
                error,
            )
            .await
        }
    }
}

async fn clear_failed_request(
    runtime: &std::sync::Arc<tokio::sync::Mutex<crate::hydra_commands::HydraProfileRuntime>>,
    request_id: &str,
    expects_acceptance: bool,
    error: String,
) -> Result<ghost_kaspa::wallet::KaspaBroadcastResult, String> {
    if !expects_acceptance {
        return Err(error);
    }
    match set_pending_contact_request(runtime, request_id, false).await {
        Ok(()) => Err(error),
        Err(cleanup) => Err(format!(
            "{error}; additionally failed to clear the unpublished contact-request checkpoint: {cleanup}"
        )),
    }
}

fn log_request_start(input: &ContactRequestInput) {
    crate::debug_log::record(
        "info",
        "handshake",
        "discovery-send-start",
        format!(
            "profile={} sid={} destination={} identity={}",
            input.profile_id, input.request_id, input.destination, input.identity_id
        ),
    );
}

fn log_request_sent(input: &ContactRequestInput, txid: &str) {
    crate::debug_log::record(
        "info",
        "handshake",
        "discovery-send-accepted-by-kaspa",
        format!(
            "profile={} sid={} txid={txid}",
            input.profile_id, input.request_id
        ),
    );
}
