#![forbid(unsafe_code)]

mod model;
mod policy;
mod service;
mod store;

pub use model::{Contact, ContactProfileUpdate, Relationship};
pub use policy::{should_deliver, AnonymousPolicy};
pub use service::ContactService;
pub use store::ContactStore;
