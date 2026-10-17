//! Public-room announcements are signed by the owner's Kaspa address key, so
//! any swarm peer can check who advertises a room without a transaction.

use ghost_rooms::{RoomAnnouncement, ROOM_ANNOUNCEMENT_SIGNATURE_DOMAIN};

use crate::wallet::{private_key_for_address, WalletPublic, WalletSecret};

pub fn sign_room_announcement(
    secret: &WalletSecret,
    public: &WalletPublic,
    mut announcement: RoomAnnouncement,
) -> Result<RoomAnnouncement, String> {
    let signing = announcement.signing_bytes()?;
    let mut private_key =
        private_key_for_address(secret, public, &announcement.owner_kaspa_address)?;
    let signed =
        crate::sign_domain_message(&private_key, ROOM_ANNOUNCEMENT_SIGNATURE_DOMAIN, &signing);
    zeroize::Zeroize::zeroize(&mut private_key);
    announcement.signature_hex = signed?;
    verify_room_announcement(&announcement)?;
    Ok(announcement)
}

pub fn verify_room_announcement(announcement: &RoomAnnouncement) -> Result<(), String> {
    crate::verify_domain_message(
        &announcement.owner_kaspa_address,
        &announcement.signature_hex,
        ROOM_ANNOUNCEMENT_SIGNATURE_DOMAIN,
        &announcement.signing_bytes()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghost_rooms::{RoomMode, ROOM_ANNOUNCEMENT_VERSION};

    fn announcement(owner: &str) -> RoomAnnouncement {
        RoomAnnouncement {
            version: ROOM_ANNOUNCEMENT_VERSION,
            room_id: "ab".repeat(16),
            name: "Lobby".into(),
            mode: RoomMode::Stage,
            member_count: 3,
            owner_label: "Host".into(),
            owner_kaspa_address: owner.into(),
            owner_hydra_id: "cd".repeat(32),
            issued_at_ms: 42,
            expires_at_ms: 1_042,
            signature_hex: String::new(),
        }
    }

    #[test]
    fn signed_announcement_verifies_and_rejects_tampering() {
        let (secret, created) =
            crate::wallet::generate_wallet("", "m/44'/111111'/0'", "mainnet").unwrap();
        let owner = created.public.receive_addresses[0].clone();
        let mut signed =
            sign_room_announcement(&secret, &created.public, announcement(&owner)).unwrap();
        assert!(verify_room_announcement(&signed).is_ok());
        signed.member_count += 1;
        assert!(verify_room_announcement(&signed).is_err());
    }
}
