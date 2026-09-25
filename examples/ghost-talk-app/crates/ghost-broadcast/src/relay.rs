use serde::{Deserialize, Serialize};

pub const RELAY_FRAME_MAGIC: &[u8; 4] = b"GTRF";
pub const RELAY_PROTOCOL_VERSION: u8 = 1;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayControl {
    Start {
        version: u8,
        session_id: String,
        rtmp_server: Option<String>,
        rtmp_stream_key: Option<String>,
    },
    Stop {
        version: u8,
        session_id: String,
    },
}

impl RelayControl {
    pub fn start(
        session_id: impl Into<String>,
        rtmp_server: Option<String>,
        rtmp_stream_key: Option<String>,
    ) -> Self {
        Self::Start {
            version: RELAY_PROTOCOL_VERSION,
            session_id: session_id.into(),
            rtmp_server,
            rtmp_stream_key,
        }
    }

    pub fn stop(session_id: impl Into<String>) -> Self {
        Self::Stop {
            version: RELAY_PROTOCOL_VERSION,
            session_id: session_id.into(),
        }
    }
}

pub fn validate_relay_url(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.starts_with("wss://") || is_local_ws(value) {
        Ok(())
    } else {
        Err("broadcast relay must use wss:// (ws:// is allowed only for localhost development)".into())
    }
}

fn is_local_ws(value: &str) -> bool {
    value.starts_with("ws://localhost")
        || value.starts_with("ws://127.0.0.1")
        || value.starts_with("ws://[::1]")
}

pub fn encode_relay_frame(sequence: u64, timestamp_ms: u64, encoded: &[u8]) -> Result<Vec<u8>, String> {
    let len = u32::try_from(encoded.len()).map_err(|_| "broadcast relay frame is too large".to_string())?;
    let mut out = Vec::with_capacity(25 + encoded.len());
    out.extend_from_slice(RELAY_FRAME_MAGIC);
    out.push(RELAY_PROTOCOL_VERSION);
    out.extend_from_slice(&sequence.to_be_bytes());
    out.extend_from_slice(&timestamp_ms.to_be_bytes());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(encoded);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_url_requires_secure_transport_except_localhost() {
        assert!(validate_relay_url("wss://relay.example/ingest").is_ok());
        assert!(validate_relay_url("ws://localhost:9000/ingest").is_ok());
        assert!(validate_relay_url("ws://relay.example/ingest").is_err());
        assert!(validate_relay_url("https://relay.example/ingest").is_err());
    }

    #[test]
    fn relay_frame_has_versioned_header_and_exact_payload() {
        let frame = encode_relay_frame(7, 11, &[1, 2, 3]).expect("frame");
        assert_eq!(&frame[..4], RELAY_FRAME_MAGIC);
        assert_eq!(frame[4], RELAY_PROTOCOL_VERSION);
        assert_eq!(&frame[5..13], &7u64.to_be_bytes());
        assert_eq!(&frame[13..21], &11u64.to_be_bytes());
        assert_eq!(&frame[21..25], &3u32.to_be_bytes());
        assert_eq!(&frame[25..], &[1, 2, 3]);
    }
}
