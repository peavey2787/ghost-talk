//! Host commands behind the swarm carrier and the room directory: the
//! per-profile spend mode and public-room announcement signing/publishing.

use crate::model::Profile;
use ghost_domain::settings::SpendMode;
use ghost_rooms::RoomAnnouncement;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

pub(crate) async fn set_spend_mode(profile_id: &str, spend_mode: SpendMode) -> Result<(), String> {
    super::invoke::invoke_unit(
        "mailbox_set_spend_mode",
        json!({ "profileId": profile_id, "spendMode": spend_mode }),
    )
    .await
}

/// What the host does with a room announcement: sign it for the swarm, or
/// sign and publish it as a Kaspa payload (which costs a fee).
#[derive(Clone, Copy)]
pub(crate) enum AnnouncementCommand {
    Sign,
    Publish,
}

pub(crate) async fn room_announcement<T: DeserializeOwned>(
    command: AnnouncementCommand,
    profile: &Profile,
    password: &str,
    announcement: RoomAnnouncement,
) -> Result<T, String> {
    let name = match command {
        AnnouncementCommand::Sign => "room_announcement_sign",
        AnnouncementCommand::Publish => "room_announcement_publish",
    };
    let request = announcement_request(profile, password, &announcement)?;
    super::invoke::invoke(name, request).await
}

fn announcement_request(
    profile: &Profile,
    password: &str,
    announcement: &RoomAnnouncement,
) -> Result<Value, String> {
    let wallet = profile
        .wallet
        .as_ref()
        .ok_or_else(|| "No wallet configured".to_string())?;
    Ok(json!({
        "request": {
            "profileId": profile.id,
            "password": password,
            "sealed": wallet.sealed,
            "public": wallet.public,
            "announcement": announcement,
            "wrpcEndpoint": wallet.wrpc_endpoint,
        }
    }))
}
