use super::{
    lifecycle::{apply_registered_route, validate_registered_route},
    runtime_owner::HydraRuntimeState,
};
use crate::hydra_commands::session_types::{PeerRouteRegistration, State};

#[tauri::command]
pub async fn hydra_register_peer_routes(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    identity_id: String,
    routes: Vec<PeerRouteRegistration>,
) -> Result<(), String> {
    if routes.len() > 4096 {
        return Err("too many Ghost Talk peer routes".into());
    }
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    for route in routes {
        let session_role = validate_registered_route(&route)?;
        apply_registered_route(&mut runtime, route, session_role)?;
    }
    Ok(())
}
