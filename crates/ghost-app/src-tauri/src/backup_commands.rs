use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use ghost_history::{rest_base_for_network, RestHistory};
use ghost_kaspa::wallet::WalletPublic;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, time::{SystemTime, UNIX_EPOCH}};
use tauri::State;
use zeroize::Zeroize;

const BACKUP_MAGIC: &[u8; 4] = b"GTBK";
const BACKUP_VERSION: u8 = 1;
const BACKUP_HEADER_BYTES: usize = 29;
const BACKUP_AAD: &[u8] = b"GhostTalk/KaspaProfileBackup/v1\0";
const MAX_BACKUP_PLAINTEXT_BYTES: usize = 1024 * 1024;
const MAX_BACKUP_FRAGMENTS: usize = 16;
const MAX_BACKUP_CONTACTS: usize = 4096;
const MAX_BACKUP_MESSAGES: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupContact {
    pub id: String,
    pub label: String,
    pub kaspa_address: String,
    #[serde(default)]
    pub hydra_handle: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupMessage {
    pub chat_id: String,
    #[serde(default)]
    pub contact_id: Option<String>,
    pub chat_label: String,
    pub id: String,
    pub direction: String,
    pub body: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileBackupArchive {
    pub version: u8,
    pub saved_at_ms: u64,
    pub contacts: Vec<BackupContact>,
    pub messages: Vec<BackupMessage>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfileBackupPublishResult {
    pub transaction_ids: Vec<String>,
    pub content_hash: String,
    pub public: WalletPublic,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfileBackupRestoreResult {
    pub content_hash: String,
    pub archive: ProfileBackupArchive,
}

#[derive(Default)]
struct BackupCandidate {
    count: usize,
    total_len: usize,
    parts: HashMap<usize, Vec<u8>>,
    max_blue_score: u64,
}

#[tauri::command]
pub async fn profile_backup_publish(
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    contacts: Vec<BackupContact>,
    messages: Vec<BackupMessage>,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<ProfileBackupPublishResult, String> {
    validate_archive_inputs(&contacts, &messages)?;
    let secret = super::wallet_commands::open_secret(&password, &sealed)?;
    super::wallet_commands::validate_public_projection(&secret, &public)?;
    let archive = ProfileBackupArchive {
        version: BACKUP_VERSION,
        saved_at_ms: now_ms(),
        contacts,
        messages,
    };
    let plaintext = serde_json::to_vec(&archive)
        .map_err(|error| format!("profile backup encode: {error}"))?;
    if plaintext.len() > MAX_BACKUP_PLAINTEXT_BYTES {
        return Err("profile backup exceeds the 1 MiB encrypted archive limit".into());
    }
    let content_hash = hex::encode(Sha256::digest(&plaintext));
    let encrypted = encrypt_archive(&secret, &plaintext)?;
    let frames = fragment_backup(&encrypted)?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "backup fee")?;
    let portal = gateway.portal(&public, wrpc_endpoint.as_deref()).await?;

    let destination = public.receive_address()?.to_owned();
    let mut projected = public;
    let mut transaction_ids = Vec::with_capacity(frames.len());
    for frame in frames {
        let result = match ghost_kaspa::wallet::send_payload(
            &portal,
            &secret,
            &projected,
            &destination,
            fee,
            &frame,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => {
                gateway.note_operation_error(&error).await;
                return Err(error);
            }
        };
        projected = result.public;
        transaction_ids.push(result.transaction_id);
    }
    Ok(ProfileBackupPublishResult {
        transaction_ids,
        content_hash,
        public: projected,
    })
}

#[tauri::command]
pub async fn profile_backup_restore(
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    rest_endpoint: Option<String>,
) -> Result<Option<ProfileBackupRestoreResult>, String> {
    let secret = super::wallet_commands::open_secret(&password, &sealed)?;
    super::wallet_commands::validate_public_projection(&secret, &public)?;
    let rest = rest_endpoint
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| rest_base_for_network(&public.network));
    let history = RestHistory::new(rest, 16);
    let addresses = public.all_addresses().cloned().collect::<Vec<_>>();
    // REST is archival-only. Never ask the indexer which addresses are current,
    // what the wallet balance is, or for transactions younger than 48 hours.
    // Recent backup frames are learned from the live Kaspa BlockAdded stream and
    // local state; archival restore may use REST only once they cross this age.
    let cutoff_ms = now_ms().saturating_sub(48 * 60 * 60 * 1_000);
    let transactions = history
        .wallet_history_before_ms(&addresses, cutoff_ms)
        .await?;
    let mut candidates: HashMap<[u8; 16], BackupCandidate> = HashMap::new();
    for transaction in transactions {
        let Some((snapshot, index, count, total_len, data)) = parse_backup_frame(&transaction.payload)
        else {
            continue;
        };
        if candidates
            .get(&snapshot)
            .is_some_and(|candidate| {
                candidate.count != 0
                    && (candidate.count != count || candidate.total_len != total_len)
            })
        {
            candidates.remove(&snapshot);
            continue;
        }
        let candidate = candidates.entry(snapshot).or_default();
        candidate.count = count;
        candidate.total_len = total_len;
        candidate.max_blue_score = candidate
            .max_blue_score
            .max(transaction.accepting_block_blue_score.unwrap_or_default());
        candidate.parts.entry(index).or_insert(data.to_vec());
    }

    let mut candidates = candidates.into_values().collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.max_blue_score));
    for candidate in candidates {
        let Some(encrypted) = reassemble_candidate(candidate) else {
            continue;
        };
        let Ok(mut plaintext) = decrypt_archive(&secret, &encrypted) else {
            continue;
        };
        let decoded = serde_json::from_slice::<ProfileBackupArchive>(&plaintext);
        let content_hash = hex::encode(Sha256::digest(&plaintext));
        plaintext.zeroize();
        let archive = match decoded {
            Ok(archive) => archive,
            Err(_) => continue,
        };
        if archive.version != BACKUP_VERSION
            || validate_archive_inputs(&archive.contacts, &archive.messages).is_err()
        {
            continue;
        }
        return Ok(Some(ProfileBackupRestoreResult {
            content_hash,
            archive,
        }));
    }
    Ok(None)
}

fn encrypt_archive(
    secret: &ghost_kaspa::wallet::WalletSecret,
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    let mut key = ghost_kaspa::wallet::profile_backup_key(secret)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| "profile backup key initialization failed".to_string())?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: BACKUP_AAD,
            },
        )
        .map_err(|_| "profile backup encryption failed".to_string());
    key.zeroize();
    let mut output = Vec::with_capacity(12 + plaintext.len() + 16);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&encrypted?);
    Ok(output)
}

