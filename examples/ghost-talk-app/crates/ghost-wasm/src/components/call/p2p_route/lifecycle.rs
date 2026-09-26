use ghost_p2p::{
    GhostP2pTransport, LocalP2pBinding, P2pInfrastructure, P2pRouteState, P2pStartConfig,
};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::{
    announce::announce_active_sessions,
    events::{current, spawn_event_loop},
    trace,
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
            automatic: profile.settings.route.eq_ignore_ascii_case("auto"),
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

/// Run one p2p-net node for the profile while its realtime route is Auto.
#[hook]
pub(in crate::components::call) fn use_p2p_effect(profile: &Profile, runtime: CallRuntime) {
    use_effect_with(RouteKey::new(profile), move |key| {
        let generation = advance_generation(&runtime);
        match key.start_config() {
            Some(config) => {
                runtime.p2p_state.set(P2pRouteState::Starting);
                spawn_local(start(runtime.clone(), generation, config));
            }
            None => runtime.p2p_state.set(P2pRouteState::Unavailable),
        }
        move || retire(&runtime)
    });
}

fn advance_generation(runtime: &CallRuntime) -> u64 {
    let mut value = runtime.p2p_generation.borrow_mut();
    *value = value.wrapping_add(1);
    *value
}

async fn start(runtime: CallRuntime, generation: u64, config: P2pStartConfig) {
    let started = start_node(&runtime, config).await;
    if !current(&runtime, generation) {
        // The effect retired while this node was starting; the node is stale.
        shutdown_detached(&runtime);
        return;
    }
    match started {
        Ok(binding) => on_started(&runtime, generation, binding).await,
        Err(error) => {
            trace(&runtime, "node-start-failed", error);
            runtime.p2p_state.set(P2pRouteState::KaspaFallback);
        }
    }
}

async fn start_node(
    runtime: &CallRuntime,
    config: P2pStartConfig,
) -> Result<LocalP2pBinding, String> {
    runtime.p2p().start(config).await
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
        return;
    }
    announce_active_sessions(runtime).await;
}

fn retire(runtime: &CallRuntime) {
    advance_generation(runtime);
    runtime.p2p_subscriptions.borrow_mut().clear();
    runtime.p2p_state.set(P2pRouteState::Unavailable);
    shutdown_detached(runtime);
}

fn shutdown_detached(runtime: &CallRuntime) {
    let mut old = std::mem::take(&mut *runtime.p2p.borrow_mut());
    spawn_local(async move {
        let _ = old.shutdown().await;
    });
}
