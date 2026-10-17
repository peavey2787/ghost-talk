//! Public-room announcement signing: the swarm room directory counterpart of
//! the signed public profile descriptor.

use ghost_kaspa::wallet::WalletPublic;
use ghost_rooms::RoomAnnouncement;
use serde::Deserialize;
use tauri::State;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomAnnouncementSignRequest {
    pub profile_id: String,
    pub password: String,
    pub sealed: Vec<u8>,
    pub public: WalletPublic,
    pub announcement: RoomAnnouncement,
}

#[tauri::command]
pub fn room_announcement_sign(
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: RoomAnnouncementSignRequest,
) -> Result<RoomAnnouncement, String> {
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    ghost_kaspa::sign_room_announcement(&secret, &request.public, request.announcement)
}
