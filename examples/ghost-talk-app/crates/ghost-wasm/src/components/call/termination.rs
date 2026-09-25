use ghost_domain::call::CallEvent;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::model::{CallDirection, CallPhase, CallRecord};

use super::signaling::send_signed_action;
use super::{apply_call_command, end_runtime_call, signaling, visible_call, CallRuntime};

pub(super) fn end_callback(
    runtime: CallRuntime,
    rendered_call: Option<CallRecord>,
    requested: CallEvent,
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
            // The modal was rendered from an older snapshot. Force it to
            // reconcile instead of silently turning the click into a no-op.
            runtime
                .render_epoch
                .set((*runtime.render_epoch).wrapping_add(1));
            return;
        };
        if !call.phase.is_active() {
            runtime
                .render_epoch
                .set((*runtime.render_epoch).wrapping_add(1));
            return;
        }
        let event = if call.phase == CallPhase::IncomingRinging && requested == CallEvent::Decline {
            CallEvent::Decline
        } else {
            CallEvent::Hangup
        };

        // Ending locally is never conditional on the network or media cleanup.
        // Commit Ended first so the modal disappears and capture is no longer
        // considered live even if every remote signaling path is unavailable.
        end_runtime_call(&runtime, &call.call_id, event);

        if call.phase == CallPhase::Connected {
            send_connected_hangup(&runtime, &call);
            return;
        }
        if let Some(action) = standalone_end_action(&call, requested) {
            spawn_signed_end(runtime.clone(), call.call_id.clone(), action);
        }
    })
}

fn send_connected_hangup(runtime: &CallRuntime, call: &CallRecord) {
    if let Some(chat_id) = call.chat_id.as_deref() {
        // Remove queued media before the terminal control packet. The HYDRA
        // control is the normal in-session path.
        runtime.realtime.discard_realtime_for_chat(chat_id);
        let _ = signaling::send_encrypted_control(
            &runtime.realtime,
            chat_id,
            &call.call_id,
            "hangup",
            None,
        );
    }

    // Also publish the already-supported signed `cancel` carrier. Remote call
    // state accepts cancel from every active phase, so this is a durable
    // out-of-band fallback when the encrypted realtime/session path is wedged.
    // Duplicate termination is intentionally idempotent at the receiver.
    spawn_signed_end(runtime.clone(), call.call_id.clone(), "cancel");
}

fn spawn_signed_end(runtime: CallRuntime, call_id: String, action: &'static str) {
    spawn_local(async move {
        if let Err(error) = send_signed_action(&runtime, &call_id, action).await {
            runtime.on_error.emit(format!(
                "Call ended locally; remote {action} signal failed: {error}"
            ));
        }
    });
}

fn standalone_end_action(call: &CallRecord, requested: CallEvent) -> Option<&'static str> {
    match (call.phase, call.direction, requested) {
        (CallPhase::IncomingRinging, _, CallEvent::Decline) => Some("decline"),
        (CallPhase::Answering, _, _) => Some("cancel"),
        (CallPhase::OutgoingRinging | CallPhase::Connecting, CallDirection::Outgoing, _) => {
            Some("cancel")
        }
        _ => None,
    }
}

pub(super) fn close_error_callback(runtime: CallRuntime) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let Some(call) = visible_call(&runtime) else {
            return;
        };
        if call.phase != CallPhase::Failed {
            return;
        }
        if let Err(error) = apply_call_command(&runtime, |calls| {
            crate::controllers::call::transition(calls, &call.call_id, CallEvent::Close)
        }) {
            runtime.on_error.emit(error);
        }
    })
}
