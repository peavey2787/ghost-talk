//! Public-room announcements for the standalone Web host: signed for the
//! swarm room directory, or signed and published as a Kaspa payload.

use crate::model::BroadcastResult;
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use ghost_rooms::RoomAnnouncement;
use serde_json::Value;

use super::{
    kaspa::{endpoint_override, profile_portal},
    support::util::{required, required_str, to_value},
};

pub(super) fn sign_announcement(args: &Value) -> Result<Value, String> {
    let (_, _, announcement) = signed(request(args)?)?;
    to_value(announcement)
}

/// Sign the announcement and broadcast it to the owner's stable address.
pub(super) async fn publish_announcement(args: &Value) -> Result<Value, String> {
    let request = request(args)?;
    let profile_id = required_str(request, "profileId")?;
    let (secret, public, announcement) = signed(request)?;
    let stable_address = public
        .receive_addresses
        .first()
        .cloned()
        .ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?;
    let wrpc = endpoint_override(request, "wrpcEndpoint");
    let portal = profile_portal(profile_id, &public, wrpc).await?;
    let payload = announcement.encode()?;
    let result =
        ghost_kaspa::wallet::send_payload(&portal, &secret, &public, &stable_address, 0, &payload)
            .await?;
    to_value(BroadcastResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        public: result.public.projection(),
    })
}

fn request(args: &Value) -> Result<&Value, String> {
    args.get("request")
        .ok_or_else(|| "browser command argument request is missing".to_string())
}

fn signed(request: &Value) -> Result<(WalletSecret, WalletPublic, RoomAnnouncement), String> {
    let password = required_str(request, "password")?;
    let sealed: Vec<u8> = required(request, "sealed")?;
    let projection: crate::model::WalletProjection = required(request, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let secret: WalletSecret = ghost_storage::open_json(password, &sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    let announcement: RoomAnnouncement = required(request, "announcement")?;
    let announcement = ghost_kaspa::sign_room_announcement(&secret, &public, announcement)?;
    Ok((secret, public, announcement))
}
