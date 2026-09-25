use super::request_ingress::upsert_incoming_request;
use crate::app::state::live_voice;
use crate::model::{
    CallRuntimeEvent, Chat, ChatStore, ContactStore, HydraIncomingRequestProjection,
    HydraMailboxResult, Profile, RealtimeControl, Settings,
};
use ghost_domain::identity::{optional_binding_matches, PeerBinding};
pub(super) fn collect_call_runtime_events(
    call_signal_is_new: bool,
    result: &HydraMailboxResult,
    events: &mut Vec<CallRuntimeEvent>,
) {
    if call_signal_is_new {
        if let Some(signal) = result.call_signal.as_ref() {
            events.push(CallRuntimeEvent::SignedSignal(signal.clone()));
        }
    }
    if let Some(accepted) = result.contact_accepted.as_ref() {
        events.push(CallRuntimeEvent::ContactAccepted(accepted.clone()));
    }
}

pub(super) fn enqueue_bootstrap_call_control(
    profile: &Profile,
    result: &HydraMailboxResult,
    realtime_controls: &mut Vec<RealtimeControl>,
) -> Result<(), String> {
    let Some(request) = result.incoming_request() else {
        return Ok(());
    };
    let Some(call_id) = request.call_id.as_deref() else {
        return Ok(());
    };
    let Some(live_kind) = call_bootstrap_live_kind(request.call_action.as_deref()) else {
        return Ok(());
    };
    let Some(chat_id) = resolve_call_chat_id(profile, request, call_id) else {
        return Ok(());
    };
    let body = live_voice::encode_live(&live_voice::LiveVoicePacket::new(
        call_id.to_string(),
        live_kind,
    ))?;
    realtime_controls.push(RealtimeControl {
        id: format!("call-bootstrap-{}", request.request_id),
        chat_id,
        session_sid: request.request_id.clone(),
        body,
    });
    Ok(())
}

fn call_bootstrap_live_kind(action: Option<&str>) -> Option<&'static str> {
    match action.unwrap_or("request") {
        "request" => Some("request"),
        "decline" => Some("decline"),
        "cancel" => Some("hangup"),
        _ => None,
    }
}

fn resolve_call_chat_id(
    profile: &Profile,
    request: &HydraIncomingRequestProjection,
    call_id: &str,
) -> Option<String> {
    request_chat_id(profile, call_id).or_else(|| peer_chat_id(profile, request))
}

fn request_chat_id(profile: &Profile, call_id: &str) -> Option<String> {
    profile
        .chats
        .iter()
        .find(|chat| {
            chat.incoming_request().is_some_and(|incoming| {
                incoming.call_id.as_deref() == Some(call_id) && incoming.state != "ignored"
            })
        })
        .map(|chat| chat.id.clone())
}

fn peer_chat_id(profile: &Profile, request: &HydraIncomingRequestProjection) -> Option<String> {
    profile
        .chats
        .iter()
        .find(|chat| chat.reusable_direct() && request_peer_matches(chat, request))
        .map(|chat| chat.id.clone())
}

fn request_peer_matches(chat: &Chat, request: &HydraIncomingRequestProjection) -> bool {
    PeerBinding::new(request.peer_address.clone(), request.peer_hydra_id.clone())
        .ok()
        .is_some_and(|peer| {
            optional_binding_matches(chat.peer_kaspa_address(), chat.peer_hydra_handle(), &peer)
        })
}

pub(super) fn apply_incoming_request(
    contacts: &ContactStore,
    chats: &mut ChatStore,
    settings: &Settings,
    result: &HydraMailboxResult,
) {
    let Some(request) = result.incoming_request().cloned() else {
        return;
    };
    if is_non_request_call_control(&request) {
        return;
    }
    upsert_incoming_request(contacts, chats, settings, request);
}

fn is_non_request_call_control(request: &HydraIncomingRequestProjection) -> bool {
    request.call_id.is_some() && request.call_action.as_deref().unwrap_or("request") != "request"
}
