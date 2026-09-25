use crate::model::{
    HydraMailboxResult, HydraRealtimeEnvelope, HydraSessionBindingProjection, MailboxSendResult, Profile,
};

mod acceptance;
mod chat_binding;
mod realtime;
mod transport_state;
pub(crate) use acceptance::accept_bootstrap;
pub(crate) use chat_binding::ensure_chat;
pub(crate) use realtime::RealtimeSender;
pub(crate) use transport_state::{
    apply_session_binding, complete_contact_bootstrap, prepare_bootstrap,
};

pub(crate) async fn open_realtime(
    profile_id: &str,
    carrier_b64: &str,
) -> Result<HydraMailboxResult, String> {
    crate::native::open_realtime(profile_id, carrier_b64).await
}

pub(crate) async fn peer_session_binding(
    profile_id: &str,
    contact_id: &str,
) -> Result<Option<HydraSessionBindingProjection>, String> {
    crate::native::peer_session_binding(profile_id, contact_id).await
}

pub(crate) async fn seal_realtime(
    profile_id: &str,
    contact_id: &str,
    message_id: &str,
    body: &str,
) -> Result<HydraRealtimeEnvelope, String> {
    crate::native::seal_realtime(profile_id, contact_id, message_id, body).await
}

pub(crate) async fn send_realtime_carrier(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    carrier_b64: &str,
) -> Result<MailboxSendResult, String> {
    crate::native::send_realtime_carrier(
        profile,
        password,
        contact_id,
        destination,
        carrier_b64,
    )
    .await
}

pub(crate) async fn send_call_contact_request(
    profile: &Profile,
    password: &str,
    destination: &str,
    request_id: &str,
    call_id: &str,
) -> Result<MailboxSendResult, String> {
    crate::native::send_call_contact_request(profile, password, destination, request_id, call_id)
        .await
}

pub(crate) async fn send_call_control_message(
    profile: &Profile,
    password: &str,
    contact_id: &str,
    destination: &str,
    body: &str,
    message_id: &str,
) -> Result<MailboxSendResult, String> {
    crate::native::send_call_control_message(
        profile,
        password,
        contact_id,
        destination,
        body,
        message_id,
    )
    .await
}

pub(crate) async fn send_call_signal(
    profile: &Profile,
    password: &str,
    destination: &str,
    signal_id: &str,
    call_id: &str,
    action: &str,
) -> Result<MailboxSendResult, String> {
    crate::native::send_call_signal(profile, password, destination, signal_id, call_id, action)
        .await
}

// Process-local call lifecycle facade. UI callbacks express semantic commands;
// CallManager remains the sole mutable owner of call records.
pub(crate) fn transition(
    manager: &mut ghost_domain::call::CallManager,
    call_id: &str,
    event: ghost_domain::call::CallEvent,
) -> Result<(), String> {
    manager.event(call_id, event)
}

pub(crate) fn end_local(
    manager: &mut ghost_domain::call::CallManager,
    call_id: &str,
    event: ghost_domain::call::CallEvent,
) -> Result<(), String> {
    if crate::app::ApplicationRouter::route_call(
        manager,
        crate::app::ApplicationEvent::CallEnded {
            call_id: call_id.to_owned(),
            event,
        },
    ) {
        Ok(())
    } else {
        Err("Call could not transition to the ended state.".into())
    }
}

pub(crate) fn fail(
    manager: &mut ghost_domain::call::CallManager,
    call_id: &str,
    error: String,
) -> Result<(), String> {
    manager.fail(call_id, error)
}

pub(crate) fn begin_outgoing(
    manager: &mut ghost_domain::call::CallManager,
    call_id: String,
    chat_id: String,
    peer_address: String,
    peer_hydra_id: String,
    peer_label: String,
) -> Result<(), String> {
    manager.begin_outgoing(call_id, chat_id, peer_address, peer_hydra_id, peer_label)
}

pub(crate) fn receive_signal(
    manager: &mut ghost_domain::call::CallManager,
    signal: &ghost_domain::call::CallSignal,
) -> ghost_domain::call::SignalDisposition {
    manager.receive_signal(signal)
}

pub(crate) fn attach_chat(
    manager: &mut ghost_domain::call::CallManager,
    call_id: &str,
    chat_id: String,
) -> Result<(), String> {
    manager.attach_chat(call_id, chat_id)
}

pub(crate) fn set_bootstrap_request(
    manager: &mut ghost_domain::call::CallManager,
    call_id: &str,
    request_id: Option<String>,
) -> Result<(), String> {
    manager.set_bootstrap_request(call_id, request_id)
}
