//! How much no-spend swarm mail this client holds for peers that were briefly
//! offline. Held packets are already end-to-end sealed; holders cannot read
//! them, only replay them when a peer asks the swarm for missed mail.

use serde::{Deserialize, Serialize};

pub const MAX_RETENTION_MINUTES: u32 = 7 * 24 * 60;
pub const MAX_RETENTION_MEGABYTES: u32 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SwarmRetention {
    /// Off: hold nothing and answer no queries (this client still asks).
    pub enabled: bool,
    /// Packets older than this are dropped.
    pub minutes: u32,
    /// Oldest packets are dropped first once the store exceeds this.
    pub megabytes: u32,
}

impl Default for SwarmRetention {
    fn default() -> Self {
        Self {
            enabled: true,
            minutes: 60,
            megabytes: 16,
        }
    }
}

impl SwarmRetention {
    /// The same policy with every limit clamped into its supported range.
    pub fn bounded(self) -> Self {
        Self {
            enabled: self.enabled,
            minutes: self.minutes.clamp(1, MAX_RETENTION_MINUTES),
            megabytes: self.megabytes.clamp(1, MAX_RETENTION_MEGABYTES),
        }
    }

    pub fn max_age_secs(self) -> u64 {
        u64::from(self.bounded().minutes) * 60
    }

    pub fn max_bytes(self) -> usize {
        usize::try_from(self.bounded().megabytes).unwrap_or(usize::MAX) * 1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_bounds() {
        let policy: SwarmRetention = serde_json::from_str("{}").unwrap();
        assert_eq!(policy, SwarmRetention::default());
        assert_eq!(policy.max_age_secs(), 3600);
        assert_eq!(policy.max_bytes(), 16 * 1024 * 1024);
        let wild = SwarmRetention {
            enabled: false,
            minutes: 0,
            megabytes: u32::MAX,
        };
        assert_eq!(wild.max_age_secs(), 60);
        assert_eq!(wild.max_bytes(), 512 * 1024 * 1024);
        assert!(!wild.bounded().enabled);
    }
}
