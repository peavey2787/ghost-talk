use std::time::Duration;

#[derive(Debug)]
pub(crate) struct PlaybackTimeline {
    initial_buffer: Duration,
    next_start: Option<Duration>,
}

impl PlaybackTimeline {
    pub(crate) fn new(initial_buffer: Duration) -> Self {
        Self {
            initial_buffer,
            next_start: None,
        }
    }

    pub(crate) fn start_for(&self, now: Duration) -> Duration {
        match self.next_start {
            Some(next) if next > now => next,
            Some(_) => now,
            None => now.saturating_add(self.initial_buffer),
        }
    }

    pub(crate) fn commit(&mut self, start: Duration, decoded_duration: Duration) {
        self.next_start = Some(start.saturating_add(decoded_duration));
    }

    pub(crate) fn reset(&mut self) {
        self.next_start = None;
    }
}
