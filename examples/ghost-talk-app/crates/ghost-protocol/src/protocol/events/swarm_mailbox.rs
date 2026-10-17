//! No-spend mailbox carrier: the exact GHST frames a Kaspa transaction would
//! carry, published on the shared p2p-net mailbox topic instead. Each packet
//! names its recipient only by a domain-separated digest of the destination
//! Kaspa address, so receivers can keep their own mail without the address
//! appearing on the wire. Authentication is unchanged: the framed envelope is
//! the same signed/sealed KKTP or Ghost carrier the mailbox already verifies.

use sha3::Digest as _;

use super::{CarrierFrame, Sha3_256};

pub const GTSM_MAGIC: [u8; 4] = *b"GTSM";
pub const GTSM_VERSION: u8 = 1;
const RECIPIENT_TAG_DOMAIN: &[u8] = b"GhostTalk/swarm-mailbox/recipient/v1\0";
const FRAME_ID_DOMAIN: &[u8] = b"GhostTalk/swarm-mailbox/frame/v1\0";
const HEADER_BYTES: usize = 4 + 1 + 32;

/// Routing digest of one Kaspa destination address (case-insensitive).
pub fn swarm_recipient_tag(kaspa_address: &str) -> [u8; 32] {
    let mut hasher = Sha3_256::new();
    hasher.update(RECIPIENT_TAG_DOMAIN);
    hasher.update(kaspa_address.trim().to_ascii_lowercase().as_bytes());
    hasher.finalize().into()
}

/// Stable 64-hex identity of one swarm-carried frame. It stands in for the
/// Kaspa transaction id so mailbox reassembly and replay suppression treat
/// swarm and chain carriers alike.
pub fn swarm_frame_id(frame: &[u8]) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(FRAME_ID_DOMAIN);
    hasher.update(frame);
    hex::encode(hasher.finalize())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwarmMailboxPacket {
    pub recipient_tag: [u8; 32],
    /// One validated GHST carrier frame.
    pub frame: Vec<u8>,
}

impl SwarmMailboxPacket {
    pub fn for_destination(kaspa_address: &str, frame: Vec<u8>) -> Result<Self, String> {
        CarrierFrame::decode(&frame)?;
        Ok(Self {
            recipient_tag: swarm_recipient_tag(kaspa_address),
            frame,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES + self.frame.len());
        out.extend_from_slice(&GTSM_MAGIC);
        out.push(GTSM_VERSION);
        out.extend_from_slice(&self.recipient_tag);
        out.extend_from_slice(&self.frame);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() <= HEADER_BYTES || bytes[..4] != GTSM_MAGIC {
            return Err("not a Ghost Talk swarm mailbox packet".into());
        }
        if bytes[4] != GTSM_VERSION {
            return Err("unsupported Ghost Talk swarm mailbox version".into());
        }
        let frame = bytes[HEADER_BYTES..].to_vec();
        CarrierFrame::decode(&frame)?;
        let mut recipient_tag = [0u8; 32];
        recipient_tag.copy_from_slice(&bytes[5..HEADER_BYTES]);
        Ok(Self {
            recipient_tag,
            frame,
        })
    }

    /// True when the packet is addressed to any of `addresses`.
    pub fn addressed_to<'a>(&self, mut addresses: impl Iterator<Item = &'a String>) -> bool {
        addresses.any(|address| swarm_recipient_tag(address) == self.recipient_tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragment;

    fn frame() -> Vec<u8> {
        fragment(ghost_core::Id128([5; 16]), b"KKTP:ANCHOR:{}")
            .unwrap()
            .remove(0)
    }

    #[test]
    fn packet_round_trips_and_routes_by_address_digest() {
        let packet = SwarmMailboxPacket::for_destination("kaspa:QXYZ", frame()).unwrap();
        let decoded = SwarmMailboxPacket::decode(&packet.encode()).unwrap();
        assert_eq!(decoded, packet);
        let mine = ["kaspa:other".to_string(), "kaspa:qxyz".to_string()];
        assert!(decoded.addressed_to(mine.iter()));
        assert!(!decoded.addressed_to(mine[..1].iter()));
        assert!(!packet.encode().windows(4).any(|window| window == b"QXYZ"));
        let id = swarm_frame_id(&packet.frame);
        assert_eq!(id.len(), 64);
        assert_ne!(id, swarm_frame_id(b"other"));
    }

    #[test]
    fn malformed_packets_fail_closed() {
        assert!(SwarmMailboxPacket::for_destination("kaspa:a", b"not ghst".to_vec()).is_err());
        let mut encoded = SwarmMailboxPacket::for_destination("kaspa:a", frame())
            .unwrap()
            .encode();
        assert!(SwarmMailboxPacket::decode(&encoded[..HEADER_BYTES]).is_err());
        encoded[4] = 9;
        assert!(SwarmMailboxPacket::decode(&encoded).is_err());
        assert!(SwarmMailboxPacket::decode(b"GHSTxxxx").is_err());
    }
}
