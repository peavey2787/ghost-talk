use super::{GatewayConnection, KaspaGatewayState, BLOCK_FANOUT_CAPACITY};
use ghost_kaspa::{NodeSession, PortalFacade};
use tokio::sync::broadcast;

pub(super) async fn portal_for_network(
    state: &KaspaGatewayState,
    network: &str,
    wrpc_override: Option<&str>,
) -> Result<PortalFacade, String> {
    if let Some(portal) = state
        .compatible_existing_network(network, wrpc_override)
        .await?
    {
        return Ok(portal);
    }
    let _guard = state.connect_lock.lock().await;
    if let Some(portal) = state
        .compatible_existing_network(network, wrpc_override)
        .await?
    {
        return Ok(portal);
    }

    connect_first_available(state, network, wrpc_override).await
}

/// An explicit endpoint when the user set one; otherwise Kaspa Portal picks
/// the first healthy public node its resolvers offer.
async fn connect_first_available(
    state: &KaspaGatewayState,
    network: &str,
    wrpc_override: Option<&str>,
) -> Result<PortalFacade, String> {
    let explicit = crate::wallet_commands::resolve_wrpc_override(wrpc_override)?;
    let (source, endpoint) = match explicit.as_deref() {
        Some(endpoint) => ("explicit-endpoint", endpoint),
        None => ("public-resolver", "resolver-selected"),
    };
    log_connect_attempt(network, source, endpoint);
    let portal = match explicit.as_deref() {
        Some(endpoint) => PortalFacade::connect(network, endpoint).await,
        None => PortalFacade::connect_public(network).await,
    }
    .map_err(|error| log_connect_failure(network, endpoint, error))?;
    install_connection(state, network, portal).await
}

fn log_connect_attempt(network: &str, source: &str, endpoint: &str) {
    crate::debug_log::record(
        "info",
        "kaspa",
        "gateway-connect-attempt",
        format!(
            "network={} source={} endpoint={}",
            network, source, endpoint
        ),
    );
    eprintln!(
        "Ghost Talk: connecting through Kaspa Portal: network={} endpoint={}",
        network, endpoint
    );
}

fn log_connect_failure(network: &str, endpoint: &str, error: String) -> String {
    eprintln!(
        "Ghost Talk: Kaspa Portal connection failed: network={} endpoint={} error={}",
        network, endpoint, error
    );
    crate::debug_log::record(
        "warn",
        "kaspa",
        "gateway-connect-failed",
        format!("network={} endpoint={} error={}", network, endpoint, error),
    );
    error
}

async fn install_connection(
    state: &KaspaGatewayState,
    network: &str,
    portal: PortalFacade,
) -> Result<PortalFacade, String> {
    let connected_endpoint = portal.endpoint()?;
    crate::debug_log::record(
        "info",
        "kaspa",
        "gateway-health-ok",
        format!(
            "network={} endpoint={} portal_builder_endpoint=true",
            network, connected_endpoint
        ),
    );
    let (blocks, pending_receiver) = broadcast::channel(BLOCK_FANOUT_CAPACITY);
    let generation = {
        let mut inner = state.inner.write().await;
        inner.generation = inner.generation.wrapping_add(1).max(1);
        let generation = inner.generation;
        inner.connection = Some(GatewayConnection {
            session: NodeSession::new(generation, network, connected_endpoint.clone()),
            portal: portal.clone(),
            blocks,
            pending_receiver: Some(pending_receiver),
            block_subscription_active: false,
        });
        generation
    };
    eprintln!(
        "Ghost Talk: Kaspa connected through Kaspa Portal: network={} endpoint={} generation={}",
        network, connected_endpoint, generation
    );
    crate::debug_log::record(
        "info",
        "kaspa",
        "gateway-connected",
        format!(
            "network={} endpoint={} generation={} portal_health=true block_subscription=false",
            network, connected_endpoint, generation
        ),
    );
    Ok(portal)
}
