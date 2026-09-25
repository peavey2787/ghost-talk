use std::time::Duration;

use crate::VoiceCodec;

/// One complete independently-decodable encoded audio unit produced by a
/// platform capture adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedAudioChunk {
    /// Codec/container used by `payload`.
    pub codec: VoiceCodec,
    /// Complete independently decodable encoded audio payload.
    pub payload: Vec<u8>,
    /// Media duration represented by this capture unit.
    pub captured_duration: Duration,
}

impl EncodedAudioChunk {
    /// Creates a complete encoded capture unit for submission to a sender.
    pub fn new(codec: VoiceCodec, payload: Vec<u8>, captured_duration: Duration) -> Self {
        Self {
            codec,
            payload,
            captured_duration,
        }
    }
}
