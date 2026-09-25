use serde::{Deserialize, Serialize};

pub const MAX_SEEN_CALL_SIGNAL_IDS: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallDirection {
    #[default]
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallPhase {
    #[default]
    Ended,
    IncomingRinging,
    OutgoingRinging,
    Answering,
    Connecting,
    Connected,
    Ending,
    Failed,
}

impl CallPhase {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::IncomingRinging
                | Self::OutgoingRinging
                | Self::Answering
                | Self::Connecting
                | Self::Connected
        )
    }
    pub fn is_visible(self) -> bool {
        !matches!(self, Self::Ended | Self::Ending)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRecord {
    pub call_id: String,
    #[serde(default)]
    pub chat_id: Option<String>,
    pub peer_address: String,
    #[serde(default)]
    pub peer_hydra_id: String,
    pub peer_label: String,
    pub direction: CallDirection,
    pub phase: CallPhase,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub bootstrap_request_id: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallEvent {
    AcceptLocal,
    AcceptRemote,
    TransportConnected,
    SetMuted(bool),
    Decline,
    Cancel,
    Hangup,
    CompleteEnd,
    Fail,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalDisposition {
    Applied,
    Busy,
    Duplicate,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSignal {
    pub call_id: String,
    pub action: String,
    pub peer_address: String,
    pub peer_hydra_id: String,
    pub peer_label: String,
}