fn decrypt_archive(
    secret: &ghost_kaspa::wallet::WalletSecret,
    encrypted: &[u8],
) -> Result<Vec<u8>, String> {
    if encrypted.len() < 28 || encrypted.len() > MAX_BACKUP_PLAINTEXT_BYTES + 28 {
        return Err("profile backup ciphertext length is invalid".into());
    }
    let mut key = ghost_kaspa::wallet::profile_backup_key(secret)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| "profile backup key initialization failed".to_string())?;
    let result = cipher
        .decrypt(
            Nonce::from_slice(&encrypted[..12]),
            Payload {
                msg: &encrypted[12..],
                aad: BACKUP_AAD,
            },
        )
        .map_err(|_| "profile backup authentication failed".to_string());
    key.zeroize();
    result
}

fn fragment_backup(encrypted: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let chunk = ghost_core::MAX_GHOST_TX_PAYLOAD
        .checked_sub(BACKUP_HEADER_BYTES)
        .ok_or_else(|| "Ghost Talk payload ceiling is smaller than backup header".to_string())?;
    let count = encrypted.len().div_ceil(chunk);
    if count == 0 || count > MAX_BACKUP_FRAGMENTS || count > usize::from(u16::MAX) {
        return Err("encrypted profile backup requires too many Kaspa transactions".into());
    }
    let total_len = u32::try_from(encrypted.len())
        .map_err(|_| "encrypted profile backup is too large".to_string())?;
    let mut snapshot = [0u8; 16];
    OsRng.fill_bytes(&mut snapshot);
    let mut frames = Vec::with_capacity(count);
    for (index, part) in encrypted.chunks(chunk).enumerate() {
        let mut frame = Vec::with_capacity(BACKUP_HEADER_BYTES + part.len());
        frame.extend_from_slice(BACKUP_MAGIC);
        frame.push(BACKUP_VERSION);
        frame.extend_from_slice(&snapshot);
        frame.extend_from_slice(&(index as u16).to_le_bytes());
        frame.extend_from_slice(&(count as u16).to_le_bytes());
        frame.extend_from_slice(&total_len.to_le_bytes());
        frame.extend_from_slice(part);
        frames.push(frame);
    }
    Ok(frames)
}

