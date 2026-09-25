use super::{
    apply_call_command, close_media, signaling, spawn_local, CallEvent, CallRuntime,
    ContactService, PeerBinding, Profile,
};

pub(super) fn process_signed_call_signal(
    projection: crate::model::HydraCallSignalProjection,
    runtime: &CallRuntime,
) {
    let signal = call_signal(runtime, &projection);
    match crate::controllers::call::receive_signal(&mut runtime.call_manager.borrow_mut(), &signal)
    {
        ghost_domain::call::SignalDisposition::Applied => {
            apply_remote_call_action(runtime, &projection)
        }
        ghost_domain::call::SignalDisposition::Busy => {
            send_busy_for_competing_call(runtime, projection)
        }
        ghost_domain::call::SignalDisposition::Duplicate
        | ghost_domain::call::SignalDisposition::Ignored => {}
    }
}

fn call_signal(
    runtime: &CallRuntime,
    projection: &crate::model::HydraCallSignalProjection,
) -> ghost_domain::call::CallSignal {
    ghost_domain::call::CallSignal {
        call_id: projection.call_id.clone(),
        action: projection.action.clone(),
        peer_address: projection.peer_address.clone(),
        peer_hydra_id: projection.peer_hydra_id.clone(),
        peer_label: authenticated_call_label(&runtime.profile_ref.borrow(), projection),
    }
}

fn apply_remote_call_action(
    runtime: &CallRuntime,
    projection: &crate::model::HydraCallSignalProjection,
) {
    match projection.action.as_str() {
        "cancel" | "hangup" => {
            close_media(runtime);
            let _ = apply_call_command(runtime, |calls| {
                crate::controllers::call::transition(
                    calls,
                    &projection.call_id,
                    CallEvent::CompleteEnd,
                )
            });
        }
        "decline" => {
            close_media(runtime);
            bump_render_epoch(runtime);
        }
        _ => bump_render_epoch(runtime),
    }
}

fn bump_render_epoch(runtime: &CallRuntime) {
    runtime
        .render_epoch
        .set((*runtime.render_epoch).wrapping_add(1));
}

fn send_busy_for_competing_call(
    runtime: &CallRuntime,
    projection: crate::model::HydraCallSignalProjection,
) {
    let runtime = runtime.clone();
    spawn_local(async move {
        if let Err(error) = signaling::send_busy_response(&runtime, &projection).await {
            runtime
                .on_error
                .emit(format!("Unable to decline competing call: {error}"));
        }
    });
}

fn authenticated_call_label(
    profile: &Profile,
    signal: &crate::model::HydraCallSignalProjection,
) -> String {
    PeerBinding::new(signal.peer_address.clone(), signal.peer_hydra_id.clone())
        .ok()
        .and_then(|peer| ContactService::resolve_peer(&profile.contacts, &peer))
        .map(|contact| contact.label.clone())
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| signal.peer_address.clone())
}
