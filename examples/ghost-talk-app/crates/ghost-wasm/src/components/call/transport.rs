use crate::model::CallPhase;
use gloo_timers::future::TimeoutFuture;

use crate::{
    live_voice::{self, LiveVoicePacket},
    model::Chat,
    random_id,
};

use super::{
    apply_call_command, call_chat, fail_runtime_call, publish_profile, signaling, CallRuntime,
};

pub(super) async fn establish_secure_transport(runtime: CallRuntime, call_id: String) {
    let Some(call) = runtime.call_manager.borrow().get(&call_id).cloned() else {
        return;
    };
    let Some(chat) = call_chat(&runtime, &call) else {
        fail_runtime_call(&runtime, &call_id, "Call chat no longer exists.".into());
        return;
    };
    if let Err(error) = send_or_bootstrap_call_request(&runtime, &chat, &call_id).await {
        fail_runtime_call(
            &runtime,
            &call_id,
            format!("Secure call transport could not be established: {error}"),
        );
        return;
    }
    TimeoutFuture::new(30_000).await;
    let still_connecting = runtime
        .call_manager
        .borrow()
        .get(&call_id)
        .is_some_and(|call| call.phase == CallPhase::Connecting);
    if still_connecting {
        let _ = signaling::send_signed_action(&runtime, &call_id, "cancel").await;
        fail_runtime_call(
            &runtime,
            &call_id,
            "Secure call transport timed out.".into(),
        );
    }
}

async fn send_or_bootstrap_call_request(
    runtime: &CallRuntime,
    chat: &Chat,
    call_id: &str,
) -> Result<(), String> {
    let destination = chat
        .peer_kaspa_address()
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "This chat has no Kaspa destination.".to_string())?;
    let Some(peer) = chat.peer_hydra_handle().map(str::to_owned) else {
        return send_call_contact_bootstrap(runtime, chat, call_id, &destination).await;
    };
    let profile_id = runtime.profile_ref.borrow().id.clone();
    match crate::controllers::call::peer_session_binding(&profile_id, &peer).await {
        Ok(Some(binding)) if !binding.restart_resumable => {
            return send_call_contact_bootstrap(runtime, chat, call_id, &destination).await;
        }
        Ok(_) => {}
        Err(error) => {
            return Err(format!(
                "Unable to inspect the secure call transport: {error}"
            ))
        }
    }
    send_existing_transport_request(runtime, chat, call_id, &peer, &destination).await
}

async fn send_existing_transport_request(
    runtime: &CallRuntime,
    chat: &Chat,
    call_id: &str,
    peer: &str,
    destination: &str,
) -> Result<(), String> {
    let body = live_voice::encode_live(&LiveVoicePacket::new(call_id.to_string(), "request"))?;
    let message_id = random_id()?;
    let mut wait_ms = 0u32;
    loop {
        ensure_call_connecting(runtime, call_id)?;
        match send_call_request_once(runtime, chat, peer, destination, &body, &message_id).await {
            Ok(()) => return Ok(()),
            Err(error) => match classify_transport_error(&error, wait_ms) {
                TransportRetry::Bootstrap => {
                    return send_call_contact_bootstrap(runtime, chat, call_id, destination).await;
                }
                TransportRetry::Wait => {
                    TimeoutFuture::new(250).await;
                    wait_ms += 250;
                }
                TransportRetry::Fail => return Err(error),
            },
        }
    }
}

fn ensure_call_connecting(runtime: &CallRuntime, call_id: &str) -> Result<(), String> {
    if call_is_connecting(runtime, call_id) {
        Ok(())
    } else {
        Err("Call was cancelled.".into())
    }
}

