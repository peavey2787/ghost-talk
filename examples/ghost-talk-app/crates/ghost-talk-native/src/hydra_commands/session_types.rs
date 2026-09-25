use super::{
    handshake_admission::reset_hydra_peer_crypto,
    mailbox_dispatch::decode_fixed_hex,
    runtime_session_queries::clear_peer_handshake_state,
    runtime_state::HydraProfileRuntime,
    session_state::{install_kktp_binding, retire_kktp_binding},
    transport_persistence::persist_transport_state,
};
pub(crate) use aes_gcm::{Aes256Gcm, Nonce};
pub(crate) use base64::engine::general_purpose::STANDARD as BASE64;
pub(crate) use ghost_api::{
    HydraCallSignalProjection, HydraContactAcceptedProjection, HydraControlProjection,
    HydraIncomingRequestProjection, HydraMailboxResult, HydraReady,
    HydraRecoveryProjection, HydraRoomInviteProjection, HydraSessionBindingProjection,
    HydraSessionEndedProjection, PeerRouteRegistration,
};
pub(crate) use ghost_hydra::{HydraFacade, ReceivedProjection, StegoProfile};
pub(crate) use ghost_kaspa::wallet::WalletPublic;
pub(crate) use ghost_protocol::{
    kktp_anchor_type, kktp_mailbox_id, GhostReactionEvent, KktpDirection, KktpFirstMessage,
    KktpHandshakeControl, KktpInnerMessage, KktpMailboxMessage, KktpSessionEnd, GHOST_KKTP_VERSION,
    KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX,
};
pub(crate) use rand::rngs::OsRng;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use sha2::{Digest, Sha256};
pub(crate) use tauri::{AppHandle, State};
pub(crate) use tokio::sync::Mutex as AsyncMutex;
pub(crate) use zeroize::{Zeroize, Zeroizing};

