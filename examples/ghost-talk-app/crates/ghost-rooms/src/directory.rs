//! Bounded, expiring directory of verified public-room announcements heard on
//! the network (swarm or Kaspa). Callers verify each announcement's owner
//! signature before admitting it; the directory only enforces the signed
//! expiry, ordering and bounds.

use std::collections::BTreeMap;

use crate::RoomAnnouncement;

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

    /// A known listing stays when it is at least as new, or when it lists
    /// the same room for longer (a short swarm re-announcement must not cut
    /// a paid Kaspa listing short).
    fn has_newer(&self, announcement: &RoomAnnouncement) -> bool {
        self.listings
            .get(&announcement.room_id)
            .is_some_and(|known| {
                known.issued_at_ms >= announcement.issued_at_ms
                    || (known.expires_at_ms > announcement.expires_at_ms
                        && same_listing(known, announcement))
            })
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

fn same_listing(left: &RoomAnnouncement, right: &RoomAnnouncement) -> bool {
    left.name == right.name
        && left.owner_label == right.owner_label
        && left.owner_kaspa_address == right.owner_kaspa_address
}

fn is_fresh(announcement: &RoomAnnouncement, now_ms: u64) -> bool {
    announcement.issued_at_ms <= now_ms.saturating_add(MAX_FUTURE_SKEW_MS)
        && now_ms < announcement.expires_at_ms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{announcement::tests::room, RoomAccess, SWARM_LISTING_LIFETIME_MS};

    const TTL: u64 = SWARM_LISTING_LIFETIME_MS;

    fn listing(id: u8, name: &str, issued_at_ms: u64) -> RoomAnnouncement {
        let mut announcement =
            RoomAnnouncement::for_room(&room(RoomAccess::Public), issued_at_ms, TTL).unwrap();
        announcement.room_id = format!("{id:02x}").repeat(16);
        announcement.name = name.into();
        announcement
    }

    #[test]
    fn newer_announcements_replace_older_ones_and_listings_sort_by_name() {
        let mut directory = RoomDirectory::default();
        let now = TTL;
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
        let now = 2 * TTL;
        assert!(!directory.admit(listing(1, "old", now - TTL - 1), now));
        assert!(!directory.admit(listing(2, "future", now + MAX_FUTURE_SKEW_MS + 1), now));
        assert!(directory.admit(listing(3, "live", now), now));
        assert!(directory.listings(now + TTL + 1).is_empty());
    }

    #[test]
    fn kaspa_listings_outlive_swarm_ones_and_are_not_cut_short() {
        let mut directory = RoomDirectory::default();
        let kaspa_ttl = crate::KASPA_LISTING_LIFETIME_MS;
        let kaspa = {
            let mut announcement = listing(1, "lobby", 0);
            announcement.expires_at_ms = kaspa_ttl;
            announcement
        };
        assert!(directory.admit(kaspa, 0));
        assert!(!directory.admit(listing(1, "lobby", 60_000), 60_000));
        assert_eq!(directory.listings(TTL * 10).len(), 1);
        assert!(directory.admit(listing(1, "renamed", 120_000), 120_000));
        assert!(directory.listings(TTL * 10).is_empty());
    }

    #[test]
    fn the_directory_stays_bounded() {
        let mut directory = RoomDirectory::default();
        let now = TTL;
        for index in 0..=MAX_LISTINGS {
            let mut announcement = listing(0, "room", now + index as u64 / 1000);
            announcement.room_id = format!("{index:032x}");
            assert!(directory.admit(announcement, now));
        }
        assert_eq!(directory.listings(now).len(), MAX_LISTINGS);
    }
}
