//! Public-room announcement signing for the standalone Web host.

use ghost_rooms::RoomAnnouncement;
use serde_json::Value;

use super::support::util::{required, required_str, to_value};

pub(super) fn sign_announcement(args: &Value) -> Result<Value, String> {
    let request = args
        .get("request")
        .ok_or("browser command argument request is missing")?;
    let password = required_str(request, "password")?;
    let sealed: Vec<u8> = required(request, "sealed")?;
    let projection: crate::model::WalletProjection = required(request, "public")?;
    let public = ghost_kaspa::wallet::WalletPublic::from_projection(&projection);
    let secret: ghost_kaspa::wallet::WalletSecret =
        ghost_storage::open_json(password, &sealed, "wallet vault")?;
    ghost_kaspa::wallet::validate_public_projection(&secret, &public)?;
    let announcement: RoomAnnouncement = required(request, "announcement")?;
    to_value(ghost_kaspa::sign_room_announcement(
        &secret,
        &public,
        announcement,
    )?)
}
