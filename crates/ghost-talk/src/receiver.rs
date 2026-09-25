use std::time::Instant;

use crate::{
    reorder::{InsertResult, ReorderBuffer},
    timing::PlaybackTimeline,
    PlaybackBackend, VoiceChunk, VoiceError, VoiceReceiverConfig, VoiceReceiverStats,
    VOICE_WIRE_VERSION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Result of submitting one chunk to a [`VoiceReceiver`].
pub enum ReceiveOutcome {
    /// One or more in-order chunks were scheduled immediately.
    Played {
        /// Number of contiguous chunks scheduled for playback.
        chunks: usize,
    },
    /// The chunk was retained while waiting for an earlier sequence.
    Buffered,
    /// The sequence had already been observed.
    Duplicate {
        /// Sequence number that had already been observed.
        sequence: u64,
    },
    /// The sequence predates the receiver's current accepted window.
    TooOld {
        /// Sequence number that fell behind the accepted receive window.
        sequence: u64,
    },
    /// A newer stream identity replaced the previous stream.
    StreamStarted {
        /// Identity of the newly accepted stream.
        stream_id: u128,
    },
    /// A chunk from a superseded stream was rejected.
    StaleStream {
        /// Identity of the superseded stream that supplied the rejected chunk.
        stream_id: u128,
    },
}

/// Reorders, schedules, and accounts for complete media chunks.
pub struct VoiceReceiver<B: PlaybackBackend> {
    config: VoiceReceiverConfig,
    reorder: ReorderBuffer,
    backend: B,
    timeline: PlaybackTimeline,
    gap_deadline: Option<Instant>,
    stats: VoiceReceiverStats,
}

impl<B: PlaybackBackend> VoiceReceiver<B> {
    /// Creates a receiver and validates its reorder/playback configuration.
    pub fn new(config: VoiceReceiverConfig, backend: B) -> Result<Self, VoiceError> {
        validate_config(&config)?;
        Ok(Self {
            reorder: ReorderBuffer::new(config.reorder_capacity),
            timeline: PlaybackTimeline::new(config.initial_buffer),
            config,
            backend,
            gap_deadline: None,
            stats: VoiceReceiverStats::default(),
        })
    }

    /// Receives one chunk using the current monotonic instant.
    pub fn receive_chunk(&mut self, chunk: VoiceChunk) -> Result<ReceiveOutcome, VoiceError> {
        self.receive_chunk_at(chunk, Instant::now())
    }

    /// Receives one chunk using an explicit monotonic instant for deterministic hosts/tests.
    pub fn receive_chunk_at(
        &mut self,
        chunk: VoiceChunk,
        now: Instant,
    ) -> Result<ReceiveOutcome, VoiceError> {
        self.advance_to(now)?;
        self.validate_chunk(&chunk)?;
        self.stats.received_chunks = self.stats.received_chunks.saturating_add(1);
        let sequence = chunk.sequence;
        let stream_id = chunk.stream_id;
        let insertion = self.reorder.insert(chunk)?;
        let outcome = match insertion {
            InsertResult::Duplicate => {
                self.stats.duplicate_chunks = self.stats.duplicate_chunks.saturating_add(1);
                ReceiveOutcome::Duplicate { sequence }
            }
            InsertResult::TooOld => {
                self.stats.late_chunks = self.stats.late_chunks.saturating_add(1);
                ReceiveOutcome::TooOld { sequence }
            }
            InsertResult::StaleStream => {
                self.stats.late_chunks = self.stats.late_chunks.saturating_add(1);
                ReceiveOutcome::StaleStream { stream_id }
            }
            InsertResult::StreamStarted => {
                self.backend.reset()?;
                self.timeline.reset();
                self.gap_deadline = None;
                let _ = self.play_ready()?;
                self.arm_gap(now);
                ReceiveOutcome::StreamStarted { stream_id }
            }
            InsertResult::Buffered => {
                self.stats.reordered_chunks = self.stats.reordered_chunks.saturating_add(1);
                self.arm_gap(now);
                ReceiveOutcome::Buffered
            }
            InsertResult::Ready => {
                let played = self.play_ready()?;
                self.arm_gap(now);
                ReceiveOutcome::Played { chunks: played }
            }
        };
        self.stats.buffered_chunks = self.reorder.len();
        Ok(outcome)
    }

    /// Advances the receiver's bounded missing-sequence policy. Platform
    /// adapters should call this when the current gap deadline fires.
    pub fn advance_to(&mut self, now: Instant) -> Result<usize, VoiceError> {
        if self.gap_deadline.is_none_or(|deadline| now < deadline) {
            return Ok(0);
        }
        self.gap_deadline = None;
        let lost = self.reorder.skip_gap_to_lowest();
        self.stats.lost_chunks = self.stats.lost_chunks.saturating_add(lost);
        let played = self.play_ready()?;
        self.arm_gap(now);
        self.stats.buffered_chunks = self.reorder.len();
        Ok(played)
    }

    /// Returns the current missing-sequence deadline, if the buffer has a gap.
    pub fn next_gap_deadline(&self) -> Option<Instant> {
        self.gap_deadline
    }

    /// Clears buffered/replay state and resets the playback backend.
    pub fn reset(&mut self) -> Result<(), VoiceError> {
        self.reorder.clear();
        self.gap_deadline = None;
        self.timeline.reset();
        self.backend.reset()?;
        self.stats.buffered_chunks = 0;
        Ok(())
    }

    /// Flushes every accepted buffered chunk in sequence order, accounting for
    /// any still-missing ranges as lost, then waits for the media backend to
    /// finish its scheduled playback.
    pub fn drain(&mut self) -> Result<(), VoiceError> {
        self.gap_deadline = None;
        while self.reorder.len() > 0 {
            let lost = self.reorder.skip_gap_to_lowest();
            self.stats.lost_chunks = self.stats.lost_chunks.saturating_add(lost);
            let before = self.reorder.len();
            let _ = self.play_ready()?;
            if lost == 0 && self.reorder.len() == before {
                break;
            }
        }
        self.stats.buffered_chunks = self.reorder.len();
        self.backend.drain()
    }

    /// Returns a snapshot of cumulative receiver statistics.
    pub fn stats(&self) -> VoiceReceiverStats {
        self.stats.clone()
    }

    /// Borrows the platform playback backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutably borrows the platform playback backend.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    fn validate_chunk(&self, chunk: &VoiceChunk) -> Result<(), VoiceError> {
        if chunk.version != VOICE_WIRE_VERSION {
            return Err(VoiceError::UnsupportedVersion(chunk.version));
        }
        if chunk.payload.len() > self.config.limits.max_chunk_bytes {
            return Err(VoiceError::ChunkTooLarge {
                actual: chunk.payload.len(),
                limit: self.config.limits.max_chunk_bytes,
            });
        }
        Ok(())
    }

    fn play_ready(&mut self) -> Result<usize, VoiceError> {
        let mut played = 0_usize;
        while let Some(chunk) = self.reorder.pop_expected() {
            let start_at = self.timeline.start_for(self.backend.now());
            match self.backend.decode_and_schedule(&chunk, start_at) {
                Ok(receipt) if !receipt.decoded_duration.is_zero() => {
                    self.timeline.commit(start_at, receipt.decoded_duration);
                    self.stats.played_chunks = self.stats.played_chunks.saturating_add(1);
                    played += 1;
                }
                Ok(_) | Err(_) => {
                    self.stats.decode_failures = self.stats.decode_failures.saturating_add(1);
                }
            }
        }
        Ok(played)
    }

    fn arm_gap(&mut self, now: Instant) {
        if self.reorder.has_gap() {
            self.gap_deadline = self
                .gap_deadline
                .or_else(|| now.checked_add(self.config.gap_wait));
        } else {
            self.gap_deadline = None;
        }
    }
}

fn validate_config(config: &VoiceReceiverConfig) -> Result<(), VoiceError> {
    if config.reorder_capacity == 0 {
        return Err(VoiceError::MalformedChunk(
            "reorder_capacity must be non-zero",
        ));
    }
    if config.limits.max_chunk_bytes == 0 {
        return Err(VoiceError::MalformedChunk(
            "max_chunk_bytes must be non-zero",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod unit_tests;
