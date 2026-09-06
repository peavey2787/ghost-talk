use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_hydra::{HydraFacade, ReceivedProjection, StegoProfile};
use ghost_protocol::{
    kktp_anchor_type, kktp_mailbox_id, GhostContactDescriptor, KktpDirection, KktpFirstMessage,
    KktpHandshakeControl, KktpInnerMessage, KktpMailboxMessage, KktpSessionEnd, GHOST_KKTP_VERSION,
    KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX,
};
use ghost_kaspa::wallet::WalletPublic;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex as AsyncMutex;
use zeroize::Zeroize;

const MAX_MAILBOX_ENVELOPE_BYTES: usize = ghost_core::MAX_EVENT_BYTES + 64 * 1024;
const MAILBOX_ENVELOPE_MAGIC: &[u8; 4] = b"GTM2";
const REALTIME_MAILBOX_MAGIC: &[u8; 4] = b"GTR1";
const REALTIME_INNER_PREFIX: &str = "\u{1e}GHOST-REALTIME-V1:";
const MAILBOX_HANDSHAKE_MAGIC: &[u8; 4] = b"GTH1";
const HANDSHAKE_OFFER: u8 = 1;
const HANDSHAKE_ANSWER: u8 = 2;
const HANDSHAKE_FINISH_MESSAGE: u8 = 3;
const HANDSHAKE_FINISH_ONLY: u8 = 4;
const MAX_MAILBOX_TRANSACTIONS: usize = ghost_core::MAX_FRAGMENTS;
const MAX_CONTACT_CARD_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct HydraReady {
    pub identity_id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreparedMailbox {
    pub payloads_hex: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HydraDirectEnvelope {
    pub envelope_b64: String,
}


#[derive(Clone, Debug, Serialize)]
pub struct HydraControlProjection {
    pub destination: String,
    pub payloads_hex: Vec<String>,
    pub completes_pending_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HydraRecoveryProjection {
    pub destination: String,
    pub offer_hex: String,
    /// Fresh KKTP SID for this replacement ratchet. The UI persists this only
    /// after the recovery offer is successfully accepted by Kaspa.
    pub sid: String,
    /// Authenticated HYDRA peer whose active conversation is being recovered.
    pub peer_hydra_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HydraIncomingRequestProjection {
    pub request_id: String,
    pub peer_address: String,
    pub local_address: String,
    pub peer_label: String,
    pub peer_hydra_id: String,
    /// Exact signed GTCR bytes, retained as public bootstrap evidence so an
    /// ignored/pending request can be accepted after an app restart without
    /// persisting the sender in HYDRA's bounded contact store first.
    pub signed_request_hex: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HydraContactAcceptedProjection {
    pub request_id: String,
    pub peer_address: String,
    pub acceptor_address: String,
    pub peer_label: String,
    pub peer_hydra_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HydraSessionEndedProjection {
    pub peer_hydra_id: String,
    pub peer_address: String,
    pub sid: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HydraMailboxResult {
    pub received: Option<ReceivedProjection>,
    pub control: Option<HydraControlProjection>,
    pub recovery: Option<HydraRecoveryProjection>,
    pub incoming_request: Option<HydraIncomingRequestProjection>,
    pub contact_accepted: Option<HydraContactAcceptedProjection>,
    pub peer_address: Option<String>,
    pub peer_label: Option<String>,
    pub message_id: Option<String>,
    pub delivery_ack: Option<String>,
    pub delivery_ack_peer: Option<String>,
    pub session_established_peer: Option<String>,
    pub session_ended: Option<HydraSessionEndedProjection>,
    pub discard: bool,
}

#[derive(Clone, Debug)]
pub(super) struct PendingOutbound {
    pub id: String,
    pub sid: String,
    pub contact_id: String,
    pub destination: String,
    pub body: String,
    pub message_id: String,
    pub stego_profile: String,
    pub offer_payloads_hex: Vec<String>,
}


#[derive(Clone, Debug)]
pub(super) struct PendingRecovery {
    pub sid: String,
    pub contact_id: String,
    pub destination: String,
    pub offer_hex: String,
    pub offer_broadcast: bool,
}

#[derive(Clone, Debug)]
pub(super) struct PreparedRecoveryFinish {
    pub sid: String,
    pub contact_id: String,
    pub destination: String,
    pub payloads_hex: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct PendingInbound {
    pub sid: String,
    pub contact_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PeerRouteRegistration {
    pub contact_id: String,
    pub kaspa_address: String,
    pub display_name: String,
    /// Persisted active Ghost Talk conversation SID for this peer, if any.
    pub session_sid: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct PeerRoute {
    pub kaspa_address: String,
    pub display_name: String,
    /// UI-persisted active conversation SID used to reject historical/replayed
    /// KKTP traffic after native volatile session state is rebuilt.
    pub session_sid: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KktpRole {
    Initiator,
    Responder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KktpSessionState {
    Discovered,
    Handshake,
    Active,
    Closed,
}

const MAX_RETIRED_KKTP_SIDS: usize = 4096;
pub(super) const MAX_PREPARED_KKTP_DELIVERIES: usize = 4096;

#[derive(Clone, Debug)]
pub(super) struct KktpSessionBinding {
    pub sid: String,
    pub role: KktpRole,
    pub peer_hydra_id: String,
    pub mailbox_id: String,
    pub send_seq: u64,
    pub recv_next_seq: u64,
    pub state: KktpSessionState,
}

impl KktpSessionBinding {
    fn outbound_direction(&self) -> KktpDirection {
        match self.role {
            KktpRole::Initiator => KktpDirection::AtoB,
            KktpRole::Responder => KktpDirection::BtoA,
        }
    }

    fn inbound_direction(&self) -> KktpDirection {
        self.outbound_direction().opposite()
    }
}


pub(super) fn fresh_kktp_sid() -> String {
    ghost_core::Id128::new_random().to_string()
}

fn make_kktp_binding(
    local_hydra_id: &str,
    peer_hydra_id: &str,
    sid: String,
    role: KktpRole,
    state: KktpSessionState,
) -> Result<KktpSessionBinding, String> {
    let (initiator, responder) = match role {
        KktpRole::Initiator => (local_hydra_id, peer_hydra_id),
        KktpRole::Responder => (peer_hydra_id, local_hydra_id),
    };
    Ok(KktpSessionBinding {
        mailbox_id: kktp_mailbox_id(&sid, initiator, responder)?,
        sid,
        role,
        peer_hydra_id: peer_hydra_id.to_owned(),
        send_seq: 0,
        recv_next_seq: 0,
        state,
    })
}

pub(super) fn install_kktp_binding(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    sid: String,
    role: KktpRole,
    state: KktpSessionState,
) -> Result<(), String> {
    let binding = make_kktp_binding(
        &runtime.identity_id,
        peer_hydra_id,
        sid,
        role,
        state,
    )?;
    crate::debug_log::record(
        "info",
        "handshake",
        "binding-installed",
        format!(
            "peer={} sid={} role={:?} state={:?} mailbox={}",
            peer_hydra_id, binding.sid, binding.role, binding.state, binding.mailbox_id
        ),
    );
    runtime.kktp_sessions.insert(peer_hydra_id.to_owned(), binding);
    Ok(())
}

fn remember_retired_kktp_sid(runtime: &mut HydraProfileRuntime, sid: String) {
    // This is only an in-memory fast replay cache. Durable mailbox packet IDs
    // and persisted chat session SIDs provide the restart-safe boundary, so cap
    // it rather than let a long-running process accumulate unbounded state.
    if runtime.retired_kktp_sids.len() >= MAX_RETIRED_KKTP_SIDS {
        runtime.retired_kktp_sids.clear();
    }
    runtime.retired_kktp_sids.insert(sid);
}

pub(super) fn retire_kktp_binding(runtime: &mut HydraProfileRuntime, peer_hydra_id: &str) {
    if let Some(binding) = runtime.kktp_sessions.remove(peer_hydra_id) {
        crate::debug_log::record(
            "info",
            "handshake",
            "binding-retired",
            format!("peer={} sid={} state={:?}", peer_hydra_id, binding.sid, binding.state),
        );
        remember_retired_kktp_sid(runtime, binding.sid);
    }
}

pub(super) fn prepare_kktp_session_end(
    runtime: &HydraProfileRuntime,
    peer_hydra_id: &str,
    sender_kaspa_address: &str,
    recipient_kaspa_address: &str,
    reason: &str,
) -> Result<KktpSessionEnd, String> {
    if runtime.blocked_peers.contains(peer_hydra_id) {
        return Err("this peer is already closed for the current chat session".into());
    }
    let binding = runtime
        .kktp_sessions
        .get(peer_hydra_id)
        .ok_or_else(|| "there is no KKTP session to end for this peer".to_string())?;
    if binding.state != KktpSessionState::Active
        || runtime.hydra.session_status(peer_hydra_id)? != "active"
    {
        return Err("the KKTP session is not active, so no session_end anchor can be emitted".into());
    }
    let route = runtime
        .peer_routes
        .get(peer_hydra_id)
        .ok_or_else(|| "KKTP session has no verified Kaspa peer route".to_string())?;
    if route.kaspa_address != recipient_kaspa_address {
        return Err("session_end destination does not match the authenticated peer route".into());
    }
    let (initiator_hydra_id, responder_hydra_id) = match binding.role {
        KktpRole::Initiator => (runtime.identity_id.clone(), peer_hydra_id.to_owned()),
        KktpRole::Responder => (peer_hydra_id.to_owned(), runtime.identity_id.clone()),
    };
    let mut end = KktpSessionEnd {
        kind: "session_end".into(),
        version: GHOST_KKTP_VERSION,
        sid: binding.sid.clone(),
        initiator_hydra_id,
        responder_hydra_id,
        sender_hydra_id: runtime.identity_id.clone(),
        sender_kaspa_address: sender_kaspa_address.to_owned(),
        recipient_kaspa_address: recipient_kaspa_address.to_owned(),
        reason: reason.to_owned(),
        pq_sig_b64: String::new(),
        sig: String::new(),
    };
    let signature = runtime
        .hydra
        .sign_application_context(&end.pq_signing_bytes()?)?;
    end.pq_sig_b64 = BASE64.encode(signature);
    Ok(end)
}

pub(super) fn kktp_handshake_payload(
    runtime: &HydraProfileRuntime,
    binding: &KktpSessionBinding,
    stage: &str,
    payload: &[u8],
    first_message: Option<KktpFirstMessage>,
) -> Result<Vec<u8>, String> {
    let (initiator_hydra_id, responder_hydra_id) = match binding.role {
        KktpRole::Initiator => (runtime.identity_id.clone(), binding.peer_hydra_id.clone()),
        KktpRole::Responder => (binding.peer_hydra_id.clone(), runtime.identity_id.clone()),
    };
    let mut control = KktpHandshakeControl {
        kind: "ghost_handshake".into(),
        version: GHOST_KKTP_VERSION,
        sid: binding.sid.clone(),
        stage: stage.into(),
        initiator_hydra_id,
        responder_hydra_id,
        payload_b64: BASE64.encode(payload),
        first_message,
        pq_sig_b64: String::new(),
    };
    let signature = runtime
        .hydra
        .sign_application_context(&control.signing_bytes()?)?;
    control.pq_sig_b64 = BASE64.encode(signature);
    control.encode()
}

fn reset_kktp_after_unsent_advance(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
) -> Result<(), String> {
    if runtime.hydra.has_contact(contact_id)? {
        match runtime.hydra.session_status(contact_id)?.as_str() {
            "active" => runtime.hydra.close_session(contact_id)?,
            "pending" => runtime.hydra.abort_handshake(contact_id)?,
            _ => {}
        }
    }
    retire_kktp_binding(runtime, contact_id);
    clear_peer_handshake_state(runtime, contact_id);
    Ok(())
}

fn seal_kktp_message(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<(KktpMailboxMessage, Vec<u8>), String> {
    let profile = parse_stego_profile(stego_profile)?;
    let binding = runtime
        .kktp_sessions
        .get(contact_id)
        .cloned()
        .ok_or_else(|| "KKTP session binding is missing for this peer".to_string())?;
    if binding.state != KktpSessionState::Active {
        return Err("KKTP session is not active for this peer".into());
    }
    let direction = binding.outbound_direction();
    let seq = binding.send_seq;
    let inner = KktpInnerMessage::text(
        binding.sid.clone(),
        binding.mailbox_id.clone(),
        direction,
        seq,
        message_id.to_owned(),
        body.to_owned(),
    );
    let inner_bytes = inner.encode()?;
    let envelopes = runtime.hydra.send(contact_id, &inner_bytes, profile)?;
    if envelopes.len() != 1 {
        // HYDRA has already advanced its sending ratchet. There is no safe
        // rollback, so retire this volatile session rather than leave KKTP with
        // a sequence number that can never be published.
        reset_kktp_after_unsent_advance(runtime, contact_id)?;
        return Err("HYDRA produced multiple logical envelopes for one KKTP direct message; the secure session was reset before publication".into());
    }
    let envelope = envelopes.into_iter().next().ok_or_else(|| "HYDRA produced no KKTP envelope".to_string())?;
    let wire = KktpMailboxMessage {
        kind: "msg".into(),
        version: GHOST_KKTP_VERSION,
        sid: binding.sid.clone(),
        mailbox_id: binding.mailbox_id.clone(),
        direction,
        seq,
        sender_hydra_id: runtime.identity_id.clone(),
        message_id: message_id.to_owned(),
        profile: stego_profile.to_owned(),
        ciphertext_b64: BASE64.encode(&envelope),
    };
    let next_seq = match seq.checked_add(1) {
        Some(value) => value,
        None => {
            reset_kktp_after_unsent_advance(runtime, contact_id)?;
            return Err("KKTP outbound sequence exhausted; the secure session was reset".into());
        }
    };
    if let Some(current) = runtime.kktp_sessions.get_mut(contact_id) {
        current.send_seq = next_seq;
    }
    Ok((wire, envelope))
}

fn kktp_first_message(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<KktpFirstMessage, String> {
    let (wire, envelope) = seal_kktp_message(runtime, contact_id, message_id, body, stego_profile)?;
    Ok(KktpFirstMessage {
        direction: wire.direction,
        mailbox_id: wire.mailbox_id,
        message_id: wire.message_id,
        profile: wire.profile,
        seq: wire.seq,
        sender_hydra_id: wire.sender_hydra_id,
        envelope_b64: BASE64.encode(envelope),
    })
}

fn fragment_kktp_wire(wire: &KktpMailboxMessage) -> Result<PreparedMailbox, String> {
    let carrier = wire.encode()?;
    let packet = ghost_core::Id128::new_random();
    let frames = ghost_protocol::fragment(packet, &carrier)?;
    if frames.len() > MAX_MAILBOX_TRANSACTIONS {
        return Err("message requires too many Kaspa mailbox transactions".into());
    }
    Ok(PreparedMailbox {
        payloads_hex: frames.into_iter().map(hex::encode).collect(),
    })
}

#[derive(Clone, Debug)]
pub(super) struct PreparedCompletion {
    pub pending_id: String,
    pub message_id: String,
    pub sid: String,
    pub contact_id: String,
    pub destination: String,
    pub payloads_hex: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct PreparedKktpDelivery {
    pub contact_id: String,
    pub body: String,
    pub stego_profile: String,
    pub payloads_hex: Vec<String>,
}

pub(super) struct HydraProfileRuntime {
    pub identity_id: String,
    pub hydra: HydraFacade,
    pub pending_outbound: Option<PendingOutbound>,
    pub prepared_completion: Option<PreparedCompletion>,
    pub pending_recovery: Option<PendingRecovery>,
    pub prepared_recovery_finish: Option<PreparedRecoveryFinish>,
    pub pending_inbound: Option<PendingInbound>,
    pub prepared_kktp_deliveries: HashMap<String, PreparedKktpDelivery>,
    pub kktp_sessions: HashMap<String, KktpSessionBinding>,
    pub retired_kktp_sids: HashSet<String>,
    /// Active/pending conversation SIDs supplied by the persisted UI model on
    /// every mailbox drain. Historical on-chain traffic outside this set cannot
    /// create/replace volatile KKTP state after restart.
    pub allowed_kktp_sids: HashSet<String>,
    pub peer_routes: HashMap<String, PeerRoute>,
    pub blocked_peers: HashSet<String>,
}

fn kktp_sid_is_current(runtime: &HydraProfileRuntime, peer: &str, sid: &str) -> bool {
    runtime.allowed_kktp_sids.contains(&sid.to_ascii_lowercase())
        || runtime
            .kktp_sessions
            .get(peer)
            .is_some_and(|binding| binding.sid == sid)
        || runtime
            .pending_outbound
            .as_ref()
            .is_some_and(|pending| pending.contact_id == peer && pending.sid == sid)
        || runtime
            .pending_inbound
            .as_ref()
            .is_some_and(|pending| pending.contact_id == peer && pending.sid == sid)
        || runtime
            .pending_recovery
            .as_ref()
            .is_some_and(|pending| pending.contact_id == peer && pending.sid == sid)
        || runtime
            .prepared_completion
            .as_ref()
            .is_some_and(|prepared| prepared.contact_id == peer && prepared.sid == sid)
        || runtime
            .prepared_recovery_finish
            .as_ref()
            .is_some_and(|prepared| prepared.contact_id == peer && prepared.sid == sid)
        || runtime
            .peer_routes
            .get(peer)
            .and_then(|route| route.session_sid.as_deref())
            == Some(sid)
}

fn clear_peer_handshake_state(runtime: &mut HydraProfileRuntime, contact_id: &str) {
    if runtime
        .pending_outbound
        .as_ref()
        .is_some_and(|pending| pending.contact_id == contact_id)
    {
        runtime.pending_outbound = None;
    }
    if runtime
        .prepared_completion
        .as_ref()
        .is_some_and(|prepared| prepared.contact_id == contact_id)
    {
        runtime.prepared_completion = None;
    }
    if runtime
        .pending_recovery
        .as_ref()
        .is_some_and(|pending| pending.contact_id == contact_id)
    {
        runtime.pending_recovery = None;
    }
    if runtime
        .prepared_recovery_finish
        .as_ref()
        .is_some_and(|prepared| prepared.contact_id == contact_id)
    {
        runtime.prepared_recovery_finish = None;
    }
    if runtime
        .pending_inbound
        .as_ref()
        .is_some_and(|pending| pending.contact_id == contact_id)
    {
        runtime.pending_inbound = None;
    }
    runtime
        .prepared_kktp_deliveries
        .retain(|_, prepared| prepared.contact_id != contact_id);
}


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

#[derive(Clone, Debug, Serialize)]
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

#[tauri::command]
pub async fn hydra_debug_state(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
) -> Result<HydraDebugState, String> {
    if !crate::debug_log::is_enabled() {
        return Ok(HydraDebugState {
            available: false,
            identity_id: None,
            sessions: Vec::new(),
            pending_outbound: None,
            prepared_completion: None,
            pending_inbound: None,
            pending_recovery: None,
            prepared_recovery_finish: None,
            retained_delivery_count: 0,
            retired_sid_count: 0,
        });
    }
    let Some(runtime) = state.runtime_if_present(&profile_id)? else {
        return Ok(HydraDebugState {
            available: false,
            identity_id: None,
            sessions: Vec::new(),
            pending_outbound: None,
            prepared_completion: None,
            pending_inbound: None,
            pending_recovery: None,
            prepared_recovery_finish: None,
            retained_delivery_count: 0,
            retired_sid_count: 0,
        });
    };
    let runtime = runtime.lock().await;
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
    Ok(HydraDebugState {
        available: true,
        identity_id: Some(runtime.identity_id.clone()),
        sessions,
        pending_outbound: runtime.pending_outbound.as_ref().map(|pending| {
            format!("sid={} peer={} pending={} message={}", pending.sid, pending.contact_id, pending.id, pending.message_id)
        }),
        prepared_completion: runtime.prepared_completion.as_ref().map(|pending| {
            format!("sid={} peer={} pending={} message={}", pending.sid, pending.contact_id, pending.pending_id, pending.message_id)
        }),
        pending_inbound: runtime.pending_inbound.as_ref().map(|pending| {
            format!("sid={} peer={}", pending.sid, pending.contact_id)
        }),
        pending_recovery: runtime.pending_recovery.as_ref().map(|pending| {
            format!("sid={} peer={} broadcast={}", pending.sid, pending.contact_id, pending.offer_broadcast)
        }),
        prepared_recovery_finish: runtime.prepared_recovery_finish.as_ref().map(|pending| {
            format!("sid={} peer={}", pending.sid, pending.contact_id)
        }),
        retained_delivery_count: runtime.prepared_kktp_deliveries.len(),
        retired_sid_count: runtime.retired_kktp_sids.len(),
    })
}

#[derive(Default)]
pub struct HydraRuntimeState {
    profiles: Mutex<HashMap<String, Arc<AsyncMutex<HydraProfileRuntime>>>>,
}

impl HydraRuntimeState {
    pub(super) fn runtime_if_present(
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

    pub(super) fn runtime(&self, profile_id: &str) -> Result<Arc<AsyncMutex<HydraProfileRuntime>>, String> {
        self.runtime_if_present(profile_id)?
            .ok_or_else(|| "Unlock this Ghost Talk ID before using encrypted messaging".to_string())
    }

    fn install(
        &self,
        profile_id: String,
        identity_id: String,
        hydra: HydraFacade,
    ) -> Result<(), String> {
        self.profiles
            .lock()
            .map_err(|_| "HYDRA runtime state is poisoned".to_string())?
            .insert(
                profile_id,
                Arc::new(AsyncMutex::new(HydraProfileRuntime {
                    identity_id,
                    hydra,
                    pending_outbound: None,
                    prepared_completion: None,
                    pending_recovery: None,
                    prepared_recovery_finish: None,
                    pending_inbound: None,
                    prepared_kktp_deliveries: HashMap::new(),
                    kktp_sessions: HashMap::new(),
                    retired_kktp_sids: HashSet::new(),
                    allowed_kktp_sids: HashSet::new(),
                    peer_routes: HashMap::new(),
                    blocked_peers: HashSet::new(),
                })),
            );
        Ok(())
    }

    pub(super) fn remove(&self, profile_id: &str) -> Result<(), String> {
        self.profiles
            .lock()
            .map_err(|_| "HYDRA runtime state is poisoned".to_string())?
            .remove(profile_id);
        Ok(())
    }
}

pub(super) fn accept_signed_contact_request(
    runtime: &mut HydraProfileRuntime,
    signed_request_hex: &str,
    local_kaspa_addresses: &[String],
) -> Result<ghost_protocol::GhostContactRequest, String> {
    if signed_request_hex.len() % 2 != 0
        || signed_request_hex.len() > MAX_MAILBOX_ENVELOPE_BYTES.saturating_mul(2)
    {
        return Err("signed Ghost Talk contact request hex length is invalid".into());
    }
    let encoded = hex::decode(signed_request_hex)
        .map_err(|_| "signed Ghost Talk contact request is not valid hex".to_string())?;
    let request = ghost_protocol::GhostContactRequest::decode(&encoded)?;
    ghost_kaspa::verify_contact_request(&request)?;
    if !local_kaspa_addresses
        .iter()
        .any(|address| address == &request.recipient_kaspa_address)
    {
        return Err("contact request is not addressed to this Ghost Talk wallet".into());
    }
    let card = BASE64
        .decode(&request.sender.hydra_contact_card_b64)
        .map_err(|_| "contact-request HYDRA contact card is not valid base64".to_string())?;
    if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
        return Err("contact-request HYDRA contact card size is invalid".into());
    }
    let preview = runtime.hydra.preview_contact(&card)?;
    if !request.sender.hydra_identity_id.is_empty()
        && request.sender.hydra_identity_id != preview.handle
    {
        return Err("contact-request HYDRA identity does not match its authenticated contact card".into());
    }
    if preview.handle == runtime.identity_id {
        return Err("cannot accept a Ghost Talk contact request from this identity itself".into());
    }
    let contact = runtime.hydra.add_contact(&card)?;
    if contact.handle != preview.handle {
        return Err("HYDRA contact changed between preview and acceptance".into());
    }
    // A fresh KKTP discovery is a new logical session boundary even when the
    // same peer has archived history. Acceptance is idempotent for the exact
    // same SID: never tear down a ratchet merely because the same signed
    // Discovery is presented twice. A SID that has already been explicitly
    // retired cannot be resurrected; the sender must start a new chat with a
    // fresh Discovery/SID.
    let same_kktp_session = request.version == GHOST_KKTP_VERSION
        && runtime
            .kktp_sessions
            .get(&contact.handle)
            .is_some_and(|binding| {
                binding.sid == request.request_id
                    && binding.role == KktpRole::Responder
                    && binding.state != KktpSessionState::Closed
            });
    if request.version == GHOST_KKTP_VERSION
        && !same_kktp_session
        && runtime.retired_kktp_sids.contains(&request.request_id)
    {
        return Err(
            "this KKTP chat request belongs to a retired session; the sender must start a new chat"
                .into(),
        );
    }
    if !same_kktp_session {
        match runtime.hydra.session_status(&contact.handle)?.as_str() {
            "active" => runtime.hydra.close_session(&contact.handle)?,
            "pending" => runtime.hydra.abort_handshake(&contact.handle)?,
            _ => {}
        }
        clear_peer_handshake_state(runtime, &contact.handle);
        retire_kktp_binding(runtime, &contact.handle);
        if request.version == GHOST_KKTP_VERSION {
            install_kktp_binding(
                runtime,
                &contact.handle,
                request.request_id.clone(),
                KktpRole::Responder,
                KktpSessionState::Discovered,
            )?;
        }
    }
    runtime.blocked_peers.remove(&contact.handle);
    runtime.peer_routes.insert(
        contact.handle,
        PeerRoute {
            kaspa_address: request.sender.kaspa_address.clone(),
            display_name: request.sender.display_name.clone(),
            session_sid: Some(request.request_id.clone()),
        },
    );
    Ok(request)
}

/// Materialize the one Ghost Talk HYDRA identity deterministically from the
/// profile's 24-word Kaspa BIP39 root. There is intentionally no independent
/// HYDRA mnemonic/import path: the Kaspa mnemonic + optional BIP39 passphrase
/// is the complete cryptographic recovery root for this development line.
#[tauri::command]
pub async fn hydra_initialize_from_wallet(
    app: AppHandle,
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
) -> Result<HydraReady, String> {
    let runtime_profile_id = profile_id.clone();
    let (ready, hydra) = super::run_blocking("HYDRA identity initialization", move || {
        require_password(&password)?;
        let secret = super::wallet_commands::open_secret(&password, &sealed)?;
        super::wallet_commands::validate_public_projection(&secret, &public)?;
        let mut seed = ghost_kaspa::wallet::hydra_identity_seed(&secret)?;
        let path = profile_path(&app, &profile_id)?;
        let mut hydra = HydraFacade::open(path, &password)?;
        let identities = hydra.list_identities();

        let identity_id = match identities.as_slice() {
            [] => {
                let result = hydra.import_identity_seed(seed, &password);
                // `import_identity_seed` zeroizes its by-value seed internally.
                seed.zeroize();
                result?
            }
            [existing] => {
                // Retry-safe setup: a frontend interruption may occur after the
                // deterministic identity is persisted. Resume only that exact
                // wallet-derived identity; never accept an unrelated legacy ID.
                let mut stored_seed = hydra.export_identity_seed(&existing.id, &password)?;
                let matches = stored_seed == seed;
                stored_seed.zeroize();
                seed.zeroize();
                if !matches {
                    return Err(
                        "this profile contains a HYDRA identity not derived from its Ghost Talk recovery root; create or restore a current account instead"
                            .into(),
                    );
                }
                hydra.set_active_identity(&existing.id, &password)?;
                existing.id.clone()
            }
            _ => {
                seed.zeroize();
                return Err(
                    "this Ghost Talk profile contains multiple HYDRA identities; current profiles support exactly one wallet-derived identity"
                        .into(),
                );
            }
        };

        let identity = hydra
            .list_identities()
            .into_iter()
            .find(|identity| identity.id == identity_id)
            .ok_or_else(|| "HYDRA identity disappeared after initialization".to_string())?;
        Ok((
            HydraReady {
                identity_id: identity.id,
                label: identity.label,
            },
            hydra,
        ))
    })
    .await?;
    state.install(runtime_profile_id, ready.identity_id.clone(), hydra)?;
    Ok(ready)
}

#[tauri::command]
pub async fn hydra_ensure(
    app: AppHandle,
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: Option<String>,
) -> Result<HydraReady, String> {
    // Account creation/restoration already opens and installs the HYDRA runtime.
    // A later session unlock must reuse that handle instead of attempting a second
    // native HydraFacade::open(), which correctly fails the same-profile lock.
    if let Some(existing) = state.runtime_if_present(&profile_id)? {
        let runtime = existing.lock().await;
        let selected = identity_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(runtime.identity_id.as_str());
        if selected != runtime.identity_id {
            return Err("selected HYDRA identity is not present in this profile".into());
        }
        let identity = runtime
            .hydra
            .list_identities()
            .into_iter()
            .find(|identity| identity.id == runtime.identity_id)
            .ok_or_else(|| "HYDRA identity disappeared from the open profile".to_string())?;
        return Ok(HydraReady {
            identity_id: identity.id,
            label: identity.label,
        });
    }

    let runtime_profile_id = profile_id.clone();
    let (ready, hydra) = super::run_blocking("HYDRA identity unlock", move || {
        require_password(&password)?;
        let path = profile_path(&app, &profile_id)?;
        let mut hydra = HydraFacade::open(path, &password)?;
        let identities = hydra.list_identities();
        if identities.len() != 1 {
            return Err("current Ghost Talk profiles require exactly one wallet-derived HYDRA identity".into());
        }
        let selected = match identity_id.filter(|value| !value.trim().is_empty()) {
            Some(requested) if identities[0].id == requested => requested,
            Some(_) => return Err("selected HYDRA identity is not present in this profile".into()),
            None => identities[0].id.clone(),
        };
        hydra.set_active_identity(&selected, &password)?;
        let identity = hydra
            .list_identities()
            .into_iter()
            .find(|identity| identity.id == selected)
            .ok_or_else(|| "HYDRA identity disappeared after unlock".to_string())?;
        Ok((
            HydraReady {
                identity_id: identity.id,
                label: identity.label,
            },
            hydra,
        ))
    })
    .await?;
    state.install(runtime_profile_id, ready.identity_id.clone(), hydra)?;
    Ok(ready)
}

#[tauri::command]
pub fn hydra_lock_profile(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
) -> Result<(), String> {
    state.remove(&profile_id)
}

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
        decode_fixed_hex::<32>(&route.contact_id, "HYDRA contact id")?;
        ghost_kaspa::validate_destination(&route.kaspa_address)?;
        if let Some(sid) = route.session_sid.as_deref() {
            if sid.len() != 32 || !sid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("registered Ghost Talk session SID must be exactly 32 hexadecimal characters".into());
            }
        }
        if !runtime.hydra.has_contact(&route.contact_id)? {
            continue;
        }
        runtime.peer_routes.insert(
            route.contact_id,
            PeerRoute {
                kaspa_address: route.kaspa_address,
                display_name: route.display_name.chars().take(96).collect(),
                session_sid: route.session_sid,
            },
        );
    }
    Ok(())
}

pub(super) fn prepare_kktp_mailbox(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<PreparedMailbox, String> {
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("message body is empty or exceeds the Ghost Talk event limit".into());
    }
    let (wire, _) = seal_kktp_message(
        runtime,
        contact_id,
        message_id,
        body,
        stego_profile,
    )?;
    let prepared = match fragment_kktp_wire(&wire) {
        Ok(prepared) => prepared,
        Err(error) => {
            reset_kktp_after_unsent_advance(runtime, contact_id)?;
            return Err(format!("{error}; the secure session was reset before any KKTP sequence gap could be created"));
        }
    };
    Ok(prepared)
}

pub(super) fn prepare_mailbox_inner(
    hydra: &mut HydraFacade,
    sender_identity_id: &str,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<PreparedMailbox, String> {
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("message body is empty or exceeds the Ghost Talk event limit".into());
    }
    let sender = decode_fixed_hex::<32>(sender_identity_id, "HYDRA sender identity")?;
    let message = decode_fixed_hex::<16>(message_id, "Ghost Talk message id")?;
    let profile = parse_stego_profile(stego_profile)?;
    let envelopes = hydra.send(contact_id, body.as_bytes(), profile)?;
    let mut payloads_hex = Vec::new();
    for envelope in envelopes {
        if envelope.len().saturating_add(53) > MAX_MAILBOX_ENVELOPE_BYTES {
            return Err("HYDRA mailbox envelope exceeds the configured carrier limit".into());
        }
        let mut carrier = Vec::with_capacity(envelope.len().saturating_add(53));
        carrier.extend_from_slice(MAILBOX_ENVELOPE_MAGIC);
        carrier.push(stego_code(profile));
        carrier.extend_from_slice(&sender);
        carrier.extend_from_slice(&message);
        carrier.extend_from_slice(&envelope);
        let packet = ghost_core::Id128::new_random();
        for frame in ghost_protocol::fragment(packet, &carrier)? {
            if payloads_hex.len() >= MAX_MAILBOX_TRANSACTIONS {
                return Err("message requires too many Kaspa mailbox transactions".into());
            }
            payloads_hex.push(hex::encode(frame));
        }
    }
    Ok(PreparedMailbox { payloads_hex })
}

pub(super) fn prepare_realtime_mailbox(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
) -> Result<PreparedMailbox, String> {
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("realtime body is empty or exceeds the Ghost Talk event limit".into());
    }
    let binding = runtime
        .kktp_sessions
        .get(contact_id)
        .cloned()
        .ok_or_else(|| "realtime transport requires an active KKTP session".to_string())?;
    if binding.state != KktpSessionState::Active {
        return Err("realtime transport requires an active KKTP session".into());
    }
    if runtime.blocked_peers.contains(contact_id) {
        return Err("realtime transport is closed for this peer".into());
    }
    let sender = decode_fixed_hex::<32>(&runtime.identity_id, "HYDRA sender identity")?;
    let message = decode_fixed_hex::<16>(message_id, "Ghost Talk message id")?;
    let sid = decode_fixed_hex::<16>(&binding.sid, "KKTP session id")?;
    let inner = format!("{REALTIME_INNER_PREFIX}{}:{body}", binding.sid);
    let envelopes = runtime.hydra.send(contact_id, inner.as_bytes(), StegoProfile::Off)?;
    if envelopes.len() != 1 {
        return Err("realtime HYDRA send produced more than one logical envelope".into());
    }
    let envelope = envelopes
        .into_iter()
        .next()
        .ok_or_else(|| "realtime HYDRA send produced no envelope".to_string())?;
    if envelope.len().saturating_add(68) > MAX_MAILBOX_ENVELOPE_BYTES {
        return Err("realtime HYDRA envelope exceeds the configured carrier limit".into());
    }
    let mut carrier = Vec::with_capacity(envelope.len().saturating_add(68));
    carrier.extend_from_slice(REALTIME_MAILBOX_MAGIC);
    carrier.extend_from_slice(&sender);
    carrier.extend_from_slice(&message);
    carrier.extend_from_slice(&sid);
    carrier.extend_from_slice(&envelope);
    let packet = ghost_core::Id128::new_random();
    let mut payloads_hex = Vec::new();
    for frame in ghost_protocol::fragment(packet, &carrier)? {
        if payloads_hex.len() >= MAX_MAILBOX_TRANSACTIONS {
            return Err("realtime control requires too many Kaspa mailbox transactions".into());
        }
        payloads_hex.push(hex::encode(frame));
    }
    Ok(PreparedMailbox { payloads_hex })
}

fn require_active_direct_binding<'a>(
    runtime: &'a HydraProfileRuntime,
    contact_id: &str,
    session_sid: &str,
) -> Result<&'a KktpSessionBinding, String> {
    let binding = runtime
        .kktp_sessions
        .get(contact_id)
        .ok_or_else(|| "direct transport requires an active KKTP session".to_string())?;
    if binding.state != KktpSessionState::Active {
        return Err("direct transport requires an active KKTP session".into());
    }
    if binding.sid != session_sid {
        return Err("direct transport SID does not match the active conversation".into());
    }
    if runtime.blocked_peers.contains(contact_id) {
        return Err("direct transport is closed for this peer".into());
    }
    Ok(binding)
}

#[tauri::command]
pub async fn hydra_seal_direct(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    contact_id: String,
    session_sid: String,
    body: String,
) -> Result<HydraDirectEnvelope, String> {
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("direct payload is empty or exceeds the Ghost Talk event limit".into());
    }
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    require_active_direct_binding(&runtime, &contact_id, &session_sid)?;
    let envelopes = runtime
        .hydra
        .send(&contact_id, body.as_bytes(), StegoProfile::Off)?;
    if envelopes.len() != 1 {
        return Err("direct HYDRA send produced more than one logical envelope".into());
    }
    let envelope = envelopes
        .into_iter()
        .next()
        .ok_or_else(|| "direct HYDRA send produced no envelope".to_string())?;
    if envelope.len() > MAX_MAILBOX_ENVELOPE_BYTES {
        return Err("direct HYDRA envelope exceeds the configured limit".into());
    }
    Ok(HydraDirectEnvelope {
        envelope_b64: BASE64.encode(envelope),
    })
}

#[tauri::command]
pub async fn hydra_open_direct(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    contact_id: String,
    session_sid: String,
    envelope_b64: String,
) -> Result<Option<String>, String> {
    let envelope = BASE64
        .decode(envelope_b64.as_bytes())
        .map_err(|_| "direct HYDRA envelope is not valid base64".to_string())?;
    if envelope.is_empty() || envelope.len() > MAX_MAILBOX_ENVELOPE_BYTES {
        return Err("direct HYDRA envelope length is invalid".into());
    }
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    require_active_direct_binding(&runtime, &contact_id, &session_sid)?;
    let received = runtime.hydra.receive(&envelope, StegoProfile::Off)?;
    let Some(received) = received else {
        return Ok(None);
    };
    if received.from != contact_id {
        return Err("direct HYDRA sender does not match the active peer".into());
    }
    Ok(Some(received.plaintext))
}

#[tauri::command]
pub async fn hydra_leave_peer(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    contact_id: String,
) -> Result<(), String> {
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    if runtime.hydra.has_contact(&contact_id)? {
        match runtime.hydra.session_status(&contact_id)?.as_str() {
            "active" => runtime.hydra.close_session(&contact_id)?,
            "pending" => runtime.hydra.abort_handshake(&contact_id)?,
            _ => {}
        }
    }
    retire_kktp_binding(&mut runtime, &contact_id);
    runtime.blocked_peers.insert(contact_id.clone());
    clear_peer_handshake_state(&mut runtime, &contact_id);
    Ok(())
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
    if runtime.hydra.has_contact(&contact_id)?
        && runtime.hydra.session_status(&contact_id)? == "pending"
    {
        runtime.hydra.abort_handshake(&contact_id)?;
    }
    retire_kktp_binding(&mut runtime, &contact_id);
    clear_peer_handshake_state(&mut runtime, &contact_id);
    runtime.blocked_peers.remove(&contact_id);
    Ok(())
}

#[tauri::command]
pub fn hydra_preview_contact_request(
    envelope_hex: String,
    local_kaspa_addresses: Vec<String>,
) -> Result<Option<HydraIncomingRequestProjection>, String> {
    if envelope_hex.len() % 2 != 0
        || envelope_hex.len() > MAX_MAILBOX_ENVELOPE_BYTES.saturating_mul(2)
    {
        return Err("mailbox envelope hex length is invalid".into());
    }
    let envelope = hex::decode(&envelope_hex)
        .map_err(|_| "mailbox envelope is not valid hex".to_string())?;
    let is_legacy_request = envelope.starts_with(&ghost_protocol::GTCR_MAGIC);
    let is_kktp_discovery = envelope.starts_with(KKTP_ANCHOR_PREFIX)
        && kktp_anchor_type(&envelope)?.as_deref() == Some("discovery");
    if !is_legacy_request && !is_kktp_discovery {
        return Ok(None);
    }
    let request = ghost_protocol::GhostContactRequest::decode(&envelope)?;
    ghost_kaspa::verify_contact_request(&request)?;
    if !local_kaspa_addresses
        .iter()
        .any(|address| address == &request.recipient_kaspa_address)
    {
        return Ok(None);
    }
    if request.sender.hydra_identity_id.len() != 64
        || !request
            .sender
            .hydra_identity_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("contact-request signed HYDRA identity must be 64 hexadecimal characters".into());
    }
    Ok(Some(HydraIncomingRequestProjection {
        request_id: request.request_id,
        peer_address: request.sender.kaspa_address,
        local_address: request.recipient_kaspa_address,
        peer_label: request.sender.display_name,
        peer_hydra_id: request.sender.hydra_identity_id,
        signed_request_hex: hex::encode(envelope),
    }))
}


fn validate_kktp_binding_header(
    binding: &KktpSessionBinding,
    sid: &str,
    mailbox_id: &str,
    direction: KktpDirection,
    sender_hydra_id: &str,
) -> Result<(), String> {
    if binding.sid != sid {
        return Err("KKTP sid does not match the active session".into());
    }
    if binding.mailbox_id != mailbox_id {
        return Err("KKTP mailbox id does not match the active session".into());
    }
    if binding.inbound_direction() != direction {
        return Err("KKTP direction does not match the peer role".into());
    }
    if binding.peer_hydra_id != sender_hydra_id {
        return Err("KKTP sender HYDRA id does not match the bound peer".into());
    }
    Ok(())
}

fn open_kktp_envelope(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    sid: &str,
    mailbox_id: &str,
    direction: KktpDirection,
    seq: u64,
    message_id: &str,
    profile_name: &str,
    encrypted_envelope: &[u8],
) -> Result<Option<ReceivedProjection>, String> {
    let binding = runtime
        .kktp_sessions
        .get(peer_hydra_id)
        .cloned()
        .ok_or_else(|| "KKTP session binding is missing for the sender".to_string())?;
    validate_kktp_binding_header(
        &binding,
        sid,
        mailbox_id,
        direction,
        peer_hydra_id,
    )?;
    if binding.state != KktpSessionState::Active {
        return Err("KKTP message arrived before the session became active".into());
    }
    if seq < binding.recv_next_seq {
        return Ok(None);
    }
    if seq > binding.recv_next_seq {
        return Err(format!(
            "KKTP sequence gap: expected {}, received {seq}",
            binding.recv_next_seq
        ));
    }
    let profile = parse_stego_profile(profile_name)?;
    let received = runtime
        .hydra
        .receive(encrypted_envelope, profile)?
        .ok_or_else(|| "HYDRA did not yield plaintext for the expected KKTP sequence".to_string())?;
    if received.from != peer_hydra_id {
        return Err("HYDRA sender does not match the KKTP peer binding".into());
    }
    let inner = KktpInnerMessage::decode(received.plaintext.as_bytes())?;
    if inner.sid != sid
        || inner.mailbox_id != mailbox_id
        || inner.direction != direction
        || inner.seq != seq
        || inner.message_id != message_id
    {
        return Err("KKTP encrypted inner metadata does not match the authenticated outer record".into());
    }
    let next_seq = seq
        .checked_add(1)
        .ok_or_else(|| "KKTP inbound sequence exhausted".to_string())?;
    if let Some(current) = runtime.kktp_sessions.get_mut(peer_hydra_id) {
        current.recv_next_seq = next_seq;
    }
    match inner.kind.as_str() {
        "msg" => Ok(Some(ReceivedProjection {
            from: peer_hydra_id.to_owned(),
            plaintext: inner.body,
            session_sid: Some(sid.to_owned()),
        })),
        "session_end" => {
            if runtime.hydra.session_status(peer_hydra_id)? == "active" {
                runtime.hydra.close_session(peer_hydra_id)?;
            }
            if let Some(current) = runtime.kktp_sessions.get_mut(peer_hydra_id) {
                current.state = KktpSessionState::Closed;
            }
            Ok(None)
        }
        _ => Err("unknown authenticated KKTP inner message type".into()),
    }
}

fn handle_kktp_handshake(
    runtime: &mut HydraProfileRuntime,
    control: KktpHandshakeControl,
) -> Result<HydraMailboxResult, String> {
    let local = runtime.identity_id.clone();
    let (peer, local_role) = if control.initiator_hydra_id == local {
        (control.responder_hydra_id.clone(), KktpRole::Initiator)
    } else if control.responder_hydra_id == local {
        (control.initiator_hydra_id.clone(), KktpRole::Responder)
    } else {
        return Err("KKTP PQ handshake is addressed to another HYDRA identity".into());
    };
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-control-received",
        format!("stage={} sid={} local_role={:?} peer={}", control.stage, control.sid, local_role, peer),
    );
    if runtime.blocked_peers.contains(&peer) {
        crate::debug_log::record("warn", "handshake", "pq-control-blocked-peer", format!("stage={} sid={} peer={}", control.stage, control.sid, peer));
        return Ok(discard_result());
    }
    // Address-history scans include transactions we authored because our input
    // and change addresses also participate in those transactions. KKTP stage
    // semantics identify the sender role unambiguously, so discard authenticated
    // loopback controls instead of retaining them forever as protocol errors.
    let expected_sender = match control.stage.as_str() {
        "pq_init" | "pq_finish" => &control.initiator_hydra_id,
        "pq_resp" => &control.responder_hydra_id,
        _ => return Err("unknown Ghost Talk KKTP PQ handshake stage".into()),
    };
    if expected_sender == &local {
        return Ok(discard_result());
    }
    if expected_sender != &peer {
        return Err("KKTP PQ handshake sender role is inconsistent".into());
    }
    if !runtime.hydra.has_contact(&peer)? {
        return Err("KKTP PQ handshake names an unknown HYDRA contact".into());
    }
    let route = runtime
        .peer_routes
        .get(&peer)
        .cloned()
        .ok_or_else(|| "KKTP PQ handshake has no verified Kaspa peer route".to_string())?;
    if !kktp_sid_is_current(runtime, &peer, &control.sid) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "stale-handshake-discarded",
            format!("stage={} sid={} peer={}", control.stage, control.sid, peer),
        );
        return Ok(discard_result());
    }
    let pq_signature = BASE64
        .decode(&control.pq_sig_b64)
        .map_err(|_| "KKTP PQ context signature is not valid base64".to_string())?;
    runtime.hydra.verify_contact_application_context(
        &peer,
        &control.signing_bytes()?,
        &pq_signature,
    )?;
    crate::debug_log::record(
        "info",
        "handshake",
        "pq-control-authenticated",
        format!("stage={} sid={} peer={}", control.stage, control.sid, peer),
    );
    let payload = BASE64
        .decode(&control.payload_b64)
        .map_err(|_| "KKTP PQ handshake payload is not valid base64".to_string())?;

    match control.stage.as_str() {
        "pq_init" => {
            if local_role != KktpRole::Responder {
                return Err("KKTP pq_init role mapping is invalid".into());
            }
            if runtime.retired_kktp_sids.contains(&control.sid) {
                return Ok(discard_result());
            }
            if let Some(existing) = runtime.kktp_sessions.get(&peer).cloned() {
                if existing.sid == control.sid {
                    if existing.role != KktpRole::Responder {
                        return Err("KKTP session role changed for an existing SID".into());
                    }
                    if matches!(
                        existing.state,
                        KktpSessionState::Active | KktpSessionState::Closed
                    ) {
                        // Replayed INIT for a session that has already advanced
                        // cannot reopen or rewind the ratchet.
                        return Ok(discard_result());
                    }
                } else {
                    // The entire control record (SID, roles, stage and opaque
                    // HYDRA INIT) has already been verified with the peer's
                    // ML-DSA-65 identity above. A fresh signed SID is therefore
                    // an authenticated session replacement/recovery request.
                    match runtime.hydra.session_status(&peer)?.as_str() {
                        "active" => runtime.hydra.close_session(&peer)?,
                        "pending" => runtime.hydra.abort_handshake(&peer)?,
                        _ => {}
                    }
                    clear_peer_handshake_state(runtime, &peer);
                    retire_kktp_binding(runtime, &peer);
                }
            }
            if !runtime.kktp_sessions.contains_key(&peer) {
                // A valid, ML-DSA-authenticated fresh pq_init may be recovery
                // after this process lost its KKTP wrapper state while HYDRA
                // still has an old active/pending session. Retire that stale
                // cryptographic state before installing the new SID.
                match runtime.hydra.session_status(&peer)?.as_str() {
                    "active" => runtime.hydra.close_session(&peer)?,
                    "pending" => runtime.hydra.abort_handshake(&peer)?,
                    _ => {}
                }
                clear_peer_handshake_state(runtime, &peer);
                install_kktp_binding(
                    runtime,
                    &peer,
                    control.sid.clone(),
                    KktpRole::Responder,
                    KktpSessionState::Handshake,
                )?;
            }
            if runtime.hydra.session_status(&peer)? == "pending" {
                // reply_handshake itself is idempotent for the same INIT. Keep
                // that state when the SID matches; only stale wrapper state was
                // retired above.
            }
            let answer = runtime.hydra.reply_handshake(&payload)?;
            if let Some(binding) = runtime.kktp_sessions.get_mut(&peer) {
                binding.state = KktpSessionState::Handshake;
            }
            runtime.pending_inbound = Some(PendingInbound {
                sid: control.sid.clone(),
                contact_id: peer.clone(),
            });
            let binding = runtime
                .kktp_sessions
                .get(&peer)
                .cloned()
                .ok_or_else(|| "KKTP responder binding disappeared".to_string())?;
            let response = kktp_handshake_payload(
                runtime,
                &binding,
                "pq_resp",
                &answer,
                None,
            )?;
            crate::debug_log::record(
                "info",
                "handshake",
                "pq-response-prepared",
                format!("sid={} peer={} state=Handshake", control.sid, peer),
            );
            Ok(HydraMailboxResult {
                received: None,
                control: Some(HydraControlProjection {
                    destination: route.kaspa_address,
                    payloads_hex: frame_control(response)?.payloads_hex,
                    completes_pending_id: None,
                }),
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: None,
                peer_label: None,
                message_id: None,
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: None,
                session_ended: None,
                discard: true,
            })
        }
        "pq_resp" => {
            if local_role != KktpRole::Initiator {
                return Err("KKTP pq_resp role mapping is invalid".into());
            }
            let binding = runtime
                .kktp_sessions
                .get(&peer)
                .cloned()
                .ok_or_else(|| "KKTP response has no matching initiator session".to_string())?;
            if binding.sid != control.sid || binding.role != KktpRole::Initiator {
                return Ok(discard_result());
            }
            if let Some(prepared) = runtime.prepared_completion.clone() {
                if prepared.contact_id == peer && prepared.sid == control.sid {
                    return Ok(HydraMailboxResult {
                        received: None,
                        control: Some(HydraControlProjection {
                            destination: prepared.destination,
                            payloads_hex: prepared.payloads_hex,
                            completes_pending_id: Some(prepared.pending_id),
                        }),
                        recovery: None,
                        incoming_request: None,
                        contact_accepted: None,
                        peer_address: None,
                        peer_label: None,
                        message_id: None,
                        delivery_ack: None,
                        delivery_ack_peer: None,
                        // The initiator ratchet exists locally, but the responder has
                        // not proved that it observed pq_finish yet. The signed
                        // bootstrap acknowledgement is the peer-confirmed ACTIVE transition.
                        session_established_peer: None,
                        session_ended: None,
                        discard: true,
                    });
                }
            }
            if let Some(prepared) = runtime.prepared_recovery_finish.clone() {
                if prepared.contact_id == peer && prepared.sid == control.sid {
                    return Ok(HydraMailboxResult {
                        received: None,
                        control: Some(HydraControlProjection {
                            destination: prepared.destination,
                            payloads_hex: prepared.payloads_hex,
                            completes_pending_id: None,
                        }),
                        recovery: None,
                        incoming_request: None,
                        contact_accepted: None,
                        peer_address: None,
                        peer_label: None,
                        message_id: None,
                        delivery_ack: None,
                        delivery_ack_peer: None,
                        session_established_peer: Some(peer),
                        session_ended: None,
                        discard: true,
                    });
                }
            }
            let finish = runtime.hydra.finish_handshake(&payload)?;
            if let Some(current) = runtime.kktp_sessions.get_mut(&peer) {
                current.state = KktpSessionState::Active;
            }
            crate::debug_log::record(
                "info",
                "handshake",
                "pq-response-applied-finish-preparing",
                format!("sid={} peer={} initiator_state=Active-awaiting-finish-ack", control.sid, peer),
            );
            if let Some(pending) = runtime.pending_outbound.clone() {
                if pending.contact_id == peer && pending.sid == control.sid {
                    let first = kktp_first_message(
                        runtime,
                        &peer,
                        &pending.message_id,
                        &pending.body,
                        &pending.stego_profile,
                    )?;
                    let binding = runtime
                        .kktp_sessions
                        .get(&peer)
                        .cloned()
                        .ok_or_else(|| "KKTP initiator binding disappeared".to_string())?;
                    let finish_wire = kktp_handshake_payload(
                        runtime,
                        &binding,
                        "pq_finish",
                        &finish,
                        Some(first),
                    )?;
                    let payloads_hex = frame_control(finish_wire)?.payloads_hex;
                    runtime.prepared_completion = Some(PreparedCompletion {
                        pending_id: pending.id.clone(),
                        message_id: pending.message_id.clone(),
                        sid: control.sid.clone(),
                        contact_id: peer.clone(),
                        destination: pending.destination.clone(),
                        payloads_hex: payloads_hex.clone(),
                    });
                    crate::debug_log::record(
                        "info",
                        "handshake",
                        "pq-finish-prepared-with-first-message",
                        format!("sid={} peer={} pending={} message={}", control.sid, peer, pending.id, pending.message_id),
                    );
                    return Ok(HydraMailboxResult {
                        received: None,
                        control: Some(HydraControlProjection {
                            destination: pending.destination,
                            payloads_hex,
                            completes_pending_id: Some(pending.id),
                        }),
                        recovery: None,
                        incoming_request: None,
                        contact_accepted: None,
                        peer_address: None,
                        peer_label: None,
                        message_id: None,
                        delivery_ack: None,
                        delivery_ack_peer: None,
                        // Do not tell the UI the direct chat is active until the
                        // responder proves pq_finish + first-message receipt with
                        // its signed delivery acknowledgement.
                        session_established_peer: None,
                        session_ended: None,
                        discard: true,
                    });
                }
            }
            if let Some(pending) = runtime.pending_recovery.clone() {
                if pending.contact_id == peer && pending.sid == control.sid {
                    let binding = runtime
                        .kktp_sessions
                        .get(&peer)
                        .cloned()
                        .ok_or_else(|| "KKTP recovery binding disappeared".to_string())?;
                    let finish_wire = kktp_handshake_payload(
                        runtime,
                        &binding,
                        "pq_finish",
                        &finish,
                        None,
                    )?;
                    let payloads_hex = frame_control(finish_wire)?.payloads_hex;
                    runtime.prepared_recovery_finish = Some(PreparedRecoveryFinish {
                        sid: control.sid.clone(),
                        contact_id: peer.clone(),
                        destination: pending.destination.clone(),
                        payloads_hex: payloads_hex.clone(),
                    });
                    return Ok(HydraMailboxResult {
                        received: None,
                        control: Some(HydraControlProjection {
                            destination: pending.destination,
                            payloads_hex,
                            completes_pending_id: None,
                        }),
                        recovery: None,
                        incoming_request: None,
                        contact_accepted: None,
                        peer_address: None,
                        peer_label: None,
                        message_id: None,
                        delivery_ack: None,
                        delivery_ack_peer: None,
                        session_established_peer: Some(peer),
                        session_ended: None,
                        discard: true,
                    });
                }
            }
            Ok(discard_result())
        }
        "pq_finish" => {
            if local_role != KktpRole::Responder {
                return Err("KKTP pq_finish role mapping is invalid".into());
            }
            let binding = runtime
                .kktp_sessions
                .get(&peer)
                .cloned()
                .ok_or_else(|| "KKTP finish has no matching responder session".to_string())?;
            if binding.sid != control.sid || binding.role != KktpRole::Responder {
                return Ok(discard_result());
            }
            let Some(pending) = runtime.pending_inbound.clone() else {
                // A replayed FINISH after successful activation must not reopen
                // or advance the ratchet. It *does* need to re-trigger the signed
                // delivery ACK for the embedded first message: otherwise one
                // missed ACK can strand the initiator at "Establishing secure
                // session..." forever even though the responder is already active.
                if binding.state == KktpSessionState::Active {
                    if let Some(first) = control.first_message.as_ref() {
                        return Ok(HydraMailboxResult {
                            received: None,
                            control: None,
                            recovery: None,
                            incoming_request: None,
                            contact_accepted: None,
                            peer_address: Some(route.kaspa_address),
                            peer_label: Some(route.display_name),
                            message_id: Some(first.message_id.clone()),
                            delivery_ack: None,
                            delivery_ack_peer: None,
                            session_established_peer: Some(peer),
                            session_ended: None,
                            discard: true,
                        });
                    }
                }
                return Ok(discard_result());
            };
            if pending.contact_id != peer || pending.sid != control.sid {
                return Ok(discard_result());
            }
            runtime.hydra.accept_finish(&payload)?;
            if let Some(current) = runtime.kktp_sessions.get_mut(&peer) {
                current.state = KktpSessionState::Active;
            }
            runtime.pending_inbound = None;
            crate::debug_log::record(
                "info",
                "handshake",
                "pq-finish-accepted-session-active",
                format!("sid={} peer={} first_message={}", control.sid, peer, control.first_message.is_some()),
            );
            let mut received = None;
            let mut message_id = None;
            if let Some(first) = control.first_message {
                let encrypted = BASE64
                    .decode(&first.envelope_b64)
                    .map_err(|_| "KKTP first-message envelope is not valid base64".to_string())?;
                received = open_kktp_envelope(
                    runtime,
                    &peer,
                    &control.sid,
                    &first.mailbox_id,
                    first.direction,
                    first.seq,
                    &first.message_id,
                    &first.profile,
                    &encrypted,
                )?;
                message_id = Some(first.message_id);
            }
            Ok(HydraMailboxResult {
                received,
                control: None,
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: Some(route.kaspa_address),
                peer_label: Some(route.display_name),
                message_id,
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: Some(peer),
                session_ended: None,
                discard: true,
            })
        }
        _ => Err("unknown Ghost Talk KKTP PQ handshake stage".into()),
    }
}

fn handle_kktp_mailbox_message(
    runtime: &mut HydraProfileRuntime,
    wire: KktpMailboxMessage,
) -> Result<HydraMailboxResult, String> {
    let peer = wire.sender_hydra_id.clone();
    let route = runtime.peer_routes.get(&peer).cloned();
    if !kktp_sid_is_current(runtime, &peer, &wire.sid) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "stale-message-discarded",
            format!("sid={} peer={} message={}", wire.sid, peer, wire.message_id),
        );
        return Ok(discard_result());
    }
    if runtime.blocked_peers.contains(&peer) {
        return Ok(HydraMailboxResult {
            received: None,
            control: None,
            recovery: None,
            incoming_request: None,
            contact_accepted: None,
            peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
            peer_label: route.map(|value| value.display_name),
            message_id: Some(wire.message_id),
            delivery_ack: None,
            delivery_ack_peer: None,
            session_established_peer: None,
            session_ended: None,
            discard: true,
        });
    }
    let Some(binding) = runtime.kktp_sessions.get(&peer).cloned() else {
        // Runtime state is memory-only. A durable packet observed after restart
        // cannot be decrypted until a fresh PQ handshake is established. Ask the
        // known peer for recovery instead of discarding ciphertext.
        if !runtime.hydra.has_contact(&peer)? {
            return Err("KKTP message names an unknown HYDRA peer".into());
        }
        let route = route.ok_or_else(|| "KKTP message has no verified Kaspa peer route".to_string())?;
        if runtime.hydra.session_status(&peer)? == "pending" {
            return Ok(HydraMailboxResult {
                received: None,
                control: None,
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: Some(route.kaspa_address),
                peer_label: Some(route.display_name),
                message_id: Some(wire.message_id),
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: None,
                session_ended: None,
                discard: false,
            });
        }
        let sid = fresh_kktp_sid();
        runtime.hydra.abort_handshake(&peer)?;
        let offer = runtime.hydra.init_handshake(&peer)?;
        install_kktp_binding(
            runtime,
            &peer,
            sid.clone(),
            KktpRole::Initiator,
            KktpSessionState::Handshake,
        )?;
        let offer_hex = hex::encode(&offer);
        runtime.pending_recovery = Some(PendingRecovery {
            sid: sid.clone(),
            contact_id: peer.clone(),
            destination: route.kaspa_address.clone(),
            offer_hex: offer_hex.clone(),
            offer_broadcast: false,
        });
        return Ok(HydraMailboxResult {
            received: None,
            control: None,
            recovery: Some(HydraRecoveryProjection {
                destination: route.kaspa_address.clone(),
                offer_hex,
                sid,
                peer_hydra_id: peer.clone(),
            }),
            incoming_request: None,
            contact_accepted: None,
            peer_address: Some(route.kaspa_address),
            peer_label: Some(route.display_name),
            message_id: Some(wire.message_id),
            delivery_ack: None,
            delivery_ack_peer: None,
            session_established_peer: None,
            session_ended: None,
            discard: false,
        });
    };
    if binding.sid != wire.sid {
        // Old-session ciphertext is never admissible into a new SID. It is a
        // replay/stale carrier and cannot become valid later.
        return Ok(HydraMailboxResult {
            received: None,
            control: None,
            recovery: None,
            incoming_request: None,
            contact_accepted: None,
            peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
            peer_label: route.map(|value| value.display_name),
            message_id: Some(wire.message_id),
            delivery_ack: None,
            delivery_ack_peer: None,
            session_established_peer: None,
            session_ended: None,
            discard: true,
        });
    }
    if wire.seq < binding.recv_next_seq {
        return Ok(discard_result());
    }
    if wire.seq > binding.recv_next_seq || binding.state != KktpSessionState::Active {
        // Preserve the durable packet. DAG reordering can place seq n+1 before n,
        // and a normal message can be observed before the pq_finish carrier.
        return Ok(HydraMailboxResult {
            received: None,
            control: None,
            recovery: None,
            incoming_request: None,
            contact_accepted: None,
            peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
            peer_label: route.map(|value| value.display_name),
            message_id: Some(wire.message_id),
            delivery_ack: None,
            delivery_ack_peer: None,
            session_established_peer: None,
            session_ended: None,
            discard: false,
        });
    }
    let encrypted = BASE64
        .decode(&wire.ciphertext_b64)
        .map_err(|_| "KKTP HYDRA ciphertext is not valid base64".to_string())?;
    let received = open_kktp_envelope(
        runtime,
        &peer,
        &wire.sid,
        &wire.mailbox_id,
        wire.direction,
        wire.seq,
        &wire.message_id,
        &wire.profile,
        &encrypted,
    )?;
    let route = runtime.peer_routes.get(&peer).cloned();
    Ok(HydraMailboxResult {
        received,
        control: None,
        recovery: None,
        incoming_request: None,
        contact_accepted: None,
        peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
        peer_label: route.map(|value| value.display_name),
        message_id: Some(wire.message_id),
        delivery_ack: None,
        delivery_ack_peer: None,
        session_established_peer: None,
        session_ended: None,
        discard: true,
    })
}

fn handle_kktp_session_end(
    runtime: &mut HydraProfileRuntime,
    end: KktpSessionEnd,
    local_kaspa_addresses: &[String],
) -> Result<HydraMailboxResult, String> {
    ghost_kaspa::verify_kktp_session_end(&end)?;
    if !local_kaspa_addresses
        .iter()
        .any(|address| address == &end.recipient_kaspa_address)
    {
        return Err("KKTP session_end is not addressed to this Ghost Talk wallet".into());
    }
    if end.sender_hydra_id == runtime.identity_id {
        return Ok(discard_result());
    }
    let local_is_initiator = end.initiator_hydra_id == runtime.identity_id;
    let local_is_responder = end.responder_hydra_id == runtime.identity_id;
    if local_is_initiator == local_is_responder {
        return Err("KKTP session_end does not name this HYDRA identity exactly once".into());
    }
    let expected_peer = if local_is_initiator {
        &end.responder_hydra_id
    } else {
        &end.initiator_hydra_id
    };
    if expected_peer != &end.sender_hydra_id {
        return Err("KKTP session_end sender does not match the remote session participant".into());
    }
    if !kktp_sid_is_current(runtime, &end.sender_hydra_id, &end.sid) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "stale-session-end-discarded",
            format!("sid={} peer={}", end.sid, end.sender_hydra_id),
        );
        return Ok(discard_result());
    }
    if !runtime.hydra.has_contact(&end.sender_hydra_id)? {
        return Err("KKTP session_end signer is not a known HYDRA peer".into());
    }
    let pq_signature = BASE64
        .decode(&end.pq_sig_b64)
        .map_err(|_| "KKTP session_end PQ signature is not valid base64".to_string())?;
    runtime.hydra.verify_contact_application_context(
        &end.sender_hydra_id,
        &end.pq_signing_bytes()?,
        &pq_signature,
    )?;

    if runtime.retired_kktp_sids.contains(&end.sid) {
        return Ok(discard_result());
    }

    let matching_live_binding = if let Some(binding) = runtime.kktp_sessions.get(&end.sender_hydra_id).cloned() {
        if binding.sid != end.sid {
            // A close for an older SID can appear after a fresh conversation has
            // already started. It is authenticated but stale and must not close
            // or block the newer session.
            return Ok(discard_result());
        }
        let (expected_initiator, expected_responder) = match binding.role {
            KktpRole::Initiator => (runtime.identity_id.as_str(), end.sender_hydra_id.as_str()),
            KktpRole::Responder => (end.sender_hydra_id.as_str(), runtime.identity_id.as_str()),
        };
        if end.initiator_hydra_id != expected_initiator
            || end.responder_hydra_id != expected_responder
        {
            return Err("KKTP session_end role ordering does not match the active binding".into());
        }
        true
    } else {
        // After restart the volatile KKTP binding can be gone while the persisted
        // chat still knows its SID. Surface the authenticated close to the UI so
        // that exact persisted thread can be marked ended, but do not globally
        // block the peer in native state without a live SID to bind that block
        // to. This prevents a delayed old close from poisoning a newly-created
        // session that has not installed its fresh binding yet.
        false
    };

    if let Some(route) = runtime.peer_routes.get(&end.sender_hydra_id) {
        if route.kaspa_address != end.sender_kaspa_address {
            return Err("KKTP session_end Kaspa signer does not match the authenticated peer route".into());
        }
    }

    if matching_live_binding {
        match runtime.hydra.session_status(&end.sender_hydra_id)?.as_str() {
            "active" => runtime.hydra.close_session(&end.sender_hydra_id)?,
            "pending" => runtime.hydra.abort_handshake(&end.sender_hydra_id)?,
            _ => {}
        }
        retire_kktp_binding(runtime, &end.sender_hydra_id);
        clear_peer_handshake_state(runtime, &end.sender_hydra_id);
        runtime.blocked_peers.insert(end.sender_hydra_id.clone());
    } else {
        remember_retired_kktp_sid(runtime, end.sid.clone());
    }
    let peer_label = runtime
        .peer_routes
        .get(&end.sender_hydra_id)
        .map(|route| route.display_name.clone())
        .unwrap_or_default();
    runtime.peer_routes.insert(
        end.sender_hydra_id.clone(),
        PeerRoute {
            kaspa_address: end.sender_kaspa_address.clone(),
            display_name: peer_label.clone(),
            session_sid: None,
        },
    );

    Ok(HydraMailboxResult {
        received: None,
        control: None,
        recovery: None,
        incoming_request: None,
        contact_accepted: None,
        peer_address: Some(end.sender_kaspa_address.clone()),
        peer_label: if peer_label.is_empty() { None } else { Some(peer_label) },
        message_id: None,
        delivery_ack: None,
        delivery_ack_peer: None,
        session_established_peer: None,
        session_ended: Some(HydraSessionEndedProjection {
            peer_hydra_id: end.sender_hydra_id,
            peer_address: end.sender_kaspa_address,
            sid: end.sid,
            reason: end.reason,
        }),
        discard: true,
    })
}

