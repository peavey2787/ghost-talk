//! Realtime control bodies carried inside sealed GTR1 carriers. GTR1 framing,
//! topics and replay identity are owned by `ghost-realtime`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Plaintext prefix binding a realtime body to its exact KKTP SID inside HYDRA.
pub const REALTIME_INNER_PREFIX: &str = "\u{1e}GHOST-REALTIME-V1:";
pub const MAX_P2P_DIAL_ADDRESSES: usize = 16;
pub const MAX_P2P_DIAL_ADDRESS_BYTES: usize = 512;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RealtimeProtocolError {
    #[error("transport announcement contains too many dial addresses")]
    TooManyDialAddresses,
    #[error("transport announcement contains an empty or oversized dial address")]
    InvalidDialAddress,
    #[error("transport announcement has an empty PeerId")]
    EmptyPeerId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeCapability {
    AddressedDelivery,
    RoomBroadcast,
    Voice,
}

/// Authenticated announcement of a peer's p2p-net transport identity. It is
/// only ever sent over an already-authenticated KKTP/HYDRA/Kaspa session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportAnnounceV1 {
    pub peer_id: String,
    pub dial_addresses: Vec<String>,
    pub capabilities: Vec<RealtimeCapability>,
}

impl TransportAnnounceV1 {
    pub fn validate(&self) -> Result<(), RealtimeProtocolError> {
        if self.peer_id.trim().is_empty() {
            return Err(RealtimeProtocolError::EmptyPeerId);
        }
        if self.dial_addresses.len() > MAX_P2P_DIAL_ADDRESSES {
            return Err(RealtimeProtocolError::TooManyDialAddresses);
        }
        if self
            .dial_addresses
            .iter()
            .any(|address| address.is_empty() || address.len() > MAX_P2P_DIAL_ADDRESS_BYTES)
        {
            return Err(RealtimeProtocolError::InvalidDialAddress);
        }
        Ok(())
    }
}

/// Realtime transport control exchanged inside the authenticated session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum RealtimeBodyV1 {
    TransportAnnounce(TransportAnnounceV1),
    TransportAck(TransportAnnounceV1),
}

impl RealtimeBodyV1 {
    pub fn announcement(&self) -> &TransportAnnounceV1 {
        match self {
            Self::TransportAnnounce(value) | Self::TransportAck(value) => value,
        }
    }

    pub fn is_ack(&self) -> bool {
        matches!(self, Self::TransportAck(_))
    }
}
