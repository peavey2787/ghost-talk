//! A request for held no-spend mail. It names no recipient: holders replay
//! everything they hold since `since_secs` and every receiver keeps only the
//! packets tagged for its own addresses.

pub const GTSQ_MAGIC: [u8; 4] = *b"GTSQ";
const GTSQ_VERSION: u8 = 1;
const ENCODED_BYTES: usize = 4 + 1 + 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SwarmMailQuery {
    pub since_secs: u64,
}

impl SwarmMailQuery {
    pub fn encode(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ENCODED_BYTES);
        out.extend_from_slice(&GTSQ_MAGIC);
        out.push(GTSQ_VERSION);
        out.extend_from_slice(&self.since_secs.to_be_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != ENCODED_BYTES || bytes[..4] != GTSQ_MAGIC || bytes[4] != GTSQ_VERSION {
            return Err("not a Ghost Talk swarm mail query".into());
        }
        let mut since = [0u8; 8];
        since.copy_from_slice(&bytes[5..]);
        Ok(Self {
            since_secs: u64::from_be_bytes(since),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_round_trips_and_rejects_other_bytes() {
        let query = SwarmMailQuery { since_secs: 1234 };
        assert_eq!(SwarmMailQuery::decode(&query.encode()), Ok(query));
        let mut bad = query.encode();
        bad[4] = 2;
        assert!(SwarmMailQuery::decode(&bad).is_err());
        assert!(SwarmMailQuery::decode(b"GTSQ").is_err());
        assert!(SwarmMailQuery::decode(b"GTSMxxxxxxxxx").is_err());
    }
}