#[tauri::command]
pub async fn hydra_receive_mailbox(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    envelope_hex: String,
    local_kaspa_addresses: Vec<String>,
    active_session_sids: Vec<String>,
) -> Result<HydraMailboxResult, String> {
    require_password(&password)?;
    if envelope_hex.len() % 2 != 0
        || envelope_hex.len() > MAX_MAILBOX_ENVELOPE_BYTES.saturating_mul(2)
    {
        return Err("mailbox envelope hex length is invalid".into());
    }
    let envelope = hex::decode(envelope_hex)
        .map_err(|_| "mailbox envelope is not valid hex".to_string())?;
    if envelope.len() < 4 {
        return Err("mailbox envelope header is invalid".into());
    }
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    let mut allowed_kktp_sids = HashSet::with_capacity(active_session_sids.len());
    for sid in active_session_sids {
        if sid.len() != 32 || !sid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("active Ghost Talk session SID must be exactly 32 hexadecimal characters".into());
        }
        allowed_kktp_sids.insert(sid.to_ascii_lowercase());
    }
    runtime.allowed_kktp_sids = allowed_kktp_sids;

    let kktp_anchor_kind = if envelope.starts_with(KKTP_ANCHOR_PREFIX) {
        kktp_anchor_type(&envelope)?
    } else {
        None
    };
    let carrier_kind = kktp_anchor_kind.clone().unwrap_or_else(|| {
        if envelope.starts_with(KKTP_MESSAGE_PREFIX) { "message".to_string() }
        else if envelope.starts_with(&ghost_protocol::GTACK_MAGIC) { "delivery_ack".to_string() }
        else if envelope.starts_with(&ghost_protocol::GTCR_MAGIC) { "legacy_discovery".to_string() }
        else if envelope.starts_with(&ghost_protocol::GTCA_MAGIC) { "legacy_response".to_string() }
        else { "other".to_string() }
    });
    crate::debug_log::record(
        "info",
        "mailbox",
        "carrier-processing",
        format!("profile={} kind={} bytes={}", profile_id, carrier_kind, envelope.len()),
    );

    if envelope.starts_with(&ghost_protocol::GTCR_MAGIC)
        || kktp_anchor_kind.as_deref() == Some("discovery")
    {
        let request = ghost_protocol::GhostContactRequest::decode(&envelope)?;
        ghost_kaspa::verify_contact_request(&request)?;
        if request.sender.hydra_identity_id == runtime.identity_id {
            return Ok(discard_result());
        }
        if !local_kaspa_addresses.iter().any(|address| address == &request.recipient_kaspa_address) {
            return Err("contact request is not addressed to this Ghost Talk wallet".into());
        }
        let card = BASE64
            .decode(&request.sender.hydra_contact_card_b64)
            .map_err(|_| "contact-request HYDRA contact card is not valid base64".to_string())?;
        if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
            return Err("contact-request HYDRA contact card size is invalid".into());
        }
        // Preview only. Unsolicited/auto-ignored GTCR packets must not consume
        // HYDRA's persistent bounded contact slots. The exact signed request is
        // retained by the UI and re-verified if the user later accepts it.
        let contact = runtime.hydra.preview_contact(&card)?;
        if !request.sender.hydra_identity_id.is_empty()
            && request.sender.hydra_identity_id != contact.handle
        {
            return Err("contact-request HYDRA identity does not match its authenticated contact card".into());
        }
        if contact.handle == runtime.identity_id {
            return Ok(discard_result());
        }
        crate::debug_log::record(
            "info",
            "handshake",
            "discovery-received-verified",
            format!("profile={} sid={} peer={} destination={}", profile_id, request.request_id, contact.handle, request.recipient_kaspa_address),
        );
        return Ok(HydraMailboxResult {
            received: None,
            control: None,
            recovery: None,
            incoming_request: Some(HydraIncomingRequestProjection {
                request_id: request.request_id,
                peer_address: request.sender.kaspa_address,
                local_address: request.recipient_kaspa_address,
                peer_label: request.sender.display_name,
                peer_hydra_id: contact.handle,
                signed_request_hex: hex::encode(&envelope),
            }),
            contact_accepted: None,
            peer_address: None,
            peer_label: None,
            message_id: None,
            delivery_ack: None,
            delivery_ack_peer: None,
            session_established_peer: None,
            session_ended: None,
            discard: true,
        });
    }

    if envelope.starts_with(&ghost_protocol::GTCA_MAGIC)
        || kktp_anchor_kind.as_deref() == Some("response")
    {
        let accepted = ghost_protocol::GhostContactAccept::decode(&envelope)?;
        ghost_kaspa::verify_contact_accept(&accepted)?;
        if accepted.responder.hydra_identity_id == runtime.identity_id {
            return Ok(discard_result());
        }
        if !local_kaspa_addresses.iter().any(|address| address == &accepted.recipient_kaspa_address) {
            return Err("contact acceptance is not addressed to this Ghost Talk wallet".into());
        }
        if accepted.version == GHOST_KKTP_VERSION
            && !runtime.allowed_kktp_sids.contains(&accepted.request_id.to_ascii_lowercase())
            && !runtime
                .kktp_sessions
                .values()
                .any(|binding| binding.sid == accepted.request_id)
        {
            crate::debug_log::record(
                "warn",
                "mailbox",
                "stale-response-discarded",
                format!("sid={} peer={}", accepted.request_id, accepted.responder.hydra_identity_id),
            );
            return Ok(discard_result());
        }
        let card = BASE64
            .decode(&accepted.responder.hydra_contact_card_b64)
            .map_err(|_| "contact-accept HYDRA contact card is not valid base64".to_string())?;
        if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
            return Err("contact-accept HYDRA contact card size is invalid".into());
        }
        let contact = runtime.hydra.add_contact(&card)?;
        if !accepted.responder.hydra_identity_id.is_empty()
            && accepted.responder.hydra_identity_id != contact.handle
        {
            return Err("contact-accept HYDRA identity does not match its authenticated contact card".into());
        }
        runtime.blocked_peers.remove(&contact.handle);
        // The response fixes A/B role ordering for the entire KKTP session.
        // Reprocessing the exact same signed Response is idempotent: it must
        // never tear down an already-progressing/active ratchet. A response
        // for a SID that this runtime explicitly retired is stale and cannot
        // resurrect that conversation.
        let same_kktp_session = accepted.version == GHOST_KKTP_VERSION
            && runtime
                .kktp_sessions
                .get(&contact.handle)
                .is_some_and(|binding| {
                    binding.sid == accepted.request_id
                        && binding.role == KktpRole::Initiator
                        && binding.state != KktpSessionState::Closed
                });
        if accepted.version == GHOST_KKTP_VERSION
            && !same_kktp_session
            && runtime.retired_kktp_sids.contains(&accepted.request_id)
        {
            return Ok(discard_result());
        }
        if !same_kktp_session {
            match runtime.hydra.session_status(&contact.handle)?.as_str() {
                "active" => runtime.hydra.close_session(&contact.handle)?,
                "pending" => runtime.hydra.abort_handshake(&contact.handle)?,
                _ => {}
            }
            clear_peer_handshake_state(&mut runtime, &contact.handle);
            retire_kktp_binding(&mut runtime, &contact.handle);
            if accepted.version == GHOST_KKTP_VERSION {
                install_kktp_binding(
                    &mut runtime,
                    &contact.handle,
                    accepted.request_id.clone(),
                    KktpRole::Initiator,
                    KktpSessionState::Discovered,
                )?;
            }
        }
        runtime.peer_routes.insert(
            contact.handle.clone(),
            PeerRoute {
                kaspa_address: accepted.responder.kaspa_address.clone(),
                display_name: accepted.responder.display_name.clone(),
                session_sid: Some(accepted.request_id.clone()),
            },
        );
        crate::debug_log::record(
            "info",
            "handshake",
            "response-received-verified",
            format!("profile={} sid={} peer={} recipient={} acceptor={}", profile_id, accepted.request_id, contact.handle, accepted.recipient_kaspa_address, accepted.acceptor_kaspa_address),
        );
        return Ok(HydraMailboxResult {
            received: None,
            control: None,
            recovery: None,
            incoming_request: None,
            contact_accepted: Some(HydraContactAcceptedProjection {
                request_id: accepted.request_id,
                peer_address: accepted.responder.kaspa_address,
                acceptor_address: accepted.acceptor_kaspa_address,
                peer_label: accepted.responder.display_name,
                peer_hydra_id: contact.handle,
            }),
            peer_address: None,
            peer_label: None,
            message_id: None,
            delivery_ack: None,
            delivery_ack_peer: None,
            session_established_peer: None,
            session_ended: None,
            discard: true,
        });
    }

    if kktp_anchor_kind.as_deref() == Some("session_end") {
        let end = KktpSessionEnd::decode(&envelope)?;
        if end.sender_hydra_id == runtime.identity_id {
            return Ok(discard_result());
        }
        return handle_kktp_session_end(&mut runtime, end, &local_kaspa_addresses);
    }

    if kktp_anchor_kind.as_deref() == Some("ghost_handshake") {
        let control = KktpHandshakeControl::decode(&envelope)?;
        return handle_kktp_handshake(&mut runtime, control);
    }

    if envelope.starts_with(KKTP_MESSAGE_PREFIX)
        && !envelope.starts_with(KKTP_ANCHOR_PREFIX)
    {
        let wire = KktpMailboxMessage::decode(&envelope)?;
        if wire.sender_hydra_id == runtime.identity_id {
            return Ok(discard_result());
        }
        return handle_kktp_mailbox_message(&mut runtime, wire);
    }

    if envelope.starts_with(&ghost_protocol::GTACK_MAGIC) {
        let ack = ghost_protocol::GhostDeliveryAck::decode(&envelope)?;
        ghost_kaspa::verify_delivery_ack(&ack)?;
        if ack.signer_hydra_id == identity_id {
            return Ok(discard_result());
        }
        if ack.destination_hydra_id != identity_id {
            return Err("delivery acknowledgement is addressed to another Ghost Talk ID".into());
        }
        if !runtime.hydra.has_contact(&ack.signer_hydra_id)? {
            return Err("delivery acknowledgement signer is not a known HYDRA peer".into());
        }
        let (peer_address, peer_label) = {
            let route = runtime
                .peer_routes
                .get(&ack.signer_hydra_id)
                .ok_or_else(|| "delivery acknowledgement has no verified Kaspa peer route".to_string())?;
            if route.kaspa_address != ack.signer_kaspa_address {
                return Err("delivery acknowledgement signer does not match the verified peer route".into());
            }
            (route.kaspa_address.clone(), route.display_name.clone())
        };
        // The first signed ACK is also the initiator's proof that the responder
        // observed pq_finish and decrypted the embedded first message. Keep the
        // exact FINISH + pending plaintext cached until this point so a missed
        // FINISH or missed ACK can be repaired by byte-for-byte FINISH replay.
        let establishes_session = runtime
            .prepared_completion
            .as_ref()
            .is_some_and(|prepared| {
                prepared.contact_id == ack.signer_hydra_id
                    && prepared.message_id == ack.message_id
            });
        if establishes_session {
            runtime.pending_outbound = None;
            runtime.prepared_completion = None;
            crate::debug_log::record(
                "info",
                "handshake",
                "bootstrap-ack-received-session-active",
                format!("peer={} message={}", ack.signer_hydra_id, ack.message_id),
            );
        }

        // r47 ordinary active-session sends release their prepared carrier as
        // soon as Kaspa accepts the exact transaction; they do not create a
        // second receipt transaction. Keep accepting signed ACKs from r46 peers
        // for compatibility, while pq_finish still uses this ACK as the explicit
        // bootstrap proof that activates the direct session.
        let should_forget = runtime
            .prepared_kktp_deliveries
            .get(&ack.message_id)
            .is_some_and(|prepared| prepared.contact_id == ack.signer_hydra_id);
        if should_forget {
            runtime.prepared_kktp_deliveries.remove(&ack.message_id);
        }
        let acknowledgement_peer = ack.signer_hydra_id.clone();
        return Ok(HydraMailboxResult {
            received: None,
            control: None,
            recovery: None,
            incoming_request: None,
            contact_accepted: None,
            peer_address: Some(peer_address),
            peer_label: Some(peer_label),
            message_id: None,
            delivery_ack: Some(ack.message_id.clone()),
            delivery_ack_peer: Some(acknowledgement_peer.clone()),
            session_established_peer: establishes_session.then_some(acknowledgement_peer),
            session_ended: None,
            discard: true,
        });
    }

    if envelope.starts_with(REALTIME_MAILBOX_MAGIC) {
        if envelope.len() <= 68 {
            return Err("realtime HYDRA carrier is truncated".into());
        }
        let sender_id = hex::encode(&envelope[4..36]);
        let message_id = hex::encode(&envelope[36..52]);
        let sid = hex::encode(&envelope[52..68]);
        if !kktp_sid_is_current(&runtime, &sender_id, &sid) {
            return Ok(HydraMailboxResult {
                received: None,
                control: None,
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: runtime.peer_routes.get(&sender_id).map(|value| value.kaspa_address.clone()),
                peer_label: runtime.peer_routes.get(&sender_id).map(|value| value.display_name.clone()),
                message_id: Some(message_id),
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: None,
                session_ended: None,
                discard: true,
            });
        }
        let binding = runtime
            .kktp_sessions
            .get(&sender_id)
            .cloned()
            .ok_or_else(|| "realtime carrier has no active KKTP binding".to_string())?;
        if binding.state != KktpSessionState::Active || binding.sid != sid {
            return Err("realtime carrier SID does not match the active KKTP session".into());
        }
        let received = runtime.hydra.receive(&envelope[68..], StegoProfile::Off)?;
        let Some(received) = received else {
            return Ok(HydraMailboxResult {
                received: None,
                control: None,
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: runtime.peer_routes.get(&sender_id).map(|value| value.kaspa_address.clone()),
                peer_label: runtime.peer_routes.get(&sender_id).map(|value| value.display_name.clone()),
                message_id: Some(message_id),
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: None,
                session_ended: None,
                discard: true,
            });
        };
        if received.from != sender_id {
            return Err("realtime HYDRA sender does not match the authenticated peer".into());
        }
        let expected_prefix = format!("{REALTIME_INNER_PREFIX}{sid}:");
        let body = received
            .plaintext
            .strip_prefix(&expected_prefix)
            .ok_or_else(|| "realtime HYDRA inner SID binding is invalid".to_string())?
            .to_owned();
        let route = runtime.peer_routes.get(&sender_id).cloned();
        return Ok(HydraMailboxResult {
            received: Some(ReceivedProjection {
                from: sender_id,
                plaintext: body,
                session_sid: Some(sid),
            }),
            control: None,
            recovery: None,
            incoming_request: None,
            contact_accepted: None,
            peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
            peer_label: route.map(|value| value.display_name),
            message_id: Some(message_id),
            delivery_ack: None,
            delivery_ack_peer: None,
            session_established_peer: None,
            session_ended: None,
            discard: true,
        });
    }

    if envelope.starts_with(MAILBOX_ENVELOPE_MAGIC) {
        if envelope.len() <= 53 {
            return Err("HYDRA mailbox message carrier is truncated".into());
        }
        let profile = stego_from_code(envelope[4])?;
        let sender_id = hex::encode(&envelope[5..37]);
        let message_id = hex::encode(&envelope[37..53]);
        if runtime.blocked_peers.contains(&sender_id) {
            return Ok(HydraMailboxResult {
                received: None,
                control: None,
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: runtime.peer_routes.get(&sender_id).map(|value| value.kaspa_address.clone()),
                peer_label: runtime.peer_routes.get(&sender_id).map(|value| value.display_name.clone()),
                message_id: Some(message_id),
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: None,
                session_ended: None,
                discard: true,
            });
        }
        match runtime.hydra.receive(&envelope[53..], profile) {
            Ok(received) => {
                if let Some(message) = &received {
                    if message.from != sender_id {
                        return Err("HYDRA mailbox sender fingerprint does not match the authenticated message".into());
                    }
                }
                let route = runtime.peer_routes.get(&sender_id).cloned();
                return Ok(HydraMailboxResult {
                    received,
                    control: None,
                    recovery: None,
                    incoming_request: None,
                    contact_accepted: None,
                    peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
                    peer_label: route.map(|value| value.display_name),
                    message_id: Some(message_id),
                    delivery_ack: None,
                    delivery_ack_peer: None,
                    session_established_peer: None,
                    session_ended: None,
                    discard: true,
                });
            }
            Err(_) => {
                let status = runtime.hydra.session_status(&sender_id)?;
                if status == "missing" || status == "closed" {
                    if !runtime.hydra.has_contact(&sender_id)? {
                        return Err("stale HYDRA message names an unknown peer".into());
                    }
                    let route = runtime
                        .peer_routes
                        .get(&sender_id)
                        .cloned()
                        .ok_or_else(|| "stale HYDRA message has no verified Kaspa peer route".to_string())?;
                    if let Some(pending) = runtime.pending_recovery.as_ref() {
                        if pending.contact_id == sender_id {
                            let recovery = (!pending.offer_broadcast).then(|| HydraRecoveryProjection {
                                destination: pending.destination.clone(),
                                offer_hex: pending.offer_hex.clone(),
                                sid: pending.sid.clone(),
                                peer_hydra_id: pending.contact_id.clone(),
                            });
                            return Ok(HydraMailboxResult {
                                received: None,
                                control: None,
                                recovery,
                                incoming_request: None,
                                contact_accepted: None,
                                peer_address: Some(route.kaspa_address),
                                peer_label: Some(route.display_name),
                                message_id: Some(message_id),
                                delivery_ack: None,
                                delivery_ack_peer: None,
                                session_established_peer: None,
                                session_ended: None,
                                discard: false,
                            });
                        }
                        return Err("another HYDRA session recovery is already pending".into());
                    }
                    if runtime.pending_outbound.is_some() || runtime.prepared_completion.is_some() {
                        return Err("another HYDRA handshake is already pending".into());
                    }
                    let offer = runtime.hydra.init_handshake(&sender_id)?;
                    let offer_hex = hex::encode(&offer);
                    let recovery_sid = fresh_kktp_sid();
                    runtime.pending_recovery = Some(PendingRecovery {
                        sid: recovery_sid.clone(),
                        contact_id: sender_id.clone(),
                        destination: route.kaspa_address.clone(),
                        offer_hex: offer_hex.clone(),
                        offer_broadcast: false,
                    });
                    return Ok(HydraMailboxResult {
                        received: None,
                        control: None,
                        recovery: Some(HydraRecoveryProjection {
                            destination: route.kaspa_address.clone(),
                            offer_hex,
                            sid: recovery_sid,
                            peer_hydra_id: sender_id.clone(),
                        }),
                        incoming_request: None,
                        contact_accepted: None,
                        peer_address: Some(route.kaspa_address),
                        peer_label: Some(route.display_name),
                        message_id: Some(message_id),
                        delivery_ack: None,
                        delivery_ack_peer: None,
                        session_established_peer: None,
                        session_ended: None,
                        discard: false,
                    });
                }
                // A packet that fails authentication while a current or provisional
                // session exists can never become valid later. Drop it rather than
                // wedging the durable mailbox queue indefinitely.
                return Ok(HydraMailboxResult {
                    received: None,
                    control: None,
                    recovery: None,
                    incoming_request: None,
                    contact_accepted: None,
                    peer_address: runtime.peer_routes.get(&sender_id).map(|value| value.kaspa_address.clone()),
                    peer_label: runtime.peer_routes.get(&sender_id).map(|value| value.display_name.clone()),
                    message_id: Some(message_id),
                    delivery_ack: None,
                    delivery_ack_peer: None,
                    session_established_peer: None,
                    session_ended: None,
                    discard: true,
                });
            }
        }
    }

    if envelope.len() < 5 || !envelope.starts_with(MAILBOX_HANDSHAKE_MAGIC) {
        return Err("mailbox envelope header is invalid".into());
    }

    match envelope[4] {
        HANDSHAKE_OFFER => {
            let (descriptor, offer) = decode_offer(&envelope[5..])?;
            ghost_kaspa::verify_gtcd(&descriptor)?;
            let card = BASE64
                .decode(&descriptor.hydra_contact_card_b64)
                .map_err(|_| "GTCD HYDRA contact card is not valid base64".to_string())?;
            if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
                return Err("GTCD HYDRA contact card size is invalid".into());
            }
            let contact = runtime.hydra.add_contact(&card)?;
            if let Some(pending) = runtime.pending_inbound.as_ref() {
                if pending.contact_id != contact.handle {
                    return Err("another inbound HYDRA handshake is still pending".into());
                }
            }
            let expected_session_sid = runtime
                .peer_routes
                .get(&contact.handle)
                .and_then(|route| route.session_sid.clone());
            runtime.peer_routes.insert(
                contact.handle.clone(),
                PeerRoute {
                    kaspa_address: descriptor.kaspa_address.clone(),
                    display_name: descriptor.display_name.clone(),
                    session_sid: expected_session_sid,
                },
            );
            // Normal Ghost Talk bootstrap has one initiator: the original GTCR
            // requester. Preserve HYDRA's native error/idempotency semantics here
            // instead of silently discarding a competing offer in the wrapper.
            let answer = runtime.hydra.reply_handshake(offer)?;
            if runtime.pending_recovery.as_ref().is_some_and(|value| value.contact_id == contact.handle) {
                runtime.pending_recovery = None;
                runtime.prepared_recovery_finish = None;
            }
            runtime.pending_inbound = Some(PendingInbound {
                sid: String::new(),
                contact_id: contact.handle,
            });
            Ok(HydraMailboxResult {
                received: None,
                control: Some(HydraControlProjection {
                    destination: descriptor.kaspa_address,
                    payloads_hex: frame_control(encode_simple_control(HANDSHAKE_ANSWER, &answer))?.payloads_hex,
                    completes_pending_id: None,
                }),
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: None,
                peer_label: None,
                message_id: None,
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: None,
                session_ended: None,
                discard: true,
            })
        }
        HANDSHAKE_ANSWER => {
            if let Some(prepared) = runtime.prepared_completion.clone() {
                return Ok(HydraMailboxResult {
                    received: None,
                    control: Some(HydraControlProjection {
                        destination: prepared.destination.clone(),
                        payloads_hex: prepared.payloads_hex.clone(),
                        completes_pending_id: Some(prepared.pending_id.clone()),
                    }),
                    recovery: None,
                    incoming_request: None,
                    contact_accepted: None,
                    peer_address: None,
                    peer_label: None,
                    message_id: None,
                    delivery_ack: None,
                    delivery_ack_peer: None,
                    session_established_peer: Some(prepared.contact_id),
                    session_ended: None,
                    discard: true,
                });
            }
            if let Some(prepared) = runtime.prepared_recovery_finish.clone() {
                return Ok(HydraMailboxResult {
                    received: None,
                    control: Some(HydraControlProjection {
                        destination: prepared.destination.clone(),
                        payloads_hex: prepared.payloads_hex.clone(),
                        completes_pending_id: None,
                    }),
                    recovery: None,
                    incoming_request: None,
                    contact_accepted: None,
                    peer_address: None,
                    peer_label: None,
                    message_id: None,
                    delivery_ack: None,
                    delivery_ack_peer: None,
                    session_established_peer: Some(prepared.contact_id),
                    session_ended: None,
                    discard: true,
                });
            }
            if let Some(pending) = runtime.pending_outbound.clone() {
                let finish = match runtime.hydra.finish_handshake(&envelope[5..]) {
                    Ok(finish) => finish,
                    Err(_) => return Ok(discard_result()),
                };
                let profile = parse_stego_profile(&pending.stego_profile)?;
                let envelopes = runtime.hydra.send(&pending.contact_id, pending.body.as_bytes(), profile)?;
                if envelopes.len() != 1 {
                    return Err("first direct message requires multiple HYDRA envelopes; shorten the message and retry".into());
                }
                let payloads_hex = frame_control(encode_finish_message(
                    &finish,
                    profile,
                    &runtime.identity_id,
                    &pending.message_id,
                    &envelopes[0],
                )?)?.payloads_hex;
                runtime.prepared_completion = Some(PreparedCompletion {
                    pending_id: pending.id.clone(),
                    message_id: pending.message_id.clone(),
                    sid: pending.sid.clone(),
                    contact_id: pending.contact_id.clone(),
                    destination: pending.destination.clone(),
                    payloads_hex: payloads_hex.clone(),
                });
                return Ok(HydraMailboxResult {
                    received: None,
                    control: Some(HydraControlProjection {
                        destination: pending.destination.clone(),
                        payloads_hex,
                        completes_pending_id: Some(pending.id.clone()),
                    }),
                    recovery: None,
                    incoming_request: None,
                    contact_accepted: None,
                    peer_address: None,
                    peer_label: None,
                    message_id: None,
                    delivery_ack: None,
                    delivery_ack_peer: None,
                    // finish_handshake has established the initiator ratchet. Surface
                    // that immediately so additional locally queued messages can flush
                    // after the FINISH+first-message carrier is broadcast.
                    session_established_peer: Some(pending.contact_id.clone()),
                    session_ended: None,
                    discard: true,
                });
            }
            if let Some(pending) = runtime.pending_recovery.clone() {
                let finish = match runtime.hydra.finish_handshake(&envelope[5..]) {
                    Ok(finish) => finish,
                    Err(_) => return Ok(discard_result()),
                };
                let payloads_hex = frame_control(encode_simple_control(HANDSHAKE_FINISH_ONLY, &finish))?.payloads_hex;
                runtime.prepared_recovery_finish = Some(PreparedRecoveryFinish {
                    sid: pending.sid.clone(),
                    contact_id: pending.contact_id.clone(),
                    destination: pending.destination.clone(),
                    payloads_hex: payloads_hex.clone(),
                });
                return Ok(HydraMailboxResult {
                    received: None,
                    control: Some(HydraControlProjection {
                        destination: pending.destination,
                        payloads_hex,
                        completes_pending_id: None,
                    }),
                    recovery: None,
                    incoming_request: None,
                    contact_accepted: None,
                    peer_address: None,
                    peer_label: None,
                    message_id: None,
                    delivery_ack: None,
                    delivery_ack_peer: None,
                    session_established_peer: None,
                    session_ended: None,
                    discard: true,
                });
            }
            Ok(discard_result())
        }
        HANDSHAKE_FINISH_MESSAGE => {
            let (finish, profile, sender_id, message_id, message_envelope) =
                decode_finish_message(&envelope[5..])?;
            let Some(pending) = runtime.pending_inbound.clone() else {
                return Ok(discard_result());
            };
            if pending.contact_id != sender_id {
                return Err("HYDRA finish-message sender does not match the pending handshake".into());
            }
            if runtime.hydra.accept_finish(finish).is_err() {
                return Ok(discard_result());
            }
            let received = runtime.hydra.receive(message_envelope, profile)?;
            if received.as_ref().is_some_and(|message| message.from != sender_id) {
                return Err("HYDRA finish-message sender fingerprint mismatch".into());
            }
            runtime.pending_inbound = None;
            let route = runtime.peer_routes.get(&sender_id).cloned();
            Ok(HydraMailboxResult {
                received,
                control: None,
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
                peer_label: route.map(|value| value.display_name),
                message_id: Some(message_id),
                delivery_ack: None,
                delivery_ack_peer: None,
                // FINISH + first message establishes the responder's session just
                // as definitively as FINISH_ONLY. Surface that transition so queued
                // replies are flushed immediately instead of waiting for a timer.
                session_established_peer: Some(sender_id),
                session_ended: None,
                discard: true,
            })
        }
        HANDSHAKE_FINISH_ONLY => {
            let Some(pending) = runtime.pending_inbound.clone() else {
                return Ok(discard_result());
            };
            if runtime.hydra.accept_finish(&envelope[5..]).is_err() {
                return Ok(discard_result());
            }
            if runtime.hydra.session_status(&pending.contact_id)? != "active" {
                return Err("HYDRA recovery FINISH did not establish the expected session".into());
            }
            runtime.pending_inbound = None;
            Ok(HydraMailboxResult {
                received: None,
                control: None,
                recovery: None,
                incoming_request: None,
                contact_accepted: None,
                peer_address: runtime.peer_routes.get(&pending.contact_id).map(|value| value.kaspa_address.clone()),
                peer_label: runtime.peer_routes.get(&pending.contact_id).map(|value| value.display_name.clone()),
                message_id: None,
                delivery_ack: None,
                delivery_ack_peer: None,
                session_established_peer: Some(pending.contact_id),
                session_ended: None,
                discard: true,
            })
        }
        _ => Err("unknown Ghost Talk HYDRA handshake stage".into()),
    }
}

