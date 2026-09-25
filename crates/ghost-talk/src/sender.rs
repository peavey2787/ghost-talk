use crate::{
    sequence::SequenceCounter, stream::random_stream_id, EncodedAudioChunk, VoiceChunk, VoiceError,
    VoiceSenderConfig, VoiceSenderStats,
};

/// Stateful media sender that assigns stream identities and monotonic sequences.
pub struct VoiceSender {
    config: VoiceSenderConfig,
    running: bool,
    stream_id: Option<u128>,
    sequence: SequenceCounter,
    handler: Option<Box<dyn FnMut(VoiceChunk) + Send + 'static>>,
    stats: VoiceSenderStats,
}

impl VoiceSender {
    /// Creates a sender after validating its configuration.
    pub fn new(config: VoiceSenderConfig) -> Result<Self, VoiceError> {
        validate_config(&config)?;
        Ok(Self {
            config,
            running: false,
            stream_id: None,
            sequence: SequenceCounter::default(),
            handler: None,
            stats: VoiceSenderStats::default(),
        })
    }

    /// Registers a callback invoked for every successfully emitted chunk.
    pub fn set_chunk_handler<F>(&mut self, handler: F)
    where
        F: FnMut(VoiceChunk) + Send + 'static,
    {
        self.handler = Some(Box::new(handler));
    }

    /// Starts a new capture stream identity and resets its media sequence.
    pub fn start(&mut self) -> Result<u128, VoiceError> {
        if self.running {
            return Err(VoiceError::SenderAlreadyRunning);
        }
        let stream_id = random_stream_id()?;
        self.running = true;
        self.stream_id = Some(stream_id);
        self.sequence = SequenceCounter::default();
        Ok(stream_id)
    }

    /// Stops the active stream and clears its stream identity.
    pub fn stop(&mut self) -> Result<(), VoiceError> {
        self.running = false;
        self.stream_id = None;
        Ok(())
    }

    /// Returns whether a stream is currently active.
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Preferred capture-window duration for a platform adapter feeding this
    /// sender. The sender accepts finalized encoded units; it never interprets
    /// this value as a network timer or carrier constraint.
    pub const fn target_chunk_duration(&self) -> std::time::Duration {
        self.config.target_chunk_duration
    }

    /// Converts one complete encoded capture unit into one complete VoiceChunk.
    /// The host may serialize/fragment/encrypt the returned chunk only after this
    /// media boundary.
    pub fn push_encoded(&mut self, unit: EncodedAudioChunk) -> Result<VoiceChunk, VoiceError> {
        let stream_id = self.stream_id.ok_or(VoiceError::SenderNotRunning)?;
        self.validate_payload(unit.payload.len())?;
        let sequence = self.sequence.take()?;
        let chunk = VoiceChunk::new(stream_id, sequence, unit.codec, unit.payload);
        self.stats.captured_duration += unit.captured_duration;
        self.stats.emitted_chunks = self.stats.emitted_chunks.saturating_add(1);
        self.stats.encoded_bytes = self
            .stats
            .encoded_bytes
            .saturating_add(chunk.payload.len() as u64);
        if let Some(handler) = self.handler.as_mut() {
            handler(chunk.clone());
        }
        Ok(chunk)
    }

    /// Returns a snapshot of cumulative sender statistics.
    pub fn stats(&self) -> VoiceSenderStats {
        self.stats.clone()
    }

    fn validate_payload(&mut self, actual: usize) -> Result<(), VoiceError> {
        if actual > self.config.limits.max_chunk_bytes {
            self.stats.dropped_capture_units = self.stats.dropped_capture_units.saturating_add(1);
            return Err(VoiceError::ChunkTooLarge {
                actual,
                limit: self.config.limits.max_chunk_bytes,
            });
        }
        if let Some(limit) = self.config.target_max_chunk_bytes {
            if actual > limit {
                self.stats.dropped_capture_units =
                    self.stats.dropped_capture_units.saturating_add(1);
                return Err(VoiceError::OutputBudgetExceeded { actual, limit });
            }
        }
        Ok(())
    }
}

fn validate_config(config: &VoiceSenderConfig) -> Result<(), VoiceError> {
    if config.target_chunk_duration.is_zero() {
        return Err(VoiceError::MalformedChunk(
            "target_chunk_duration must be non-zero",
        ));
    }
    if config.limits.max_chunk_bytes == 0 {
        return Err(VoiceError::MalformedChunk(
            "max_chunk_bytes must be non-zero",
        ));
    }
    if matches!(config.target_max_chunk_bytes, Some(0)) {
        return Err(VoiceError::MalformedChunk(
            "target_max_chunk_bytes must be non-zero",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::{VoiceCodec, VoiceLimits};

    #[test]
    fn assigns_stream_and_monotonic_sequences() {
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let capture = Arc::clone(&emitted);
        let mut sender = VoiceSender::new(VoiceSenderConfig::default()).unwrap();
        assert_eq!(sender.target_chunk_duration(), Duration::from_millis(350));
        sender.set_chunk_handler(move |chunk| capture.lock().unwrap().push(chunk));
        let first_stream = sender.start().unwrap();
        let a = sender.push_encoded(unit(b"a")).unwrap();
        let b = sender.push_encoded(unit(b"b")).unwrap();
        sender.stop().unwrap();
        let second_stream = sender.start().unwrap();
        let c = sender.push_encoded(unit(b"c")).unwrap();

        assert_eq!((a.stream_id, a.sequence), (first_stream, 0));
        assert_eq!((b.stream_id, b.sequence), (first_stream, 1));
        assert_eq!((c.stream_id, c.sequence), (second_stream, 0));
        assert_ne!(first_stream, second_stream);
        assert_eq!(emitted.lock().unwrap().len(), 3);
    }

    #[test]
    fn enforces_defensive_and_host_output_limits() {
        let mut sender = VoiceSender::new(VoiceSenderConfig {
            target_chunk_duration: Duration::from_millis(350),
            target_max_chunk_bytes: Some(4),
            limits: VoiceLimits { max_chunk_bytes: 8 },
        })
        .unwrap();
        sender.start().unwrap();
        assert_eq!(
            sender.push_encoded(EncodedAudioChunk::new(
                VoiceCodec::OpusWebM,
                vec![0; 5],
                Duration::ZERO
            )),
            Err(VoiceError::OutputBudgetExceeded {
                actual: 5,
                limit: 4
            }),
        );
    }

    #[test]
    fn rejects_zero_capture_target_and_zero_limits() {
        let zero_duration = VoiceSenderConfig {
            target_chunk_duration: Duration::ZERO,
            ..VoiceSenderConfig::default()
        };
        assert!(VoiceSender::new(zero_duration).is_err());

        let zero_limit = VoiceSenderConfig {
            limits: VoiceLimits { max_chunk_bytes: 0 },
            ..VoiceSenderConfig::default()
        };
        assert!(VoiceSender::new(zero_limit).is_err());
    }

    fn unit(payload: &[u8]) -> EncodedAudioChunk {
        EncodedAudioChunk::new(
            VoiceCodec::OpusWebM,
            payload.to_vec(),
            Duration::from_millis(350),
        )
    }
}
