use std::time::Duration;

use crate::VoiceLimits;

/// Configuration for [`crate::VoiceSender`].
#[derive(Clone, Debug)]
pub struct VoiceSenderConfig {
    /// Preferred duration represented by one independently decodable capture unit.
    pub target_chunk_duration: Duration,
    /// Optional host-provided output budget.
    ///
    /// This is transport-agnostic guidance, not an MTU or carrier configuration.
    pub target_max_chunk_bytes: Option<usize>,
    /// Defensive payload ceilings enforced by the sender.
    pub limits: VoiceLimits,
}

impl Default for VoiceSenderConfig {
    fn default() -> Self {
        Self {
            target_chunk_duration: Duration::from_millis(350),
            target_max_chunk_bytes: None,
            limits: VoiceLimits::default(),
        }
    }
}

/// Configuration for [`crate::VoiceReceiver`].
#[derive(Clone, Debug)]
pub struct VoiceReceiverConfig {
    /// Maximum number of out-of-order chunks retained while waiting for gaps.
    pub reorder_capacity: usize,
    /// Maximum time to wait for a missing sequence before accounting it as lost.
    pub gap_wait: Duration,
    /// Initial playback lead time used to absorb network jitter.
    pub initial_buffer: Duration,
    /// Defensive payload ceilings enforced by the receiver.
    pub limits: VoiceLimits,
}

impl Default for VoiceReceiverConfig {
    fn default() -> Self {
        Self {
            reorder_capacity: 256,
            gap_wait: Duration::from_millis(1_200),
            initial_buffer: Duration::from_millis(400),
            limits: VoiceLimits::default(),
        }
    }
}