fn discard_result() -> HydraMailboxResult {
    HydraMailboxResult {
        received: None,
        control: None,
        recovery: None,
        incoming_request: None,
        contact_accepted: None,
        peer_address: None,
        peer_label: None,
        message_id: None,
        delivery_ack: None,
        delivery_ack_peer: None,
        session_established_peer: None,
        session_ended: None,
        discard: true,
    }
}

pub(super) fn frame_control(carrier: Vec<u8>) -> Result<PreparedMailbox, String> {
    if carrier.len() > MAX_MAILBOX_ENVELOPE_BYTES {
        return Err("HYDRA control envelope exceeds the configured carrier limit".into());
    }
    let packet = ghost_core::Id128::new_random();
    let frames = ghost_protocol::fragment(packet, &carrier)?;
    if frames.len() > MAX_MAILBOX_TRANSACTIONS {
        return Err("HYDRA control envelope requires too many Kaspa mailbox transactions".into());
    }
    Ok(PreparedMailbox {
        payloads_hex: frames.into_iter().map(hex::encode).collect(),
    })
}

fn encode_simple_control(stage: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + body.len());
    out.extend_from_slice(MAILBOX_HANDSHAKE_MAGIC);
    out.push(stage);
    out.extend_from_slice(body);
    out
}

