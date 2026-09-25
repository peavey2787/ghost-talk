use crate::validation::require_min_password;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zeroize::Zeroize;

use super::super::{
    contact_identity_lifecycle::{existing_ready, open_hydra_profile},
    mailbox_dispatch::{decode_fixed_hex, profile_path},
    runtime_owner::{release_other_runtimes, release_runtime, HydraRuntimeState},
    runtime_state::HydraProfileRuntime,
    session_types::{
        HydraFacade, HydraReady, HydraSessionBindingProjection, KktpRole, KktpSessionState,
        PeerRoute, PeerRouteRegistration, State,
    },
    transport_persistence::transport_state_key_from_seed,
};

pub(crate) async fn install_opened_runtime(
    state: &HydraRuntimeState,
    profile_id: &str,
    opened: (HydraReady, HydraFacade),
    profile_path: PathBuf,
    transport_state_key: [u8; 32],
) -> Result<HydraReady, String> {
    let (ready, hydra) = opened;
    state.install(
        profile_id.to_owned(),
        ready.identity_id.clone(),
        hydra,
        profile_path,
        transport_state_key,
    )?;
    Ok(ready)
}

pub(crate) fn retry_native_open(error: &str, attempt: usize, limit: usize) -> bool {
    error.contains("native profile is already open") && attempt < limit
}

#[expect(clippy::too_many_arguments, reason = "cohesive runtime-open boundary")]
async fn open_runtime_with_retries(
    state: &HydraRuntimeState,
    profile_id: &str,
    password: &str,
    identity_id: Option<String>,
    path: PathBuf,
    mut identity_seed: [u8; 32],
    mut transport_state_key: [u8; 32],
    started: Instant,
) -> Result<HydraReady, String> {
    const OPEN_RETRIES: usize = 100;
    for attempt in 0..=OPEN_RETRIES {
        if let Some(ready) =
            existing_ready(state, profile_id, identity_id.as_deref(), password).await?
        {
            identity_seed.zeroize();
            transport_state_key.zeroize();
            crate::debug_log::record(
                "info",
                "unlock",
                "hydra-unlock-race-reused",
                format!(
                    "profile={} elapsed_ms={} attempt={}",
                    profile_id,
                    started.elapsed().as_millis(),
                    attempt
                ),
            );
            return Ok(ready);
        }
        let opened = open_hydra_profile(
            path.clone(),
            password.to_owned(),
            identity_id.clone(),
            identity_seed,
        )
        .await;
        match opened {
            Ok(opened) => {
                identity_seed.zeroize();
                let installed = install_opened_runtime(
                    state,
                    profile_id,
                    opened,
                    path.clone(),
                    transport_state_key,
                )
                .await;
                transport_state_key.zeroize();
                let ready = installed?;
                crate::debug_log::record(
                    "info",
                    "unlock",
                    "hydra-unlock-complete",
                    format!(
                        "profile={} elapsed_ms={} attempts={}",
                        profile_id,
                        started.elapsed().as_millis(),
                        attempt + 1
                    ),
                );
                return Ok(ready);
            }
            Err(error) if retry_native_open(&error, attempt, OPEN_RETRIES) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => {
                identity_seed.zeroize();
                transport_state_key.zeroize();
                crate::debug_log::record(
                    "error",
                    "unlock",
                    "hydra-unlock-failed",
                    format!(
                        "profile={} elapsed_ms={} error={}",
                        profile_id,
                        started.elapsed().as_millis(),
                        error
                    ),
                );
                return Err(error);
            }
        }
    }
    identity_seed.zeroize();
    transport_state_key.zeroize();
    Err("Ghost Talk ID is still closing; retry unlock in a moment".into())
}

