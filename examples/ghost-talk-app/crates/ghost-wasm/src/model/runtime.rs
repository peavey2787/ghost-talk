use super::{Deserialize, Serialize};
use ghost_api::HydraContactAcceptedProjection;

pub(crate) use ghost_api::{
    BackupContact, BackupMessage, BroadcastResult, DebugLogEntry, DebugLogSnapshot,
    HydraCallSignalProjection, HydraControlProjection, HydraMailboxResult, HydraRealtimeEnvelope,
    HydraSessionBindingProjection, KasKoldBackupResult, PeerRouteRegistration,
    ProfileBackupPublishResult, ProfileBackupRestoreResult, PublishedGhostDescriptor,
};

#[derive(Clone, Debug, PartialEq)]
pub enum CallRuntimeEvent {
    SignedSignal(HydraCallSignalProjection),
    ContactAccepted(HydraContactAcceptedProjection),
}

impl CallRuntimeEvent {
    pub fn id(&self) -> String {
        match self {
            Self::SignedSignal(signal) => format!("signal:{}", signal.signal_id),
            Self::ContactAccepted(accepted) => format!("accept:{}", accepted.request_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RealtimeControl {
    pub id: String,
    pub chat_id: String,
    pub session_sid: String,
    pub body: String,
}