fn decode_offer(bytes: &[u8]) -> Result<(GhostContactDescriptor, &[u8]), String> {
    if bytes.len() < 4 {
        return Err("HYDRA handshake offer descriptor length is missing".into());
    }
    let length = u32::from_le_bytes(bytes[..4].try_into().map_err(|_| "invalid GTCD length".to_string())?) as usize;
    if length == 0 || length > bytes.len().saturating_sub(4) {
        return Err("HYDRA handshake offer GTCD length is invalid".into());
    }
    let descriptor = GhostContactDescriptor::decode(&bytes[4..4 + length])?;
    let offer = &bytes[4 + length..];
    if offer.is_empty() {
        return Err("HYDRA handshake offer is empty".into());
    }
    Ok((descriptor, offer))
}

fn encode_finish_message(
    finish: &[u8],
    profile: StegoProfile,
    sender_identity_id: &str,
    message_id: &str,
    envelope: &[u8],
) -> Result<Vec<u8>, String> {
    let sender = decode_fixed_hex::<32>(sender_identity_id, "HYDRA sender identity")?;
    let message = decode_fixed_hex::<16>(message_id, "Ghost Talk message id")?;
    let length = u32::try_from(finish.len()).map_err(|_| "HYDRA finish is too large".to_string())?;
    let mut out = Vec::with_capacity(58 + finish.len() + envelope.len());
    out.extend_from_slice(MAILBOX_HANDSHAKE_MAGIC);
    out.push(HANDSHAKE_FINISH_MESSAGE);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(finish);
    out.push(stego_code(profile));
    out.extend_from_slice(&sender);
    out.extend_from_slice(&message);
    out.extend_from_slice(envelope);
    Ok(out)
}

