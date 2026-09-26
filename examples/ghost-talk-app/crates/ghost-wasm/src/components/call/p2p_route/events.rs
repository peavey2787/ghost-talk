use ghost_p2p::{P2pNetEvent, P2pNetEventSubscription, P2pRouteState};
use wasm_bindgen_futures::spawn_local;

use super::{announce::announce_active_sessions, trace};
use crate::components::call::CallRuntime;

const REDIAL_DELAY_MS: u32 = 2_000;

/// Follow p2p-net lifecycle events for one node generation.
pub(super) fn spawn_event_loop(runtime: CallRuntime, generation: u64) -> Result<(), String> {
    let events = runtime.p2p().subscribe_events()?;
    spawn_local(run(runtime, generation, events));
    Ok(())
}

async fn run(runtime: CallRuntime, generation: u64, mut events: P2pNetEventSubscription) {
    loop {
        let next = events.next().await;
        if !current(&runtime, generation) {
            return;
        }
        match next {
            Ok(event) => apply(&runtime, event).await,
            Err(error) => {
                runtime.p2p_state.set(P2pRouteState::KaspaFallback);
                runtime.on_error.emit(format!("p2p-net events: {error}"));
                return;
            }
        }
    }
}

async fn apply(runtime: &CallRuntime, event: P2pNetEvent) {
    trace(runtime, "node-event", format!("{event:?}"));
    if matches!(event, P2pNetEvent::LocalBindingChanged(_)) {
        announce_active_sessions(runtime).await;
        return;
    }
    if let Some(state) = next_state(runtime, &event) {
        runtime.p2p_state.set(state);
    }
    if let P2pNetEvent::PeerDisconnected(peer_id) = &event {
        schedule_redial(runtime, peer_id);
    }
}

/// Idle relay circuits are closed by p2p-net; while the peer's session is
/// still eligible, reconnect so realtime keeps its direct route.
fn schedule_redial(runtime: &CallRuntime, peer_id: &str) {
    if !bound(runtime, peer_id) || !super::route_is_automatic(runtime) {
        return;
    }
    let (runtime, peer_id) = (runtime.clone(), peer_id.to_owned());
    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(REDIAL_DELAY_MS).await;
        if bound(&runtime, &peer_id) && !runtime.p2p().is_connected(&peer_id).await.unwrap_or(true)
        {
            super::control::redial(&runtime, &peer_id).await;
        }
    });
}

/// Route-state transitions; only peers bound to an authenticated session count.
fn next_state(runtime: &CallRuntime, event: &P2pNetEvent) -> Option<P2pRouteState> {
    match event {
        P2pNetEvent::PeerConnected(peer_id) => {
            bound(runtime, peer_id).then_some(P2pRouteState::Connected)
        }
        P2pNetEvent::PeerDisconnected(peer_id) => {
            bound(runtime, peer_id).then_some(P2pRouteState::KaspaFallback)
        }
        P2pNetEvent::Online => {
            (*runtime.p2p_state == P2pRouteState::Unavailable).then_some(P2pRouteState::Connecting)
        }
        P2pNetEvent::Offline => Some(P2pRouteState::KaspaFallback),
        P2pNetEvent::LocalBindingChanged(_) => None,
    }
}

fn bound(runtime: &CallRuntime, peer_id: &str) -> bool {
    runtime.p2p().has_bound_peer(peer_id)
}

pub(super) fn current(runtime: &CallRuntime, generation: u64) -> bool {
    *runtime.p2p_generation.borrow() == generation
}
