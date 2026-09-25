use super::{normalized_override, KaspaGatewayState, PortalFacade};

impl KaspaGatewayState {
    pub(super) async fn compatible_existing_network(
        &self,
        network: &str,
        wrpc_override: Option<&str>,
    ) -> Result<Option<PortalFacade>, String> {
        let inner = self.inner.read().await;
        let Some(connection) = inner.connection.as_ref() else {
            return Ok(None);
        };
        if connection.session.network() != network {
            return Err(format!(
                "Kaspa gateway is already connected to network {} at {}; refusing to open a second public-node connection for {}",
                connection.session.network(), connection.session.endpoint(), network
            ));
        }
        if let Some(requested) = normalized_override(wrpc_override) {
            if requested != connection.session.endpoint() {
                return Err(format!(
                    "Kaspa gateway is already connected to {}; refusing to open a second public-node connection for override {}",
                    connection.session.endpoint(), requested
                ));
            }
        }
        Ok(Some(connection.portal.clone()))
    }
}
