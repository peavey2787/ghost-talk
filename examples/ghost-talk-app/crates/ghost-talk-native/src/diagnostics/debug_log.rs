#![forbid(unsafe_code)]

use ghost_api::{DebugLogEntry, DebugLogSnapshot};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const MAX_ENTRIES: usize = 4_000;
const MAX_CATEGORY_CHARS: usize = 64;
const MAX_EVENT_CHARS: usize = 96;
const MAX_DETAILS_CHARS: usize = 2_048;

static ENABLED: AtomicBool = AtomicBool::new(false);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static ENTRIES: OnceLock<Mutex<VecDeque<DebugLogEntry>>> = OnceLock::new();

fn entries() -> &'static Mutex<VecDeque<DebugLogEntry>> {
    ENTRIES.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)))
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn record(level: &str, category: &str, event: &str, details: impl AsRef<str>) {
    if !is_enabled() {
        return;
    }
    let entry = DebugLogEntry {
        sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed).saturating_add(1),
        timestamp_ms: crate::time::unix_millis(),
        level: bounded(level, 16),
        category: bounded(category, MAX_CATEGORY_CHARS),
        event: bounded(event, MAX_EVENT_CHARS),
        details: bounded(details.as_ref(), MAX_DETAILS_CHARS),
    };
    if let Ok(mut queue) = entries().lock() {
        while queue.len() >= MAX_ENTRIES {
            queue.pop_front();
        }
        queue.push_back(entry.clone());
    }
    eprintln!(
        "[GhostTalk debug #{} {} {}] {}: {}",
        entry.sequence, entry.level, entry.category, entry.event, entry.details
    );
}

#[tauri::command]
pub fn debug_log_set_enabled(enabled: bool) -> DebugLogSnapshot {
    ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        record(
            "info",
            "debug",
            "logging-enabled",
            "Protocol debug logging enabled by user setting",
        );
    }
    snapshot(None)
}

#[tauri::command]
pub fn debug_log_record(level: String, category: String, event: String, details: String) {
    record(&level, &category, &event, details);
}

#[tauri::command]
pub fn debug_log_snapshot(since_sequence: Option<u64>) -> DebugLogSnapshot {
    snapshot(since_sequence)
}

fn snapshot(since_sequence: Option<u64>) -> DebugLogSnapshot {
    let floor = since_sequence.unwrap_or_default();
    let values = entries()
        .lock()
        .map(|queue| {
            queue
                .iter()
                .filter(|entry| entry.sequence > floor)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    DebugLogSnapshot {
        enabled: is_enabled(),
        latest_sequence: SEQUENCE.load(Ordering::Relaxed),
        entries: values,
    }
}

#[tauri::command]
pub fn debug_log_clear() {
    if let Ok(mut queue) = entries().lock() {
        queue.clear();
    }
    SEQUENCE.store(0, Ordering::Relaxed);
    if is_enabled() {
        record("info", "debug", "log-cleared", "Debug log cleared by user");
    }
}
