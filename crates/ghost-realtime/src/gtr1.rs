//! GTR1: the carrier-neutral realtime envelope.
//!
//! ```text
//! 0..4    magic = GTR1
//! 4..36   sender HYDRA id (32 bytes)
//! 36..52  message id (16 bytes)
//! 52..68  exact session id / KKTP SID (16 bytes)
//! 68..    sealed ciphertext
//! ```
//!
//! The same sealed bytes travel over p2p-net or the Kaspa fallback, so a
//! carrier switch never re-encrypts a logical message.

use thiserror::Error;

pub const GTR1_MAGIC: [u8; 4] = *b"GTR1";
pub const GTR1_HEADER_LEN: usize = 68;
pub const MAX_REALTIME_CIPHERTEXT_BYTES: usize = 256 * 1024;

/// Replay identity of one logical realtime message. The same identity is
/// shared by every physical carrier of that message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReplayIdentity {
    pub sender: [u8; 32],
    pub message_id: [u8; 16],
    pub session_id: [u8; 16],
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum Gtr1Error {
    #[error("GTR1 carrier is truncated")]
    Truncated,
    #[error("GTR1 carrier magic is invalid")]
    InvalidMagic,
    #[error("GTR1 ciphertext is empty or exceeds the carrier limit")]
    InvalidCiphertextLength,
    #[error("{0} must be exact hexadecimal of the required width")]
    InvalidHex(&'static str),
}

/// A validated GTR1 carrier owning its ciphertext.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gtr1Envelope {
    pub sender_hydra_id: [u8; 32],
    pub message_id: [u8; 16],
    pub sid: [u8; 16],
    pub ciphertext: Vec<u8>,
}

impl Gtr1Envelope {
    pub fn new(
        sender_hydra_id: [u8; 32],
        message_id: [u8; 16],
        sid: [u8; 16],
        ciphertext: Vec<u8>,
    ) -> Result<Self, Gtr1Error> {
        validate_ciphertext(&ciphertext)?;
        Ok(Self {
            sender_hydra_id,
            message_id,
            sid,
            ciphertext,
        })
    }

