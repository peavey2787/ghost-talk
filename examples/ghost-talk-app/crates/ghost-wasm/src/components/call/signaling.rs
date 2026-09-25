use ghost_domain::call::CallEvent;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    live_voice::{self, LiveVoicePacket},
    model::{CallPhase, CallRecord, RealtimeControl},
    random_id,
};

use super::identity::{call_peer_matches_chat, call_peer_matches_contact};
use super::{
    active_call, apply_call_command, end_runtime_call, fail_runtime_call, media, publish_profile,
    CallRuntime,
};
use crate::controllers::call::RealtimeSender;
mod transport_acceptance;
use transport_acceptance::{
    accept_bootstrap_request, accept_transport, decline_competing_transport,
    pending_bootstrap_request, send_encrypted_accept,
};

pub(super) async fn send_initial_ring(runtime: CallRuntime, call_id: String) {
    if let Err(error) = send_signed_action(&runtime, &call_id, "request").await {
        fail_runtime_call(
            &runtime,
            &call_id,
            format!("Call request could not be delivered: {error}"),
        );
        return;
    }
    TimeoutFuture::new(30_000).await;
    let still_ringing = runtime
        .call_manager
        .borrow()
        .get(&call_id)
        .is_some_and(|call| call.phase == CallPhase::OutgoingRinging);
    if !still_ringing {
        return;
    }
    let _ = send_signed_action(&runtime, &call_id, "cancel").await;
    fail_runtime_call(&runtime, &call_id, "No answer.".into());
}

pub(super) async fn send_busy_response(
    runtime: &CallRuntime,
    signal: &crate::model::HydraCallSignalProjection,
) -> Result<(), String> {
    let profile = runtime.profile_ref.borrow().clone();
    let signal_id = random_id()?;
    let sent = crate::controllers::call::send_call_signal(
        &profile,
        &runtime.password,
        &signal.peer_address,
        &signal_id,
        &signal.call_id,
        "decline",
    )
    .await?;
    publish_wallet_progress(runtime, sent.public);
    Ok(())
}

pub(super) async fn send_signed_action(
    runtime: &CallRuntime,
    call_id: &str,
    action: &str,
) -> Result<(), String> {
    let profile = runtime.profile_ref.borrow().clone();
    let call = runtime
        .call_manager
        .borrow()
        .get(call_id)
        .cloned()
        .ok_or_else(|| "Call no longer exists.".to_string())?;
    let signal_id = random_id()?;
    let sent = crate::controllers::call::send_call_signal(
        &profile,
        &runtime.password,
        &call.peer_address,
        &signal_id,
        call_id,
        action,
    )
    .await?;
    publish_wallet_progress(runtime, sent.public);
    Ok(())
}

pub(super) fn publish_wallet_progress(
    runtime: &CallRuntime,
    public: crate::model::WalletProjection,
) {
    let before = runtime.profile_ref.borrow().clone();
    if let Some((profile, patch)) =
        crate::controllers::account::apply_wallet_progress(&before, public)
    {
        publish_profile(runtime, profile, patch);
    }
}

pub(super) fn accept_callback(runtime: CallRuntime) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let Some(call) = active_call(&runtime) else {
            return;
        };
        if call.phase != CallPhase::IncomingRinging {
            return;
        }
        let chat_id = match ensure_accepted_call_chat(&runtime, &call) {
            Ok(chat_id) => chat_id,
            Err(error) => {
                runtime.on_error.emit(error);
                return;
            }
        };
        let result = apply_call_command(&runtime, |calls| {
            crate::controllers::call::attach_chat(calls, &call.call_id, chat_id.clone())?;
            crate::controllers::call::transition(calls, &call.call_id, CallEvent::AcceptLocal)
        });
        if let Err(error) = result {
            runtime.on_error.emit(error);
            return;
        }
        runtime.on_select.emit(chat_id);
        let runtime = runtime.clone();
        let call_id = call.call_id.clone();
        spawn_local(wait_for_accept_delivery(runtime, call_id));
    })
}

