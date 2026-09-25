use crate::{
    model::{
        CallRuntimeEvent, Chat, ChatStore, HydraMailboxResult, Profile, ProfilePatch,
        RealtimeControl,
    },
    storage::{self, MailboxService},
};

use super::{
    super::contact_accept::handle_contact_accepted,
    super::state::native,
    call_mailbox::{
        apply_incoming_request, collect_call_runtime_events, enqueue_bootstrap_call_control,
    },
    change_log::MailboxChangeLog,
    plaintext::{
        apply_delivery_ack, apply_session_ended, handle_received_plaintext,
        handle_session_established,
    },
};

pub(crate) async fn process_ready_mailbox(
    mut profile: Profile,
    password: &str,
) -> Result<
    (
        Profile,
        Vec<ProfilePatch>,
        Vec<RealtimeControl>,
        Vec<CallRuntimeEvent>,
    ),
    String,
> {
    if !mailbox_processing_ready(&profile, password) {
        return Ok((profile, Vec::new(), Vec::new(), Vec::new()));
    }
    let mut patches = Vec::new();
    let mut realtime_controls = Vec::new();
    let mut call_events = Vec::new();
    for _ in 0..=128 {
        let (next, progressed) = process_mailbox_pass(
            profile,
            password,
            &mut patches,
            &mut realtime_controls,
            &mut call_events,
        )
        .await?;
        profile = next;
        if !progressed {
            break;
        }
    }
    Ok((profile, patches, realtime_controls, call_events))
}

async fn process_mailbox_pass(
    mut profile: Profile,
    password: &str,
    patches: &mut Vec<ProfilePatch>,
    realtime_controls: &mut Vec<RealtimeControl>,
    call_events: &mut Vec<CallRuntimeEvent>,
) -> Result<(Profile, bool), String> {
    let Some(wallet) = profile.wallet.as_ref() else {
        return Ok((profile, false));
    };
    let envelopes = MailboxService::ready(wallet);
    if envelopes.is_empty() {
        return Ok((profile, false));
    }
    let before = wallet.mailbox_pending.len();
    for envelope in envelopes {
        let (next, _disposition) = process_envelope_isolated(
            profile,
            password,
            &envelope,
            patches,
            realtime_controls,
            call_events,
        )
        .await;
        profile = next;
    }
    let after = profile
        .wallet
        .as_ref()
        .map(|wallet| wallet.mailbox_pending.len())
        .unwrap_or(0);
    Ok((profile, after < before))
}

async fn process_envelope_isolated(
    profile: Profile,
    password: &str,
    envelope: &storage::ReadyEnvelope,
    patches: &mut Vec<ProfilePatch>,
    realtime_controls: &mut Vec<RealtimeControl>,
    call_events: &mut Vec<CallRuntimeEvent>,
) -> (Profile, ghost_domain::mailbox::PacketDisposition) {
    let mut changes = MailboxChangeLog::new(&profile);
    let mut local_controls = Vec::new();
    let mut local_call_events = Vec::new();
    match process_mailbox_envelope(
        profile.clone(),
        password,
        envelope,
        &mut changes,
        &mut local_controls,
        &mut local_call_events,
    )
    .await
    {
        Ok((mut working, disposition)) => {
            if disposition.removes_envelope() {
                super::super::ApplicationRouter::route_wallet(
                    &mut working.wallet,
                    super::super::ApplicationEvent::MailboxEnvelopeConsumed {
                        packet_id: envelope.packet_id.clone(),
                    },
                );
            }
            let patch = changes.finish(&working);
            if !patch.is_empty() {
                patches.push(patch);
            }
            realtime_controls.extend(local_controls);
            call_events.extend(local_call_events);
            (working, disposition)
        }
        Err(error) => {
            log_deferred_envelope(&envelope.packet_id, &error);
            (profile, ghost_domain::mailbox::PacketDisposition::Retryable)
        }
    }
}

fn log_deferred_envelope(packet_id: &str, error: &str) {
    web_sys::console::warn_1(
        &format!("Ghost Talk deferred mailbox packet {packet_id} after processing error: {error}")
            .into(),
    );
}

pub(crate) fn mailbox_processing_ready(profile: &Profile, password: &str) -> bool {
    profile.wallet.is_some() && profile.hydra_identity_id.is_some() && !password.is_empty()
}

