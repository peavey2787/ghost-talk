pub(super) mod video;

use ghost_domain::call::CallEvent;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    live_voice::{self, LiveVoicePacket},
    model::{CallPhase, CallRecord, Chat},
};

use super::{
    active_call, apply_call_command, fail_runtime_call, p2p_route, CallRuntime, VideoControls,
};

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
    runtime
        .voice_sender()?
        .start(move |encoded| {
            if let Some((call_id, chat_id)) = connected_call(&runtime) {
                let packet = LiveVoicePacket::audio(call_id, &encoded);
                if let Ok(body) = live_voice::encode_live(&packet) {
                    runtime.realtime.enqueue(chat_id, body);
                }
            }
        })
        .await
}

/// Camera controls for the connected call.
pub(super) fn video_controls(runtime: &CallRuntime) -> VideoControls {
    video::controls(runtime, send_video_frame)
}

fn send_video_frame(runtime: &CallRuntime, frame: Vec<u8>) {
    if let Some((call_id, chat_id)) = connected_call(runtime) {
        let packet = LiveVoicePacket::video(call_id, &frame);
        if let Ok(body) = live_voice::encode_live(&packet) {
            runtime.realtime.enqueue_video(chat_id, body);
        }
    }
}

/// The connected call's id and chat, when media may flow.
fn connected_call(runtime: &CallRuntime) -> Option<(String, String)> {
    let call = active_call(runtime)?;
    if call.phase != CallPhase::Connected {
        return None;
    }
    Some((call.call_id, call.chat_id?))
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

pub(super) fn show_video(call: &CallRecord, packet: &LiveVoicePacket) {
    if call.phase != CallPhase::Connected {
        return;
    }
    if let Some(frame) = packet.video_bytes() {
        ghost_talk_wasm::show_video_frame(video::CALL_TILE, &frame);
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
