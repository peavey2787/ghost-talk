mod connect;
use ghost_kaspa::{wallet::WalletPublic, LiveBlockEvent, NodeSession, PortalFacade};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

const BLOCK_FANOUT_CAPACITY: usize = 256;

struct GatewayConnection {
    session: NodeSession,
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

#[derive(Clone)]
struct BlockSubscriptionContext {
    session: NodeSession,
    blocks: broadcast::Sender<LiveBlockEvent>,
}

impl KaspaGatewayState {
    pub async fn portal(
        &self,
        public: &WalletPublic,
        wrpc_override: Option<&str>,
    ) -> Result<PortalFacade, String> {
        self.portal_for_network(&public.network, wrpc_override)
            .await
    }

    pub async fn portal_for_network(
        &self,
        network: &str,
        wrpc_override: Option<&str>,
    ) -> Result<PortalFacade, String> {
        connect::portal_for_network(self, network, wrpc_override).await
    }

    async fn existing_block_receiver(
        &self,
    ) -> Result<Option<broadcast::Receiver<LiveBlockEvent>>, String> {
        let inner = self.inner.read().await;
        let connection = inner.connection.as_ref().ok_or_else(|| {
            "Kaspa gateway connection disappeared before BlockAdded subscription".to_string()
        })?;
        Ok(connection
            .block_subscription_active
            .then(|| connection.blocks.subscribe()))
    }

    async fn block_subscription_context(&self) -> Result<BlockSubscriptionContext, String> {
        let inner = self.inner.read().await;
        let connection = inner.connection.as_ref().ok_or_else(|| {
            "Kaspa gateway connection disappeared before BlockAdded subscription".to_string()
        })?;
        Ok(BlockSubscriptionContext {
            session: connection.session.clone(),
            blocks: connection.blocks.clone(),
        })
    }

    async fn activate_block_subscription(
        &self,
        generation: u64,
    ) -> Result<broadcast::Receiver<LiveBlockEvent>, String> {
        let mut inner = self.inner.write().await;
        let connection = inner.connection.as_mut().ok_or_else(|| {
            "Kaspa gateway connection disappeared after BlockAdded subscription".to_string()
        })?;
        if connection.session.generation() != generation {
            return Err("Kaspa gateway changed during BlockAdded subscription".into());
        }
        connection.block_subscription_active = true;
        Ok(connection
            .pending_receiver
            .take()
            .unwrap_or_else(|| connection.blocks.subscribe()))
    }

    async fn subscribe_portal_blocks(
        &self,
        portal: &PortalFacade,
        context: &BlockSubscriptionContext,
    ) -> Result<(), String> {
        if let Err(error) = portal.subscribe_block_added().await {
            crate::debug_log::record(
                "warn",
                "kaspa",
                "gateway-block-subscribe-failed",
                format!(
                    "network={} endpoint={} generation={} error={}",
                    context.session.network(),
                    context.session.endpoint(),
                    context.session.generation(),
                    error
                ),
            );
            if is_transport_failure(&error) {
                self.invalidate_generation(
                    context.session.generation(),
                    context.session.endpoint(),
                    &error,
                )
                .await;
            }
            return Err(error);
        }
        Ok(())
    }

    pub async fn subscribe_blocks(
        &self,
        public: &WalletPublic,
        wrpc_override: Option<&str>,
    ) -> Result<broadcast::Receiver<LiveBlockEvent>, String> {
        let portal = self.portal(public, wrpc_override).await?;
        let _guard = self.subscription_lock.lock().await;
        if let Some(receiver) = self.existing_block_receiver().await? {
            return Ok(receiver);
        }
        let context = self.block_subscription_context().await?;
        crate::debug_log::record(
            "info",
            "kaspa",
            "gateway-block-subscribe-start",
            format!(
                "network={} endpoint={} generation={}",
                context.session.network(),
                context.session.endpoint(),
                context.session.generation()
            ),
        );
        self.subscribe_portal_blocks(&portal, &context).await?;
        let receiver = self
            .activate_block_subscription(context.session.generation())
            .await?;
        crate::debug_log::record(
            "info",
            "kaspa",
            "gateway-block-subscribe-ok",
            format!(
                "network={} endpoint={} generation={}",
                context.session.network(),
                context.session.endpoint(),
                context.session.generation()
            ),
        );
        self.spawn_block_pump(
            context.session.generation(),
            context.session.network().to_owned(),
            context.session.endpoint().to_owned(),
            portal.clone(),
            context.blocks,
        );
        Ok(receiver)
    }

    pub async fn note_operation_error(&self, error: &str) {
        if !is_transport_failure(error) {
            return;
        }
        let current = {
            let inner = self.inner.read().await;
            inner.connection.as_ref().map(|connection| {
                (
                    connection.session.generation(),
                    connection.session.endpoint().to_owned(),
                    connection.session.network().to_owned(),
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
            self.invalidate_generation(generation, &endpoint, error)
                .await;
        }
    }
}

mod block_pump;
mod compatibility;
mod policy;
use policy::{is_transport_failure, normalized_override};

#[cfg(test)]
#[cfg(test)]
mod tests;
