#![forbid(unsafe_code)]

//! Lightweight current-state index for Ghost-native records observed on Kaspa L1.
//!
//! This crate owns discovery acceleration only. Kaspa L1 remains authoritative,
//! and Kasia records deliberately never enter this index.

use ghost_protocol::{GhostContactDescriptor, GTCD_VERSION};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct IndexedDescriptor {
    descriptor: GhostContactDescriptor,
    blue_score: u64,
}

/// Thread-safe projection of the newest verified Ghost descriptor per Kaspa address.
#[derive(Clone, Default)]
pub struct GhostProfileIndex {
    entries: Arc<Mutex<HashMap<String, IndexedDescriptor>>>,
}

impl GhostProfileIndex {
    /// Observe one Kaspa payload from the live chain stream.
    ///
    /// Invalid, expired, non-Ghost, or older descriptors are ignored. The
    /// descriptor is retained only after its Kaspa destination and signature
    /// have been verified against the authoritative payload.
    pub fn ingest_payload(&self, payload: &[u8], blue_score: u64, current_daa: u64) {
        let Some(descriptor) = verified_descriptor(payload, current_daa) else {
            return;
        };
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries
            .get(&descriptor.kaspa_address)
            .is_some_and(|existing| existing.blue_score > blue_score)
        {
            return;
        }
        entries.insert(
            descriptor.kaspa_address.clone(),
            IndexedDescriptor {
                descriptor,
                blue_score,
            },
        );
    }

    /// Return the newest verified descriptor currently projected for an address.
    pub fn latest(&self, address: &str) -> Option<(GhostContactDescriptor, u64)> {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(address).cloned())
            .map(|entry| (entry.descriptor, entry.blue_score))
    }

    /// Snapshot all currently verified descriptors for platform-neutral peer lookup.
    pub fn all(&self) -> Vec<(GhostContactDescriptor, u64)> {
        self.entries
            .lock()
            .map(|entries| {
                entries
                    .values()
                    .cloned()
                    .map(|entry| (entry.descriptor, entry.blue_score))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Clear all acceleration state. Rebuilding from Kaspa L1 remains authoritative.
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

fn verified_descriptor(payload: &[u8], current_daa: u64) -> Option<GhostContactDescriptor> {
    if !payload.starts_with(&ghost_protocol::GTCD_MAGIC) {
        return None;
    }
    let descriptor = GhostContactDescriptor::decode(payload).ok()?;
    if descriptor.version != GTCD_VERSION || is_expired(&descriptor, current_daa) {
        return None;
    }
    ghost_kaspa::validate_destination(&descriptor.kaspa_address).ok()?;
    ghost_kaspa::verify_gtcd(&descriptor).ok()?;
    Some(descriptor)
}

fn is_expired(descriptor: &GhostContactDescriptor, current_daa: u64) -> bool {
    descriptor
        .expires_daa
        .is_some_and(|expires| current_daa != 0 && expires <= current_daa)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_ghost_payload_is_ignored_and_clear_is_idempotent() {
        let index = GhostProfileIndex::default();
        index.ingest_payload(b"not-a-ghost-record", 42, 42);
        assert!(index.latest("kaspa:any").is_none());
        index.clear();
        index.clear();
    }
}
