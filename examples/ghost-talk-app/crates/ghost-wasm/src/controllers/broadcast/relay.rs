use ghost_broadcast::{encode_relay_frame, validate_relay_url, RelayControl, SinkFailure};
use std::{cell::RefCell, collections::{HashMap, VecDeque}};
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{Event, WebSocket};

const MAX_PENDING_BYTES: usize = 8 * 1024 * 1024;

struct RelaySession {
    socket: WebSocket,
    pending: VecDeque<Vec<u8>>,
    pending_bytes: usize,
    failure: Option<String>,
    _handlers: Vec<Closure<dyn FnMut(Event)>>,
}

thread_local! {
    static RELAYS: RefCell<HashMap<String, RelaySession>> = RefCell::new(HashMap::new());
}

pub(super) fn start(
    session_id: &str,
    relay_url: Option<&str>,
    rtmp_server: Option<&str>,
    rtmp_stream_key: Option<&str>,
) -> Result<(), String> {
    let Some(relay_url) = normalized(relay_url) else {
        return Ok(());
    };
    validate_relay_url(relay_url)?;
    let socket = WebSocket::new(relay_url).map_err(crate::native::js_error)?;
    let start = RelayControl::start(
        session_id,
        normalized(rtmp_server).map(str::to_owned),
        normalized(rtmp_stream_key).map(str::to_owned),
    );
    let start_json = serde_json::to_string(&start).map_err(|error| error.to_string())?;
    let handlers = handlers(session_id, &socket, start_json);
    RELAYS.with(|relays| {
        let mut relays = relays.borrow_mut();
        if relays.contains_key(session_id) {
            return Err("broadcast relay session already exists".into());
        }
        relays.insert(session_id.to_owned(), RelaySession {
            socket,
            pending: VecDeque::new(),
            pending_bytes: 0,
            failure: None,
            _handlers: handlers,
        });
        Ok(())
    })
}

pub(super) fn push(
    session_id: &str,
    sequence: u64,
    timestamp_ms: u64,
    encoded: &[u8],
) -> Result<(), String> {
    let frame = encode_relay_frame(sequence, timestamp_ms, encoded)?;
    RELAYS.with(|relays| {
        let mut relays = relays.borrow_mut();
        let Some(session) = relays.get_mut(session_id) else {
            return Ok(());
        };
        if let Some(error) = session.failure.clone() {
            return Err(error);
        }
        if let Err(error) = send_or_queue(session, frame) {
            session.failure = Some(error.clone());
            return Err(error);
        }
        Ok(())
    })
}

pub(super) fn stop(session_id: &str) -> Vec<SinkFailure> {
    RELAYS.with(|relays| {
        let Some(mut session) = relays.borrow_mut().remove(session_id) else {
            return Vec::new();
        };
        let stop = RelayControl::stop(session_id);
        if session.socket.ready_state() == WebSocket::OPEN {
            if let Ok(json) = serde_json::to_string(&stop) {
                if session.socket.send_with_str(&json).is_err() && session.failure.is_none() {
                    session.failure = Some("broadcast relay stop frame failed".into());
                }
            }
        } else if session.pending_bytes > 0 && session.failure.is_none() {
            session.failure = Some("broadcast relay closed before queued frames were delivered".into());
        }
        let _ = session.socket.close();
        session.failure.into_iter().map(relay_failure).collect()
    })
}

fn handlers(
    session_id: &str,
    socket: &WebSocket,
    start_json: String,
) -> Vec<Closure<dyn FnMut(Event)>> {
    let open = on_open(session_id, socket, start_json);
    let error = on_failure(session_id, "broadcast relay WebSocket error");
    let close = on_failure(session_id, "broadcast relay WebSocket closed unexpectedly");
    socket.set_onopen(Some(open.as_ref().unchecked_ref()));
    socket.set_onerror(Some(error.as_ref().unchecked_ref()));
    socket.set_onclose(Some(close.as_ref().unchecked_ref()));
    vec![open, error, close]
}

fn on_open(
    session_id: &str,
    socket: &WebSocket,
    start_json: String,
) -> Closure<dyn FnMut(Event)> {
    let id = session_id.to_owned();
    let socket = socket.clone();
    Closure::new(move |_event: Event| {
        if socket.send_with_str(&start_json).is_err() {
            set_failure(&id, "broadcast relay start frame failed");
            return;
        }
        flush_pending(&id, &socket);
    })
}

fn on_failure(session_id: &str, message: &'static str) -> Closure<dyn FnMut(Event)> {
    let id = session_id.to_owned();
    Closure::new(move |_event: Event| set_failure(&id, message))
}

fn flush_pending(session_id: &str, socket: &WebSocket) {
    let frames = RELAYS.with(|relays| {
        let mut relays = relays.borrow_mut();
        let Some(session) = relays.get_mut(session_id) else {
            return Vec::new();
        };
        session.pending_bytes = 0;
        session.pending.drain(..).collect::<Vec<_>>()
    });
    for frame in frames {
        if socket.send_with_u8_array(&frame).is_err() {
            set_failure(session_id, "broadcast relay frame send failed");
            break;
        }
    }
}

fn send_or_queue(session: &mut RelaySession, frame: Vec<u8>) -> Result<(), String> {
    match session.socket.ready_state() {
        WebSocket::OPEN => session.socket.send_with_u8_array(&frame)
            .map_err(|_| "broadcast relay frame send failed".to_string()),
        WebSocket::CONNECTING => queue_frame(session, frame),
        _ => Err("broadcast relay is not connected".into()),
    }
}

fn queue_frame(session: &mut RelaySession, frame: Vec<u8>) -> Result<(), String> {
    let next = session.pending_bytes.saturating_add(frame.len());
    if next > MAX_PENDING_BYTES {
        return Err("broadcast relay pending queue exceeded 8 MiB".into());
    }
    session.pending_bytes = next;
    session.pending.push_back(frame);
    Ok(())
}

fn set_failure(session_id: &str, message: &str) {
    RELAYS.with(|relays| {
        if let Some(session) = relays.borrow_mut().get_mut(session_id) {
            session.failure.get_or_insert_with(|| message.to_owned());
        }
    });
}

fn relay_failure(error: String) -> SinkFailure {
    SinkFailure { sink: "relay".into(), error }
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
