use super::super::{
    runtime_state::HydraProfileRuntime,
    session_types::{AsyncMutex, HydraFacade, Serialize, State, Zeroizing},
    transport_persistence::{persist_transport_state, transport_state_path},
    transport_restore::load_transport_state,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Debug, Serialize)]
pub struct HydraDebugSession {
    pub peer_hydra_id: String,
    pub sid: String,
    pub role: String,
    pub state: String,
    pub hydra_status: String,
    pub mailbox_id: String,
    pub send_seq: u64,
    pub recv_next_seq: u64,
    pub peer_kaspa_address: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct HydraDebugState {
    pub available: bool,
    pub identity_id: Option<String>,
    pub sessions: Vec<HydraDebugSession>,
    pub pending_outbound: Option<String>,
    pub prepared_completion: Option<String>,
    pub pending_inbound: Option<String>,
    pub pending_recovery: Option<String>,
    pub prepared_recovery_finish: Option<String>,
    pub retained_delivery_count: usize,
    pub retired_sid_count: usize,
}

pub(crate) fn debug_pending_summary<I>(entries: I) -> Option<String>
where
    I: IntoIterator<Item = String>,
{
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort();
    (!entries.is_empty()).then(|| entries.join(" | "))
}

fn debug_sessions(runtime: &HydraProfileRuntime) -> Vec<HydraDebugSession> {
    let mut sessions = runtime
        .kktp_sessions
        .values()
        .map(|binding| HydraDebugSession {
            peer_hydra_id: binding.peer_hydra_id.clone(),
            sid: binding.sid.clone(),
            role: format!("{:?}", binding.role),
            state: format!("{:?}", binding.state),
            hydra_status: runtime
                .hydra
                .session_status(&binding.peer_hydra_id)
                .unwrap_or_else(|error| format!("error: {error}")),
            mailbox_id: binding.mailbox_id.clone(),
            send_seq: binding.send_seq,
            recv_next_seq: binding.recv_next_seq,
            peer_kaspa_address: runtime
                .peer_routes
                .get(&binding.peer_hydra_id)
                .map(|route| route.kaspa_address.clone()),
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| left.peer_hydra_id.cmp(&right.peer_hydra_id));
    sessions
}

fn available_debug_state(runtime: &HydraProfileRuntime) -> HydraDebugState {
    HydraDebugState {
        available: true,
        identity_id: Some(runtime.identity_id.clone()),
        sessions: debug_sessions(runtime),
        pending_outbound: debug_pending_summary(runtime.pending_outbound.values().map(|pending| {
            format!(
                "sid={} peer={} pending={} message={}",
                pending.sid, pending.contact_id, pending.id, pending.message_id
            )
        })),
        prepared_completion: debug_pending_summary(runtime.prepared_completion.values().map(
            |pending| {
                format!(
                    "sid={} peer={} pending={} message={}",
                    pending.sid, pending.contact_id, pending.pending_id, pending.message_id
                )
            },
        )),
        pending_inbound: debug_pending_summary(
            runtime
                .pending_inbound
                .values()
                .map(|pending| format!("sid={} peer={}", pending.sid, pending.contact_id)),
        ),
        pending_recovery: debug_pending_summary(runtime.pending_recovery.values().map(|pending| {
            format!(
                "sid={} peer={} broadcast={}",
                pending.sid, pending.contact_id, pending.offer_broadcast
            )
        })),
        prepared_recovery_finish: debug_pending_summary(
            runtime
                .prepared_recovery_finish
                .values()
                .map(|pending| format!("sid={} peer={}", pending.sid, pending.contact_id)),
        ),
        retained_delivery_count: runtime.prepared_kktp_deliveries.len(),
        retired_sid_count: runtime.retired_kktp_sids.len(),
    }
}

#[tauri::command]
pub async fn hydra_debug_state(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
) -> Result<HydraDebugState, String> {
    if !crate::debug_log::is_enabled() {
        return Ok(HydraDebugState::default());
    }
    let Some(runtime) = state.runtime_if_present(&profile_id)? else {
        return Ok(HydraDebugState::default());
    };
    let runtime = runtime.lock().await;
    Ok(available_debug_state(&runtime))
}

#[derive(Default)]
pub struct HydraRuntimeState {
    pub(crate) profiles: Mutex<HashMap<String, Arc<AsyncMutex<HydraProfileRuntime>>>>,
    pub(crate) lifecycle: AsyncMutex<()>,
}

impl HydraRuntimeState {
    pub(crate) fn runtime_if_present(
        &self,
        profile_id: &str,
    ) -> Result<Option<Arc<AsyncMutex<HydraProfileRuntime>>>, String> {
        Ok(self
            .profiles
            .lock()
            .map_err(|_| "HYDRA runtime state is poisoned".to_string())?
            .get(profile_id)
            .cloned())
    }

    pub(crate) fn runtime(
        &self,
        profile_id: &str,
    ) -> Result<Arc<AsyncMutex<HydraProfileRuntime>>, String> {
        self.runtime_if_present(profile_id)?
            .ok_or_else(|| "Unlock this Ghost Talk ID before using encrypted messaging".to_string())
    }

    pub(crate) fn take_runtime(
        &self,
        profile_id: &str,
    ) -> Result<Option<Arc<AsyncMutex<HydraProfileRuntime>>>, String> {
        Ok(self
            .profiles
            .lock()
            .map_err(|_| "HYDRA runtime state is poisoned".to_string())?
            .remove(profile_id))
    }

    pub(crate) fn other_profile_ids(&self, profile_id: &str) -> Result<Vec<String>, String> {
        Ok(self
            .profiles
            .lock()
            .map_err(|_| "HYDRA runtime state is poisoned".to_string())?
            .keys()
            .filter(|id| id.as_str() != profile_id)
            .cloned()
            .collect())
    }

    pub(crate) fn restore_runtime(
        &self,
        profile_id: String,
        runtime: Arc<AsyncMutex<HydraProfileRuntime>>,
    ) -> Result<(), String> {
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| "HYDRA runtime state is poisoned".to_string())?;
        if profiles.contains_key(&profile_id) {
            return Err("HYDRA runtime state already contains this Ghost Talk ID".into());
        }
        profiles.insert(profile_id, runtime);
        Ok(())
    }

    pub(crate) fn install(
        &self,
        profile_id: String,
        identity_id: String,
        hydra: HydraFacade,
        profile_path: PathBuf,
        transport_state_key: [u8; 32],
    ) -> Result<(), String> {
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| "HYDRA runtime state is poisoned".to_string())?;
        if profiles.contains_key(&profile_id) {
            return Err("HYDRA runtime is already managed for this Ghost Talk ID".into());
        }
        if !profiles.is_empty() {
            return Err("another Ghost Talk ID is still closing".into());
        }
        let mut runtime = HydraProfileRuntime {
            identity_id,
            hydra,
            transport_state_path: transport_state_path(&profile_path),
            transport_state_key: Zeroizing::new(transport_state_key),
            pending_outbound: HashMap::new(),
            prepared_completion: HashMap::new(),
            pending_recovery: HashMap::new(),
            prepared_recovery_finish: HashMap::new(),
            pending_inbound: HashMap::new(),
            prepared_kktp_deliveries: HashMap::new(),
            kktp_sessions: HashMap::new(),
            retired_kktp_sids: HashSet::new(),
            allowed_kktp_sids: HashMap::new(),
            pending_contact_request_sids: HashSet::new(),
            peer_routes: HashMap::new(),
            blocked_peers: HashSet::new(),
        };
        load_transport_state(&mut runtime)?;
        profiles.insert(profile_id, Arc::new(AsyncMutex::new(runtime)));
        Ok(())
    }
}

pub(crate) const RUNTIME_RELEASE_RETRIES: usize = 100;

pub(crate) async fn release_runtime(
    runtime: &Arc<AsyncMutex<HydraProfileRuntime>>,
) -> Result<(), String> {
    {
        let mut runtime_guard = runtime.lock().await;
        persist_transport_state(&runtime_guard)?;
        runtime_guard.hydra.lock_active_identity()?;
    }

    for _ in 0..RUNTIME_RELEASE_RETRIES {
        if Arc::strong_count(runtime) == 1 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    Err("Ghost Talk ID is still busy; retry switching IDs in a moment".into())
}

pub(crate) async fn release_other_runtimes(
    state: &HydraRuntimeState,
    profile_id: &str,
) -> Result<(), String> {
    for other_id in state.other_profile_ids(profile_id)? {
        let Some(runtime) = state.take_runtime(&other_id)? else {
            continue;
        };
        if let Err(error) = release_runtime(&runtime).await {
            state.restore_runtime(other_id, runtime)?;
            return Err(error);
        }
        // `runtime` is dropped here only after all in-flight users have released
        // their Arcs, which closes the process-wide native HYDRA profile handle.
    }
    Ok(())
}