fn parse_backup_frame(payload: &[u8]) -> Option<([u8; 16], usize, usize, usize, &[u8])> {
    if payload.len() < BACKUP_HEADER_BYTES
        || &payload[..4] != BACKUP_MAGIC
        || payload[4] != BACKUP_VERSION
    {
        return None;
    }
    let snapshot: [u8; 16] = payload[5..21].try_into().ok()?;
    let index = usize::from(u16::from_le_bytes(payload[21..23].try_into().ok()?));
    let count = usize::from(u16::from_le_bytes(payload[23..25].try_into().ok()?));
    let total_len = usize::try_from(u32::from_le_bytes(payload[25..29].try_into().ok()?)).ok()?;
    if count == 0
        || count > MAX_BACKUP_FRAGMENTS
        || index >= count
        || total_len < 28
        || total_len > MAX_BACKUP_PLAINTEXT_BYTES + 28
        || payload.len() > ghost_core::MAX_GHOST_TX_PAYLOAD
    {
        return None;
    }
    Some((snapshot, index, count, total_len, &payload[BACKUP_HEADER_BYTES..]))
}

fn reassemble_candidate(candidate: BackupCandidate) -> Option<Vec<u8>> {
    if candidate.count == 0
        || candidate.count > MAX_BACKUP_FRAGMENTS
        || candidate.total_len > MAX_BACKUP_PLAINTEXT_BYTES + 28
        || candidate.parts.len() != candidate.count
    {
        return None;
    }
    let mut encrypted = Vec::with_capacity(candidate.total_len);
    for index in 0..candidate.count {
        encrypted.extend_from_slice(candidate.parts.get(&index)?);
        if encrypted.len() > candidate.total_len {
            return None;
        }
    }
    (encrypted.len() == candidate.total_len).then_some(encrypted)
}

fn validate_archive_inputs(
    contacts: &[BackupContact],
    messages: &[BackupMessage],
) -> Result<(), String> {
    if contacts.len() > MAX_BACKUP_CONTACTS {
        return Err("profile backup contains too many contacts".into());
    }
    if messages.len() > MAX_BACKUP_MESSAGES {
        return Err("profile backup contains too many messages".into());
    }
    for contact in contacts {
        bounded(&contact.id, 1, 128, "contact id")?;
        bounded(&contact.label, 1, 256, "contact label")?;
        bounded(&contact.kaspa_address, 1, 192, "contact Kaspa address")?;
        ghost_core::KaspaAddress::parse(&contact.kaspa_address)?;
        if let Some(handle) = contact.hydra_handle.as_deref() {
            bounded(handle, 1, 256, "HYDRA contact id")?;
        }
    }
    for message in messages {
        bounded(&message.chat_id, 1, 160, "backup chat id")?;
        bounded(&message.chat_label, 1, 256, "backup chat label")?;
        bounded(&message.id, 1, 160, "backup message id")?;
        if let Some(contact_id) = message.contact_id.as_deref() {
            bounded(contact_id, 1, 128, "backup contact id")?;
        }
        if !matches!(message.direction.as_str(), "in" | "out") {
            return Err("backup message direction must be in or out".into());
        }
        if message.body.is_empty() || message.body.len() > ghost_core::MAX_EVENT_BYTES {
            return Err("backup message body is empty or exceeds the event limit".into());
        }
    }
    Ok(())
}

fn bounded(value: &str, min: usize, max: usize, label: &str) -> Result<(), String> {
    if value.len() < min || value.len() > max {
        return Err(format!("{label} length is invalid"));
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_frame_parser_rejects_bad_counts_and_lengths() {
        assert!(parse_backup_frame(b"GTBK").is_none());
        let mut frame = vec![0u8; BACKUP_HEADER_BYTES];
        frame[..4].copy_from_slice(BACKUP_MAGIC);
        frame[4] = BACKUP_VERSION;
        frame[23..25].copy_from_slice(&1u16.to_le_bytes());
        frame[25..29].copy_from_slice(&28u32.to_le_bytes());
        assert!(parse_backup_frame(&frame).is_some());
        frame[23..25].copy_from_slice(&0u16.to_le_bytes());
        assert!(parse_backup_frame(&frame).is_none());
    }
}
