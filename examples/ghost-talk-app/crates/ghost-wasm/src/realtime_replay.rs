#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

const REPLAY_WINDOW_CAPACITY: usize = 8192;

thread_local! {
    static WINDOW: RefCell<ghost_realtime::ReplayWindow> =
        RefCell::new(ghost_realtime::ReplayWindow::new(REPLAY_WINDOW_CAPACITY));
}

pub(crate) fn accept_gtr1(carrier: &ghost_protocol::Gtr1Envelope) -> bool {
    WINDOW.with(|window| {
        window
            .borrow_mut()
            .accept(ghost_realtime::ReplayKey::from_gtr1(carrier))
    })
}

pub(crate) fn accept_fields(sender: &str, sid: &str, message_id: &str) -> bool {
    let Ok(carrier) = ghost_protocol::Gtr1Envelope::from_hex_ids(
        sender,
        message_id,
        sid,
        vec![1],
    ) else {
        return false;
    };
    accept_gtr1(&carrier)
}
