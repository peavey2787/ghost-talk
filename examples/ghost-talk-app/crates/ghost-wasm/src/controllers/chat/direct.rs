//! Ephemeral p2p-net chat text: recorded locally, never stored on Kaspa.

use ghost_protocol::DirectTextV1;

use crate::model::{Message, Profile, ProfilePatch};

/// Record direct text sent (`outgoing`) or received over p2p-net. Returns
/// `None` when the message is already known (replayed delivery).
pub(crate) fn record_direct_text(
    profile: &Profile,
    chat_id: &str,
    session_sid: Option<String>,
    text: &DirectTextV1,
    outgoing: bool,
) -> Option<(Profile, ProfilePatch)> {
    let known = profile
        .chats
        .iter()
        .filter(|chat| chat.id == chat_id)
        .flat_map(|chat| chat.messages())
        .any(|message| message.id == text.message_id);
    if known {
        return outgoing.then(|| mark_known(profile, chat_id, &text.message_id, None));
    }
    let mut after = profile.clone();
    let message = Message {
        id: text.message_id.clone(),
        wire_id: Some(text.message_id.clone()),
        session_sid,
        direction: if outgoing { "out" } else { "in" }.into(),
        body: text.body.clone(),
        created_at: crate::now_ms(),
        send_state: outgoing.then(|| "delivered".to_string()),
        carrier: Some(ghost_chat::CARRIER_P2P.into()),
        ..Default::default()
    };
    if outgoing {
        ghost_chat::ChatService::record_message(&mut after.chats, chat_id, message);
    } else {
        ghost_chat::ChatService::record_incoming_message(&mut after.chats, chat_id, message);
    }
    let patch = super::chat_profile_transition(profile, &after, chat_id);
    Some((after, patch))
}

/// Record an outgoing p2p-net text that could not be sent, so it stays
/// visible with its error and can be retried.
pub(crate) fn record_failed_direct_text(
    profile: &Profile,
    chat_id: &str,
    session_sid: Option<String>,
    text: &DirectTextV1,
    error: String,
) -> (Profile, ProfilePatch) {
    let base = record_direct_text(profile, chat_id, session_sid, text, true)
        .map_or_else(|| profile.clone(), |(after, _)| after);
    let (after, _) = mark_known(&base, chat_id, &text.message_id, Some(error));
    let patch = super::chat_profile_transition(profile, &after, chat_id);
    (after, patch)
}

/// An already recorded outgoing message is delivered (`None`) or failed.
fn mark_known(
    profile: &Profile,
    chat_id: &str,
    message_id: &str,
    error: Option<String>,
) -> (Profile, ProfilePatch) {
    let mut after = profile.clone();
    match error {
        None => {
            ghost_chat::ChatService::mark_message_delivered(&mut after.chats, chat_id, message_id)
        }
        Some(error) => ghost_chat::ChatService::mark_message_failed(
            &mut after.chats,
            chat_id,
            message_id,
            error,
        ),
    };
    let patch = super::chat_profile_transition(profile, &after, chat_id);
    (after, patch)
}
