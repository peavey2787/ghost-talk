use std::time::Duration;

use super::{chunk, MockPlayback};
use crate::{VoiceReceiver, VoiceReceiverConfig};

#[test]
fn scheduling_uses_full_actual_decoded_duration() {
    let mut receiver = VoiceReceiver::new(
        VoiceReceiverConfig {
            initial_buffer: Duration::from_millis(20),
            ..VoiceReceiverConfig::default()
        },
        MockPlayback::default(),
    )
    .unwrap();
    receiver.backend_mut().now = Duration::from_secs(10);
    for (sequence, duration) in [(0, 47), (1, 62), (2, 51)] {
        receiver
            .receive_chunk(chunk(1, sequence, duration))
            .unwrap();
    }
    let scheduled = &receiver.backend().scheduled;
    assert_eq!(scheduled[0].1, Duration::from_millis(10_020));
    assert_eq!(scheduled[1].1, scheduled[0].1 + scheduled[0].2);
    assert_eq!(scheduled[2].1, scheduled[1].1 + scheduled[1].2);
}

#[test]
fn reset_and_drain_delegate_to_media_backend() {
    let mut receiver =
        VoiceReceiver::new(VoiceReceiverConfig::default(), MockPlayback::default()).unwrap();
    receiver.receive_chunk(chunk(1, 0, 10)).unwrap();
    receiver.reset().unwrap();
    receiver.drain().unwrap();
    assert!(receiver.backend().reset_count >= 2);
    assert_eq!(receiver.backend().drain_count, 1);
}

#[test]
fn drain_flushes_buffered_audio_across_missing_sequences() {
    let mut receiver =
        VoiceReceiver::new(VoiceReceiverConfig::default(), MockPlayback::default()).unwrap();
    receiver.receive_chunk(chunk(2, 0, 10)).unwrap();
    receiver.receive_chunk(chunk(2, 2, 10)).unwrap();
    assert_eq!(receiver.stats().buffered_chunks, 1);
    receiver.drain().unwrap();
    assert_eq!(
        receiver
            .backend()
            .scheduled
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(receiver.stats().lost_chunks, 1);
    assert_eq!(receiver.stats().buffered_chunks, 0);
    assert_eq!(receiver.backend().drain_count, 1);
}