async fn send_call_request_once(
    runtime: &CallRuntime,
    chat: &Chat,
    peer: &str,
    destination: &str,
    body: &str,
    message_id: &str,
) -> Result<(), String> {
    let profile = runtime.profile_ref.borrow().clone();
    let sent = crate::controllers::call::send_call_control_message(
        &profile,
        &runtime.password,
        peer,
        destination,
        body,
        message_id,
    )
    .await?;
    sync_call_binding(runtime, &chat.id, peer, sent.pending_handshake).await;
    signaling::publish_wallet_progress(runtime, sent.public);
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransportRetry {
    Bootstrap,
    Wait,
    Fail,
}

fn classify_transport_error(error: &str, wait_ms: u32) -> TransportRetry {
    if requires_contact_bootstrap(error)
        || (waits_for_existing_transport(error) && wait_ms >= 15_000)
    {
        TransportRetry::Bootstrap
    } else if waits_for_existing_transport(error) {
        TransportRetry::Wait
    } else {
        TransportRetry::Fail
    }
}

fn call_is_connecting(runtime: &CallRuntime, call_id: &str) -> bool {
    runtime
        .call_manager
        .borrow()
        .get(call_id)
        .is_some_and(|call| call.phase == CallPhase::Connecting)
}

fn waits_for_existing_transport(error: &str) -> bool {
    matches!(
        error,
        "The secure session is still being established for a previous message"
            | "A secure-session handshake is already pending for this peer; retry after it completes"
            | "The secure-session FINISH is still awaiting the peer's signed acknowledgement; wait for handshake completion before sending another message"
            | "The secure-session recovery FINISH is still awaiting Kaspa broadcast; wait for handshake recovery before sending another message"
            | "Waiting for the chat initiator's authenticated PQ handshake"
    )
}

fn requires_contact_bootstrap(error: &str) -> bool {
    matches!(
        error,
        "The recipient has not accepted this Ghost Talk chat yet"
            | "KKTP message names an unknown HYDRA peer"
            | "Realtime chat has no authenticated HYDRA peer."
            | "Waiting for the original chat initiator to restore the encrypted peer transport after restart"
            | "Secure peer transport is restoring after restart; no new handshake was started"
    ) || error.starts_with("restart restore is missing the persisted KKTP ")
}

async fn send_call_contact_bootstrap(
    runtime: &CallRuntime,
    chat: &Chat,
    call_id: &str,
    destination: &str,
) -> Result<(), String> {
    let request_id = random_id()?;
    apply_call_command(runtime, |calls| {
        crate::controllers::call::set_bootstrap_request(calls, call_id, Some(request_id.clone()))
    })?;
    let before = runtime.profile_ref.borrow().clone();
    let (profile, patch) =
        crate::controllers::call::prepare_bootstrap(&before, &chat.id, &request_id)?;
    publish_profile(runtime, profile.clone(), patch);
    match crate::controllers::call::send_call_contact_request(
        &profile,
        &runtime.password,
        destination,
        &request_id,
        call_id,
    )
    .await
    {
        Ok(sent) => {
            signaling::publish_wallet_progress(runtime, sent.public);
            Ok(())
        }
        Err(error) => {
            let _ = apply_call_command(runtime, |calls| {
                crate::controllers::call::set_bootstrap_request(calls, call_id, None)
            });
            Err(error)
        }
    }
}

async fn sync_call_binding(
    runtime: &CallRuntime,
    chat_id: &str,
    peer: &str,
    pending_handshake: bool,
) {
    let profile_id = runtime.profile_ref.borrow().id.clone();
    let Ok(Some(binding)) = crate::controllers::call::peer_session_binding(&profile_id, peer).await
    else {
        return;
    };
    let before = runtime.profile_ref.borrow().clone();
    if let Some((profile, patch)) = crate::controllers::call::apply_session_binding(
        &before,
        chat_id,
        binding,
        pending_handshake,
    ) {
        publish_profile(runtime, profile, patch);
    }
}

pub(super) async fn complete_contact_bootstrap(
    runtime: CallRuntime,
    accepted: crate::model::HydraContactAcceptedProjection,
) -> Result<(), String> {
    let call = runtime
        .call_manager
        .borrow()
        .find_by_bootstrap_request(&accepted.request_id)
        .cloned()
        .ok_or_else(|| "Call bootstrap is no longer active.".to_string())?;
    let chat_id = call
        .chat_id
        .clone()
        .ok_or_else(|| "Call bootstrap has no chat.".to_string())?;

    let before = runtime.profile_ref.borrow().clone();
    let body = live_voice::encode_live(&LiveVoicePacket::new(call.call_id.clone(), "request"))?;
    let message_id = random_id()?;
    let (profile, patch) = crate::controllers::call::complete_contact_bootstrap(
        &before,
        &runtime.password,
        &chat_id,
        &accepted,
        &body,
        &message_id,
    )
    .await?;
    publish_profile(&runtime, profile, patch);
    apply_call_command(&runtime, |calls| {
        crate::controllers::call::set_bootstrap_request(calls, &call.call_id, None)
    })?;
    Ok(())
}
