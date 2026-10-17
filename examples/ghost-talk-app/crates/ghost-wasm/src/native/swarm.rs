//! Host commands behind the swarm carrier: the per-profile spend mode and
//! public-room announcement signing.

use crate::model::Profile;
use ghost_domain::settings::SpendMode;
use ghost_rooms::RoomAnnouncement;
use serde_json::json;

pub(crate) async fn set_spend_mode(profile_id: &str, spend_mode: SpendMode) -> Result<(), String> {
    super::invoke::invoke_unit(
        "mailbox_set_spend_mode",
        json!({ "profileId": profile_id, "spendMode": spend_mode }),
    )
    .await
}

pub(crate) async fn sign_room_announcement(
    profile: &Profile,
    password: &str,
    announcement: RoomAnnouncement,
) -> Result<RoomAnnouncement, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    super::invoke::invoke(
        "room_announcement_sign",
        json!({
            "request": {
                "profileId": profile.id,
                "password": password,
                "sealed": wallet.sealed,
                "public": wallet.public,
                "announcement": announcement,
            }
        }),
    )
    .await
}
