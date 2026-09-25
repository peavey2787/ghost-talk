use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{VoiceChunk, VoiceError};

const RECENT_SEEN_WINDOW: u64 = 2_048;
const RETIRED_STREAM_CAPACITY: usize = 16;

#[derive(Debug)]
pub(crate) struct ReorderBuffer {
    capacity: usize,
    stream_id: Option<u128>,
    expected: u64,
    sequence_exhausted: bool,
    pending: BTreeMap<u64, VoiceChunk>,
    recent_seen: BTreeSet<u64>,
    retired_streams: VecDeque<u128>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InsertResult {
    Ready,
    Buffered,
    Duplicate,
    TooOld,
    StreamStarted,
    StaleStream,
}

impl ReorderBuffer {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            stream_id: None,
            expected: 0,
            sequence_exhausted: false,
            pending: BTreeMap::new(),
            recent_seen: BTreeSet::new(),
            retired_streams: VecDeque::new(),
        }
    }

    pub(crate) fn insert(&mut self, chunk: VoiceChunk) -> Result<InsertResult, VoiceError> {
        let started = self.select_stream(chunk.stream_id);
        if started == StreamSelection::Retired {
            return Ok(InsertResult::StaleStream);
        }
        if let Some(rejected) = self.sequence_rejection(chunk.sequence) {
            return Ok(rejected);
        }
        self.ensure_insert_capacity()?;
        let sequence = chunk.sequence;
        self.pending.insert(sequence, chunk);
        self.recent_seen.insert(sequence);
        self.trim_seen();
        Ok(self.insert_result(started, sequence))
    }

    fn sequence_rejection(&self, sequence: u64) -> Option<InsertResult> {
        if self.sequence_exhausted || sequence < self.expected {
            return Some(if self.recent_seen.contains(&sequence) {
                InsertResult::Duplicate
            } else {
                InsertResult::TooOld
            });
        }
        self.pending
            .contains_key(&sequence)
            .then_some(InsertResult::Duplicate)
    }

    fn ensure_insert_capacity(&self) -> Result<(), VoiceError> {
        if self.pending.len() >= self.capacity {
            return Err(VoiceError::ReorderBufferFull {
                capacity: self.capacity,
            });
        }
        Ok(())
    }

    fn insert_result(&self, started: StreamSelection, sequence: u64) -> InsertResult {
        if started == StreamSelection::Started {
            InsertResult::StreamStarted
        } else if sequence == self.expected {
            InsertResult::Ready
        } else {
            InsertResult::Buffered
        }
    }

    pub(crate) fn pop_expected(&mut self) -> Option<VoiceChunk> {
        let chunk = self.pending.remove(&self.expected)?;
        match self.expected.checked_add(1) {
            Some(next) => self.expected = next,
            None => self.sequence_exhausted = true,
        }
        self.trim_seen();
        Some(chunk)
    }

    pub(crate) fn skip_gap_to_lowest(&mut self) -> u64 {
        let Some((&lowest, _)) = self.pending.first_key_value() else {
            return 0;
        };
        if lowest <= self.expected {
            return 0;
        }
        let lost = lowest - self.expected;
        self.expected = lowest;
        lost
    }

    pub(crate) fn has_gap(&self) -> bool {
        self.pending
            .first_key_value()
            .is_some_and(|(&lowest, _)| lowest > self.expected)
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(crate) fn clear(&mut self) {
        if let Some(stream_id) = self.stream_id.take() {
            self.retire(stream_id);
        }
        self.expected = 0;
        self.sequence_exhausted = false;
        self.pending.clear();
        self.recent_seen.clear();
    }

    fn select_stream(&mut self, incoming: u128) -> StreamSelection {
        if self.stream_id == Some(incoming) {
            return StreamSelection::Current;
        }
        if self.retired_streams.contains(&incoming) {
            return StreamSelection::Retired;
        }
        if let Some(current) = self.stream_id {
            self.retire(current);
        }
        self.start_stream(incoming);
        StreamSelection::Started
    }

    fn start_stream(&mut self, incoming: u128) {
        self.stream_id = Some(incoming);
        self.expected = 0;
        self.sequence_exhausted = false;
        self.pending.clear();
        self.recent_seen.clear();
    }

    fn retire(&mut self, stream_id: u128) {
        if self.retired_streams.contains(&stream_id) {
            return;
        }
        self.retired_streams.push_back(stream_id);
        while self.retired_streams.len() > RETIRED_STREAM_CAPACITY {
            self.retired_streams.pop_front();
        }
    }

    fn trim_seen(&mut self) {
        let floor = self.expected.saturating_sub(RECENT_SEEN_WINDOW);
        self.recent_seen.retain(|sequence| *sequence >= floor);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamSelection {
    Current,
    Started,
    Retired,
}
