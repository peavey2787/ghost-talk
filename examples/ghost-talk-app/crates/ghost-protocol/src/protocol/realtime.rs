use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const GTR1_MAGIC: [u8; 4] = *b"GTR1";
pub const REALTIME_INNER_PREFIX: &str = "\u{1e}GHOST-REALTIME-V1:";
pub const GTR1_HEADER_LEN: usize = 68;
pub const MAX_REALTIME_CIPHERTEXT_BYTES: usize = 256 * 1024;
pub const MAX_P2P_DIAL_ADDRESSES: usize = 16;
pub const MAX_P2P_DIAL_ADDRESS_BYTES: usize = 512;
pub const MAX_VOICE_BATCH_FRAMES: usize = 32;
pub const MAX_OPUS_FRAME_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gtr1Envelope {
    pub sender_hydra_id: [u8; 32],
    pub message_id: [u8; 16],
    pub sid: [u8; 16],
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RealtimeProtocolError {
    #[error("GTR1 carrier is truncated")]
    Truncated,
    #[error("GTR1 carrier magic is invalid")]
    InvalidMagic,
    #[error("GTR1 ciphertext is empty or exceeds the carrier limit")]
    InvalidCiphertextLength,
    #[error("{0} must be exact lowercase or uppercase hexadecimal of the required width")]
    InvalidHex(&'static str),
    #[error("transport announcement contains too many dial addresses")]
    TooManyDialAddresses,
    #[error("transport announcement contains an empty or oversized dial address")]
    InvalidDialAddress,
    #[error("voice batch is empty or exceeds the frame limit")]
    InvalidVoiceBatch,
    #[error("Opus frame is empty or exceeds the frame limit")]
    InvalidOpusFrame,
}

impl Gtr1Envelope {
    pub fn new(
        sender_hydra_id: [u8; 32],
        message_id: [u8; 16],
        sid: [u8; 16],
        ciphertext: Vec<u8>,
    ) -> Result<Self, RealtimeProtocolError> {
        validate_ciphertext(&ciphertext)?;
        Ok(Self { sender_hydra_id, message_id, sid, ciphertext })
    }

    pub fn from_hex_ids(
        sender_hydra_id: &str,
        message_id: &str,
        sid: &str,
        ciphertext: Vec<u8>,
    ) -> Result<Self, RealtimeProtocolError> {
        Self::new(
            decode_hex::<32>(sender_hydra_id, "HYDRA sender identity")?,
            decode_hex::<16>(message_id, "Ghost Talk message id")?,
            decode_hex::<16>(sid, "KKTP session id")?,
            ciphertext,
        )
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RealtimeProtocolError> {
        if bytes.len() <= GTR1_HEADER_LEN {
            return Err(RealtimeProtocolError::Truncated);
        }
        if bytes[..4] != GTR1_MAGIC {
            return Err(RealtimeProtocolError::InvalidMagic);
        }
        let ciphertext = bytes[GTR1_HEADER_LEN..].to_vec();
        validate_ciphertext(&ciphertext)?;
        let mut sender_hydra_id = [0u8; 32];
        let mut message_id = [0u8; 16];
        let mut sid = [0u8; 16];
        sender_hydra_id.copy_from_slice(&bytes[4..36]);
        message_id.copy_from_slice(&bytes[36..52]);
        sid.copy_from_slice(&bytes[52..68]);
        Ok(Self { sender_hydra_id, message_id, sid, ciphertext })
    }

    pub fn encode(&self) -> Result<Vec<u8>, RealtimeProtocolError> {
        validate_ciphertext(&self.ciphertext)?;
        let mut out = Vec::with_capacity(GTR1_HEADER_LEN + self.ciphertext.len());
        out.extend_from_slice(&GTR1_MAGIC);
        out.extend_from_slice(&self.sender_hydra_id);
        out.extend_from_slice(&self.message_id);
        out.extend_from_slice(&self.sid);
        out.extend_from_slice(&self.ciphertext);
        Ok(out)
    }

    pub fn sender_hex(&self) -> String { hex::encode(self.sender_hydra_id) }
    pub fn message_id_hex(&self) -> String { hex::encode(self.message_id) }
    pub fn sid_hex(&self) -> String { hex::encode(self.sid) }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeCapability {
    AddressedDelivery,
    RoomBroadcast,
    Voice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransportAnnounceV1 {
    pub peer_id: String,
    pub dial_addresses: Vec<String>,
    pub capabilities: Vec<RealtimeCapability>,
}

impl TransportAnnounceV1 {
    pub fn validate(&self) -> Result<(), RealtimeProtocolError> {
        if self.dial_addresses.len() > MAX_P2P_DIAL_ADDRESSES {
            return Err(RealtimeProtocolError::TooManyDialAddresses);
        }
        if self.dial_addresses.iter().any(|address| {
            address.is_empty() || address.len() > MAX_P2P_DIAL_ADDRESS_BYTES
        }) {
            return Err(RealtimeProtocolError::InvalidDialAddress);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CallControlV1 {
    pub call_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OpusFrameV1 {
    pub sequence: u64,
    pub timestamp_48khz: u64,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VoiceBatchV1 {
    pub stream_id: String,
    pub frames: Vec<OpusFrameV1>,
}

impl VoiceBatchV1 {
    pub fn validate(&self) -> Result<(), RealtimeProtocolError> {
        if self.frames.is_empty() || self.frames.len() > MAX_VOICE_BATCH_FRAMES {
            return Err(RealtimeProtocolError::InvalidVoiceBatch);
        }
        if self.frames.iter().any(|frame| {
            frame.payload.is_empty() || frame.payload.len() > MAX_OPUS_FRAME_BYTES
        }) {
            return Err(RealtimeProtocolError::InvalidOpusFrame);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoomPresenceV1 {
    pub room_id: String,
    pub epoch: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum RealtimeBodyV1 {
    TransportAnnounce(TransportAnnounceV1),
    TransportAck(TransportAnnounceV1),
    CallRequest(CallControlV1),
    CallAccept(CallControlV1),
    CallDecline(CallControlV1),
    CallHangup(CallControlV1),
    VoiceBatch(VoiceBatchV1),
    RoomPresence(RoomPresenceV1),
    RoomVoice(VoiceBatchV1),
}

pub fn session_topic_id(sid: &[u8; 16]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"GhostTalk/realtime-topic/v1\0");
    hasher.update(sid);
    let encoded = hasher.finalize().to_hex().to_string();
    encoded[..32].to_owned()
}

pub fn session_topic(sid: &[u8; 16]) -> String {
    format!("ghost-talk/realtime/v1/{}", session_topic_id(sid))
}

fn validate_ciphertext(ciphertext: &[u8]) -> Result<(), RealtimeProtocolError> {
    if ciphertext.is_empty() || ciphertext.len() > MAX_REALTIME_CIPHERTEXT_BYTES {
        return Err(RealtimeProtocolError::InvalidCiphertextLength);
    }
    Ok(())
}

fn decode_hex<const N: usize>(value: &str, label: &'static str) -> Result<[u8; N], RealtimeProtocolError> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RealtimeProtocolError::InvalidHex(label));
    }
    let bytes = hex::decode(value).map_err(|_| RealtimeProtocolError::InvalidHex(label))?;
    bytes.try_into().map_err(|_| RealtimeProtocolError::InvalidHex(label))
}
