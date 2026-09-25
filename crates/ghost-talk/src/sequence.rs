use crate::VoiceError;

#[derive(Debug, Default)]
pub(crate) struct SequenceCounter {
    next: u64,
    exhausted: bool,
}

impl SequenceCounter {
    pub(crate) fn take(&mut self) -> Result<u64, VoiceError> {
        if self.exhausted {
            return Err(VoiceError::SequenceExhausted);
        }
        let current = self.next;
        match self.next.checked_add(1) {
            Some(next) => self.next = next,
            None => self.exhausted = true,
        }
        Ok(current)
    }

    #[cfg(test)]
    pub(crate) fn set_for_test(&mut self, next: u64) {
        self.next = next;
        self.exhausted = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_boundary_emits_max_once_then_fails_closed() {
        let mut counter = SequenceCounter::default();
        counter.set_for_test(u64::MAX);
        assert_eq!(counter.take().unwrap(), u64::MAX);
        assert_eq!(counter.take(), Err(VoiceError::SequenceExhausted));
    }
}
