use std::time::{Duration, Instant};

use super::*;
use crate::{PlaybackReceipt, VoiceCodec, VoiceLimits};

#[derive(Debug, Default)]
struct MockPlayback {
    now: Duration,
    scheduled: Vec<(u64, Duration, Duration)>,
    reset_count: usize,
    drain_count: usize,
}

impl PlaybackBackend for MockPlayback {
    fn now(&self) -> Duration {
        self.now
    }

    fn decode_and_schedule(
        &mut self,
        chunk: &VoiceChunk,
        start_at: Duration,
    ) -> Result<PlaybackReceipt, VoiceError> {
        if chunk.payload == b"bad" {
            return Err(VoiceError::AudioBackend("decode failed".into()));
        }
        let millis = chunk.payload.first().copied().map(u64::from).unwrap_or(1);
        let decoded_duration = Duration::from_millis(millis);
        self.scheduled
            .push((chunk.sequence, start_at, decoded_duration));
        Ok(PlaybackReceipt { decoded_duration })
    }

    fn reset(&mut self) -> Result<(), VoiceError> {
        self.reset_count += 1;
        self.scheduled.clear();
        Ok(())
    }

    fn drain(&mut self) -> Result<(), VoiceError> {
        self.drain_count += 1;
        Ok(())
    }
}

fn chunk(stream: u128, sequence: u64, duration_ms: u8) -> VoiceChunk {
    VoiceChunk::new(stream, sequence, VoiceCodec::OpusWebM, vec![duration_ms])
}

fn receiver() -> VoiceReceiver<MockPlayback> {
    VoiceReceiver::new(
        VoiceReceiverConfig {
            reorder_capacity: 8,
            gap_wait: Duration::from_millis(100),
            initial_buffer: Duration::from_millis(20),
            limits: VoiceLimits::default(),
        },
        MockPlayback::default(),
    )
    .unwrap()
}

