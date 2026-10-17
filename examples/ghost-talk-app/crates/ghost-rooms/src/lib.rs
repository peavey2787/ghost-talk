#![forbid(unsafe_code)]

mod announcement;
mod channel;
mod directory;
mod model;
#[cfg(test)]
mod policy_tests;
mod service;
mod store;
mod tombstones;

pub use announcement::{
    RoomAnnouncement, GTRA_MAGIC, ROOM_ANNOUNCEMENT_SIGNATURE_DOMAIN, ROOM_ANNOUNCEMENT_VERSION,
};
pub use channel::{ChannelPolicy, Role, RoomMode, RoomPolicies};
pub use directory::{RoomDirectory, ROOM_LISTING_TTL_MS};
pub use model::{Room, RoomAccess, RoomBan, RoomMember, RoomMessage, RoomWire};
pub use service::{NewRoom, RoomService, RoomState};
pub use store::RoomStore;
pub use tombstones::{RoomTombstone, RoomTombstoneService};
