use crate::backup_validation::validate_archive_inputs;
use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use ghost_api::{ProfileBackupArchive, ProfileBackupRestoreResult};
use ghost_history::{rest_base_for_network, HistoryTx, RestHistory};
use ghost_kaspa::wallet::WalletPublic;
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use zeroize::Zeroize;

const BACKUP_MAGIC: &[u8; 4] = b"GTBK";
const BACKUP_VERSION: u8 = 1;
const BACKUP_HEADER_BYTES: usize = 29;
const BACKUP_AAD: &[u8] = b"GhostTalk/KaspaProfileBackup/v1\0";
const MAX_BACKUP_PLAINTEXT_BYTES: usize = 1024 * 1024;
const MAX_BACKUP_FRAGMENTS: usize = 16;

#[derive(Default)]
struct BackupCandidate {
    count: usize,
    total_len: usize,
    parts: HashMap<usize, Vec<u8>>,
    max_blue_score: u64,
}

mod publish;
pub use publish::profile_backup_publish;

#[tauri::command]
pub async fn profile_backup_restore(
    password: String,
    sealed: Vec<u8>,
    public: WalletPublic,
    rest_endpoint: Option<String>,
) -> Result<Option<ProfileBackupRestoreResult>, String> {
    let secret = crate::wallet_commands::open_secret(&password, &sealed)?;
    crate::wallet_commands::validate_public_projection(&secret, &public)?;
    let transactions = archival_backup_transactions(&public, rest_endpoint.as_deref()).await?;
    let candidates = collect_backup_candidates(transactions);
    Ok(restore_newest_backup(&secret, candidates))
}

async fn archival_backup_transactions(
    public: &WalletPublic,
    rest_endpoint: Option<&str>,
) -> Result<Vec<HistoryTx>, String> {
    let rest = rest_endpoint
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| rest_base_for_network(&public.network));
    let history = RestHistory::new(rest, 16);
    let addresses = public.all_addresses().cloned().collect::<Vec<_>>();
    let cutoff_ms = crate::time::unix_millis().saturating_sub(48 * 60 * 60 * 1_000);
    Ok(history
        .wallet_history_before_ms(&addresses, cutoff_ms)
        .await?
        .transactions)
}

fn collect_backup_candidates(transactions: Vec<HistoryTx>) -> Vec<BackupCandidate> {
    let mut candidates: HashMap<[u8; 16], BackupCandidate> = HashMap::new();
    for transaction in transactions {
        let Some(frame) = parse_backup_frame(&transaction.payload) else {
            continue;
        };
        let conflicts = candidates.get(&frame.snapshot).is_some_and(|candidate| {
            candidate.count != 0
                && (candidate.count != frame.count || candidate.total_len != frame.total_len)
        });
        if conflicts {
            candidates.remove(&frame.snapshot);
            continue;
        }
        let candidate = candidates.entry(frame.snapshot).or_default();
        candidate.count = frame.count;
        candidate.total_len = frame.total_len;
        candidate.max_blue_score = candidate
            .max_blue_score
            .max(transaction.accepting_block_blue_score.unwrap_or_default());
        candidate
            .parts
            .entry(frame.index)
            .or_insert(frame.data.to_vec());
    }
    let mut values = candidates.into_values().collect::<Vec<_>>();
    values.sort_by_key(|candidate| std::cmp::Reverse(candidate.max_blue_score));
    values
}

fn restore_newest_backup(
    secret: &ghost_kaspa::wallet::WalletSecret,
    candidates: Vec<BackupCandidate>,
) -> Option<ProfileBackupRestoreResult> {
    candidates
        .into_iter()
        .find_map(|candidate| restore_candidate(secret, candidate))
}

fn restore_candidate(
    secret: &ghost_kaspa::wallet::WalletSecret,
    candidate: BackupCandidate,
) -> Option<ProfileBackupRestoreResult> {
    let encrypted = reassemble_candidate(candidate)?;
    let mut plaintext = decrypt_archive(secret, &encrypted).ok()?;
    let decoded = serde_json::from_slice::<ProfileBackupArchive>(&plaintext).ok();
    let content_hash = hex::encode(Sha256::digest(&plaintext));
    plaintext.zeroize();
    let archive = decoded?;
    if archive.version != BACKUP_VERSION
        || validate_archive_inputs(&archive.contacts, &archive.messages).is_err()
    {
        return None;
    }
    Some(ProfileBackupRestoreResult {
        content_hash,
        archive,
    })
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

struct ParsedBackupFrame<'a> {
    snapshot: [u8; 16],
    index: usize,
    count: usize,
    total_len: usize,
    data: &'a [u8],
}

fn parse_backup_frame(payload: &[u8]) -> Option<ParsedBackupFrame<'_>> {
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
        || !(28..=MAX_BACKUP_PLAINTEXT_BYTES + 28).contains(&total_len)
        || payload.len() > ghost_core::MAX_GHOST_TX_PAYLOAD
    {
        return None;
    }
    Some(ParsedBackupFrame {
        snapshot,
        index,
        count,
        total_len,
        data: &payload[BACKUP_HEADER_BYTES..],
    })
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

#[cfg(test)]
#[cfg(test)]
mod tests;
