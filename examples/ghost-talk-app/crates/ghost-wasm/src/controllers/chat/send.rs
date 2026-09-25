use crate::model::{Chat, MailboxSendResult, Message, Profile, ProfilePatch};
mod outcome;
use outcome::{apply_failure, apply_success};

pub(crate) struct PreparedSend {
    pub(crate) patch: ProfilePatch,
    pub(crate) plan: OutgoingPlan,
    pub(crate) establishing: bool,
    pub(crate) needs_request: bool,
}

pub(crate) struct SendOutcome {
    pub(crate) patch: ProfilePatch,
    pub(crate) status: String,
}

pub(crate) struct OutgoingPlan {
    pub(super) current: Profile,
    pub(super) chat: Chat,
    pub(super) body: String,
    pub(super) destination: String,
    pub(super) wire_id: String,
    pub(super) request_id: Option<String>,
    pub(super) needs_request: bool,
    pub(super) recover_existing: bool,
}

pub(crate) fn prepare_send(
    profile: &Profile,
    chat: &Chat,
    body: String,
) -> Result<PreparedSend, String> {
    let context = prepare_context(chat, &body)?;
    let mut current = profile.clone();
    append_outgoing_message(
        &mut current.chats,
        chat,
        &body,
        &context.new_wire_id,
        context.request_id.clone(),
        context.needs_request,
        context.establishing || context.queued_recovery.is_some(),
    );
    let (body, wire_id) = context
        .queued_recovery
        .unwrap_or((body, context.new_wire_id));
    if context.recover_existing {
        mark_recovery_message_sending(&mut current.chats, chat, &wire_id);
    }
    let patch = super::chat_profile_transition(profile, &current, &chat.id);
    Ok(PreparedSend {
        patch,
        establishing: context.establishing,
        needs_request: context.needs_request,
        plan: OutgoingPlan {
            current,
            chat: chat.clone(),
            body,
            destination: context.destination,
            wire_id,
            request_id: context.request_id,
            needs_request: context.needs_request,
            recover_existing: context.recover_existing,
        },
    })
}

struct SendContext {
    destination: String,
    new_wire_id: String,
    request_id: Option<String>,
    establishing: bool,
    needs_request: bool,
    recover_existing: bool,
    queued_recovery: Option<(String, String)>,
}

fn prepare_context(chat: &Chat, body: &str) -> Result<SendContext, String> {
    if body.trim().is_empty() {
        return Err("Message is empty.".into());
    }
    validate_chat_send(chat)?;
    let destination = chat
        .peer_kaspa_address()
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "This chat has no Kaspa destination.".to_string())?;
    let new_wire_id = crate::random_id()?;
    let passive_restore_wait =
        chat.transport_restore_pending() && chat.session_role() == Some("responder");
    let establishing = chat_establishing(chat) || passive_restore_wait;
    let recover_existing = recovery_can_resume(chat, establishing);
    let needs_request = !chat.bootstrap_complete() && !establishing && !recover_existing;
    let request_id = needs_request.then(crate::random_id).transpose()?;
    let queued_recovery = queued_recovery_message(chat, recover_existing);
    Ok(SendContext {
        destination,
        new_wire_id,
        request_id,
        establishing,
        needs_request,
        recover_existing,
        queued_recovery,
    })
}

fn recovery_can_resume(chat: &Chat, establishing: bool) -> bool {
    !chat.bootstrap_complete()
        && !establishing
        && chat.session_sid().is_some()
        && chat.peer_hydra_handle().is_some()
}

pub(crate) async fn execute_send(mut plan: OutgoingPlan, password: &str) -> SendOutcome {
    let submitted = plan.current.clone();
    match submit_outgoing(&plan, password).await {
        Ok(sent) => apply_success(&submitted, &mut plan, sent),
        Err(error) => apply_failure(&submitted, &mut plan, error),
    }
}

