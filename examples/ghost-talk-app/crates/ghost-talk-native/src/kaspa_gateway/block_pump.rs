use super::{broadcast, is_transport_failure, KaspaGatewayState, LiveBlockEvent, PortalFacade};

impl KaspaGatewayState {
    pub(super) fn spawn_block_pump(
        &self,
        generation: u64,
        network: String,
        endpoint: String,
        portal: PortalFacade,
        blocks: broadcast::Sender<LiveBlockEvent>,
    ) {
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            pump_blocks(state, generation, network, endpoint, portal, blocks).await;
        });
    }

    async fn deactivate_block_subscription(&self, generation: u64, endpoint: &str, error: &str) {
        let mut inner = self.inner.write().await;
        let Some(connection) = inner.connection.as_mut() else {
            return;
        };
        if connection.session.generation() == generation
            && connection.session.endpoint() == endpoint
        {
            connection.block_subscription_active = false;
            crate::debug_log::record(
                "warn",
                "kaspa",
                "gateway-block-subscription-inactive",
                format!(
                    "endpoint={} generation={} portal_connection_retained=true error={}",
                    endpoint, generation, error
                ),
            );
        }
    }

    pub(super) async fn invalidate_generation(&self, generation: u64, endpoint: &str, error: &str) {
        let mut inner = self.inner.write().await;
        let matches = inner.connection.as_ref().is_some_and(|connection| {
            connection.session.generation() == generation
                && connection.session.endpoint() == endpoint
        });
        if matches {
            inner.connection = None;
            crate::debug_log::record(
                "warn",
                "kaspa",
                "gateway-disconnected",
                format!(
                    "endpoint={} generation={} error={}",
                    endpoint, generation, error
                ),
            );
        }
    }
}

async fn pump_blocks(
    state: KaspaGatewayState,
    generation: u64,
    network: String,
    endpoint: String,
    portal: PortalFacade,
    blocks: broadcast::Sender<LiveBlockEvent>,
) {
    let mut observed_first_block = false;
    loop {
        match portal.next_block_added().await {
            Ok(event) => handle_block_event(
                &network,
                &endpoint,
                generation,
                &mut observed_first_block,
                &blocks,
                event,
            ),
            Err(error) => {
                handle_block_stream_failure(&state, &network, &endpoint, generation, &error).await;
                break;
            }
        }
    }
}

fn handle_block_event(
    network: &str,
    endpoint: &str,
    generation: u64,
    observed_first_block: &mut bool,
    blocks: &broadcast::Sender<LiveBlockEvent>,
    event: LiveBlockEvent,
) {
    if !*observed_first_block {
        *observed_first_block = true;
        crate::debug_log::record(
            "info",
            "kaspa",
            "gateway-first-block-added",
            format!("network={network} endpoint={endpoint} generation={generation} block={} daa_score={}", event.block_hash, event.daa_score),
        );
    }
    let _ = blocks.send(event);
}

async fn handle_block_stream_failure(
    state: &KaspaGatewayState,
    network: &str,
    endpoint: &str,
    generation: u64,
    error: &str,
) {
    crate::debug_log::record(
        "warn",
        "kaspa",
        "gateway-block-stream-failed",
        format!("network={network} endpoint={endpoint} generation={generation} error={error}"),
    );
    if is_transport_failure(error) {
        state
            .invalidate_generation(generation, endpoint, error)
            .await;
    } else {
        state
            .deactivate_block_subscription(generation, endpoint, error)
            .await;
    }
}