pub(crate) const MAX_MAILBOX_ENVELOPE_BYTES: usize = ghost_core::MAX_MAILBOX_ENVELOPE_BYTES;
pub(crate) const MAX_MAILBOX_TRANSACTIONS: usize = ghost_core::MAX_FRAGMENTS;
pub(crate) const MAX_CONTACT_CARD_BYTES: usize = 64 * 1024;
pub(crate) const PERSISTENT_TRANSPORT_MAGIC: &[u8; 4] = b"GTP1";
pub(crate) const PERSISTENT_REALTIME_MAGIC: &[u8; 4] = b"GTRP";
pub(crate) const PERSISTENT_TRANSPORT_STATE_MAGIC: &[u8] = b"GHOST-TALK-TRANSPORT-STATE-V1\n";
pub(crate) const PERSISTENT_TRANSPORT_STATE_FILE: &str = "ghost-transport.v1";
pub(crate) const MAX_PERSISTENT_TRANSPORT_STATE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct PreparedMailbox {
    pub payloads_hex: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingOutbound {
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
pub(crate) struct PendingRecovery {
    pub sid: String,
    pub contact_id: String,
    pub destination: String,
    pub offer_hex: String,
    pub offer_broadcast: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedRecoveryFinish {
    pub sid: String,
    pub contact_id: String,
    pub destination: String,
    pub payloads_hex: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingInbound {
    pub sid: String,
    pub contact_id: String,
    /// Digest of the authenticated opaque HYDRA INIT payload for KKTP PQ
    /// handshakes. A restart-successor SID is deterministic, so two process
    /// attempts can legitimately reuse the SID while carrying different
    /// ephemeral HYDRA offers. This digest lets the newer offer replace an
    /// abandoned pending offer instead of being mistaken for a replay.
    pub init_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActiveSessionRegistration {
    pub peer_hydra_id: String,
    pub sid: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PeerRoute {
    pub kaspa_address: String,
    pub display_name: String,
    /// UI-persisted active conversation SID used to reject historical/replayed
    /// KKTP traffic after native volatile session state is rebuilt.
    pub session_sid: Option<String>,
    pub session_role: Option<KktpRole>,
    pub resume_required: bool,
}

impl PeerRoute {
    pub(crate) fn session_sid(&self) -> Option<&str> {
        self.session_sid.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KktpRole {
    Initiator,
    Responder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KktpSessionState {
    Discovered,
    Handshake,
    Active,
    Closed,
}

pub(crate) const MAX_RETIRED_KKTP_SIDS: usize = 4096;
pub(crate) const MAX_PREPARED_KKTP_DELIVERIES: usize = 4096;

#[derive(Clone)]
pub(crate) struct PersistentTransportState {
    pub send_chain_key: [u8; 32],
    pub recv_chain_key: [u8; 32],
    pub realtime_send_key: [u8; 32],
    pub realtime_recv_key: [u8; 32],
}

impl Drop for PersistentTransportState {
    fn drop(&mut self) {
        self.send_chain_key.zeroize();
        self.recv_chain_key.zeroize();
        self.realtime_send_key.zeroize();
        self.realtime_recv_key.zeroize();
    }
}

#[derive(Clone)]
pub(crate) struct KktpSessionBinding {
    pub sid: String,
    pub role: KktpRole,
    pub peer_hydra_id: String,
    pub mailbox_id: String,
    pub send_seq: u64,
    pub recv_next_seq: u64,
    pub state: KktpSessionState,
    /// Ghost Talk-owned restart ratchet established inside the authenticated
    /// HYDRA session. HYDRA a8b4b317 deliberately does not persist live
    /// sessions, so this state lets the already-established conversation resume
    /// locally after a process restart without another paid Kaspa handshake.
    pub persistent_transport: Option<PersistentTransportState>,
}

impl KktpSessionBinding {
    pub(crate) fn outbound_direction(&self) -> KktpDirection {
        match self.role {
            KktpRole::Initiator => KktpDirection::AtoB,
            KktpRole::Responder => KktpDirection::BtoA,
        }
    }

    pub(crate) fn inbound_direction(&self) -> KktpDirection {
        self.outbound_direction().opposite()
    }
}

pub(crate) fn fresh_kktp_sid() -> String {
    ghost_core::Id128::new_random().to_string()
}

/// Derive the only valid restart-successor SID for an established persisted
/// session. Both peers can compute it from the last peer-confirmed SID, so the
/// responder never has to accept an arbitrary fresh signed SID after restart.
pub(crate) fn restart_successor_sid(
    prior_sid: &str,
    initiator_hydra_id: &str,
    responder_hydra_id: &str,
) -> Result<String, String> {
    let prior = decode_fixed_hex::<16>(prior_sid, "persisted KKTP SID")?;
    let initiator = decode_fixed_hex::<32>(initiator_hydra_id, "KKTP initiator HYDRA id")?;
    let responder = decode_fixed_hex::<32>(responder_hydra_id, "KKTP responder HYDRA id")?;
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/KKTP/restart-successor/v1\0");
    hasher.update(prior);
    hasher.update(initiator);
    hasher.update(responder);
    let digest = hasher.finalize();
    Ok(hex::encode(&digest[..16]))
}

pub(crate) fn make_kktp_binding(
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
        persistent_transport: None,
    })
}

pub(crate) fn prepare_restart_successor(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
) -> Result<bool, String> {
    let Some(route) = runtime
        .peer_routes
        .get(peer_hydra_id)
        .filter(|route| route.resume_required)
        .cloned()
    else {
        return Ok(false);
    };
    let prior_sid = route
        .session_sid
        .as_deref()
        .ok_or_else(|| "restart restore is missing the persisted KKTP SID".to_string())?;
    let role = route
        .session_role
        .ok_or_else(|| "restart restore is missing the persisted KKTP role".to_string())?;
    (role == KktpRole::Initiator)
        .then_some(())
        .ok_or_else(|| "Waiting for the original chat initiator to restore the encrypted peer transport after restart".to_string())?;
    let successor = restart_successor_sid(prior_sid, &runtime.identity_id, peer_hydra_id)?;
    if runtime
        .kktp_sessions
        .get(peer_hydra_id)
        .is_some_and(|binding| {
            binding.sid == successor
                && binding.role == KktpRole::Initiator
                && matches!(
                    binding.state,
                    KktpSessionState::Discovered
                        | KktpSessionState::Handshake
                        | KktpSessionState::Active
                )
        })
    {
        return Ok(true);
    }
    reset_hydra_peer_crypto(runtime, peer_hydra_id)?;
    clear_peer_handshake_state(runtime, peer_hydra_id);
    retire_kktp_binding(runtime, peer_hydra_id);
    install_kktp_binding(
        runtime,
        peer_hydra_id,
        successor,
        KktpRole::Initiator,
        KktpSessionState::Discovered,
    )?;
    // The old persisted resumption secret is no longer valid once we replace
    // the session with a successor handshake. Commit that retirement before
    // any new handshake packet can be published.
    persist_transport_state(runtime)?;
    Ok(true)
}
