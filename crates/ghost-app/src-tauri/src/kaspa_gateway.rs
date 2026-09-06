use ghost_kaspa::{wallet::WalletPublic, LiveBlockEvent, PortalFacade};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

const BLOCK_FANOUT_CAPACITY: usize = 256;

struct GatewayConnection {
    generation: u64,
    network: String,
    endpoint: String,
    portal: PortalFacade,
    blocks: broadcast::Sender<LiveBlockEvent>,
    pending_receiver: Option<broadcast::Receiver<LiveBlockEvent>>,
    block_subscription_active: bool,
}

#[derive(Default)]
struct GatewayInner {
    generation: u64,
    connection: Option<GatewayConnection>,
}

/// The one and only native gateway from Ghost Talk to a public Kaspa wRPC node.
///
/// Every wallet, mailbox, directory, backup, and hardware-signing RPC must get
/// its Portal through this state. One app process owns at most one active Portal
/// for the selected network/endpoint. Kaspa Portal 1.0.1 owns the persistent
/// wRPC socket, request/notification multiplexing, reconnect, and subscription
/// replay; Ghost Talk only fans Portal's decoded BlockAdded events out to its
/// application consumers. A replacement Portal is created only after Portal
/// reports an unrecoverable transport failure and endpoint failover is required.
#[derive(Clone, Default)]
pub struct KaspaGatewayState {
    inner: Arc<RwLock<GatewayInner>>,
    connect_lock: Arc<Mutex<()>>,
    subscription_lock: Arc<Mutex<()>>,
}

impl KaspaGatewayState {
    pub async fn portal(
        &self,
        public: &WalletPublic,
        wrpc_override: Option<&str>,
    ) -> Result<PortalFacade, String> {
        self.portal_for_network(&public.network, wrpc_override).await
    }

