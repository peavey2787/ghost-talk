use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use super::super::{
    delivery_state::{PreparedCompletion, PreparedKktpDelivery},
    session_types::{
        HydraFacade, KktpSessionBinding, PeerRoute, PendingInbound, PendingOutbound,
        PendingRecovery, PreparedRecoveryFinish, Zeroizing,
    },
};

pub(crate) struct HydraProfileRuntime {
    pub identity_id: String,
    pub hydra: HydraFacade,
    /// Encrypted Ghost Talk persistent transport sidecar. The key is derived
    /// from the wallet-derived HYDRA identity seed and zeroized with the runtime.
    pub transport_state_path: PathBuf,
    pub transport_state_key: Zeroizing<[u8; 32]>,
    /// Handshake state is peer-scoped. Multiple room members can establish or
    /// restore transports independently without overwriting one another.
    pub pending_outbound: HashMap<String, PendingOutbound>,
    pub prepared_completion: HashMap<String, PreparedCompletion>,
    pub pending_recovery: HashMap<String, PendingRecovery>,
    pub prepared_recovery_finish: HashMap<String, PreparedRecoveryFinish>,
    pub pending_inbound: HashMap<String, PendingInbound>,
    pub prepared_kktp_deliveries: HashMap<String, PreparedKktpDelivery>,
    pub kktp_sessions: HashMap<String, KktpSessionBinding>,
    pub retired_kktp_sids: HashSet<String>,
    /// Active/pending conversation SIDs supplied by the persisted UI model on
    /// every mailbox drain, keyed by authenticated HYDRA peer. Historical or
    /// cross-peer on-chain traffic cannot borrow another peer's valid SID.
    pub allowed_kktp_sids: HashMap<String, HashSet<String>>,
    /// Discovery/contact-request SIDs that this native runtime has actually
    /// prepared for publication and is willing to accept a signed response for.
    /// This must not depend on whichever frontend chat surface currently wins
    /// canonical-SID projection: calls, rooms, and direct chats can coexist for
    /// one peer while a new request is awaiting acceptance.
    pub pending_contact_request_sids: HashSet<String>,
    pub peer_routes: HashMap<String, PeerRoute>,
    pub blocked_peers: HashSet<String>,
}
