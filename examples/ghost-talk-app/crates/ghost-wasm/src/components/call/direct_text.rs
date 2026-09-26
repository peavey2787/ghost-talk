//! Ephemeral p2p-net chat text. Sent and received only over an authenticated
//! p2p-net session route; never stored on Kaspa.

use ghost_p2p::P2pRouteState;
use ghost_protocol::DirectTextV1;
use yew::prelude::*;

use super::{publish_profile, CallRuntime};
use crate::model::RealtimeControl;

/// Direct-text capability offered to the chat composer.
#[derive(Clone, PartialEq)]
pub(crate) struct DirectTextContext {
    pub(crate) connected: bool,
    pub(crate) send: Callback<DirectTextRequest>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct DirectTextRequest {
    pub(crate) chat_id: String,
    pub(crate) text: DirectTextV1,
    pub(crate) done: Callback<Result<(), String>>,
}

pub(super) fn context(runtime: &CallRuntime) -> DirectTextContext {
    let realtime = runtime.realtime.clone();
    DirectTextContext {
        connected: *runtime.p2p_state == P2pRouteState::Connected,
        send: Callback::from(
            move |request: DirectTextRequest| match request.text.encode() {
                Ok(body) => realtime.enqueue_direct(request.chat_id, body, request.done),
                Err(error) => request.done.emit(Err(error)),
            },
        ),
    }
}

/// Record authenticated direct text received over p2p-net. Returns false when
/// the body is not direct text.
pub(super) fn receive(control: &RealtimeControl, runtime: &CallRuntime) -> bool {
    let Some(text) = DirectTextV1::decode(&control.body) else {
        return false;
    };
    let before = runtime.profile_ref.borrow().clone();
    let recorded = crate::controllers::chat::record_direct_text(
        &before,
        &control.chat_id,
        Some(control.session_sid.clone()),
        &text,
        false,
    );
    if let Some((after, patch)) = recorded {
        publish_profile(runtime, after, patch);
    }
    true
}