#[test]
fn out_of_order_chunks_play_in_sequence() {
    let mut receiver = receiver();
    let now = Instant::now();
    for sequence in [0, 1, 3, 4, 2, 5] {
        receiver
            .receive_chunk_at(chunk(7, sequence, 10), now)
            .unwrap();
    }
    let sequences: Vec<_> = receiver
        .backend()
        .scheduled
        .iter()
        .map(|entry| entry.0)
        .collect();
    assert_eq!(sequences, vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(receiver.stats().reordered_chunks, 2);
}

#[test]
fn duplicate_and_old_chunks_do_not_replay() {
    let mut receiver = receiver();
    let first = chunk(9, 0, 10);
    receiver.receive_chunk(first.clone()).unwrap();
    assert_eq!(
        receiver.receive_chunk(first).unwrap(),
        ReceiveOutcome::Duplicate { sequence: 0 }
    );
    assert_eq!(receiver.stats().played_chunks, 1);
}

#[test]
fn bounded_gap_timeout_skips_missing_sequences() {
    let mut receiver = receiver();
    let now = Instant::now();
    receiver.receive_chunk_at(chunk(11, 0, 10), now).unwrap();
    receiver.receive_chunk_at(chunk(11, 3, 10), now).unwrap();
    assert_eq!(receiver.backend().scheduled.len(), 1);
    receiver
        .advance_to(now + Duration::from_millis(99))
        .unwrap();
    assert_eq!(receiver.backend().scheduled.len(), 1);
    receiver
        .advance_to(now + Duration::from_millis(100))
        .unwrap();
    assert_eq!(receiver.stats().lost_chunks, 2);
    assert_eq!(
        receiver
            .backend()
            .scheduled
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![0, 3]
    );
}

#[test]
fn single_missing_chunk_is_counted_and_following_audio_continues() {
    let mut receiver = receiver();
    let now = Instant::now();
    receiver.receive_chunk_at(chunk(12, 0, 10), now).unwrap();
    receiver.receive_chunk_at(chunk(12, 2, 10), now).unwrap();
    receiver
        .advance_to(now + Duration::from_millis(100))
        .unwrap();
    assert_eq!(receiver.stats().lost_chunks, 1);
    assert_eq!(
        receiver
            .backend()
            .scheduled
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[test]
fn too_old_chunk_after_gap_is_rejected_without_replay() {
    let mut receiver = receiver();
    let now = Instant::now();
    receiver.receive_chunk_at(chunk(13, 0, 10), now).unwrap();
    receiver.receive_chunk_at(chunk(13, 5, 10), now).unwrap();
    receiver
        .advance_to(now + Duration::from_millis(100))
        .unwrap();
    assert_eq!(
        receiver
            .receive_chunk_at(chunk(13, 3, 10), now + Duration::from_millis(101))
            .unwrap(),
        ReceiveOutcome::TooOld { sequence: 3 },
    );
    assert_eq!(receiver.stats().played_chunks, 2);
}

#[test]
fn reordering_continues_after_a_timed_out_gap() {
    let mut receiver = receiver();
    let now = Instant::now();
    receiver.receive_chunk_at(chunk(14, 0, 10), now).unwrap();
    receiver.receive_chunk_at(chunk(14, 2, 10), now).unwrap();
    receiver
        .advance_to(now + Duration::from_millis(100))
        .unwrap();
    receiver
        .receive_chunk_at(chunk(14, 4, 10), now + Duration::from_millis(101))
        .unwrap();
    receiver
        .receive_chunk_at(chunk(14, 3, 10), now + Duration::from_millis(102))
        .unwrap();
    assert_eq!(
        receiver
            .backend()
            .scheduled
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![0, 2, 3, 4]
    );
}

#[test]
fn stream_restart_rejects_delayed_prior_stream() {
    let mut receiver = receiver();
    receiver.receive_chunk(chunk(100, 0, 10)).unwrap();
    receiver.receive_chunk(chunk(200, 0, 10)).unwrap();
    let outcome = receiver.receive_chunk(chunk(100, 1, 10)).unwrap();
    assert_eq!(outcome, ReceiveOutcome::StaleStream { stream_id: 100 });
    assert_eq!(
        receiver
            .backend()
            .scheduled
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![0]
    );
}

#[test]
fn decode_failure_is_consumed_without_blocking_following_audio() {
    let mut receiver = receiver();
    let mut bad = chunk(1, 0, 1);
    bad.payload = b"bad".to_vec();
    receiver.receive_chunk(bad).unwrap();
    receiver.receive_chunk(chunk(1, 1, 12)).unwrap();
    assert_eq!(receiver.stats().decode_failures, 1);
    assert_eq!(receiver.stats().played_chunks, 1);
    assert_eq!(receiver.backend().scheduled[0].0, 1);
}

#[test]
fn reorder_capacity_is_bounded() {
    let mut receiver = VoiceReceiver::new(
        VoiceReceiverConfig {
            reorder_capacity: 2,
            ..VoiceReceiverConfig::default()
        },
        MockPlayback::default(),
    )
    .unwrap();
    let now = Instant::now();
    receiver.receive_chunk_at(chunk(1, 2, 10), now).unwrap();
    receiver.receive_chunk_at(chunk(1, 3, 10), now).unwrap();
    assert!(receiver.receive_chunk_at(chunk(1, 4, 10), now).is_err());
}

#[test]
fn maximum_sequence_is_consumed_once_then_rejected_as_duplicate() {
    let mut receiver = receiver();
    let now = Instant::now();
    receiver
        .receive_chunk_at(chunk(77, u64::MAX, 10), now)
        .unwrap();
    receiver
        .advance_to(now + Duration::from_millis(100))
        .unwrap();
    assert_eq!(
        receiver
            .backend()
            .scheduled
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![u64::MAX]
    );
    assert_eq!(
        receiver
            .receive_chunk_at(chunk(77, u64::MAX, 10), now + Duration::from_millis(101))
            .unwrap(),
        ReceiveOutcome::Duplicate { sequence: u64::MAX },
    );
    assert_eq!(receiver.stats().played_chunks, 1);
}

mod backend_lifecycle;
