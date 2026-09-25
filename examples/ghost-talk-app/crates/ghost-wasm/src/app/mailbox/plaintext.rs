use super::{
    super::room_ingress::handle_room_wire,
    super::state::{decode_room_wire, live_voice, native},
    mailbox_pipeline::active_peer_chat,
    queued_messages::flush_queued_messages,
};
use crate::model::{
    Chat, ChatStore, ContactStore, HydraMailboxResult, HydraSessionBindingProjection, Message,
    Profile, RealtimeControl, ReceivedProjection, WalletProjection,
};
use crate::storage;
pub(crate) async fn handle_received_plaintext(
    mut profile: Profile,
    password: &str,
    envelope: &storage::ReadyEnvelope,
    result: &HydraMailboxResult,
    realtime_controls: &mut Vec<RealtimeControl>,
) -> Result<Profile, String> {
    let Some(received) = result.received.as_ref() else {
        return Ok(profile);
    };
    if received.plaintext == native::SESSION_RESTORE_BODY {
        return Ok(profile);
    }
    if received.content_type.as_deref() == Some("ghost-reaction-v1") {
        apply_incoming_reaction(&mut profile.chats, received);
        return Ok(profile);
    }
    if let Some(room_wire) = decode_room_wire(&received.plaintext) {
        return handle_room_wire(profile, password, &received.from, room_wire).await;
    }
    handle_direct_plaintext(
        &mut profile.chats,
        &mut profile.contacts,
        envelope,
        result,
        received,
        realtime_controls,
    );
    Ok(profile)
}

pub(crate) fn handle_direct_plaintext(
    chats: &mut ChatStore,
    contacts: &mut ContactStore,
    envelope: &storage::ReadyEnvelope,
    result: &HydraMailboxResult,
    received: &ReceivedProjection,
    realtime_controls: &mut Vec<RealtimeControl>,
) {
    let (Some(wire_id), Some(sid)) = (result.message_id.as_ref(), received.session_sid.as_ref())
    else {
        return;
    };
    let chat_id = ensure_visible_direct_chat(
        chats,
        contacts,
        &received.from,
        sid,
        result.peer_address.as_deref(),
    );
    if live_voice::is_realtime_body(&received.plaintext) {
        if !crate::realtime_replay::accept_fields(&received.from, sid, wire_id) {
            return;
        }
        if let Some(chat) = ghost_chat::ChatService::by_id(chats, &chat_id) {
            realtime_controls.push(realtime_control(wire_id, chat, sid, &received.plaintext));
        }
    } else {
        insert_direct_message(chats, &chat_id, envelope, wire_id, sid, &received.plaintext);
    }
}

pub(crate) fn realtime_control(
    wire_id: &str,
    chat: &Chat,
    sid: &str,
    body: &str,
) -> RealtimeControl {
    RealtimeControl {
        id: wire_id.to_string(),
        chat_id: chat.id.clone(),
        session_sid: sid.to_string(),
        body: body.to_string(),
    }
}

pub(crate) fn insert_direct_message(
    chats: &mut ghost_chat::ChatStore,
    chat_id: &str,
    envelope: &storage::ReadyEnvelope,
    wire_id: &str,
    sid: &str,
    body: &str,
) {
    let duplicate = ghost_chat::ChatService::by_id(chats, chat_id).is_some_and(|chat| {
        chat.messages()
            .iter()
            .any(|message| message.wire_id.as_deref() == Some(wire_id))
    });
    if duplicate {
        return;
    }
    let _ = super::super::ApplicationRouter::route_chat(
        chats,
        super::super::ApplicationEvent::IncomingMessage {
            chat_id: chat_id.to_string(),
            message: Message {
                id: wire_id.to_string(),
                wire_id: Some(wire_id.to_string()),
                session_sid: Some(sid.to_string()),
                direction: "in".into(),
                body: body.to_string(),
                created_at: envelope
                    .block_time
                    .map(|value| value as f64 * 1000.0)
                    .unwrap_or_else(crate::now_ms),
                txid: Some(envelope.transaction_id.clone()),
                send_state: Some("delivered".into()),
                ..Default::default()
            },
        },
    );
}

pub(crate) fn apply_delivery_ack(chats: &mut ChatStore, result: &HydraMailboxResult) {
    let (Some(message_id), Some(peer)) = (
        result.delivery_ack.as_deref(),
        result.delivery_ack_peer.as_deref(),
    ) else {
        return;
    };
    let targets: Vec<(String, String)> = chats
        .iter()
        .filter(|chat| chat.peer_hydra_handle() == Some(peer))
        .filter_map(|chat| {
            chat.messages()
                .iter()
                .find(|message| message_matches_ack(message, message_id))
                .map(|message| (chat.id.clone(), message.id.clone()))
        })
        .collect();
    for (chat_id, stored_message_id) in targets {
        let _ =
            ghost_chat::ChatService::mark_message_delivered(chats, &chat_id, &stored_message_id);
    }
}

