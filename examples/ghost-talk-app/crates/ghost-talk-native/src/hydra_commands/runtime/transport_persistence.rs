use aes_gcm::aead::Payload;
use aes_gcm::{aead::Aead, KeyInit};
use rand::RngCore;
use sha2::Digest;
use std::{
    collections::{HashMap, HashSet},
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;

use super::super::{
    runtime_state::HydraProfileRuntime,
    session_types::{
        Aes256Gcm, Deserialize, KktpRole, KktpSessionBinding, KktpSessionState, Nonce, OsRng,
        PersistentTransportState, Serialize, Sha256, MAX_PERSISTENT_TRANSPORT_STATE_BYTES,
        PERSISTENT_TRANSPORT_STATE_FILE, PERSISTENT_TRANSPORT_STATE_MAGIC,
    },
};

#[derive(Serialize, Deserialize)]
pub(crate) struct PersistedTransportVault {
    pub(crate) version: u16,
    pub(crate) identity_id: String,
    pub(crate) sessions: Vec<PersistedTransportSession>,
    #[serde(default)]
    pub(crate) prepared_deliveries: HashMap<String, PreparedKktpDelivery>,
    #[serde(default)]
    pub(crate) retired_sids: Vec<String>,
    #[serde(default)]
    pub(crate) pending_contact_request_sids: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct PersistedTransportSession {
    pub(crate) peer_hydra_id: String,
    pub(crate) sid: String,
    pub(crate) role: String,
    pub(crate) send_seq: u64,
    pub(crate) recv_next_seq: u64,
    pub(crate) send_chain_key_hex: String,
    pub(crate) recv_chain_key_hex: String,
    pub(crate) realtime_send_key_hex: String,
    pub(crate) realtime_recv_key_hex: String,
}

pub(crate) fn transport_state_key_from_seed(identity_seed: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/PersistentTransport/storage-key/v1\0");
    hasher.update(identity_seed);
    hasher.finalize().into()
}

pub(crate) fn transport_state_path(profile_path: &Path) -> PathBuf {
    profile_path.join(PERSISTENT_TRANSPORT_STATE_FILE)
}

pub(crate) fn persistent_transport_kdf(seed: &[u8; 32], label: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/PersistentTransport/session/v1\0");
    hasher.update(label);
    hasher.update([0]);
    hasher.update(seed);
    hasher.finalize().into()
}

pub(crate) fn persistent_transport_from_seed(
    seed: &[u8; 32],
    role: KktpRole,
) -> PersistentTransportState {
    let (send_chain, recv_chain, realtime_send, realtime_recv): (&[u8], &[u8], &[u8], &[u8]) =
        match role {
            KktpRole::Initiator => (
                b"durable/AtoB",
                b"durable/BtoA",
                b"realtime/AtoB",
                b"realtime/BtoA",
            ),
            KktpRole::Responder => (
                b"durable/BtoA",
                b"durable/AtoB",
                b"realtime/BtoA",
                b"realtime/AtoB",
            ),
        };
    PersistentTransportState {
        send_chain_key: persistent_transport_kdf(seed, send_chain),
        recv_chain_key: persistent_transport_kdf(seed, recv_chain),
        realtime_send_key: persistent_transport_kdf(seed, realtime_send),
        realtime_recv_key: persistent_transport_kdf(seed, realtime_recv),
    }
}

pub(crate) fn persistent_message_key(chain_key: &[u8; 32]) -> [u8; 32] {
    persistent_transport_kdf(chain_key, b"message-key")
}

pub(crate) fn persistent_next_chain_key(chain_key: &[u8; 32]) -> [u8; 32] {
    persistent_transport_kdf(chain_key, b"next-chain")
}

pub(crate) fn persisted_role(role: KktpRole) -> &'static str {
    match role {
        KktpRole::Initiator => "initiator",
        KktpRole::Responder => "responder",
    }
}

pub(crate) fn parse_persisted_role(value: &str) -> Result<KktpRole, String> {
    match value {
        "initiator" => Ok(KktpRole::Initiator),
        "responder" => Ok(KktpRole::Responder),
        _ => Err("persistent transport state contains an invalid KKTP role".into()),
    }
}

pub(crate) fn persistent_transport_aad(identity_id: &str) -> Vec<u8> {
    let mut aad = PERSISTENT_TRANSPORT_STATE_MAGIC.to_vec();
    aad.extend_from_slice(identity_id.as_bytes());
    aad
}

pub(crate) fn transport_state_backup_path(path: &Path) -> PathBuf {
    path.with_extension("bak")
}

pub(crate) fn write_private_transport_state(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "persistent transport state path has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create persistent transport state directory: {error}"))?;
    let temp = path.with_extension("tmp");
    let backup = transport_state_backup_path(path);
    let _ = fs::remove_file(&temp);
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .map_err(|error| format!("create persistent transport state temp file: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("write persistent transport state: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync persistent transport state: {error}"))?;
    drop(file);

    // Windows cannot atomically rename over an existing destination. Preserve
    // the previous authenticated checkpoint as a recovery file until the new
    // checkpoint has been committed, so a crash never turns a routine restart
    // into a forced on-chain session re-handshake.
    if path.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup)
            .map_err(|error| format!("stage previous persistent transport state: {error}"))?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(format!("commit persistent transport state: {error}"));
    }
    let _ = fs::remove_file(&backup);
    Ok(())
}

