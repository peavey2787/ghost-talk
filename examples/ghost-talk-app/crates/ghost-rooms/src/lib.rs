#![forbid(unsafe_code)]

mod announcement;
mod directory;
mod model;
mod role;
mod service;
mod store;
mod tombstones;

pub use announcement::{
    RoomAnnouncement, GTRA_MAGIC, KASPA_LISTING_LIFETIME_MS, ROOM_ANNOUNCEMENT_SIGNATURE_DOMAIN,
    ROOM_ANNOUNCEMENT_VERSION, SWARM_LISTING_LIFETIME_MS,
};
pub use directory::RoomDirectory;
pub use model::{Room, RoomAccess, RoomBan, RoomMember, RoomMessage, RoomWire};
pub use role::Role;
pub use service::{NewRoom, RoomService, RoomState};
pub use store::RoomStore;
pub use tombstones::{RoomTombstone, RoomTombstoneService};
