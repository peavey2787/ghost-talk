//! p2p-net route for authenticated conversations. Kaspa carries the signed
//! transport announcements (the signal layer); p2p-net then carries the exact
//! sealed GTR1 realtime bytes, with Kaspa as the fallback carrier.

mod announce;
mod control;
mod events;
mod lifecycle;
mod room_announcer;
mod sessions;
mod shared_topics;
mod subscription;

pub(super) use announce::start_session;
pub(super) use control::process_transport_control;
pub(super) use lifecycle::use_p2p_effect;
pub(super) use room_announcer::use_room_announcer;
pub(super) use sessions::use_session_announce_effect;
pub(super) use shared_topics::use_retention_policy;

use super::CallRuntime;

/// Record a p2p/realtime transition in the Protocol Debug log when enabled.
fn trace(runtime: &CallRuntime, event: &'static str, details: String) {
    if runtime.profile_ref.borrow().settings.debug_logging {
        crate::controllers::debug::record("p2p-net", event, details);
    }
}

fn route_is_automatic(runtime: &CallRuntime) -> bool {
    runtime.profile_ref.borrow().settings.runs_swarm()
}