    pub fn from_hex_ids(
        sender_hydra_id: &str,
        message_id: &str,
        sid: &str,
        ciphertext: Vec<u8>,
    ) -> Result<Self, Gtr1Error> {
        Self::new(
            parse_hydra_id(sender_hydra_id)?,
            decode_hex::<16>(message_id, "Ghost Talk message id")?,
            parse_session_id(sid)?,
            ciphertext,
        )
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Gtr1Error> {
        let (identity, ciphertext) = split(bytes)?;
        Ok(Self {
            sender_hydra_id: identity.sender,
            message_id: identity.message_id,
            sid: identity.session_id,
            ciphertext: ciphertext.to_vec(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, Gtr1Error> {
        validate_ciphertext(&self.ciphertext)?;
        Ok(encode_gtr1(self.identity(), &self.ciphertext))
    }

    pub fn identity(&self) -> ReplayIdentity {
        ReplayIdentity {
            sender: self.sender_hydra_id,
            message_id: self.message_id,
            session_id: self.sid,
        }
    }

    pub fn sender_hex(&self) -> String {
        hex::encode(self.sender_hydra_id)
    }

    pub fn message_id_hex(&self) -> String {
        hex::encode(self.message_id)
    }

    pub fn sid_hex(&self) -> String {
        hex::encode(self.sid)
    }
}

/// Parse a hexadecimal KKTP session id (SID).
pub fn parse_session_id(value: &str) -> Result<[u8; 16], Gtr1Error> {
    decode_hex(value, "KKTP session id")
}

/// Parse a hexadecimal HYDRA identity.
pub fn parse_hydra_id(value: &str) -> Result<[u8; 32], Gtr1Error> {
    decode_hex(value, "HYDRA sender identity")
}

/// Frame already-sealed bytes as a GTR1 carrier.
pub fn encode_gtr1(identity: ReplayIdentity, envelope: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(GTR1_HEADER_LEN + envelope.len());
    out.extend_from_slice(&GTR1_MAGIC);
    out.extend_from_slice(&identity.sender);
    out.extend_from_slice(&identity.message_id);
    out.extend_from_slice(&identity.session_id);
    out.extend_from_slice(envelope);
    out
}

/// Validate a GTR1 carrier and borrow its sealed payload.
pub fn decode_gtr1(bytes: &[u8]) -> Result<(ReplayIdentity, &[u8]), String> {
    split(bytes).map_err(|error| error.to_string())
}

fn split(bytes: &[u8]) -> Result<(ReplayIdentity, &[u8]), Gtr1Error> {
    if bytes.len() < GTR1_HEADER_LEN {
        return Err(Gtr1Error::Truncated);
    }
    if bytes[..4] != GTR1_MAGIC {
        return Err(Gtr1Error::InvalidMagic);
    }
    let ciphertext = &bytes[GTR1_HEADER_LEN..];
    validate_ciphertext(ciphertext)?;
    let identity = ReplayIdentity {
        sender: fixed(&bytes[4..36]),
        message_id: fixed(&bytes[36..52]),
        session_id: fixed(&bytes[52..68]),
    };
    Ok((identity, ciphertext))
}

fn fixed<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(bytes);
    out
}

fn validate_ciphertext(ciphertext: &[u8]) -> Result<(), Gtr1Error> {
    if ciphertext.is_empty() || ciphertext.len() > MAX_REALTIME_CIPHERTEXT_BYTES {
        return Err(Gtr1Error::InvalidCiphertextLength);
    }
    Ok(())
}

fn decode_hex<const N: usize>(value: &str, label: &'static str) -> Result<[u8; N], Gtr1Error> {
    if value.len() != N * 2 {
        return Err(Gtr1Error::InvalidHex(label));
    }
    let bytes = hex::decode(value).map_err(|_| Gtr1Error::InvalidHex(label))?;
    Ok(fixed(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ReplayIdentity {
        ReplayIdentity {
            sender: [1; 32],
            message_id: [2; 16],
            session_id: [3; 16],
        }
    }

    #[test]
    fn envelope_round_trip_preserves_exact_carrier_bytes() {
        let envelope = Gtr1Envelope::new([1; 32], [2; 16], [3; 16], vec![4, 5, 6]).unwrap();
        let encoded = envelope.encode().unwrap();
        assert_eq!(&encoded[..4], b"GTR1");
        assert_eq!(encoded.len(), GTR1_HEADER_LEN + 3);
        assert_eq!(Gtr1Envelope::decode(&encoded).unwrap(), envelope);
        assert_eq!(envelope.identity(), identity());
    }

    #[test]
    fn functional_codec_matches_envelope_layout() {
        let raw = encode_gtr1(identity(), b"hydra");
        let (decoded, payload) = decode_gtr1(&raw).unwrap();
        assert_eq!(decoded, identity());
        assert_eq!(payload, b"hydra");
        let envelope = Gtr1Envelope::decode(&raw).unwrap();
        assert_eq!(envelope.encode().unwrap(), raw);
    }

    #[test]
    fn malformed_carriers_fail_closed() {
        assert_eq!(Gtr1Envelope::decode(b"GTR1"), Err(Gtr1Error::Truncated));
        let mut raw = encode_gtr1(identity(), b"x");
        raw[0] = b'X';
        assert_eq!(Gtr1Envelope::decode(&raw), Err(Gtr1Error::InvalidMagic));
        let empty = encode_gtr1(identity(), b"");
        assert_eq!(
            Gtr1Envelope::decode(&empty),
            Err(Gtr1Error::InvalidCiphertextLength)
        );
        assert!(decode_gtr1(&empty).is_err());
        let oversized = vec![0u8; MAX_REALTIME_CIPHERTEXT_BYTES + 1];
        assert_eq!(
            Gtr1Envelope::new([0; 32], [0; 16], [0; 16], oversized),
            Err(Gtr1Error::InvalidCiphertextLength)
        );
        let invalid = Gtr1Envelope {
            sender_hydra_id: [0; 32],
            message_id: [0; 16],
            sid: [0; 16],
            ciphertext: Vec::new(),
        };
        assert_eq!(invalid.encode(), Err(Gtr1Error::InvalidCiphertextLength));
    }

    #[test]
    fn hex_identities_are_width_checked() {
        let envelope = Gtr1Envelope::from_hex_ids(
            &"01".repeat(32),
            &"02".repeat(16),
            &"03".repeat(16),
            vec![9],
        )
        .unwrap();
        assert_eq!(envelope.sender_hex(), "01".repeat(32));
        assert_eq!(envelope.message_id_hex(), "02".repeat(16));
        assert_eq!(envelope.sid_hex(), "03".repeat(16));
        assert_eq!(
            Gtr1Envelope::from_hex_ids("01", &"02".repeat(16), &"03".repeat(16), vec![9]),
            Err(Gtr1Error::InvalidHex("HYDRA sender identity"))
        );
        assert_eq!(
            Gtr1Envelope::from_hex_ids(
                &"zz".repeat(32),
                &"02".repeat(16),
                &"03".repeat(16),
                vec![9]
            ),
            Err(Gtr1Error::InvalidHex("HYDRA sender identity"))
        );
        assert_eq!(parse_session_id(&"0a".repeat(16)), Ok([10; 16]));
        assert_eq!(
            parse_session_id("0a"),
            Err(Gtr1Error::InvalidHex("KKTP session id"))
        );
        assert_eq!(parse_hydra_id(&"0b".repeat(32)), Ok([11; 32]));
    }
}
