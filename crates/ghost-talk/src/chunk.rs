use crate::{VoiceCodec, VoiceError, VoiceLimits};

/// Current stable Ghost Talk media-chunk wire version.
pub const VOICE_WIRE_VERSION: u8 = 1;
const MAGIC: [u8; 4] = *b"GTVC";
const HEADER_LEN: usize = 36;

/// Complete Ghost Talk media unit. This is never a network fragment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceChunk {
    /// Wire-format version carried by this chunk.
    pub version: u8,
    /// Random identifier for the logical sender stream.
    pub stream_id: u128,
    /// Monotonic sequence within `stream_id`.
    pub sequence: u64,
    /// Codec/container used by the encoded payload.
    pub codec: VoiceCodec,
    /// Complete independently-decodable encoded audio payload.
    pub payload: Vec<u8>,
}

impl VoiceChunk {
    /// Creates a chunk using the current wire version.
    pub fn new(stream_id: u128, sequence: u64, codec: VoiceCodec, payload: Vec<u8>) -> Self {
        Self {
            version: VOICE_WIRE_VERSION,
            stream_id,
            sequence,
            codec,
            payload,
        }
    }

    /// Encodes the complete chunk into the stable Ghost Talk binary wire format.
    pub fn encode(&self) -> Result<Vec<u8>, VoiceError> {
        validate_version(self.version)?;
        let payload_len = u32::try_from(self.payload.len())
            .map_err(|_| VoiceError::MalformedChunk("payload length exceeds wire format"))?;
        let mut output = Vec::with_capacity(HEADER_LEN + self.payload.len());
        output.extend_from_slice(&MAGIC);
        output.push(self.version);
        output.push(self.codec.wire_id());
        output.extend_from_slice(&[0_u8; 2]);
        output.extend_from_slice(&self.stream_id.to_le_bytes());
        output.extend_from_slice(&self.sequence.to_le_bytes());
        output.extend_from_slice(&payload_len.to_le_bytes());
        output.extend_from_slice(&self.payload);
        Ok(output)
    }

    /// Decodes a chunk using [`VoiceLimits::default`].
    pub fn decode(bytes: &[u8]) -> Result<Self, VoiceError> {
        Self::decode_with_limits(bytes, VoiceLimits::default())
    }

    /// Decodes a chunk while enforcing caller-provided defensive limits.
    pub fn decode_with_limits(bytes: &[u8], limits: VoiceLimits) -> Result<Self, VoiceError> {
        if bytes.len() < HEADER_LEN {
            return Err(VoiceError::MalformedChunk("truncated header"));
        }
        if bytes[..4] != MAGIC {
            return Err(VoiceError::MalformedChunk("invalid magic"));
        }
        let version = bytes[4];
        validate_version(version)?;
        let codec = VoiceCodec::from_wire_id(bytes[5])?;
        if bytes[6] != 0 || bytes[7] != 0 {
            return Err(VoiceError::MalformedChunk(
                "reserved header bits are non-zero",
            ));
        }
        let stream_id = u128::from_le_bytes(array(bytes, 8)?);
        let sequence = u64::from_le_bytes(array(bytes, 24)?);
        let payload_len = u32::from_le_bytes(array(bytes, 32)?) as usize;
        if payload_len > limits.max_chunk_bytes {
            return Err(VoiceError::ChunkTooLarge {
                actual: payload_len,
                limit: limits.max_chunk_bytes,
            });
        }
        let expected_len = HEADER_LEN
            .checked_add(payload_len)
            .ok_or(VoiceError::MalformedChunk("payload length overflow"))?;
        if bytes.len() != expected_len {
            return Err(VoiceError::MalformedChunk(
                "payload length does not match input",
            ));
        }
        Ok(Self {
            version,
            stream_id,
            sequence,
            codec,
            payload: bytes[HEADER_LEN..].to_vec(),
        })
    }
}

fn validate_version(version: u8) -> Result<(), VoiceError> {
    if version == VOICE_WIRE_VERSION {
        Ok(())
    } else {
        Err(VoiceError::UnsupportedVersion(version))
    }
}

fn array<const N: usize>(bytes: &[u8], start: usize) -> Result<[u8; N], VoiceError> {
    let end = start
        .checked_add(N)
        .ok_or(VoiceError::MalformedChunk("header index overflow"))?;
    bytes
        .get(start..end)
        .ok_or(VoiceError::MalformedChunk("truncated header"))?
        .try_into()
        .map_err(|_| VoiceError::MalformedChunk("invalid fixed-width field"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_round_trip_is_stable() {
        let original = VoiceChunk::new(
            0x1020_3040_5060_7080_90a0_b0c0_d0e0_f001,
            42,
            VoiceCodec::OpusOgg,
            vec![1, 2, 3, 4],
        );
        let encoded = original.encode().unwrap();
        assert_eq!(&encoded[..4], b"GTVC");
        assert_eq!(
            hex(&encoded),
            "475456430102000001f0e0d0c0b0a09080706050403020102a000000000000000400000001020304"
        );
        assert_eq!(VoiceChunk::decode(&encoded).unwrap(), original);
    }

    #[test]
    fn malformed_and_unsupported_wire_data_is_rejected() {
        assert!(matches!(
            VoiceChunk::decode(b"short"),
            Err(VoiceError::MalformedChunk(_))
        ));

        let mut encoded = VoiceChunk::new(1, 0, VoiceCodec::OpusWebM, vec![1])
            .encode()
            .unwrap();
        encoded[4] = 9;
        assert_eq!(
            VoiceChunk::decode(&encoded),
            Err(VoiceError::UnsupportedVersion(9))
        );

        let mut encoded = VoiceChunk::new(1, 0, VoiceCodec::OpusWebM, vec![1])
            .encode()
            .unwrap();
        encoded[5] = 99;
        assert_eq!(
            VoiceChunk::decode(&encoded),
            Err(VoiceError::UnsupportedCodec(99))
        );

        let mut encoded = VoiceChunk::new(1, 0, VoiceCodec::OpusWebM, vec![1])
            .encode()
            .unwrap();
        encoded[6] = 1;
        assert!(matches!(
            VoiceChunk::decode(&encoded),
            Err(VoiceError::MalformedChunk(_))
        ));

        let mut encoded = VoiceChunk::new(1, 0, VoiceCodec::OpusWebM, vec![1])
            .encode()
            .unwrap();
        encoded[32..36].copy_from_slice(&2_u32.to_le_bytes());
        assert!(matches!(
            VoiceChunk::decode(&encoded),
            Err(VoiceError::MalformedChunk(_))
        ));
    }

    #[test]
    fn defensive_payload_limit_is_not_a_transport_mtu() {
        let encoded = VoiceChunk::new(1, 0, VoiceCodec::OpusWebM, vec![7; 65])
            .encode()
            .unwrap();
        let error = VoiceChunk::decode_with_limits(
            &encoded,
            VoiceLimits {
                max_chunk_bytes: 64,
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            VoiceError::ChunkTooLarge {
                actual: 65,
                limit: 64
            }
        );
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
