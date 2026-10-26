//! Signed public-room advertisement, carried on the p2p-net room directory
//! topic (short-lived, re-announced while the owner is online) or in a Kaspa
//! transaction payload (long-lived, costs a fee). Only `RoomAccess::Public`
//! rooms can produce one; private rooms are never announced. The owner signs the
//! canonical body with the Kaspa P2PK key of `owner_kaspa_address`; the host
//! verifies that signature before a listing reaches the directory.

use serde::{Deserialize, Serialize};

use crate::{Room, RoomAccess};

pub const GTRA_MAGIC: [u8; 4] = *b"GTRA";
pub const ROOM_ANNOUNCEMENT_VERSION: u16 = 2;
/// Kaspa domain-signature domain of a room announcement.
pub const ROOM_ANNOUNCEMENT_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/RoomAnnouncement/v1";
const MAX_ENCODED_BYTES: usize = 4 * 1024;
/// Swarm announcements are re-published every minute and expire quickly.
pub const SWARM_LISTING_LIFETIME_MS: u64 = 10 * 60 * 1000;
/// A Kaspa announcement lists the room for a day.
pub const KASPA_LISTING_LIFETIME_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_LISTING_LIFETIME_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const MAX_NAME_CHARS: usize = 80;
const MAX_LABEL_CHARS: usize = 80;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomAnnouncement {
    pub version: u16,
    pub room_id: String,
    pub name: String,
    pub member_count: u32,
    pub owner_label: String,
    pub owner_kaspa_address: String,
    pub owner_hydra_id: String,
    pub issued_at_ms: u64,
    /// Listeners drop the listing at this time (signed).
    pub expires_at_ms: u64,
    #[serde(default)]
    pub signature_hex: String,
}

/// The signed fields, in a fixed order, without the signature.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Signing<'a> {
    version: u16,
    room_id: &'a str,
    name: &'a str,
    member_count: u32,
    owner_label: &'a str,
    owner_kaspa_address: &'a str,
    owner_hydra_id: &'a str,
    issued_at_ms: u64,
    expires_at_ms: u64,
}

impl RoomAnnouncement {
    /// An unsigned announcement for an accepted public room, listed for
    /// `lifetime_ms`; `None` for every other room.
    pub fn for_room(room: &Room, issued_at_ms: u64, lifetime_ms: u64) -> Option<Self> {
        if room.access() != RoomAccess::Public || room.pending_acceptance() {
            return None;
        }
        let announcement = Self {
            version: ROOM_ANNOUNCEMENT_VERSION,
            room_id: room.id.clone(),
            name: room.name.trim().to_owned(),
            member_count: u32::try_from(room.members().len()).unwrap_or(u32::MAX),
            owner_label: room.owner_label.trim().to_owned(),
            owner_kaspa_address: room.owner_kaspa_address.clone(),
            owner_hydra_id: room.owner_hydra_id.clone(),
            issued_at_ms,
            expires_at_ms: issued_at_ms.saturating_add(lifetime_ms),
            signature_hex: String::new(),
        };
        announcement.validate().ok()?;
        Some(announcement)
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        serde_json::to_vec(&Signing {
            version: self.version,
            room_id: &self.room_id,
            name: &self.name,
            member_count: self.member_count,
            owner_label: &self.owner_label,
            owner_kaspa_address: &self.owner_kaspa_address,
            owner_hydra_id: &self.owner_hydra_id,
            issued_at_ms: self.issued_at_ms,
            expires_at_ms: self.expires_at_ms,
        })
        .map_err(|error| error.to_string())
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let json = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        let mut out = Vec::with_capacity(GTRA_MAGIC.len() + json.len());
        out.extend_from_slice(&GTRA_MAGIC);
        out.extend_from_slice(&json);
        bounded(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let json = bounded(bytes.to_vec())?
            .strip_prefix(&GTRA_MAGIC)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| "not a Ghost Talk room announcement".to_string())?;
        let value: Self = serde_json::from_slice(&json).map_err(|error| error.to_string())?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), String> {
        let valid = self.version == ROOM_ANNOUNCEMENT_VERSION
            && is_lower_hex(&self.room_id, 32)
            && is_lower_hex(&self.owner_hydra_id, 64)
            && bounded_text(&self.name, MAX_NAME_CHARS)
            && bounded_text(&self.owner_label, MAX_LABEL_CHARS)
            && self.owner_kaspa_address.starts_with("kaspa")
            && self.owner_kaspa_address.len() <= 128
            && self.expires_at_ms > self.issued_at_ms
            && self.expires_at_ms - self.issued_at_ms <= MAX_LISTING_LIFETIME_MS;
        valid
            .then_some(())
            .ok_or_else(|| "Ghost Talk room announcement is invalid".to_string())
    }
}

fn bounded(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.len() > MAX_ENCODED_BYTES {
        return Err("Ghost Talk room announcement is too large".into());
    }
    Ok(bytes)
}

fn bounded_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max_chars
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{NewRoom, RoomService};

    pub(crate) fn room(access: RoomAccess) -> Room {
        RoomService::new_room(NewRoom {
            id: "ab".repeat(16),
            name: " Lobby ".into(),
            owner_hydra_id: "cd".repeat(32),
            owner_kaspa_address: "kaspa:qowner".into(),
            owner_label: "Host".into(),
            access,
            pending_acceptance: false,
            revision: 0,
            members: Vec::new(),
            bans: Vec::new(),
        })
    }

    #[test]
    fn only_public_rooms_are_announced() {
        assert!(RoomAnnouncement::for_room(&room(RoomAccess::Private), 1, 10).is_none());
        let announcement = RoomAnnouncement::for_room(&room(RoomAccess::Public), 7, 10).unwrap();
        assert_eq!(announcement.name, "Lobby");
        assert_eq!(announcement.issued_at_ms, 7);
        assert_eq!(announcement.expires_at_ms, 17);
    }

    #[test]
    fn announcement_round_trips_and_signing_excludes_the_signature() {
        let mut announcement =
            RoomAnnouncement::for_room(&room(RoomAccess::Public), 9, 10).unwrap();
        let unsigned = announcement.signing_bytes().unwrap();
        announcement.signature_hex = "ee".repeat(64);
        assert_eq!(announcement.signing_bytes().unwrap(), unsigned);
        let decoded = RoomAnnouncement::decode(&announcement.encode().unwrap()).unwrap();
        assert_eq!(decoded, announcement);
    }

    #[test]
    fn malformed_announcements_fail_closed() {
        let mut announcement =
            RoomAnnouncement::for_room(&room(RoomAccess::Public), 1, 10).unwrap();
        assert!(RoomAnnouncement::decode(b"{}").is_err());
        assert!(RoomAnnouncement::decode(&[b'G'; MAX_ENCODED_BYTES + 1]).is_err());
        announcement.version = 1;
        assert!(
            announcement.encode().is_err(),
            "older listings are rejected"
        );
        announcement.version = ROOM_ANNOUNCEMENT_VERSION;
        announcement.room_id = "not-hex".into();
        assert!(announcement.signing_bytes().is_err());
        announcement.room_id = "ab".repeat(16);
        announcement.expires_at_ms = announcement.issued_at_ms;
        assert!(announcement.encode().is_err(), "expiry must follow issue");
        announcement.expires_at_ms = MAX_LISTING_LIFETIME_MS + 2;
        assert!(announcement.encode().is_err(), "lifetime is bounded");
    }
}