pub(crate) fn message_matches_ack(message: &Message, message_id: &str) -> bool {
    message.wire_id.as_deref() == Some(message_id) || message.id == message_id
}

pub(crate) async fn handle_session_established(
    mut profile: Profile,
    password: &str,
    result: &HydraMailboxResult,
) -> Result<Profile, String> {
    let Some(peer) = result.session_established_peer.as_deref() else {
        return Ok(profile);
    };
    let binding = native::peer_session_binding(&profile.id, peer).await?;
    mark_peer_established(&mut profile.chats, peer, binding.as_ref());
    if let Some(progress) = send_bootstrap_ack(&profile, password, result, peer).await? {
        super::super::ApplicationRouter::route_wallet(
            &mut profile.wallet,
            super::super::ApplicationEvent::WalletUpdated {
                projection: progress,
            },
        );
    }
    flush_queued_messages(profile, password, peer).await
}

pub(crate) fn mark_peer_established(
    chats: &mut ChatStore,
    peer: &str,
    binding: Option<&HydraSessionBindingProjection>,
) {
    let chat_ids: Vec<String> = chats
        .iter()
        .filter(|chat| active_peer_chat(chat, peer))
        .map(|chat| chat.id.clone())
        .collect();
    for chat_id in chat_ids {
        if let Some(binding) = binding {
            let _ = ghost_chat::HydraSessionManager::establish(
                chats,
                &chat_id,
                binding.sid.clone(),
                binding.role.clone(),
            );
        } else {
            let _ = ghost_chat::HydraSessionManager::complete_bootstrap(chats, &chat_id, None);
        }
        let clear_request = ghost_chat::ChatService::by_id(chats, &chat_id).is_some_and(|chat| {
            !chat.room_transport_only()
                && chat
                    .incoming_request()
                    .is_some_and(|request| request.state == "accepted")
        });
        if clear_request {
            let _ = ghost_chat::ChatService::clear_incoming_request(chats, &chat_id);
        }
    }
}

pub(crate) async fn send_bootstrap_ack(
    profile: &Profile,
    password: &str,
    result: &HydraMailboxResult,
    peer: &str,
) -> Result<Option<WalletProjection>, String> {
    let Some(message_id) = result.message_id.as_deref() else {
        return Ok(None);
    };
    let Some(destination) = established_peer_destination(profile, result, peer) else {
        return Ok(None);
    };
    let ack = native::send_delivery_ack(profile, password, &destination, peer, message_id).await?;
    Ok(Some(ack.public))
}

pub(crate) fn established_peer_destination(
    profile: &Profile,
    result: &HydraMailboxResult,
    peer: &str,
) -> Option<String> {
    result.peer_address.clone().or_else(|| {
        profile
            .chats
            .iter()
            .find(|chat| chat.peer_hydra_handle() == Some(peer))
            .and_then(|chat| chat.peer_kaspa_address().map(str::to_owned))
    })
}

pub(crate) fn apply_session_ended(chats: &mut ChatStore, result: &HydraMailboxResult) {
    let Some(ended) = result.session_ended.as_ref() else {
        return;
    };
    let _ = super::super::ApplicationRouter::route_chat(
        chats,
        super::super::ApplicationEvent::ChatSessionEnded {
            peer_hydra_id: ended.peer_hydra_id.clone(),
            sid: ended.sid.clone(),
        },
    );
}

mod direct_chat;
pub(crate) use direct_chat::ensure_visible_direct_chat;

pub(crate) fn apply_incoming_reaction(chats: &mut ChatStore, received: &ReceivedProjection) {
    let Ok(event) = serde_json::from_str::<ghost_protocol::GhostReactionEvent>(&received.plaintext)
    else {
        return;
    };
    let Some(sid) = received.session_sid.as_deref() else {
        return;
    };
    let Some(chat_id) = chats
        .iter()
        .find(|chat| {
            !chat.room_transport_only()
                && chat.peer_hydra_handle() == Some(received.from.as_str())
                && chat.session_sid() == Some(sid)
        })
        .map(|chat| chat.id.clone())
    else {
        return;
    };
    let target = chats
        .iter()
        .find(|chat| chat.id == chat_id)
        .and_then(|chat| {
            chat.messages()
                .iter()
                .find(|message| {
                    message.wire_id.as_deref() == Some(event.target_message_id.as_str())
                        || message.id == event.target_message_id
                })
                .map(|message| message.id.clone())
        });
    if let Some(target) = target {
        let _ = ghost_chat::ChatService::set_message_reaction(
            chats,
            &chat_id,
            &target,
            &received.from,
            event.reaction,
        );
    }
}
