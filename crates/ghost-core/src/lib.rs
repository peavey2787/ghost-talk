#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

pub const APP_NAME: &str = "Ghost Talk";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Logical Ghost Talk carrier/message ceiling. Portal 1.0.1 can carry up to
/// KSPT v1's u16 payload limit in one transaction; larger logical carriers are
/// split only when they genuinely exceed that wire-format boundary.
pub const MAX_GHOST_TX_PAYLOAD: usize = 80 * 1024;
/// KSPT v1 payload-length field ceiling used by Kaspa Portal 1.0.1.
pub const MAX_KSPT_V1_PAYLOAD_BYTES: usize = u16::MAX as usize;
pub const MAX_EVENT_BYTES: usize = 512 * 1024;
pub const MAX_FRAGMENTS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Id128(pub [u8; 16]);

impl Id128 {
    pub fn new_random() -> Self {
        Self(rand::random())
    }
}

impl fmt::Display for Id128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for Id128 {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s).map_err(|e| e.to_string())?;
        let value: [u8; 16] = bytes
            .try_into()
            .map_err(|_| "expected 16-byte id".to_string())?;
        Ok(Self(value))
    }
}

pub type AccountId = Id128;
pub type ContactId = Id128;
pub type ConversationId = Id128;
pub type RoomId = Id128;
pub type EventId = Id128;
pub type PacketId = Id128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Network {
    Mainnet,
    Testnet10,
    Testnet11,
    Custom,
}

impl Network {
    pub fn kaspa_prefix(self) -> &'static str {
        match self {
            Self::Mainnet => "kaspa",
            Self::Testnet10 | Self::Testnet11 => "kaspatest",
            Self::Custom => "kaspa",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KaspaAddress(String);

impl KaspaAddress {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if !(s.starts_with("kaspa:") || s.starts_with("kaspatest:")) {
            return Err("unsupported Kaspa address prefix".into());
        }
        if s.len() < 20 || s.len() > 128 {
            return Err("Kaspa address length".into());
        }
        if !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b':') {
            return Err("invalid Kaspa address characters".into());
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
