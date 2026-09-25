use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};

const ARCHIVE_MAGIC: &[u8; 5] = b"GMAR1";
const HEADER_LEN: usize = 5 + 32 + 4 + 4 + 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KaspaArchiveChunk {
    pub index: u32,
    pub transaction_id: String,
    pub content_hash: String,
    pub size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KaspaArchiveLocator {
    pub network: String,
    pub address: String,
    pub chunks: Vec<KaspaArchiveChunk>,
}

impl KaspaArchiveLocator {
    pub fn encode(&self) -> Result<String, String> {
        serde_json::to_vec(self)
            .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
            .map_err(|error| format!("archive locator encode failed: {error}"))
    }

    pub fn decode(value: &str) -> Result<Self, String> {
        let bytes = URL_SAFE_NO_PAD
            .decode(value.trim())
            .map_err(|error| format!("archive locator base64 failed: {error}"))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("archive locator decode failed: {error}"))
    }
}

pub fn archive_payload_overhead() -> usize {
    HEADER_LEN
}

pub fn encode_archive_chunk(
    media_id: &str,
    index: u32,
    count: u32,
    bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let media_id = digest(media_id, "media id")?;
    let chunk_hash = blake3::hash(bytes);
    let mut out = Vec::with_capacity(HEADER_LEN + bytes.len());
    out.extend_from_slice(ARCHIVE_MAGIC);
    out.extend_from_slice(&media_id);
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(chunk_hash.as_bytes());
    out.extend_from_slice(bytes);
    Ok(out)
}

pub fn decode_archive_chunk(payload: &[u8]) -> Result<(String, u32, u32, Vec<u8>), String> {
    if payload.len() < HEADER_LEN || &payload[..5] != ARCHIVE_MAGIC {
        return Err("not a Ghost media archive chunk".into());
    }
    let media_id = hex::encode(&payload[5..37]);
    let index = u32::from_le_bytes(
        payload[37..41]
            .try_into()
            .map_err(|_| "invalid archive index")?,
    );
    let count = u32::from_le_bytes(
        payload[41..45]
            .try_into()
            .map_err(|_| "invalid archive count")?,
    );
    let expected_hash = &payload[45..77];
    let bytes = payload[77..].to_vec();
    if blake3::hash(&bytes).as_bytes() != expected_hash {
        return Err("Ghost media archive chunk hash mismatch".into());
    }
    Ok((media_id, index, count, bytes))
}

fn digest(value: &str, label: &str) -> Result<[u8; 32], String> {
    let decoded = hex::decode(value.trim()).map_err(|error| format!("invalid {label}: {error}"))?;
    decoded
        .try_into()
        .map_err(|_| format!("{label} must be 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_carrier_round_trip_verifies_chunk() {
        let id = crate::content_hash(b"whole");
        let encoded = encode_archive_chunk(&id, 1, 3, b"part").unwrap();
        let (decoded_id, index, count, body) = decode_archive_chunk(&encoded).unwrap();
        assert_eq!(decoded_id, id);
        assert_eq!((index, count, body), (1, 3, b"part".to_vec()));
    }

    #[test]
    fn archive_carrier_rejects_corrupted_body() {
        let id = crate::content_hash(b"whole");
        let mut encoded = encode_archive_chunk(&id, 0, 1, b"part").unwrap();
        *encoded.last_mut().unwrap() ^= 0x01;
        assert!(decode_archive_chunk(&encoded).is_err());
    }

    #[test]
    fn archive_locator_round_trip_preserves_exact_transactions() {
        let locator = KaspaArchiveLocator {
            network: "mainnet".into(),
            address: "kaspa:qtest".into(),
            chunks: vec![KaspaArchiveChunk {
                index: 0,
                transaction_id: "ab".repeat(32),
                content_hash: crate::content_hash(b"part"),
                size: 4,
            }],
        };
        assert_eq!(
            KaspaArchiveLocator::decode(&locator.encode().unwrap()).unwrap(),
            locator
        );
    }
}
