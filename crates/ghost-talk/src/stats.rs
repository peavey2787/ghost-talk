use std::time::Duration;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Cumulative sender counters for the lifetime of a [`crate::VoiceSender`].
pub struct VoiceSenderStats {
    /// Total media duration accepted from capture adapters.
    pub captured_duration: Duration,
    /// Number of chunks emitted successfully.
    pub emitted_chunks: u64,
    /// Total encoded payload bytes emitted.
    pub encoded_bytes: u64,
    /// Number of capture units rejected by configured limits.
    pub dropped_capture_units: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Cumulative receiver counters for the lifetime of a [`crate::VoiceReceiver`].
pub struct VoiceReceiverStats {
    /// Number of chunks accepted for receiver processing.
    pub received_chunks: u64,
    /// Number of chunks successfully decoded and scheduled.
    pub played_chunks: u64,
    /// Number of duplicate sequence numbers observed.
    pub duplicate_chunks: u64,
    /// Number of stale or too-old chunks discarded.
    pub late_chunks: u64,
    /// Number of missing sequence positions accounted as lost.
    pub lost_chunks: u64,
    /// Number of chunks buffered because they arrived out of order.
    pub reordered_chunks: u64,
    /// Current number of chunks held in the reorder buffer.
    pub buffered_chunks: usize,
    /// Number of chunks that could not be decoded or scheduled.
    pub decode_failures: u64,
}
