use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

use ghost_talk::VoiceChunk;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;

mod audio_queue;
use audio_queue::AudioQueue;

const GAP_WAIT_MS: u32 = 1_200;
const INITIAL_BUFFER_SECONDS: f64 = 0.4;
const MAX_BUFFERED_CHUNKS: usize = 256;
const MAX_CHUNK_BYTES: usize = 128 * 1024;

/// Browser receiver for complete serialized Ghost Talk `VoiceChunk`s.
/// It provides bounded ordering/gap behavior and schedules every decoded Opus
/// window to its full decoded duration on one continuous AudioContext timeline.
#[derive(Clone)]
pub struct BrowserVoiceReceiver {
    inner: Rc<RefCell<ReceiverInner>>,
}

struct ReceiverInner {
    active_stream: Option<u128>,
    retired_streams: VecDeque<u128>,
    expected: u64,
    buffered: BTreeMap<u64, VoiceChunk>,
    generation: u64,
    gap_generation: u64,
    gap_armed: bool,
    audio: AudioQueue,
}

impl BrowserVoiceReceiver {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(ReceiverInner {
                active_stream: None,
                retired_streams: VecDeque::new(),
                expected: 0,
                buffered: BTreeMap::new(),
                generation: 0,
                gap_generation: 0,
                gap_armed: false,
                audio: AudioQueue::new(),
            })),
        }
    }

    pub fn receive_encoded(&self, bytes: &[u8]) -> Result<(), String> {
        let chunk = decode_voice_chunk(bytes)?;
        let start_gap = {
            let mut state = self.inner.borrow_mut();
            if !select_stream(&mut state, chunk.stream_id) {
                return Ok(());
            }
            if !queue_voice_chunk(&mut state, chunk)? {
                return Ok(());
            }
            drain_ready(&mut state);
            update_gap_timer_state(&mut state)
        };
        if start_gap {
            arm_gap_timer(self.inner.clone());
        }
        Ok(())
    }

    pub fn reset(&self) {
        let mut state = self.inner.borrow_mut();
        if let Some(active) = state.active_stream.take() {
            state.retired_streams.push_back(active);
        }
        state.expected = 0;
        state.buffered.clear();
        state.generation = state.generation.wrapping_add(1);
        state.gap_generation = state.gap_generation.wrapping_add(1);
        state.gap_armed = false;
        state.audio.reset();
    }

    pub fn close(&self) {
        self.reset();
    }
}

impl Default for BrowserVoiceReceiver {
    fn default() -> Self {
        Self::new()
    }
}

fn decode_voice_chunk(bytes: &[u8]) -> Result<VoiceChunk, String> {
    let chunk = VoiceChunk::decode(bytes).map_err(|error| error.to_string())?;
    if chunk.payload.len() > MAX_CHUNK_BYTES {
        return Err("Voice chunk exceeds the browser receiver limit.".into());
    }
    Ok(chunk)
}

fn select_stream(state: &mut ReceiverInner, stream_id: u128) -> bool {
    if state.active_stream == Some(stream_id) {
        return true;
    }
    if state.retired_streams.contains(&stream_id) {
        return false;
    }
    if let Some(active) = state.active_stream {
        retire_stream(state, active);
    }
    reset_for_stream(state, stream_id);
    true
}

fn retire_stream(state: &mut ReceiverInner, stream_id: u128) {
    state.retired_streams.push_back(stream_id);
    while state.retired_streams.len() > 8 {
        state.retired_streams.pop_front();
    }
}

fn reset_for_stream(state: &mut ReceiverInner, stream_id: u128) {
    state.active_stream = Some(stream_id);
    state.expected = 0;
    state.buffered.clear();
    state.generation = state.generation.wrapping_add(1);
    state.gap_generation = state.gap_generation.wrapping_add(1);
    state.gap_armed = false;
    state.audio.reset();
}

fn queue_voice_chunk(state: &mut ReceiverInner, chunk: VoiceChunk) -> Result<bool, String> {
    if chunk.sequence < state.expected || state.buffered.contains_key(&chunk.sequence) {
        return Ok(false);
    }
    if state.buffered.len() >= MAX_BUFFERED_CHUNKS {
        return Err("Voice reorder buffer is full.".into());
    }
    state.buffered.insert(chunk.sequence, chunk);
    Ok(true)
}

fn update_gap_timer_state(state: &mut ReceiverInner) -> bool {
    let has_gap = state
        .buffered
        .keys()
        .next()
        .is_some_and(|sequence| *sequence > state.expected);
    match (has_gap, state.gap_armed) {
        (true, false) => {
            state.gap_generation = state.gap_generation.wrapping_add(1);
            state.gap_armed = true;
            true
        }
        (false, true) => {
            state.gap_generation = state.gap_generation.wrapping_add(1);
            state.gap_armed = false;
            false
        }
        _ => false,
    }
}

fn drain_ready(state: &mut ReceiverInner) {
    while let Some(chunk) = state.buffered.remove(&state.expected) {
        state.expected = state.expected.saturating_add(1);
        state.audio.enqueue(chunk.payload);
    }
}

fn arm_gap_timer(inner: Rc<RefCell<ReceiverInner>>) {
    let marker = inner.borrow().gap_generation;
    spawn_local(async move {
        TimeoutFuture::new(GAP_WAIT_MS).await;
        let mut state = inner.borrow_mut();
        if state.gap_generation != marker || !state.gap_armed {
            return;
        }
        state.gap_armed = false;
        let Some(lowest) = state.buffered.keys().next().copied() else {
            return;
        };
        if lowest <= state.expected {
            drain_ready(&mut state);
            return;
        }
        state.expected = lowest;
        state.gap_generation = state.gap_generation.wrapping_add(1);
        drain_ready(&mut state);
        if state
            .buffered
            .keys()
            .next()
            .is_some_and(|sequence| *sequence > state.expected)
        {
            state.gap_generation = state.gap_generation.wrapping_add(1);
            state.gap_armed = true;
            drop(state);
            arm_gap_timer(inner.clone());
        }
    });
}
