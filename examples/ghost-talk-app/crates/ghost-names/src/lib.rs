#![forbid(unsafe_code)]

mod dotk;
mod dotk_chain;
mod dotk_deed;
mod kns;
mod resolver;

pub use dotk::{DotkResolver, VerifiedDotkName, DOTK_REGISTRY};
pub use kns::KnsResolver;
pub use resolver::GhostNameResolver;