fn decode_finish_message(bytes: &[u8]) -> Result<(&[u8], StegoProfile, String, String, &[u8]), String> {
    if bytes.len() < 53 {
        return Err("HYDRA finish-message envelope is truncated".into());
    }
    let length = u32::from_le_bytes(bytes[..4].try_into().map_err(|_| "invalid HYDRA finish length".to_string())?) as usize;
    if length == 0 || bytes.len() <= 53 + length {
        return Err("HYDRA finish-message length is invalid".into());
    }
    let finish = &bytes[4..4 + length];
    let profile = stego_from_code(bytes[4 + length])?;
    let sender_start = 5 + length;
    let message_start = sender_start + 32;
    let envelope_start = message_start + 16;
    if bytes.len() <= envelope_start {
        return Err("HYDRA first-message envelope is empty".into());
    }
    Ok((
        finish,
        profile,
        hex::encode(&bytes[sender_start..message_start]),
        hex::encode(&bytes[message_start..envelope_start]),
        &bytes[envelope_start..],
    ))
}

fn decode_fixed_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} must be exactly {} hexadecimal characters", N * 2));
    }
    let decoded = hex::decode(value).map_err(|_| format!("{label} is not valid hex"))?;
    decoded.try_into().map_err(|_| format!("{label} has the wrong size"))
}

