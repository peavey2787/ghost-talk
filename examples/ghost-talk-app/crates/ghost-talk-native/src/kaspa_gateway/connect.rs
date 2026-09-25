use super::{normalized_override, GatewayConnection, KaspaGatewayState, BLOCK_FANOUT_CAPACITY};
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

async fn connect_first_available(
    state: &KaspaGatewayState,
    network: &str,
    wrpc_override: Option<&str>,
) -> Result<PortalFacade, String> {
    let endpoints = crate::wallet_commands::resolve_wrpc_endpoints(network, wrpc_override).await?;
    let source = endpoint_source(wrpc_override);
    let mut last_error = "no Kaspa wRPC endpoint accepted the Portal connection".to_string();
    for endpoint in endpoints {
        match connect_endpoint(state, network, source, &endpoint).await {
            Ok(portal) => return Ok(portal),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn endpoint_source(wrpc_override: Option<&str>) -> &'static str {
    if normalized_override(wrpc_override).is_some() {
        "explicit-endpoint"
    } else {
        "public-resolver"
    }
}

async fn connect_endpoint(
    state: &KaspaGatewayState,
    network: &str,
    source: &str,
    endpoint: &str,
) -> Result<PortalFacade, String> {
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
    let portal = PortalFacade::connect(network, endpoint)
        .await
        .map_err(|error| {
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
        })?;
    install_connection(state, network, portal).await
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
