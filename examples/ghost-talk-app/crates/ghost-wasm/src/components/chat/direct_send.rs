//! Chat text carrier selection. Kaspa is the durable default; users may send
//! ephemeral text over p2p-net that is never stored on Kaspa.

use ghost_protocol::DirectTextV1;
use ghost_realtime::{text_route, RealtimeCarrier, TextPreference};
use yew::prelude::*;

use super::outgoing::{OutgoingText, TextSender};
use super::view::{ChatProps, ChatUiState};
use crate::components::call::{DirectTextContext, DirectTextRequest};
use crate::model::{Chat, Profile, ProfilePatch};

const P2P_ONLY_BLOCKED: &str =
    "Not sent: text is set to P2P only and there is no direct p2p-net connection to this peer.";

/// Route composed text to p2p-net or to the durable Kaspa sender. Answers
/// whether the text was taken; a refusal leaves it with the caller.
pub(crate) fn route_text(
    props: &ChatProps,
    state: &ChatUiState,
    chat: Chat,
    durable: TextSender,
) -> TextSender {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    let direct = state.direct.clone();
    Callback::from(move |text: OutgoingText| {
        let preference = TextPreference::from_setting(&profile.settings.text_route);
        let context = direct
            .clone()
            .filter(|context| context.connected && direct_ready(&chat));
        let (Some(decision), Some(context)) = (text_route(preference, context.is_some()), context)
        else {
            return route_without_p2p(preference, text, &durable, &status);
        };
        if decision.primary != RealtimeCarrier::P2pNet {
            return durable.emit(text);
        }
        let outcome = DirectOutcome {
            profile: profile.clone(),
            chat_id: chat.id.clone(),
            session_sid: chat.session_sid().map(str::to_owned),
            on_update: on_update.clone(),
            status: status.clone(),
            fallback: decision.fallback.map(|_| durable.clone()),
        };
        send_direct(context, text, outcome)
    })
}

/// No connected p2p-net route: Kaspa takes the text unless the user chose
/// P2P only, in which case it is refused and stays with the caller.
fn route_without_p2p(
    preference: TextPreference,
    text: OutgoingText,
    durable: &TextSender,
    status: &UseStateHandle<String>,
) -> bool {
    if text_route(preference, false).is_some() {
        return durable.emit(text);
    }
    status.set(P2P_ONLY_BLOCKED.into());
    false
}

fn direct_ready(chat: &Chat) -> bool {
    chat.bootstrap_complete()
        && chat.session_sid().is_some()
        && !chat.archived()
        && !chat.left()
        && !chat.peer_left()
}

struct DirectOutcome {
    profile: Profile,
    chat_id: String,
    session_sid: Option<String>,
    on_update: Callback<ProfilePatch>,
    status: UseStateHandle<String>,
    fallback: Option<TextSender>,
}

fn send_direct(context: DirectTextContext, text: OutgoingText, outcome: DirectOutcome) -> bool {
    let message_id = match text.retry_wire_id.map_or_else(crate::random_id, Ok) {
        Ok(id) => id,
        Err(error) => {
            outcome.status.set(error);
            return false;
        }
    };
    let text = DirectTextV1 {
        message_id,
        body: text.body,
    };
    outcome.status.set("Sending directly over p2p-net…".into());
    let chat_id = outcome.chat_id.clone();
    let sent = text.clone();
    let done = Callback::from(move |result: Result<(), String>| match result {
        Ok(()) => record_sent(&outcome, &sent),
        Err(error) => fall_back(&outcome, &sent, error),
    });
    context.send.emit(DirectTextRequest {
        chat_id,
        text,
        done,
    });
    true
}

fn record_sent(outcome: &DirectOutcome, text: &DirectTextV1) {
    let recorded = crate::controllers::chat::record_direct_text(
        &outcome.profile,
        &outcome.chat_id,
        outcome.session_sid.clone(),
        text,
        true,
    );
    if let Some((_, patch)) = recorded {
        outcome.on_update.emit(patch);
    }
    outcome
        .status
        .set("Delivered over p2p-net. This message is not stored on Kaspa.".into());
}

/// A failed p2p-net send falls back to Kaspa when allowed, under the same id
/// so a copy that did arrive is deduplicated; under P2P only it is recorded
/// as failed so it can be retried.
fn fall_back(outcome: &DirectOutcome, text: &DirectTextV1, error: String) {
    if let Some(durable) = &outcome.fallback {
        outcome.status.set(format!(
            "Direct p2p-net send failed ({error}); sending over Kaspa."
        ));
        durable.emit(OutgoingText {
            body: text.body.clone(),
            retry_wire_id: Some(text.message_id.clone()),
        });
        return;
    }
    let (_, patch) = crate::controllers::chat::record_failed_direct_text(
        &outcome.profile,
        &outcome.chat_id,
        outcome.session_sid.clone(),
        text,
        error.clone(),
    );
    outcome.on_update.emit(patch);
    outcome
        .status
        .set(format!("Not sent over p2p-net: {error}"));
}
