use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
/// Errors returned by Ghost Talk media framing, sender, receiver, and backend operations.
pub enum VoiceError {
    #[error("unsupported Ghost Talk wire version {0}")]
    /// The wire version is not supported by this SDK build.
    UnsupportedVersion(u8),
    #[error("unsupported voice codec id {0}")]
    /// The encoded codec identifier is unknown.
    UnsupportedCodec(u8),
    #[error("voice chunk wire data is malformed: {0}")]
    /// The binary chunk is structurally invalid.
    MalformedChunk(&'static str),
    #[error("voice chunk payload is {actual} bytes; limit is {limit} bytes")]
    /// A payload exceeds the configured defensive limit.
    ChunkTooLarge {
        /// Number of payload bytes that were supplied.
        actual: usize,
        /// Maximum payload size accepted by the configured receiver/sender.
        limit: usize,
    },
    #[error("voice sender is not running")]
    /// Encoded media was pushed while the sender was stopped.
    SenderNotRunning,
    #[error("voice sender is already running")]
    /// A sender start was requested while a stream was already active.
    SenderAlreadyRunning,
    #[error("host output budget is {limit} bytes but encoded audio unit is {actual} bytes")]
    /// A payload exceeds the host-provided output budget.
    OutputBudgetExceeded {
        /// Number of encoded bytes produced for the media unit.
        actual: usize,
        /// Maximum number of bytes the host allowed for this output.
        limit: usize,
    },
    #[error("voice sequence space exhausted")]
    /// The current stream exhausted its sequence-number space.
    SequenceExhausted,
    #[error("receiver reorder buffer is full ({capacity} chunks)")]
    /// The receiver cannot buffer another out-of-order chunk.
    ReorderBufferFull {
        /// Configured maximum number of out-of-order chunks that may be buffered.
        capacity: usize,
    },
    #[error("audio backend error: {0}")]
    /// A platform playback backend reported an error.
    AudioBackend(String),
    #[error("operating system randomness unavailable")]
    /// Secure operating-system randomness was unavailable.
    RandomnessUnavailable,
}