    pub async fn portal_for_network(
        &self,
        network: &str,
        wrpc_override: Option<&str>,
    ) -> Result<PortalFacade, String> {
        if let Some(portal) = self.compatible_existing_network(network, wrpc_override).await? {
            return Ok(portal);
        }

        let _guard = self.connect_lock.lock().await;
        if let Some(portal) = self.compatible_existing_network(network, wrpc_override).await? {
            return Ok(portal);
        }

        let endpoints = super::wallet_commands::resolve_wrpc_endpoints(network, wrpc_override).await?;
        let source = if normalized_override(wrpc_override).is_some() {
            "explicit-endpoint"
        } else {
            "public-resolver"
        };
        let mut last_error = "no Kaspa wRPC endpoint accepted the Portal connection".to_string();

        for endpoint in endpoints {
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

            // This is the published Kaspa Portal 1.0.1 connection contract:
            // KaspaPortal::builder().network(...).endpoint(...).connect().await.
            // Endpoint discovery may choose the concrete node, but Portal alone
            // owns the persistent wRPC transport, reconnect/replay behavior,
            // health RPC, chain calls, and BlockAdded wire protocol.
            let portal = match PortalFacade::connect(network, &endpoint).await {
                Ok(portal) => portal,
                Err(error) => {
                    eprintln!(
                        "Ghost Talk: Kaspa Portal connection failed: network={} endpoint={} error={}",
                        network, endpoint, error
                    );
                    crate::debug_log::record(
                        "warn",
                        "kaspa",
                        "gateway-connect-failed",
                        format!(
                            "network={} endpoint={} error={}",
                            network, endpoint, error
                        ),
                    );
                    last_error = error;
                    continue;
                }
            };
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

            // A successful Portal connect includes the live DAA health RPC and is
            // the Kaspa connection boundary. BlockAdded uses Portal's first-class
            // subscription API on that same connection; subscription state remains
            // separate from basic node health so a consumer error cannot masquerade
            // as a disconnected node.
            let (blocks, pending_receiver) = broadcast::channel(BLOCK_FANOUT_CAPACITY);
            let generation = {
                let mut inner = self.inner.write().await;
                inner.generation = inner.generation.wrapping_add(1).max(1);
                let generation = inner.generation;
                inner.connection = Some(GatewayConnection {
                    generation,
                    network: network.to_owned(),
                    endpoint: connected_endpoint.clone(),
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
            return Ok(portal);
        }

        Err(last_error)
    }

    pub async fn subscribe_blocks(
        &self,
        public: &WalletPublic,
        wrpc_override: Option<&str>,
    ) -> Result<broadcast::Receiver<LiveBlockEvent>, String> {
        let portal = self.portal(public, wrpc_override).await?;
        let _guard = self.subscription_lock.lock().await;

        let existing = {
            let inner = self.inner.read().await;
            let connection = inner.connection.as_ref().ok_or_else(|| {
                "Kaspa gateway connection disappeared before BlockAdded subscription".to_string()
            })?;
            if connection.block_subscription_active {
                Some(connection.blocks.subscribe())
            } else {
                None
            }
        };
        if let Some(receiver) = existing {
            return Ok(receiver);
        }

        let (generation, network, endpoint, blocks) = {
            let inner = self.inner.read().await;
            let connection = inner.connection.as_ref().ok_or_else(|| {
                "Kaspa gateway connection disappeared before BlockAdded subscription".to_string()
            })?;
            (
                connection.generation,
                connection.network.clone(),
                connection.endpoint.clone(),
                connection.blocks.clone(),
            )
        };

        crate::debug_log::record(
            "info",
            "kaspa",
            "gateway-block-subscribe-start",
            format!("network={} endpoint={} generation={}", network, endpoint, generation),
        );
        if let Err(error) = portal.subscribe_block_added().await {
            eprintln!(
                "Ghost Talk: Kaspa BlockAdded subscription failed (connection remains healthy unless this is a transport error): network={} endpoint={} error={}",
                network, endpoint, error
            );
            crate::debug_log::record(
                "warn",
                "kaspa",
                "gateway-block-subscribe-failed",
                format!(
                    "network={} endpoint={} generation={} error={}",
                    network, endpoint, generation, error
                ),
            );
            if is_transport_failure(&error) {
                self.invalidate_generation(generation, &endpoint, &error).await;
            }
            return Err(error);
        }

        let receiver = {
            let mut inner = self.inner.write().await;
            let connection = inner.connection.as_mut().ok_or_else(|| {
                "Kaspa gateway connection disappeared after BlockAdded subscription".to_string()
            })?;
            if connection.generation != generation {
                return Err("Kaspa gateway changed during BlockAdded subscription".into());
            }
            connection.block_subscription_active = true;
            connection
                .pending_receiver
                .take()
                .unwrap_or_else(|| connection.blocks.subscribe())
        };

        crate::debug_log::record(
            "info",
            "kaspa",
            "gateway-block-subscribe-ok",
            format!("network={} endpoint={} generation={}", network, endpoint, generation),
        );
        self.spawn_block_pump(
            generation,
            network,
            endpoint,
            portal.clone(),
            blocks,
        );
        Ok(receiver)
    }

    /// Whether a Portal health check has established a usable Kaspa wRPC
    /// connection for this network. This intentionally does not depend on the
    /// optional BlockAdded mailbox subscription.
    pub async fn is_connected_to(&self, network: &str) -> bool {
        let inner = self.inner.read().await;
        inner
            .connection
            .as_ref()
            .is_some_and(|connection| connection.network == network)
    }

    pub async fn note_operation_error(&self, error: &str) {
        if !is_transport_failure(error) {
            return;
        }
        let current = {
            let inner = self.inner.read().await;
            inner.connection.as_ref().map(|connection| {
                (
                    connection.generation,
                    connection.endpoint.clone(),
                    connection.network.clone(),
                )
            })
        };
        if let Some((generation, endpoint, network)) = current {
            eprintln!(
                "Ghost Talk: Kaspa transport failed: network={} endpoint={} error={}",
                network, endpoint, error
            );
            crate::debug_log::record(
                "warn",
                "kaspa",
                "gateway-transport-error-observed",
                format!(
                    "network={} endpoint={} generation={} error={}",
                    network, endpoint, generation, error
                ),
            );
            self.invalidate_generation(generation, &endpoint, error).await;
        }
    }

    async fn compatible_existing_network(
        &self,
        network: &str,
        wrpc_override: Option<&str>,
    ) -> Result<Option<PortalFacade>, String> {
        let inner = self.inner.read().await;
        let Some(connection) = inner.connection.as_ref() else {
            return Ok(None);
        };
        if connection.network != network {
            return Err(format!(
                "Kaspa gateway is already connected to network {} at {}; refusing to open a second public-node connection for {}",
                connection.network, connection.endpoint, network
            ));
        }
        if let Some(requested) = normalized_override(wrpc_override) {
            if requested != connection.endpoint {
                return Err(format!(
                    "Kaspa gateway is already connected to {}; refusing to open a second public-node connection for override {}",
                    connection.endpoint, requested
                ));
            }
        }
        Ok(Some(connection.portal.clone()))
    }

    fn spawn_block_pump(
        &self,
        generation: u64,
        network: String,
        endpoint: String,
        portal: PortalFacade,
        blocks: broadcast::Sender<LiveBlockEvent>,
    ) {
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut observed_first_block = false;
            loop {
                match portal.next_block_added().await {
                    Ok(event) => {
                        if !observed_first_block {
                            observed_first_block = true;
                            crate::debug_log::record(
                                "info",
                                "kaspa",
                                "gateway-first-block-added",
                                format!(
                                    "network={} endpoint={} generation={} block={} daa_score={}",
                                    network, endpoint, generation, event.block_hash, event.daa_score
                                ),
                            );
                        }
                        // No receiver simply means no active mailbox monitor yet.
                        // The gateway still remains the sole BlockAdded subscriber.
                        let _ = blocks.send(event);
                    }
                    Err(error) => {
                        crate::debug_log::record(
                            "warn",
                            "kaspa",
                            "gateway-block-stream-failed",
                            format!(
                                "network={} endpoint={} generation={} error={}",
                                network, endpoint, generation, error
                            ),
                        );
                        if is_transport_failure(&error) {
                            state
                                .invalidate_generation(generation, &endpoint, &error)
                                .await;
                        } else {
                            state
                                .deactivate_block_subscription(generation, &endpoint, &error)
                                .await;
                        }
                        break;
                    }
                }
            }
        });
    }

    async fn deactivate_block_subscription(
        &self,
        generation: u64,
        endpoint: &str,
        error: &str,
    ) {
        let mut inner = self.inner.write().await;
        let Some(connection) = inner.connection.as_mut() else {
            return;
        };
        if connection.generation == generation && connection.endpoint == endpoint {
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

    async fn invalidate_generation(&self, generation: u64, endpoint: &str, error: &str) {
        let mut inner = self.inner.write().await;
        let matches = inner.connection.as_ref().is_some_and(|connection| {
            connection.generation == generation && connection.endpoint == endpoint
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

fn normalized_override(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn is_transport_failure(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    // Portal 1.0.1 performs its own reconnect/replay before returning these
    // failures. Classify the public error strings only after Portal gives up so
    // the application can try its next resolver endpoint.
    error.contains("websocket connection failed")
        || error.contains("websocket connect timeout")
        || error.contains("websocket rpc response timeout")
        || error.contains("websocket send failed")
        || error.contains("kaspa wrpc driver stopped")
        || error.contains("connection reset without closing handshake")
        || error.contains("closed before kaspa wrpc frame")
        || error.contains("kaspa wrpc connection ended")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_transport_failures_without_evicting_application_errors() {
        assert!(is_transport_failure(
            "WebSocket connection failed: Connection reset without closing handshake"
        ));
        assert!(is_transport_failure("WebSocket connect timeout (15s)"));
        assert!(is_transport_failure("WebSocket RPC response timeout (15s)"));
        assert!(is_transport_failure("WebSocket send failed"));
        assert!(!is_transport_failure("insufficient funds"));
        assert!(!is_transport_failure("recipient address is invalid"));
    }
}
