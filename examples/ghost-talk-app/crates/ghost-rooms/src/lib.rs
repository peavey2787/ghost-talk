#![forbid(unsafe_code)]

mod channel;
mod model;
#[cfg(test)]
mod policy_tests;
mod service;
mod store;
mod tombstones;

pub use channel::{ChannelPolicy, Role, RoomMode, RoomPolicies};
pub use model::{Room, RoomAccess, RoomBan, RoomMember, RoomMessage, RoomWire};
pub use service::{NewRoom, RoomService, RoomState};
pub use store::RoomStore;
pub use tombstones::{RoomTombstone, RoomTombstoneService};
