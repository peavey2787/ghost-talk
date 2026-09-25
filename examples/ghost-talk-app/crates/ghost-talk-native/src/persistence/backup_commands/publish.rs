use super::{encrypt_archive, fragment_backup, BACKUP_VERSION, MAX_BACKUP_PLAINTEXT_BYTES};
use crate::backup_validation::validate_archive_inputs;
use ghost_api::{BackupContact, BackupMessage, ProfileBackupArchive, ProfileBackupPublishResult};
use ghost_kaspa::wallet::WalletPublic;
use sha2::{Digest, Sha256};
use tauri::State;

#[expect(clippy::too_many_arguments, reason = "stable flat Tauri IPC contract")]
#[tauri::command]
pub async fn profile_backup_publish(
    gateway: State<'_, crate::kaspa_gateway::KaspaGatewayState>,
    wallet_state: State<'_, crate::wallet_commands::WalletRuntimeState>,
    profile_id: String,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    contacts: Vec<BackupContact>,
    messages: Vec<BackupMessage>,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<ProfileBackupPublishResult, String> {
    let outbound_lock = wallet_state.outbound_lock(&profile_id)?;
    let _outbound_guard = outbound_lock.lock().await;
    validate_archive_inputs(&contacts, &messages)?;
    let secret = crate::wallet_commands::open_secret(&password, &sealed)?;
    crate::wallet_commands::validate_public_projection(&secret, &public)?;
    let (frames, content_hash) = prepare_backup_frames(&secret, contacts, messages)?;
    let fee = crate::wallet_commands::parse_u64_decimal(&fee_sompi, "backup fee")?;
    let portal = gateway.portal(&public, wrpc_endpoint.as_deref()).await?;
    let (projected, transaction_ids) =
        send_backup_frames(&gateway, &portal, &secret, public, frames, fee).await?;
    Ok(ProfileBackupPublishResult {
        transaction_ids,
        content_hash,
        public: crate::wallet_commands::wallet_projection(&projected),
    })
}

fn prepare_backup_frames(
    secret: &ghost_kaspa::wallet::WalletSecret,
    contacts: Vec<BackupContact>,
    messages: Vec<BackupMessage>,
) -> Result<(Vec<Vec<u8>>, String), String> {
    let archive = ProfileBackupArchive {
        version: BACKUP_VERSION,
        saved_at_ms: crate::time::unix_millis(),
        contacts,
        messages,
    };
    let plaintext =
        serde_json::to_vec(&archive).map_err(|error| format!("profile backup encode: {error}"))?;
    if plaintext.len() > MAX_BACKUP_PLAINTEXT_BYTES {
        return Err("profile backup exceeds the 1 MiB encrypted archive limit".into());
    }
    let content_hash = hex::encode(Sha256::digest(&plaintext));
    let encrypted = encrypt_archive(secret, &plaintext)?;
    Ok((fragment_backup(&encrypted)?, content_hash))
}

async fn send_backup_frames(
    gateway: &crate::kaspa_gateway::KaspaGatewayState,
    portal: &ghost_kaspa::PortalFacade,
    secret: &ghost_kaspa::wallet::WalletSecret,
    mut public: WalletPublic,
    frames: Vec<Vec<u8>>,
    fee: u64,
) -> Result<(WalletPublic, Vec<String>), String> {
    let destination = public.receive_address()?.to_owned();
    let mut transaction_ids = Vec::with_capacity(frames.len());
    for frame in frames {
        let result =
            ghost_kaspa::wallet::send_payload(portal, secret, &public, &destination, fee, &frame)
                .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                gateway.note_operation_error(&error).await;
                return Err(error);
            }
        };
        public = result.public;
        transaction_ids.push(result.transaction_id);
    }
    Ok((public, transaction_ids))
}
