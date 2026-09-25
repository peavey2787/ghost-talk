#![forbid(unsafe_code)]

mod binding;
mod transport;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use binding::{LocalP2pBinding, SessionPeerBinding, SessionPeerBindings, MAX_BOUND_SESSIONS};
pub use transport::{
    GhostP2pTransport, P2pFuture, P2pIncoming, P2pRouteState, P2pStartConfig,
    P2pSubscription,
};
#[cfg(target_arch = "wasm32")]
pub use wasm::{P2pNetEvent, P2pNetEventSubscription, P2pNetSubscription, P2pNetTransport};
