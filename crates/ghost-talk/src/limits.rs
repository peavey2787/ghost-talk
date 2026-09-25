/// Defensive resource limits for complete Ghost Talk media chunks.
///
/// These limits are SDK safety ceilings, not network MTUs. Hosts may fragment
/// serialized chunks after they leave the SDK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoiceLimits {
    /// Maximum encoded payload size accepted for one complete media chunk.
    pub max_chunk_bytes: usize,
}

impl Default for VoiceLimits {
    fn default() -> Self {
        Self {
            max_chunk_bytes: 128 * 1024,
        }
    }
}
