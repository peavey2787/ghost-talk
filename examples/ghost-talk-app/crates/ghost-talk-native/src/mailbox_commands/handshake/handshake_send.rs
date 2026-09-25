use super::super::{
    gateway::mailbox_send_result,
    send_state::{MailboxSendResult, State},
    submission::outbound_send::OutboundMailboxSend,
};
use super::send_prepare::prepare_send_payloads;

pub(crate) async fn release_prepared_delivery(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    profile_id: &str,
    message_id: &str,
    prepared: &PreparedSend,
    reuse_change: bool,
) -> Result<(), String> {
    if prepared.pending_id.is_some() || reuse_change {
        return Ok(());
    }
    let runtime = hydra_state.runtime(profile_id)?;
    let mut runtime = runtime.lock().await;
    runtime.prepared_kktp_deliveries.remove(message_id);
    crate::hydra_commands::persist_transport_state(&runtime)?;
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn mailbox_send_message(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    hydra_state: State<'_, crate::hydra_commands::HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    contact_id: String,
    destination: String,
    body: String,
    reaction: Option<ghost_protocol::GhostReactionEvent>,
    message_id: String,
    stego_profile: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
    reuse_change: bool,
    allow_handshake: bool,
) -> Result<MailboxSendResult, String> {
    let context = MessageContext {
        profile_id: &profile_id,
        identity_id: &identity_id,
        contact_id: &contact_id,
        destination: &destination,
        body: &body,
        reaction: reaction.as_ref(),
        message_id: &message_id,
        stego_profile: &stego_profile,
        reuse_change,
        allow_handshake,
    };
    validate_send_content(&context)?;
    let outbound_lock = wallet_state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    log_message_send_requested(&context);
    let outbound = OutboundMailboxSend::prepare(
        &gateway,
        wallet_state.inner(),
        &profile_id,
        &password,
        &sealed,
        &public,
        &fee_sompi,
        "mailbox fee",
        wrpc_endpoint.as_deref(),
    )?;
    finish_message_send(hydra_state.inner(), &outbound, context).await
}

async fn finish_message_send(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    outbound: &OutboundMailboxSend<'_>,
    context: MessageContext<'_>,
) -> Result<MailboxSendResult, String> {
    let prepared = prepare_locked_send(hydra_state, &context).await?;
    let result = outbound
        .send(
            &prepared.destination,
            &prepared.payloads,
            context.reuse_change,
        )
        .await?;
    log_message_send_accepted(
        context.profile_id,
        context.contact_id,
        context.message_id,
        &prepared,
        &result.transaction_id,
    );
    release_prepared_delivery(
        hydra_state,
        context.profile_id,
        context.message_id,
        &prepared,
        context.reuse_change,
    )
    .await?;
    let pending = prepared.pending_id.is_some();
    Ok(mailbox_send_result(result, pending, prepared.pending_id))
}

async fn prepare_locked_send(
    hydra_state: &crate::hydra_commands::HydraRuntimeState,
    context: &MessageContext<'_>,
) -> Result<PreparedSend, String> {
    let runtime = hydra_state.runtime(context.profile_id)?;
    let mut runtime = runtime.lock().await;
    prepare_send_payloads(
        &mut runtime,
        context.profile_id,
        context.identity_id,
        context.contact_id,
        context.body,
        context.reaction,
        context.message_id,
        context.stego_profile,
        context.reuse_change,
        context.allow_handshake,
    )
}

fn log_message_send_requested(context: &MessageContext<'_>) {
    crate::debug_log::record(
        "info",
        "message",
        "send-requested",
        format!(
            "profile={} peer={} message={} destination={} reuse_change={} allow_handshake={}",
            context.profile_id,
            context.contact_id,
            context.message_id,
            context.destination,
            context.reuse_change,
            context.allow_handshake
        ),
    );
}

fn log_message_send_accepted(
    profile_id: &str,
    contact_id: &str,
    message_id: &str,
    prepared: &PreparedSend,
    txid: &str,
) {
    let pending = prepared.pending_id.as_deref().unwrap_or("none");
    let (scope, event) = if prepared.pending_id.is_some() {
        ("handshake", "carrier-send-accepted-by-kaspa")
    } else {
        ("message", "message-send-accepted-by-kaspa")
    };
    crate::debug_log::record(
        "info", scope, event,
        format!("profile={profile_id} peer={contact_id} message={message_id} pending={pending} destination={} txid={txid}", prepared.destination),
    );
}
use super::super::send_state::{validate_message_send, PreparedSend};
use ghost_kaspa::wallet::WalletPublic;

struct MessageContext<'a> {
    profile_id: &'a str,
    identity_id: &'a str,
    contact_id: &'a str,
    destination: &'a str,
    body: &'a str,
    reaction: Option<&'a ghost_protocol::GhostReactionEvent>,
    message_id: &'a str,
    stego_profile: &'a str,
    reuse_change: bool,
    allow_handshake: bool,
}

fn validate_send_content(context: &MessageContext<'_>) -> Result<(), String> {
    match context.reaction {
        Some(reaction) => validate_reaction_send(context, reaction),
        None => validate_message_send(context.body, context.message_id, context.stego_profile),
    }
}

fn validate_reaction_send(
    context: &MessageContext<'_>,
    reaction: &ghost_protocol::GhostReactionEvent,
) -> Result<(), String> {
    if !context.body.is_empty() {
        return Err("Reaction send cannot contain a text body".into());
    }
    if context.allow_handshake || context.reuse_change {
        return Err("Reaction send requires an established durable Ghost PQ session".into());
    }
    ghost_protocol::validate_kktp_message_id(&reaction.target_message_id)
        .map_err(|_| "Reaction target message id must be 32 hexadecimal characters".to_string())?;
    validate_message_send("reaction", context.message_id, context.stego_profile)
}