#[tauri::command]
pub async fn hydra_ensure(
    app: State<'_, crate::NativeAppState>,
    state: State<'_, HydraRuntimeState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: Option<String>,
) -> Result<HydraReady, String> {
    let started = Instant::now();
    let app = app.handle();
    let _lifecycle = state.lifecycle.lock().await;
    if let Some(ready) =
        existing_ready(&state, &profile_id, identity_id.as_deref(), &password).await?
    {
        crate::debug_log::record(
            "info",
            "unlock",
            "hydra-unlock-reused",
            format!(
                "profile={} elapsed_ms={}",
                profile_id,
                started.elapsed().as_millis()
            ),
        );
        return Ok(ready);
    }
    release_other_runtimes(&state, &profile_id).await?;
    require_min_password(&password, "ID")?;
    let identity_seed = wallet_state.hydra_seed_for_profile(&profile_id)?;
    let transport_state_key = transport_state_key_from_seed(&identity_seed);
    let path = profile_path(&app, &profile_id)?;
    open_runtime_with_retries(
        &state,
        &profile_id,
        &password,
        identity_id,
        path,
        identity_seed,
        transport_state_key,
        started,
    )
    .await
}

#[tauri::command]
pub async fn hydra_lock_profile(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
) -> Result<(), String> {
    let _lifecycle = state.lifecycle.lock().await;
    let Some(runtime) = state.take_runtime(&profile_id)? else {
        return Ok(());
    };
    // hydra-msg permits only one native profile handle at a time. Locking just
    // the active identity is insufficient: the HydraFacade itself must be
    // dropped before another ID can open. If an in-flight operation still owns
    // this runtime, put it back into managed state instead of orphaning the
    // native handle where a later unlock could only see "profile already open".
    match release_runtime(&runtime).await {
        Ok(()) => Ok(()),
        Err(error) => {
            state.restore_runtime(profile_id, runtime)?;
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn hydra_peer_session_binding(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    contact_id: String,
) -> Result<Option<HydraSessionBindingProjection>, String> {
    let runtime = state.runtime(&profile_id)?;
    let runtime = runtime.lock().await;
    Ok(runtime.kktp_sessions.get(&contact_id).and_then(|binding| {
        (binding.state == KktpSessionState::Active).then(|| HydraSessionBindingProjection {
            sid: binding.sid.clone(),
            role: match binding.role {
                KktpRole::Initiator => "initiator".into(),
                KktpRole::Responder => "responder".into(),
            },
            restart_resumable: binding.persistent_transport.is_some(),
        })
    }))
}

pub(crate) fn parse_registered_role(
    route: &PeerRouteRegistration,
) -> Result<Option<KktpRole>, String> {
    let Some(label) = route
        .session_role
        .as_deref()
        .filter(|_| route.session_sid.is_some())
    else {
        return Ok(None);
    };
    [
        ("initiator", KktpRole::Initiator),
        ("responder", KktpRole::Responder),
    ]
    .into_iter()
    .find(|(candidate, _)| *candidate == label)
    .map(|(_, role)| Some(role))
    .ok_or_else(|| "registered Ghost Talk session role is invalid".into())
}

pub(crate) fn validate_registered_route(
    route: &PeerRouteRegistration,
) -> Result<Option<KktpRole>, String> {
    decode_fixed_hex::<32>(&route.contact_id, "HYDRA contact id")?;
    ghost_kaspa::validate_destination(&route.kaspa_address)?;
    if let Some(sid) = route.session_sid.as_deref() {
        let valid = sid.len() == 32 && sid.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid {
            return Err(
                "registered Ghost Talk session SID must be exactly 32 hexadecimal characters"
                    .into(),
            );
        }
    }
    parse_registered_role(route)
}

pub(crate) fn apply_registered_route(
    runtime: &mut HydraProfileRuntime,
    route: PeerRouteRegistration,
    session_role: Option<KktpRole>,
) -> Result<(), String> {
    if !runtime.hydra.has_contact(&route.contact_id)? {
        return Ok(());
    }
    if route.active {
        runtime.blocked_peers.remove(&route.contact_id);
    }
    let active_binding = runtime
        .kktp_sessions
        .get(&route.contact_id)
        .filter(|binding| binding.state == KktpSessionState::Active)
        .cloned();
    let native_binding_is_active = active_binding.is_some();
    runtime.peer_routes.insert(
        route.contact_id,
        PeerRoute {
            kaspa_address: route.kaspa_address,
            display_name: route.display_name.chars().take(96).collect(),
            session_sid: active_binding
                .as_ref()
                .map(|binding| binding.sid.clone())
                .or(route.session_sid),
            session_role: active_binding
                .as_ref()
                .map(|binding| binding.role)
                .or(session_role),
            resume_required: route.resume_required && !native_binding_is_active,
        },
    );
    Ok(())
}
