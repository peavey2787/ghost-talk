//! Ghost Talk's browser adapter for the shared realtime transport contract.
//!
//! The contract (`GhostP2pTransport`, session bindings, route state) is owned
//! by `ghost-realtime`; this crate implements it in the browser by driving
//! p2p-net's `WasmNode` in process.

#![forbid(unsafe_code)]

mod events;
#[cfg(any(target_arch = "wasm32", test))]
mod incoming;
#[cfg(target_arch = "wasm32")]
mod node;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use events::P2pNetEvent;
pub use ghost_realtime::{
    GhostP2pTransport, LocalP2pBinding, P2pInfrastructure, P2pRouteState, P2pStartConfig,
    P2pSubscription, SessionPeerBinding,
};
#[cfg(target_arch = "wasm32")]
pub use wasm::{P2pNetEventSubscription, P2pNetSubscription, P2pNetTransport};
