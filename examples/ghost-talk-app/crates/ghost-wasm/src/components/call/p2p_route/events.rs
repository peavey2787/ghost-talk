use ghost_p2p::{P2pNetEvent, P2pNetEventSubscription, P2pRouteState};
use wasm_bindgen_futures::spawn_local;

use super::{announce::announce_active_sessions, trace};
use crate::components::call::CallRuntime;

/// Waits before each re-dial of a bound peer that is still not connected. A
/// relay-circuit dial can fail silently (p2p-net reports it only in its
/// diagnostics), e.g. when it races the peer's relay reservation after a node
/// restart, so one attempt is never enough. The last wait repeats for as long
/// as the peer stays bound and disconnected.
const REDIAL_DELAYS_MS: [u32; 6] = [2_000, 4_000, 8_000, 15_000, 30_000, 60_000];
const ANNOUNCE_SETTLE_MS: u32 = 1_500;

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
                runtime.swarm.offline();
                runtime.on_error.emit(format!("p2p-net events: {error}"));
                return;
            }
        }
    }
}

async fn apply(runtime: &CallRuntime, event: P2pNetEvent) {
    trace(runtime, "node-event", format!("{event:?}"));
    runtime.swarm.apply(&event);
    if matches!(event, P2pNetEvent::LocalBindingChanged(_)) {
        schedule_announce(runtime);
        return;
    }
    if let Some(state) = next_state(runtime, &event) {
        runtime.p2p_state.set(state);
    }
    if let P2pNetEvent::PeerDisconnected(peer_id) = &event {
        schedule_redial(runtime, peer_id);
    }
}

thread_local! {
    static ANNOUNCE_TICKET: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Binding changes arrive in bursts while relays settle; announce once, after
/// they stop, so a burst costs one Kaspa transaction per session.
fn schedule_announce(runtime: &CallRuntime) {
    let ticket = ANNOUNCE_TICKET.with(|t| {
        t.set(t.get().wrapping_add(1));
        t.get()
    });
    let runtime = runtime.clone();
    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(ANNOUNCE_SETTLE_MS).await;
        if ANNOUNCE_TICKET.with(std::cell::Cell::get) == ticket {
            announce_active_sessions(&runtime).await;
        }
    });
}

thread_local! {
    /// Bound peers with a re-dial loop already running.
    static REDIALING: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::default();
}

/// Keep re-dialing a bound peer until it connects: after an idle relay circuit
/// closes, and after an announcement's dial that may have failed silently.
pub(super) fn schedule_redial(runtime: &CallRuntime, peer_id: &str) {
    if !bound(runtime, peer_id) || !super::route_is_automatic(runtime) {
        return;
    }
    if !REDIALING.with(|peers| peers.borrow_mut().insert(peer_id.to_owned())) {
        return;
    }
    let (runtime, peer_id) = (runtime.clone(), peer_id.to_owned());
    spawn_local(async move {
        redial_until_connected(&runtime, &peer_id).await;
        REDIALING.with(|peers| peers.borrow_mut().remove(&peer_id));
    });
}

async fn redial_until_connected(runtime: &CallRuntime, peer_id: &str) {
    let last = REDIAL_DELAYS_MS[REDIAL_DELAYS_MS.len() - 1];
    for delay in REDIAL_DELAYS_MS.into_iter().chain(std::iter::repeat(last)) {
        gloo_timers::future::TimeoutFuture::new(delay).await;
        if !bound(runtime, peer_id) || !super::route_is_automatic(runtime) {
            return;
        }
        if runtime.swarm.is_connected(peer_id) {
            super::control::mark_if_connected(runtime, peer_id);
            return;
        }
        let last_error = runtime.p2p().last_dial_error().await.unwrap_or_default();
        trace(
            runtime,
            "redial",
            format!("peer={peer_id} last_dial_error={last_error}"),
        );
        super::control::redial(runtime, peer_id).await;
    }
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
        // `Online` is the local node being up, not a peer route; the route
        // is driven by peer connects/disconnects and dials. (Reading the
        // route state here would also see this long-lived handler's stale
        // first-render value and knock a live route back to Connecting.)
        P2pNetEvent::Online | P2pNetEvent::LocalBindingChanged(_) => None,
        P2pNetEvent::Offline => Some(P2pRouteState::KaspaFallback),
    }
}

fn bound(runtime: &CallRuntime, peer_id: &str) -> bool {
    runtime.p2p().has_bound_peer(peer_id)
}

pub(super) fn current(runtime: &CallRuntime, generation: u64) -> bool {
    *runtime.p2p_generation.borrow() == generation
}
