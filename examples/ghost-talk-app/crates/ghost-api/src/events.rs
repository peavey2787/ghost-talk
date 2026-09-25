use crate::{PublicGhostProfile, WalletSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxEvent {
    pub transaction_id: String,
    #[serde(default)]
    pub blue_score: String,
    pub payload_hex: String,
    pub block_time: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkStatusEvent {
    pub profile_id: String,
    pub status: String,
    pub reconnect_attempts: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WalletLiveEvent {
    pub profile_id: String,
    pub snapshot: Option<WalletSnapshot>,
    pub checkpoint: String,
    #[serde(default)]
    pub mailbox: Vec<MailboxEvent>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DirectoryLiveEvent {
    pub profile_id: String,
    pub directory_checkpoint: String,
    #[serde(default)]
    pub public_profiles: Vec<PublicGhostProfile>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugLogEntry {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub level: String,
    pub category: String,
    pub event: String,
    pub details: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugLogSnapshot {
    pub enabled: bool,
    pub latest_sequence: u64,
    #[serde(default)]
    pub entries: Vec<DebugLogEntry>,
}
