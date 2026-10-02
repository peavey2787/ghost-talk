//! Public Kaspa node discovery through the community wRPC resolvers, from
//! Kaspa Portal. Only TLS (`wss://`) endpoints are accepted from a resolver.

use crate::upstream::portal::{network::resolver, primitives::NetworkId};

pub use resolver::PUBLIC_RESOLVERS as PUBLIC_WRPC_RESOLVERS;

/// Resolver query for a TLS Borsh wRPC node on `network`.
pub fn resolver_query_url(resolver_url: &str, network: &str) -> Result<String, String> {
    Ok(resolver::query_url(
        resolver_url,
        NetworkId::parse(network)?,
    ))
}

/// The `wss://` node endpoint a resolver answered with.
pub fn resolver_endpoint(body: &str) -> Result<String, String> {
    resolver::parse_endpoint(body).map_err(|error| error.to_string())
}
