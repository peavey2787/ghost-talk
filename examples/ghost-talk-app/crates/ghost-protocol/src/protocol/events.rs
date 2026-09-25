use ghost_media::MediaReference;
mod contact_request_decode;
use contact_request_decode::{
    decode_contact_request_anchor, decode_contact_request_v1, validate_canonical_contact_request,
    validate_contact_request_payload_size, validate_decoded_contact_request,
};

use super::{
    canonical_json::canonical_json,
    kktp_codec::{
        decode_kktp_anchor, encode_kktp_anchor, validate_call_invite_context,
        validate_room_invite_context, validate_sid,
    },
    kktp_validation_types::{KktpDiscoveryOwned, KktpDiscoverySigning, KktpDiscoveryWire},
};
pub(crate) use base64::engine::general_purpose::STANDARD as BASE64;
pub(crate) use ghost_core::{
    ConversationId, EventId, Id128, PacketId, RoomId, MAX_EVENT_BYTES, MAX_FRAGMENTS,
    MAX_GHOST_TX_PAYLOAD, MAX_KSPT_V1_PAYLOAD_BYTES,
};
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use sha3::Sha3_256;

pub const EVENT_VERSION: u16 = 1;
pub const GHST_MAGIC: [u8; 4] = *b"GHST";
pub const GHST_VERSION: u8 = 1;
pub const GHST_HEADER: usize = 30;
/// Maximum logical bytes carried by one KSPT-v1 transaction after the
/// Ghost Talk frame header. Ordinary HYDRA carriers now fit in one transaction.
pub const GHST_DATA_MAX: usize = MAX_KSPT_V1_PAYLOAD_BYTES - GHST_HEADER;
pub const GTCD_MAGIC: [u8; 4] = *b"GTCD";
pub const GTCD_VERSION: u16 = 1;
pub const GTACK_MAGIC: [u8; 4] = *b"GTAK";
pub const GTCR_MAGIC: [u8; 4] = *b"GTCR";
pub const GTVA_MAGIC: [u8; 4] = *b"GTVA";
pub const GHOST_KKTP_VERSION: u16 = 2;
pub const KKTP_ANCHOR_PREFIX: &[u8] = b"KKTP:ANCHOR:";
pub const KKTP_MESSAGE_PREFIX: &[u8] = b"KKTP:";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatEvent {
    pub version: u16,
    pub id: EventId,
    pub conversation: ConversationId,
    pub kind: EventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum EventKind {
    Text {
        body: String,
        reply_to: Option<EventId>,
    },
    Reaction {
        target: EventId,
        emoji: String,
    },
    Receipt {
        target: EventId,
        read: bool,
    },
    FriendInvite {
        close_friend: bool,
    },
    FriendAccept {
        close_friend: bool,
    },
    FriendDecline,
    RoomInvite {
        room: RoomId,
        label: String,
    },
    RoomRole {
        room: RoomId,
        role: String,
    },
    Game {
        game: String,
        action: String,
        data: String,
    },
    ProfileUpdate {
        display_name: String,
    },
}

impl ChatEvent {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let v = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if v.len() > MAX_EVENT_BYTES {
            return Err("event too large".into());
        }
        Ok(v)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() > MAX_EVENT_BYTES {
            return Err("event too large".into());
        }
        let event: Self = serde_json::from_slice(v).map_err(|e| e.to_string())?;
        if event.version != EVENT_VERSION {
            return Err("unsupported event version".into());
        }
        Ok(event)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarrierFrame {
    pub packet: PacketId,
    pub index: u16,
    pub count: u16,
    pub payload: Vec<u8>,
}

impl CarrierFrame {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.count == 0 || self.index >= self.count || self.count as usize > MAX_FRAGMENTS {
            return Err("invalid fragment metadata".into());
        }
        if self.payload.len() > GHST_DATA_MAX {
            return Err("fragment payload too large".into());
        }
        let mut out = Vec::with_capacity(GHST_HEADER + self.payload.len());
        out.extend_from_slice(&GHST_MAGIC);
        out.push(GHST_VERSION);
        out.push(0);
        out.extend_from_slice(&self.packet.0);
        out.extend_from_slice(&self.index.to_le_bytes());
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() < GHST_HEADER || v[..4] != GHST_MAGIC {
            return Err("not GHST".into());
        }
        if v[4] != GHST_VERSION {
            return Err("unsupported GHST version".into());
        }
        let mut packet_bytes = [0u8; 16];
        packet_bytes.copy_from_slice(&v[6..22]);
        let packet = Id128(packet_bytes);
        let index = u16::from_le_bytes([v[22], v[23]]);
        let count = u16::from_le_bytes([v[24], v[25]]);
        let len = u32::from_le_bytes([v[26], v[27], v[28], v[29]]) as usize;
        if count == 0
            || index >= count
            || count as usize > MAX_FRAGMENTS
            || len > GHST_DATA_MAX
            || v.len() != GHST_HEADER + len
        {
            return Err("invalid GHST bounds".into());
        }
        Ok(Self {
            packet,
            index,
            count,
            payload: v[30..].to_vec(),
        })
    }
}

pub fn fragment(packet: PacketId, data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let count = data.len().max(1).div_ceil(GHST_DATA_MAX);
    if count > MAX_FRAGMENTS {
        return Err("message requires too many Kaspa transactions".into());
    }
    let mut out = Vec::with_capacity(count);
    if data.is_empty() {
        out.push(
            CarrierFrame {
                packet,
                index: 0,
                count: 1,
                payload: vec![],
            }
            .encode()?,
        );
        return Ok(out);
    }
    for (i, chunk) in data.chunks(GHST_DATA_MAX).enumerate() {
        out.push(
            CarrierFrame {
                packet,
                index: i as u16,
                count: count as u16,
                payload: chunk.to_vec(),
            }
            .encode()?,
        );
    }
    Ok(out)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhostContactDescriptor {
    pub version: u16,
    pub kaspa_address: String,
    pub hydra_contact_card_b64: String,
    pub display_name: String,
    #[serde(default)]
    pub hydra_identity_id: String,
    #[serde(default)]
    pub discoverable: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub interests: Vec<String>,
    #[serde(default)]
    pub avatar: Option<MediaReference>,
    pub capabilities: Vec<String>,
    pub expires_daa: Option<u64>,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GhostRoomInviteContext {
    pub room_id: String,
    pub room_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GhostCallInviteContext {
    pub call_id: String,
    pub action: String,
}

mod contact_request;
pub use contact_request::GhostContactRequest;
