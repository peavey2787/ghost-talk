use base64::Engine as _;
use ghost_api::HydraMailboxResult;

use super::super::{
    handshake_admission::reset_hydra_peer_crypto,
    runtime_owner::HydraRuntimeState,
    runtime_session_queries::clear_peer_handshake_state,
    session_state::retire_kktp_binding,
    session_types::{HydraIncomingRequestProjection, State, BASE64},
    transport_persistence::persist_transport_state,
};

#[tauri::command]
pub async fn hydra_open_realtime(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    carrier_b64: String,
) -> Result<HydraMailboxResult, String> {
    let carrier = BASE64
        .decode(carrier_b64.as_bytes())
        .map_err(|_| "GTR1 realtime carrier is not valid base64".to_string())?;
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    super::super::transport::inbound::handle_realtime_carrier(&mut runtime, &carrier)
}

#[tauri::command]
pub async fn hydra_leave_peer(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    contact_id: String,
    expected_session_sid: Option<String>,
) -> Result<(), String> {
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    if stale_leave_cleanup(&runtime, &contact_id, expected_session_sid.as_deref()) {
        return Ok(());
    }
    reset_hydra_peer_crypto(&mut runtime, &contact_id)?;
    retire_kktp_binding(&mut runtime, &contact_id);
    runtime.blocked_peers.insert(contact_id.clone());
    clear_peer_handshake_state(&mut runtime, &contact_id);
    persist_transport_state(&runtime)?;
    Ok(())
}

fn stale_leave_cleanup(
    runtime: &super::super::runtime_state::HydraProfileRuntime,
    contact_id: &str,
    expected_sid: Option<&str>,
) -> bool {
    let Some(expected_sid) = expected_sid else {
        return false;
    };
    let current = runtime
        .kktp_sessions
        .get(contact_id)
        .is_some_and(|binding| binding.sid == expected_sid)
        || runtime
            .peer_routes
            .get(contact_id)
            .and_then(|route| route.session_sid())
            == Some(expected_sid);
    if !current {
        crate::debug_log::record(
            "info",
            "handshake",
            "stale-leave-cleanup-ignored",
            format!("peer={contact_id} expected_sid={expected_sid}"),
        );
    }
    !current
}

#[tauri::command]
pub async fn hydra_rejoin_peer(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    contact_id: String,
) -> Result<(), String> {
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    // Rejoin is a fresh logical session boundary. Do not resurrect a FINISH,
    // ANSWER, or recovery carrier cached by the thread the user explicitly left.
    reset_hydra_peer_crypto(&mut runtime, &contact_id)?;
    retire_kktp_binding(&mut runtime, &contact_id);
    clear_peer_handshake_state(&mut runtime, &contact_id);
    if let Some(route) = runtime.peer_routes.get_mut(&contact_id) {
        route.session_sid = None;
        route.session_role = None;
        route.resume_required = false;
    }
    runtime.blocked_peers.remove(&contact_id);
    persist_transport_state(&runtime)?;
    Ok(())
}

#[tauri::command]
pub fn hydra_preview_contact_request(
    envelope_hex: String,
    local_kaspa_addresses: Vec<String>,
) -> Result<Option<HydraIncomingRequestProjection>, String> {
    ghost_kaspa::preview_contact_request(&envelope_hex, &local_kaspa_addresses)
}
