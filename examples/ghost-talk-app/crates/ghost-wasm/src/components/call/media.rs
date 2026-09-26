use ghost_domain::call::CallEvent;
use ghost_talk_wasm::BrowserVoiceSender;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    live_voice::{self, LiveVoicePacket},
    model::{CallPhase, CallRecord, Chat},
};

use super::{active_call, apply_call_command, fail_runtime_call, p2p_route, CallRuntime};

pub(super) fn start_connected_media(runtime: CallRuntime, chat: Chat) {
    p2p_route::start_session(runtime.clone(), chat);
    spawn_local(async move {
        if let Err(error) = start_capture(runtime.clone()).await {
            if let Some(call) = active_call(&runtime) {
                if let Some(chat_id) = call.chat_id.as_deref() {
                    let Ok(body) = live_voice::encode_live(&LiveVoicePacket::new(
                        call.call_id.clone(),
                        "hangup",
                    )) else {
                        fail_runtime_call(&runtime, &call.call_id, error);
                        return;
                    };
                    runtime
                        .realtime
                        .enqueue_control(chat_id.to_string(), body, None);
                }
                fail_runtime_call(&runtime, &call.call_id, error);
            }
        }
    });
}

async fn start_capture(runtime: CallRuntime) -> Result<(), String> {
    // Bind first: an `if let` scrutinee's `borrow()` would otherwise stay alive
    // through the `else` branch and make its `borrow_mut()` panic.
    let existing = runtime.sender_ref.borrow().clone();
    let sender = if let Some(existing) = existing {
        existing
    } else {
        let created = BrowserVoiceSender::new()?;
        *runtime.sender_ref.borrow_mut() = Some(created.clone());
        created
    };
    sender
        .start(move |encoded| {
            let Some(call) = active_call(&runtime) else {
                return;
            };
            if call.phase != CallPhase::Connected {
                return;
            }
            let Some(chat_id) = call.chat_id.clone() else {
                return;
            };
            let packet = LiveVoicePacket::audio(call.call_id.clone(), &encoded);
            let Ok(body) = live_voice::encode_live(&packet) else {
                return;
            };
            runtime.realtime.enqueue(chat_id, body);
        })
        .await
}

pub(super) fn play_audio(runtime: &CallRuntime, call: &CallRecord, packet: &LiveVoicePacket) {
    if call.phase != CallPhase::Connected {
        return;
    }
    let Some(bytes) = packet.audio_bytes() else {
        return;
    };
    if let Err(error) = runtime.receiver.borrow().receive_encoded(&bytes) {
        runtime.on_error.emit(error);
    }
}

pub(super) fn toggle_mute_callback(
    runtime: CallRuntime,
    rendered_call: Option<CallRecord>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let Some(rendered_call) = rendered_call.clone() else {
            return;
        };
        let Some(call) = runtime
            .call_manager
            .borrow()
            .get(&rendered_call.call_id)
            .cloned()
        else {
            runtime
                .render_epoch
                .set((*runtime.render_epoch).wrapping_add(1));
            return;
        };
        if call.phase != CallPhase::Connected {
            runtime
                .render_epoch
                .set((*runtime.render_epoch).wrapping_add(1));
            return;
        }
        let next_muted = !call.muted;
        if let Err(error) = apply_call_command(&runtime, |calls| {
            crate::controllers::call::transition(
                calls,
                &call.call_id,
                CallEvent::SetMuted(next_muted),
            )
        }) {
            runtime.on_error.emit(error);
            return;
        }
        // The UI state is already committed. Browser track mutation is
        // process-local best effort and cannot roll the call state back.
        if let Some(sender) = runtime.sender_ref.borrow().as_ref() {
            sender.set_muted(next_muted);
        }
    })
}