async fn wait_for_accept_delivery(runtime: CallRuntime, call_id: String) {
    if let Err(error) = send_signed_action(&runtime, &call_id, "accept").await {
        fail_runtime_call(
            &runtime,
            &call_id,
            format!("Call acceptance could not be delivered: {error}"),
        );
        return;
    }
    TimeoutFuture::new(30_000).await;
    let still_answering = runtime
        .call_manager
        .borrow()
        .get(&call_id)
        .is_some_and(|call| call.phase == CallPhase::Answering);
    if still_answering {
        fail_runtime_call(
            &runtime,
            &call_id,
            "Secure call transport timed out after acceptance.".into(),
        );
    }
}

fn ensure_accepted_call_chat(runtime: &CallRuntime, call: &CallRecord) -> Result<String, String> {
    let profile = runtime.profile_ref.borrow().clone();
    validate_call_peer(&profile, call)?;
    let binding = crate::controllers::call::ensure_chat(&profile, call)?;
    if let (Some(profile), Some(patch)) = (binding.profile, binding.patch) {
        publish_profile(runtime, profile, patch);
    }
    Ok(binding.chat_id)
}

fn validate_call_peer(profile: &crate::model::Profile, call: &CallRecord) -> Result<(), String> {
    if let Some(chat_id) = call.chat_id.as_deref() {
        let matches = profile
            .chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .is_some_and(|chat| call_peer_matches_chat(chat, call));
        if !matches {
            return Err("Call peer does not match the attached secure chat.".into());
        }
    }
    let conflicting_contact = profile.contacts.iter().any(|contact| {
        contact
            .kaspa_address()
            .eq_ignore_ascii_case(&call.peer_address)
            && !call_peer_matches_contact(contact, call)
    });
    if conflicting_contact {
        return Err("Call peer identity conflicts with the stored contact binding.".into());
    }
    Ok(())
}

pub(super) fn process_live_packet(
    envelope: &RealtimeControl,
    packet: LiveVoicePacket,
    runtime: &CallRuntime,
) -> bool {
    if packet.kind == "request" {
        return process_transport_request(envelope, packet, runtime);
    }
    let Some(call) = active_call(runtime) else {
        return true;
    };
    if call.call_id != packet.call_id {
        return true;
    }
    if call.chat_id.as_deref() != Some(envelope.chat_id.as_str()) {
        return true;
    }
    apply_live_packet(runtime, &call, &packet);
    true
}

fn apply_live_packet(runtime: &CallRuntime, call: &CallRecord, packet: &LiveVoicePacket) {
    match packet.kind.as_str() {
        "accept" => accept_transport(runtime, call),
        "decline" => fail_runtime_call(runtime, &call.call_id, "Call declined.".into()),
        "hangup" => end_runtime_call(runtime, &call.call_id, CallEvent::Hangup),
        "audio" => media::play_audio(runtime, call, packet),
        _ => {}
    }
}

fn process_transport_request(
    envelope: &RealtimeControl,
    packet: LiveVoicePacket,
    runtime: &CallRuntime,
) -> bool {
    let Some(call) = active_call(runtime) else {
        decline_competing_transport(envelope, &packet, &runtime.realtime);
        return true;
    };
    if call.call_id != packet.call_id || call.phase != CallPhase::Answering {
        decline_competing_transport(envelope, &packet, &runtime.realtime);
        return true;
    }
    if call.chat_id.as_deref() != Some(envelope.chat_id.as_str()) {
        let chat_id = envelope.chat_id.clone();
        if let Err(error) = apply_call_command(runtime, |calls| {
            crate::controllers::call::attach_chat(calls, &call.call_id, chat_id)
        }) {
            runtime.on_error.emit(error);
            return true;
        }
    }
    let Some(chat) = runtime
        .profile_ref
        .borrow()
        .chats
        .iter()
        .find(|chat| chat.id == envelope.chat_id)
        .cloned()
    else {
        return false;
    };
    if pending_bootstrap_request(&chat, &call.call_id).is_some() {
        accept_bootstrap_request(call, chat, runtime.clone());
    } else {
        send_encrypted_accept(call, chat, runtime.clone());
    }
    true
}

pub(super) fn send_encrypted_control(
    realtime: &RealtimeSender,
    chat_id: &str,
    call_id: &str,
    kind: &str,
    on_complete: Option<Callback<Result<(), String>>>,
) -> bool {
    let Ok(body) = live_voice::encode_live(&LiveVoicePacket::new(call_id.to_string(), kind)) else {
        return false;
    };
    realtime.enqueue_control(chat_id.to_string(), body, on_complete);
    true
}