pub(crate) fn persisted_session(binding: &KktpSessionBinding) -> Option<PersistedTransportSession> {
    let transport = binding.persistent_transport.as_ref()?;
    (binding.state == KktpSessionState::Active).then(|| PersistedTransportSession {
        peer_hydra_id: binding.peer_hydra_id.clone(),
        sid: binding.sid.clone(),
        role: persisted_role(binding.role).to_owned(),
        send_seq: binding.send_seq,
        recv_next_seq: binding.recv_next_seq,
        send_chain_key_hex: hex::encode(transport.send_chain_key),
        recv_chain_key_hex: hex::encode(transport.recv_chain_key),
        realtime_send_key_hex: hex::encode(transport.realtime_send_key),
        realtime_recv_key_hex: hex::encode(transport.realtime_recv_key),
    })
}

pub(crate) fn persist_transport_state(runtime: &HydraProfileRuntime) -> Result<(), String> {
    let vault = transport_vault(runtime);
    if transport_vault_is_empty(&vault) {
        return remove_transport_state_files(&runtime.transport_state_path);
    }
    let blob = encrypt_transport_vault(runtime, &vault)?;
    write_private_transport_state(&runtime.transport_state_path, &blob)
}

fn transport_vault(runtime: &HydraProfileRuntime) -> PersistedTransportVault {
    let sessions = runtime
        .kktp_sessions
        .values()
        .filter_map(persisted_session)
        .collect::<Vec<_>>();
    let active_peers = sessions
        .iter()
        .map(|session| session.peer_hydra_id.as_str())
        .collect::<HashSet<_>>();
    let prepared_deliveries = runtime
        .prepared_kktp_deliveries
        .iter()
        .filter(|(_, delivery)| active_peers.contains(delivery.contact_id.as_str()))
        .map(|(id, delivery)| (id.clone(), delivery.clone()))
        .collect::<HashMap<_, _>>();
    PersistedTransportVault {
        version: 1,
        identity_id: runtime.identity_id.clone(),
        sessions,
        prepared_deliveries,
        retired_sids: runtime.retired_kktp_sids.iter().cloned().collect(),
        pending_contact_request_sids: runtime
            .pending_contact_request_sids
            .iter()
            .cloned()
            .collect(),
    }
}

fn transport_vault_is_empty(vault: &PersistedTransportVault) -> bool {
    vault.sessions.is_empty()
        && vault.prepared_deliveries.is_empty()
        && vault.retired_sids.is_empty()
        && vault.pending_contact_request_sids.is_empty()
}

fn remove_transport_state_files(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("remove empty persistent transport state: {error}"))?;
    }
    let backup = transport_state_backup_path(path);
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("remove empty persistent transport backup: {error}"))?;
    }
    Ok(())
}

fn encrypt_transport_vault(
    runtime: &HydraProfileRuntime,
    vault: &PersistedTransportVault,
) -> Result<Vec<u8>, String> {
    let mut plaintext = serde_json::to_vec(vault)
        .map_err(|error| format!("encode persistent transport state: {error}"))?;
    if plaintext.len() > MAX_PERSISTENT_TRANSPORT_STATE_BYTES {
        plaintext.zeroize();
        return Err("persistent transport state exceeds its configured size limit".into());
    }
    let result = encrypt_transport_plaintext(runtime, &plaintext);
    plaintext.zeroize();
    result
}

fn encrypt_transport_plaintext(
    runtime: &HydraProfileRuntime,
    plaintext: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(runtime.transport_state_key.as_ref())
        .map_err(|_| "persistent transport state key has invalid length".to_string())?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let aad = persistent_transport_aad(&runtime.identity_id);
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| "encrypt persistent transport state failed".to_string())?;
    let mut blob =
        Vec::with_capacity(PERSISTENT_TRANSPORT_STATE_MAGIC.len() + nonce.len() + encrypted.len());
    blob.extend_from_slice(PERSISTENT_TRANSPORT_STATE_MAGIC);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&encrypted);
    Ok(blob)
}
use super::super::delivery_state::PreparedKktpDelivery;
