use std::time::Duration;

use crate::{VoiceChunk, VoiceError};

/// Result returned after one chunk has been decoded and scheduled for playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackReceipt {
    /// Actual decoded duration, not the configured capture-window duration.
    pub decoded_duration: Duration,
}

/// Platform audio backend used by [`crate::VoiceReceiver`].
///
/// Implementations decode the complete chunk and schedule the complete decoded
/// buffer at `start_at`. The returned duration drives the next SDK schedule.
pub trait PlaybackBackend {
    /// Returns the backend's current monotonic media-clock position.
    fn now(&self) -> Duration;

    /// Decodes and schedules one complete media chunk at `start_at`.
    fn decode_and_schedule(
        &mut self,
        chunk: &VoiceChunk,
        start_at: Duration,
    ) -> Result<PlaybackReceipt, VoiceError>;

    /// Resets backend playback state when a new stream replaces the old stream.
    fn reset(&mut self) -> Result<(), VoiceError> {
        Ok(())
    }

    /// Waits for already scheduled playback to finish, when supported.
    fn drain(&mut self) -> Result<(), VoiceError> {
        Ok(())
    }
}
