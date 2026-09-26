use ghost_api::{DebugLogEntry, DebugLogSnapshot};
use serde_json::Value;
use std::{cell::RefCell, collections::VecDeque};

const MAX_ENTRIES: usize = 2_000;

#[derive(Default)]
struct BrowserDebugLog {
    enabled: bool,
    sequence: u64,
    entries: VecDeque<DebugLogEntry>,
}

thread_local! {
    static LOG: RefCell<BrowserDebugLog> = RefCell::new(BrowserDebugLog::default());
}

pub(in crate::native::browser_host) fn invoke(
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    match command {
        "debug_log_set_enabled" => set_enabled(args),
        "debug_log_snapshot" => snapshot(args),
        "debug_log_clear" => clear(),
        "debug_log_record" => record_command(args),
        _ => Err(format!("unknown browser debug command: {command}")),
    }
}

pub(in crate::native::browser_host) fn record(
    level: &str,
    category: &str,
    event: &str,
    details: impl Into<String>,
) {
    LOG.with(|log| {
        let mut log = log.borrow_mut();
        if !log.enabled {
            return;
        }
        log.sequence = log.sequence.saturating_add(1);
        let sequence = log.sequence;
        log.entries.push_back(DebugLogEntry {
            sequence,
            timestamp_ms: js_sys::Date::now().max(0.0) as u64,
            level: level.to_owned(),
            category: category.to_owned(),
            event: event.to_owned(),
            details: details.into(),
        });
        while log.entries.len() > MAX_ENTRIES {
            log.entries.pop_front();
        }
    });
}

pub(in crate::native::browser_host) fn enabled() -> bool {
    LOG.with(|log| log.borrow().enabled)
}

fn set_enabled(args: &Value) -> Result<Value, String> {
    let enabled = args
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or("browser command argument enabled is missing or not a boolean")?;
    LOG.with(|log| log.borrow_mut().enabled = enabled);
    snapshot(&serde_json::json!({}))
}

fn snapshot(args: &Value) -> Result<Value, String> {
    let since = args
        .get("sinceSequence")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    LOG.with(|log| {
        let log = log.borrow();
        serde_json::to_value(DebugLogSnapshot {
            enabled: log.enabled,
            latest_sequence: log.sequence,
            entries: log
                .entries
                .iter()
                .filter(|entry| entry.sequence > since)
                .cloned()
                .collect(),
        })
        .map_err(|error| error.to_string())
    })
}

fn clear() -> Result<Value, String> {
    LOG.with(|log| {
        let mut log = log.borrow_mut();
        log.entries.clear();
        log.sequence = 0;
    });
    Ok(Value::Null)
}

fn record_command(args: &Value) -> Result<Value, String> {
    let field = |name: &str| {
        args.get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("browser command argument {name} is missing or not a string"))
    };
    record(
        field("level")?,
        field("category")?,
        field("event")?,
        field("details")?,
    );
    Ok(Value::Null)
}
