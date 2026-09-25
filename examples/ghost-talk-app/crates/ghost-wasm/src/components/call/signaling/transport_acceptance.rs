use crate::controllers::call::RealtimeSender;
use ghost_domain::call::CallEvent;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    live_voice::LiveVoicePacket,
    model::{CallPhase, CallRecord, Chat, RealtimeControl},
};

use super::super::{
    apply_call_command, call_chat, fail_runtime_call, media, publish_profile, CallRuntime,
};
use super::send_encrypted_control;

pub(super) fn pending_bootstrap_request<'a>(
    chat: &'a Chat,
    call_id: &str,
) -> Option<&'a crate::model::IncomingRequest> {
    chat.incoming_request()
        .filter(|request| request.state == "pending" && request.call_id.as_deref() == Some(call_id))
}

pub(super) fn accept_bootstrap_request(call: CallRecord, chat: Chat, runtime: CallRuntime) {
    let Some(request) = pending_bootstrap_request(&chat, &call.call_id).cloned() else {
        fail_runtime_call(
            &runtime,
            &call.call_id,
            "Incoming call bootstrap is no longer available.".into(),
        );
        return;
    };
    spawn_local(async move {
        let snapshot = runtime.profile_ref.borrow().clone();
        match crate::controllers::call::accept_bootstrap(
            &snapshot,
            &runtime.password,
            &chat,
            &request,
        )
        .await
        {
            Ok((latest, patch)) => publish_profile(&runtime, latest, patch),
            Err(error) => fail_runtime_call(
                &runtime,
                &call.call_id,
                format!("Call acceptance could not be delivered: {error}"),
            ),
        }
    });
}

pub(super) fn send_encrypted_accept(call: CallRecord, chat: Chat, runtime: CallRuntime) {
    let completion_runtime = runtime.clone();
    let completion_call = call.clone();
    let completion_chat = chat.clone();
    let completion = Callback::from(move |result: Result<(), String>| match result {
        Err(error) => fail_runtime_call(
            &completion_runtime,
            &completion_call.call_id,
            format!("Call acceptance could not be delivered: {error}"),
        ),
        Ok(()) => {
            if let Err(error) = apply_call_command(&completion_runtime, |calls| {
                crate::controllers::call::transition(
                    calls,
                    &completion_call.call_id,
                    CallEvent::TransportConnected,
                )
            }) {
                completion_runtime.on_error.emit(error);
                return;
            }
            media::start_connected_media(completion_runtime.clone(), completion_chat.clone());
        }
    });
    if !send_encrypted_control(
        &runtime.realtime,
        &chat.id,
        &call.call_id,
        "accept",
        Some(completion),
    ) {
        fail_runtime_call(
            &runtime,
            &call.call_id,
            "Unable to encode call acceptance.".into(),
        );
    }
}

pub(super) fn accept_transport(runtime: &CallRuntime, call: &CallRecord) {
    if call.phase != CallPhase::Connecting {
        return;
    }
    if let Err(error) = apply_call_command(runtime, |calls| {
        crate::controllers::call::transition(calls, &call.call_id, CallEvent::TransportConnected)
    }) {
        runtime.on_error.emit(error);
        return;
    }
    let Some(chat) = call_chat(runtime, call) else {
        return;
    };
    media::start_connected_media(runtime.clone(), chat);
}

pub(super) fn decline_competing_transport(
    envelope: &RealtimeControl,
    packet: &LiveVoicePacket,
    realtime: &RealtimeSender,
) {
    let _ = send_encrypted_control(
        realtime,
        &envelope.chat_id,
        &packet.call_id,
        "decline",
        None,
    );
}
