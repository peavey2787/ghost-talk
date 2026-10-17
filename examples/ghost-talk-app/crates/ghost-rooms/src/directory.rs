//! Bounded, expiring directory of verified public-room announcements heard on
//! the network. Callers verify each announcement's owner signature before
//! admitting it; the directory only enforces freshness, ordering and bounds.

use std::collections::BTreeMap;

use crate::RoomAnnouncement;

/// Announcements older than this are no longer listed (owners re-announce
/// well within it while their room stays public).
pub const ROOM_LISTING_TTL_MS: u64 = 10 * 60 * 1000;
const MAX_FUTURE_SKEW_MS: u64 = 2 * 60 * 1000;
const MAX_LISTINGS: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomDirectory {
    listings: BTreeMap<String, RoomAnnouncement>,
}

impl RoomDirectory {
    /// Admit a verified announcement; true when the listing changed.
    pub fn admit(&mut self, announcement: RoomAnnouncement, now_ms: u64) -> bool {
        if !is_fresh(&announcement, now_ms) || self.has_newer(&announcement) {
            return false;
        }
        self.expire(now_ms);
        if !self.listings.contains_key(&announcement.room_id) && self.listings.len() >= MAX_LISTINGS
        {
            self.evict_oldest();
        }
        self.listings
            .insert(announcement.room_id.clone(), announcement);
        true
    }

    /// Fresh listings, ordered by room name.
    pub fn listings(&self, now_ms: u64) -> Vec<RoomAnnouncement> {
        let mut fresh: Vec<_> = self
            .listings
            .values()
            .filter(|announcement| is_fresh(announcement, now_ms))
            .cloned()
            .collect();
        fresh.sort_by_cached_key(|announcement| announcement.name.to_lowercase());
        fresh
    }

    fn has_newer(&self, announcement: &RoomAnnouncement) -> bool {
        self.listings
            .get(&announcement.room_id)
            .is_some_and(|known| known.issued_at_ms >= announcement.issued_at_ms)
    }

    fn expire(&mut self, now_ms: u64) {
        self.listings
            .retain(|_, announcement| is_fresh(announcement, now_ms));
    }

    fn evict_oldest(&mut self) {
        let oldest = self
            .listings
            .values()
            .min_by_key(|announcement| announcement.issued_at_ms)
            .map(|announcement| announcement.room_id.clone());
        if let Some(room_id) = oldest {
            self.listings.remove(&room_id);
        }
    }
}

fn is_fresh(announcement: &RoomAnnouncement, now_ms: u64) -> bool {
    announcement.issued_at_ms <= now_ms.saturating_add(MAX_FUTURE_SKEW_MS)
        && now_ms.saturating_sub(announcement.issued_at_ms) <= ROOM_LISTING_TTL_MS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{announcement::tests::room, RoomMode};

    fn listing(id: u8, name: &str, issued_at_ms: u64) -> RoomAnnouncement {
        let mut announcement =
            RoomAnnouncement::for_room(&room(RoomMode::CommunityVoice), issued_at_ms).unwrap();
        announcement.room_id = format!("{id:02x}").repeat(16);
        announcement.name = name.into();
        announcement
    }

    #[test]
    fn newer_announcements_replace_older_ones_and_listings_sort_by_name() {
        let mut directory = RoomDirectory::default();
        let now = ROOM_LISTING_TTL_MS;
        assert!(directory.admit(listing(1, "zeta", now), now));
        assert!(directory.admit(listing(2, "Alpha", now), now));
        assert!(!directory.admit(listing(1, "stale", now - 1), now));
        assert!(directory.admit(listing(1, "beta", now + 1), now));
        let names: Vec<_> = directory
            .listings(now)
            .into_iter()
            .map(|announcement| announcement.name)
            .collect();
        assert_eq!(names, ["Alpha", "beta"]);
    }

    #[test]
    fn stale_or_future_announcements_are_not_listed() {
        let mut directory = RoomDirectory::default();
        let now = 2 * ROOM_LISTING_TTL_MS;
        assert!(!directory.admit(listing(1, "old", now - ROOM_LISTING_TTL_MS - 1), now));
        assert!(!directory.admit(listing(2, "future", now + MAX_FUTURE_SKEW_MS + 1), now));
        assert!(directory.admit(listing(3, "live", now), now));
        assert!(directory.listings(now + ROOM_LISTING_TTL_MS + 1).is_empty());
    }

    #[test]
    fn the_directory_stays_bounded() {
        let mut directory = RoomDirectory::default();
        let now = ROOM_LISTING_TTL_MS;
        for index in 0..=MAX_LISTINGS {
            let mut announcement = listing(0, "room", now + index as u64 / 1000);
            announcement.room_id = format!("{index:032x}");
            assert!(directory.admit(announcement, now));
        }
        assert_eq!(directory.listings(now).len(), MAX_LISTINGS);
    }
}
