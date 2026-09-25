#[cfg(feature = "upstream")]
mod groups;
#[cfg(feature = "upstream")]
mod hydra_facade;
#[cfg(all(feature = "upstream", target_arch = "wasm32"))]
mod browser_persistence;
#[cfg(feature = "upstream")]
mod identity_methods;
mod runtime;

#[cfg(feature = "upstream")]
pub use hydra_facade::{GroupGuard, GroupMemberBootstrap};
#[cfg(feature = "upstream")]
pub use runtime::HydraFacade;
pub use runtime::{ContactProjection, IdentityProjection, ReceivedProjection, StegoProfile};
