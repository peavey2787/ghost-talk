//! Chat text carrier selection. Kaspa is the durable default; users may send
//! ephemeral text over p2p-net that is never stored on Kaspa.

use ghost_protocol::DirectTextV1;
use ghost_realtime::{text_route, RealtimeCarrier, TextPreference};
use yew::prelude::*;

use super::view::{ChatProps, ChatUiState};
use crate::components::call::{DirectTextContext, DirectTextRequest};
use crate::model::{Chat, Profile, ProfilePatch};

const P2P_ONLY_BLOCKED: &str =
    "Not sent: text is set to P2P only and there is no direct p2p-net connection to this peer.";

/// Route composed text to p2p-net or to the durable Kaspa sender.
pub(crate) fn route_text(
    props: &ChatProps,
    state: &ChatUiState,
    chat: Chat,
    durable: Callback<String>,
) -> Callback<String> {
    let profile = props.profile.clone();
    let on_update = props.on_update.clone();
    let status = state.status.clone();
    let direct = state.direct.clone();
    Callback::from(move |body: String| {
        let preference = TextPreference::from_setting(&profile.settings.text_route);
        let context = direct
            .clone()
            .filter(|context| context.connected && direct_ready(&chat));
        match text_route(preference, context.is_some()) {
            None => status.set(P2P_ONLY_BLOCKED.into()),
            Some(decision) if decision.primary == RealtimeCarrier::P2pNet => {
                let fallback = decision.fallback.map(|_| durable.clone());
                let outcome = DirectOutcome {
                    profile: profile.clone(),
                    chat_id: chat.id.clone(),
                    session_sid: chat.session_sid().map(str::to_owned),
                    on_update: on_update.clone(),
                    status: status.clone(),
                    fallback,
                };
                send_direct(context, body, outcome);
            }
            Some(_) => durable.emit(body),
        }
    })
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
    fallback: Option<Callback<String>>,
}

fn send_direct(context: Option<DirectTextContext>, body: String, outcome: DirectOutcome) {
    let (Some(context), Ok(message_id)) = (context, crate::random_id()) else {
        outcome.status.set(P2P_ONLY_BLOCKED.into());
        return;
    };
    let text = DirectTextV1 { message_id, body };
    outcome.status.set("Sending directly over p2p-net…".into());
    let chat_id = outcome.chat_id.clone();
    let sent = text.clone();
    let done = Callback::from(move |result: Result<(), String>| match result {
        Ok(()) => record_sent(&outcome, &sent),
        Err(error) => fall_back(&outcome, &sent.body, error),
    });
    context.send.emit(DirectTextRequest {
        chat_id,
        text,
        done,
    });
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

fn fall_back(outcome: &DirectOutcome, body: &str, error: String) {
    match &outcome.fallback {
        Some(durable) => {
            outcome.status.set(format!(
                "Direct p2p-net send failed ({error}); sending over Kaspa."
            ));
            durable.emit(body.to_owned());
        }
        None => outcome
            .status
            .set(format!("Not sent over p2p-net: {error}")),
    }
}