pub(super) async fn process_mailbox_envelope(
    mut profile: Profile,
    password: &str,
    envelope: &storage::ReadyEnvelope,
    changes: &mut MailboxChangeLog,
    realtime_controls: &mut Vec<RealtimeControl>,
    call_events: &mut Vec<CallRuntimeEvent>,
) -> Result<(Profile, ghost_domain::mailbox::PacketDisposition), String> {
    let result = native::receive_mailbox(&profile, password, &envelope.envelope_hex).await?;
    changes.track_result(&profile, &result);
    let call_signal_is_new = result
        .call_signal
        .as_ref()
        .is_some_and(|signal| profile.record_seen_call_signal(signal.signal_id.clone()));
    collect_call_runtime_events(call_signal_is_new, &result, call_events);
    apply_incoming_request(
        &profile.contacts,
        &mut profile.chats,
        &profile.settings,
        &result,
    );
    enqueue_bootstrap_call_control(&profile, &result, realtime_controls)?;
    profile = apply_contact_acceptance(profile, password, &result).await?;
    let (next, recovering) = handle_mailbox_recovery(profile, password, &result).await?;
    profile = next;
    if recovering {
        return Ok((profile, ghost_domain::mailbox::PacketDisposition::Deferred));
    }
    profile = handle_mailbox_control(profile, password, &result).await?;
    profile =
        handle_received_plaintext(profile, password, envelope, &result, realtime_controls).await?;
    apply_delivery_ack(&mut profile.chats, &result);
    profile = handle_session_established(profile, password, &result).await?;
    apply_session_ended(&mut profile.chats, &result);
    changes.track_created_records(&profile, &result);
    let disposition = if result.discard {
        ghost_domain::mailbox::PacketDisposition::Consumed
    } else {
        ghost_domain::mailbox::PacketDisposition::Deferred
    };
    Ok((profile, disposition))
}

pub(crate) async fn apply_contact_acceptance(
    profile: Profile,
    password: &str,
    result: &HydraMailboxResult,
) -> Result<Profile, String> {
    let Some(accepted) = result.contact_accepted.clone() else {
        return Ok(profile);
    };
    handle_contact_accepted(profile, password, accepted).await
}

pub(crate) async fn handle_mailbox_recovery(
    mut profile: Profile,
    password: &str,
    result: &HydraMailboxResult,
) -> Result<(Profile, bool), String> {
    let Some(recovery) = result.recovery.as_ref() else {
        return Ok((profile, false));
    };
    let sent = native::send_recovery_offer(&profile, password, recovery).await?;
    super::super::ApplicationRouter::route_wallet(
        &mut profile.wallet,
        super::super::ApplicationEvent::WalletUpdated {
            projection: sent.public.clone(),
        },
    );
    mark_peer_recovering(&mut profile.chats, &recovery.peer_hydra_id, &recovery.sid);
    Ok((profile, true))
}

pub(crate) fn mark_peer_recovering(chats: &mut ChatStore, peer: &str, sid: &str) {
    let chat_ids: Vec<String> = chats
        .iter()
        .filter(|chat| active_peer_chat(chat, peer))
        .map(|chat| chat.id.clone())
        .collect();
    for chat_id in chat_ids {
        let _ = ghost_chat::HydraSessionManager::begin(
            chats,
            &chat_id,
            Some(sid.to_string()),
            Some("initiator".into()),
        );
    }
}

pub(crate) fn active_peer_chat(chat: &Chat, peer: &str) -> bool {
    chat.peer_hydra_handle() == Some(peer) && !chat.archived() && !chat.left() && !chat.peer_left()
}

pub(crate) async fn handle_mailbox_control(
    mut profile: Profile,
    password: &str,
    result: &HydraMailboxResult,
) -> Result<Profile, String> {
    let Some(control) = result.control.as_ref() else {
        return Ok(profile);
    };
    let sent = native::send_mailbox_control(&profile, password, control).await?;
    super::super::ApplicationRouter::route_wallet(
        &mut profile.wallet,
        super::super::ApplicationEvent::WalletUpdated {
            projection: sent.public.clone(),
        },
    );
    if let Some(id) = control.completes_pending_id.as_deref() {
        mark_pending_finish(&mut profile.chats, id, &sent.transaction_id);
    }
    Ok(profile)
}

pub(crate) fn mark_pending_finish(chats: &mut ChatStore, pending_id: &str, transaction_id: &str) {
    let targets: Vec<(String, String)> = chats
        .iter()
        .flat_map(|chat| {
            chat.messages()
                .iter()
                .filter(|message| message.pending_id.as_deref() == Some(pending_id))
                .map(|message| (chat.id.clone(), message.id.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    for (chat_id, message_id) in targets {
        let _ = ghost_chat::ChatService::mark_message_finish_sent(
            chats,
            &chat_id,
            &message_id,
            transaction_id.to_string(),
        );
    }
}
