use ghost_p2p::{
    GhostP2pTransport, LocalP2pBinding, P2pInfrastructure, P2pRouteState, P2pStartConfig,
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::{
    announce::{announce_active_sessions, forget_sent},
    events::{current, spawn_event_loop},
    shared_topics, trace,
};
use crate::{components::call::CallRuntime, model::Profile};

/// Everything that decides whether (and how) the profile's node runs.
#[derive(Clone, PartialEq)]
struct RouteKey {
    profile_id: String,
    network: Option<String>,
    automatic: bool,
    infrastructure: P2pInfrastructure,
}

impl RouteKey {
    fn new(profile: &Profile) -> Self {
        Self {
            profile_id: profile.id.clone(),
            network: profile
                .wallet
                .as_ref()
                .map(|wallet| wallet.public.network.clone()),
            automatic: profile.settings.runs_swarm(),
            infrastructure: P2pInfrastructure {
                bootstrap_peers: profile.settings.p2p_bootstrap_peers.clone(),
                relay_peers: profile.settings.p2p_relay_peers.clone(),
            },
        }
    }

    fn start_config(&self) -> Option<P2pStartConfig> {
        let network_id = self.network.clone().filter(|_| self.automatic)?;
        Some(P2pStartConfig {
            network_id,
            profile_id: self.profile_id.clone(),
            infrastructure: self.infrastructure.clone(),
        })
    }
}

/// Run one p2p-net node for the profile while its realtime route is Auto or
/// it is in no-spend mode (the swarm then carries its mailbox).
#[hook]
pub(in crate::components::call) fn use_p2p_effect(profile: &Profile, runtime: CallRuntime) {
    use_effect_with(RouteKey::new(profile), move |key| {
        let generation = advance_generation(&runtime);
        match key.start_config() {
            Some(config) => {
                runtime.p2p_state.set(P2pRouteState::Starting);
                runtime.swarm.starting();
                spawn_local(start(runtime.clone(), generation, config));
            }
            None => {
                runtime.p2p_state.set(P2pRouteState::Unavailable);
                runtime.swarm.disable();
            }
        }
        move || retire(&runtime)
    });
}

fn advance_generation(runtime: &CallRuntime) -> u64 {
    let mut value = runtime.p2p_generation.borrow_mut();
    *value = value.wrapping_add(1);
    *value
}

/// A node start that never resolves (an unreachable relay or bootstrap peer)
/// must not leave the swarm down for the whole session.
const START_TIMEOUT_MS: u32 = 30_000;
/// Waits before each restart of a node that failed or timed out; the last one
/// repeats while this generation stays current.
const START_RETRY_MS: [u32; 4] = [5_000, 10_000, 30_000, 60_000];

async fn start(runtime: CallRuntime, generation: u64, config: P2pStartConfig) {
    for attempt in 0.. {
        let started = start_node(&runtime, config.clone()).await;
        if !current(&runtime, generation) {
            // The effect retired while this node was starting; the node is stale.
            shutdown_detached(&runtime);
            return;
        }
        match started {
            Ok(binding) => return on_started(&runtime, generation, binding).await,
            Err(error) => on_start_failed(&runtime, error),
        }
        let delay = START_RETRY_MS[attempt.min(START_RETRY_MS.len() - 1)];
        gloo_timers::future::TimeoutFuture::new(delay).await;
        if !current(&runtime, generation) {
            return;
        }
        runtime.p2p_state.set(P2pRouteState::Starting);
        runtime.swarm.starting();
    }
}

fn on_start_failed(runtime: &CallRuntime, error: String) {
    trace(runtime, "node-start-failed", error);
    runtime.p2p_state.set(P2pRouteState::KaspaFallback);
    runtime.swarm.offline();
    shutdown_detached(runtime);
}

async fn start_node(
    runtime: &CallRuntime,
    config: P2pStartConfig,
) -> Result<LocalP2pBinding, String> {
    let mut p2p = runtime.p2p();
    let start = p2p.start(config);
    let timeout = gloo_timers::future::TimeoutFuture::new(START_TIMEOUT_MS);
    futures_util::pin_mut!(start);
    match futures_util::future::select(start, timeout).await {
        futures_util::future::Either::Left((result, _)) => result,
        futures_util::future::Either::Right(_) => Err(format!(
            "p2p-net did not start within {START_TIMEOUT_MS} ms"
        )),
    }
}

async fn on_started(runtime: &CallRuntime, generation: u64, binding: LocalP2pBinding) {
    trace(
        runtime,
        "node-started",
        format!(
            "peer={} dialAddresses={}",
            binding.peer_id,
            binding.dial_addresses.len()
        ),
    );
    runtime.p2p_state.set(P2pRouteState::Connecting);
    if spawn_event_loop(runtime.clone(), generation).is_err() {
        runtime.p2p_state.set(P2pRouteState::KaspaFallback);
        runtime.swarm.offline();
        return;
    }
    if let Err(error) = shared_topics::start(runtime, generation).await {
        trace(runtime, "shared-topics-failed", error);
    }
    announce_active_sessions(runtime).await;
}

fn retire(runtime: &CallRuntime) {
    advance_generation(runtime);
    runtime.p2p_subscriptions.borrow_mut().clear();
    forget_sent();
    runtime.p2p_state.set(P2pRouteState::Unavailable);
    runtime.swarm.disable();
    crate::swarm_carrier::detach();
    shutdown_detached(runtime);
}

fn shutdown_detached(runtime: &CallRuntime) {
    let mut old = std::mem::take(&mut *runtime.p2p.borrow_mut());
    let runtime = runtime.clone();
    spawn_local(async move {
        if let Err(error) = old.shutdown().await {
            trace(&runtime, "node-shutdown-failed", error);
        }
    });
}