fn profile_path(app: &AppHandle, profile_id: &str) -> Result<PathBuf, String> {
    if profile_id.is_empty()
        || profile_id.len() > 80
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("invalid local profile id".into());
    }
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Ghost Talk app-data directory: {error}"))?;
    Ok(root.join("hydra").join(profile_id))
}

fn require_password(password: &str) -> Result<(), String> {
    if password.len() < 8 {
        return Err("ID password must be at least 8 characters".into());
    }
    Ok(())
}

pub(super) fn parse_stego_profile(value: &str) -> Result<StegoProfile, String> {
    match value {
        "Off" => Ok(StegoProfile::Off),
        "Deterministic" => Ok(StegoProfile::Deterministic),
        "Fast Unicode" => Ok(StegoProfile::FastUnicode),
        "Fast Hybrid" => Ok(StegoProfile::FastHybrid),
        "Arithmetic" => Ok(StegoProfile::Arithmetic),
        _ => Err("unknown HYDRA stego profile".into()),
    }
}

fn stego_code(profile: StegoProfile) -> u8 {
    match profile {
        StegoProfile::Off => 0,
        StegoProfile::Deterministic => 1,
        StegoProfile::FastUnicode => 2,
        StegoProfile::FastHybrid => 3,
        StegoProfile::Arithmetic => 4,
    }
}

fn stego_from_code(value: u8) -> Result<StegoProfile, String> {
    match value {
        0 => Ok(StegoProfile::Off),
        1 => Ok(StegoProfile::Deterministic),
        2 => Ok(StegoProfile::FastUnicode),
        3 => Ok(StegoProfile::FastHybrid),
        4 => Ok(StegoProfile::Arithmetic),
        _ => Err("mailbox envelope uses an unknown stego profile".into()),
    }
}
