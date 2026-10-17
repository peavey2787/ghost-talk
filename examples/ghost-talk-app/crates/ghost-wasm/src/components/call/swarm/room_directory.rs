//! Public rooms heard on the network: on the swarm room-directory topic and
//! in Kaspa blocks. Every announcement's owner signature is checked here;
//! `ghost_rooms::RoomDirectory` keeps only fresh ones, and Discover reads
//! them through `RoomListings` context. The app owns one tracker, shared by
//! the Kaspa live-event listener and the swarm topic consumer.

use ghost_rooms::{RoomAnnouncement, RoomDirectory};
use std::{cell::RefCell, rc::Rc};
use yew::prelude::*;

/// Context handle; `version` changes whenever a listing is admitted.
#[derive(Clone)]
pub struct RoomListings {
    directory: Rc<RefCell<RoomDirectory>>,
    version: u64,
}

impl PartialEq for RoomListings {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.directory, &other.directory) && self.version == other.version
    }
}

impl RoomListings {
    /// Listings that are still fresh now, sorted by name.
    pub fn fresh(&self) -> Vec<RoomAnnouncement> {
        self.directory.borrow().listings(now_ms())
    }
}

#[derive(Clone)]
pub(crate) struct RoomDirectoryTracker {
    directory: Rc<RefCell<RoomDirectory>>,
    // Long-lived topic consumers hold this tracker, so the counter cannot be
    // read back from the (render-captured) state handle.
    counter: Rc<RefCell<u64>>,
    version: UseStateHandle<u64>,
}

impl RoomDirectoryTracker {
    pub(crate) fn listings(&self) -> RoomListings {
        RoomListings {
            directory: self.directory.clone(),
            version: *self.version,
        }
    }

    /// Admit a GTRA packet whose owner signature verifies.
    pub(crate) fn admit_packet(&self, packet: &[u8]) {
        let Ok(announcement) = RoomAnnouncement::decode(packet) else {
            return;
        };
        if ghost_kaspa::verify_room_announcement(&announcement).is_err() {
            return;
        }
        if self.directory.borrow_mut().admit(announcement, now_ms()) {
            let mut counter = self.counter.borrow_mut();
            *counter = counter.wrapping_add(1);
            self.version.set(*counter);
        }
    }
}

impl PartialEq for RoomDirectoryTracker {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.directory, &other.directory) && *self.version == *other.version
    }
}

#[hook]
pub(crate) fn use_room_directory_tracker() -> RoomDirectoryTracker {
    RoomDirectoryTracker {
        directory: use_mut_ref(RoomDirectory::default),
        counter: use_mut_ref(|| 0u64),
        version: use_state(|| 0u64),
    }
}

fn now_ms() -> u64 {
    crate::now_ms().max(0.0) as u64
}
