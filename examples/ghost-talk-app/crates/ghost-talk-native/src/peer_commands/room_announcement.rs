//! Public-room announcements, the room counterpart of the signed public
//! profile descriptor: signed for the swarm room directory, or signed and
//! published as a Kaspa payload.

use ghost_api::BroadcastResult;
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomAnnouncementPublishRequest {
    #[serde(flatten)]
    pub sign: RoomAnnouncementSignRequest,
    #[serde(default)]
    pub wrpc_endpoint: Option<String>,
}

#[tauri::command]
pub fn room_announcement_sign(
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: RoomAnnouncementSignRequest,
) -> Result<RoomAnnouncement, String> {
    signed(&wallet_state, request).map(|(_, announcement)| announcement)
}

/// Sign the announcement and broadcast it to the owner's stable address.
#[tauri::command]
pub async fn room_announcement_publish(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    request: RoomAnnouncementPublishRequest,
) -> Result<BroadcastResult, String> {
    let public = request.sign.public.clone();
    let outbound_lock = wallet_state.outbound_lock(&request.sign.profile_id)?;
    let (secret, announcement) = signed(&wallet_state, request.sign)?;
    let stable_address = crate::wallet_commands::stable_address(&public)?;
    let _outbound_guard = outbound_lock.lock().await;
    let result = super::publish::publish_payload(
        &gateway,
        &secret,
        &public,
        &stable_address,
        &announcement.encode()?,
        request.wrpc_endpoint.as_deref(),
    )
    .await?;
    Ok(BroadcastResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        public: crate::wallet_commands::wallet_projection(&result.public),
    })
}

fn signed(
    wallet_state: &crate::wallet_commands::WalletRuntimeState,
    request: RoomAnnouncementSignRequest,
) -> Result<(WalletSecret, RoomAnnouncement), String> {
    let secret = wallet_state.secret_or_open(
        &request.profile_id,
        &request.password,
        &request.sealed,
        &request.public,
    )?;
    let announcement =
        ghost_kaspa::sign_room_announcement(&secret, &request.public, request.announcement)?;
    Ok((secret, announcement))
}