fn queued_recovery_message(chat: &Chat, recover_existing: bool) -> Option<(String, String)> {
    recover_existing
        .then(|| {
            chat.messages()
                .iter()
                .find(|message| {
                    message.direction == "out"
                        && message.pending
                        && message.pending_id.is_none()
                        && message.txid.is_none()
                        && message.pending_stage.as_deref() == Some("handshake")
                })
                .map(|message| {
                    (
                        message.body.clone(),
                        message
                            .wire_id
                            .clone()
                            .unwrap_or_else(|| message.id.clone()),
                    )
                })
        })
        .flatten()
}

fn mark_recovery_message_sending(chats: &mut ghost_chat::ChatStore, chat: &Chat, wire_id: &str) {
    let message_id = ghost_chat::ChatService::by_id(chats, &chat.id)
        .and_then(|thread| {
            thread.messages().iter().find(|message| {
                message.wire_id.as_deref() == Some(wire_id) || message.id == wire_id
            })
        })
        .map(|message| message.id.clone());
    if let Some(message_id) = message_id {
        let _ = ghost_chat::ChatService::mark_message_sending(chats, &chat.id, &message_id);
    }
}

fn append_outgoing_message(
    chats: &mut ghost_chat::ChatStore,
    chat: &Chat,
    body: &str,
    wire_id: &str,
    request_id: Option<String>,
    needs_request: bool,
    establishing: bool,
) {
    let session_sid = request_id.clone().or_else(|| {
        ghost_chat::ChatService::by_id(chats, &chat.id)
            .and_then(|thread| thread.session_sid().map(str::to_owned))
    });
    if needs_request {
        let _ = ghost_chat::HydraSessionManager::set_role(chats, &chat.id, "initiator".into());
    }
    let _ = ghost_chat::ChatService::record_message(
        chats,
        &chat.id,
        Message {
            id: wire_id.into(),
            wire_id: Some(wire_id.into()),
            session_sid,
            direction: "out".into(),
            body: body.into(),
            created_at: crate::now_ms(),
            send_state: Some(if establishing { "queued" } else { "sending" }.into()),
            pending: true,
            pending_stage: Some(
                if needs_request {
                    "request"
                } else if establishing {
                    "handshake"
                } else {
                    "delivery"
                }
                .into(),
            ),
            contact_request_id: request_id,
            ..Default::default()
        },
    );
}

async fn submit_outgoing(plan: &OutgoingPlan, password: &str) -> Result<MailboxSendResult, String> {
    if plan.needs_request {
        reset_known_peer_for_fresh_chat(plan).await?;
        return crate::native::send_contact_request(
            &plan.current,
            password,
            &plan.destination,
            plan.request_id.as_deref().unwrap_or(""),
        )
        .await;
    }
    let handle = plan
        .chat
        .peer_hydra_handle()
        .map(str::to_owned)
        .unwrap_or_default();
    crate::native::send_mailbox_message(
        &plan.current,
        password,
        &handle,
        &plan.destination,
        &plan.body,
        &plan.wire_id,
    )
    .await
}

async fn reset_known_peer_for_fresh_chat(plan: &OutgoingPlan) -> Result<(), String> {
    let Some(peer) = plan.chat.peer_hydra_handle() else {
        return Ok(());
    };
    crate::native::rejoin_peer(&plan.current.id, peer).await
}

fn chat_establishing(chat: &Chat) -> bool {
    !chat.bootstrap_complete()
        && (chat
            .incoming_request()
            .is_some_and(|request| request.state == "accepted")
            || chat.messages().iter().any(handshake_pending_message))
}

fn handshake_pending_message(message: &Message) -> bool {
    message.pending
        && matches!(
            message.pending_stage.as_deref(),
            Some("request" | "handshake" | "finish")
        )
        && (message.pending_id.is_some() || message.txid.is_some())
}

fn validate_chat_send(chat: &Chat) -> Result<(), String> {
    if chat.archived() || chat.left() || chat.peer_left() {
        return Err("This chat is closed. Start a new chat with this peer.".into());
    }
    if chat
        .incoming_request()
        .is_some_and(|request| request.call_id.is_none() && request.state != "accepted")
    {
        return Err("Accept this incoming secure chat request before sending.".into());
    }
    Ok(())
}
