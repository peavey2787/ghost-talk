use super::super::contact_request_helpers::{accepted_contact_request, signed_contact_accept};
use super::super::{
    gateway::{decode_mailbox_payloads, mailbox_send_result},
    send_state::{MailboxSendResult, State},
    submission::outbound_send::OutboundMailboxSend,
};
use super::logging::{
    log_contact_accept_sent, log_contact_accept_start, log_contact_accept_verified,
};
use ghost_kaspa::wallet::WalletPublic;

struct ContactAcceptInput {
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    signed_request_hex: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
}

struct PreparedContactAccept {
    accepted: ghost_protocol::GhostContactAccept,
    destination: String,
    payloads: Vec<Vec<u8>>,
    reuse_change: bool,
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_contact_accept(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    sender_display_name: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    signed_request_hex: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<MailboxSendResult, String> {
    let input = ContactAcceptInput {
        profile_id,
        password,
        identity_id,
        sender_display_name,
        sealed,
        public,
        signed_request_hex,
        fee_sompi,
        wrpc_endpoint,
    };
    let outbound_lock = wallet_state.outbound_lock(&input.profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    send_contact_accept(
        gateway.inner(),
        wallet_state.inner(),
        hydra_state.inner(),
        input,
    )
    .await
}

async fn send_contact_accept(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    input: ContactAcceptInput,
) -> Result<MailboxSendResult, String> {
    log_contact_accept_start(
        &input.profile_id,
        &input.identity_id,
        input.signed_request_hex.len() / 2,
    );
    let outbound = OutboundMailboxSend::prepare(
        gateway,
        wallet_state,
        &input.profile_id,
        &input.password,
        &input.sealed,
        &input.public,
        &input.fee_sompi,
        "mailbox contact-accept fee",
        input.wrpc_endpoint.as_deref(),
    )?;
    let prepared = prepare_accept(hydra_state, outbound.secret(), &input).await?;
    let result = outbound
        .send(
            &prepared.destination,
            &prepared.payloads,
            prepared.reuse_change,
        )
        .await?;
    log_contact_accept_sent(
        &input.profile_id,
        &prepared.accepted.request_id,
        &result.transaction_id,
        &prepared.destination,
    );
    Ok(mailbox_send_result(result, true, None))
}

async fn prepare_accept(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    secret: &ghost_kaspa::wallet::WalletSecret,
    input: &ContactAcceptInput,
) -> Result<PreparedContactAccept, String> {
    let local_addresses = input
        .public
        .receive_addresses
        .iter()
        .chain(input.public.change_addresses.iter())
        .cloned()
        .collect::<Vec<_>>();
    let runtime = hydra_state.runtime(&input.profile_id)?;
    let (request, descriptor) = accepted_contact_request(
        &runtime,
        &input.identity_id,
        secret,
        &input.public,
        &input.sender_display_name,
        &input.signed_request_hex,
        &local_addresses,
    )
    .await?;
    let request_id = request.request_id.clone();
    let sender_hydra_id = request.sender.hydra_identity_id.clone();
    let (accepted, destination, acceptor, reuse_change) =
        signed_contact_accept(secret, &input.public, request, descriptor)?;
    log_contact_accept_verified(
        &input.profile_id,
        &request_id,
        &sender_hydra_id,
        &destination,
        &acceptor,
    );
    let framed = crate::hydra_commands::frame_control(accepted.encode()?)?;
    Ok(PreparedContactAccept {
        accepted,
        destination,
        payloads: decode_mailbox_payloads(&framed.payloads_hex)?,
        reuse_change,
    })
}
