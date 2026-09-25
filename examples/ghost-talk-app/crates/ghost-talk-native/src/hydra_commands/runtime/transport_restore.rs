use aes_gcm::aead::Payload;
use aes_gcm::{aead::Aead, KeyInit};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;

use super::super::{
    mailbox_dispatch::decode_fixed_hex,
    runtime_state::HydraProfileRuntime,
    session_types::{
        make_kktp_binding, Aes256Gcm, KktpSessionBinding, KktpSessionState, Nonce,
        PersistentTransportState, MAX_PERSISTENT_TRANSPORT_STATE_BYTES,
        MAX_PREPARED_KKTP_DELIVERIES, MAX_RETIRED_KKTP_SIDS, PERSISTENT_TRANSPORT_STATE_MAGIC,
    },
    transport_persistence::{
        parse_persisted_role, persistent_transport_aad, transport_state_backup_path,
        PersistedTransportSession, PersistedTransportVault,
    },
};

fn transport_state_source(runtime: &HydraProfileRuntime) -> Option<PathBuf> {
    if runtime.transport_state_path.exists() {
        return Some(runtime.transport_state_path.clone());
    }
    let backup = transport_state_backup_path(&runtime.transport_state_path);
    backup.exists().then_some(backup)
}

fn decrypt_transport_vault(
    runtime: &HydraProfileRuntime,
    source: &Path,
) -> Result<PersistedTransportVault, String> {
    let blob =
        fs::read(source).map_err(|error| format!("read persistent transport state: {error}"))?;
    let header = PERSISTENT_TRANSPORT_STATE_MAGIC.len();
    let too_large = blob.len() > MAX_PERSISTENT_TRANSPORT_STATE_BYTES + header + 64;
    if blob.len() <= header + 12 || too_large {
        return Err("persistent transport state container size is invalid".into());
    }
    if !blob.starts_with(PERSISTENT_TRANSPORT_STATE_MAGIC) {
        return Err("persistent transport state magic is invalid".into());
    }
    let cipher = Aes256Gcm::new_from_slice(runtime.transport_state_key.as_ref())
        .map_err(|_| "persistent transport state key has invalid length".to_string())?;
    let aad = persistent_transport_aad(&runtime.identity_id);
    let mut plaintext = cipher
        .decrypt(
            Nonce::from_slice(&blob[header..header + 12]),
            Payload {
                msg: &blob[header + 12..],
                aad: &aad,
            },
        )
        .map_err(|_| "persistent transport state authentication failed".to_string())?;
    let decoded = serde_json::from_slice(&plaintext)
        .map_err(|error| format!("decode persistent transport state: {error}"));
    plaintext.zeroize();
    decoded
}

fn validate_transport_vault(
    runtime: &HydraProfileRuntime,
    vault: &PersistedTransportVault,
) -> Result<(), String> {
    if vault.version != 1 || vault.identity_id != runtime.identity_id {
        return Err("persistent transport state belongs to another Ghost Talk identity".into());
    }
    if vault.sessions.len() > 4096 || vault.prepared_deliveries.len() > MAX_PREPARED_KKTP_DELIVERIES
    {
        return Err("persistent transport state exceeds configured session limits".into());
    }
    Ok(())
}

fn restore_transport_session(
    runtime: &HydraProfileRuntime,
    session: PersistedTransportSession,
) -> Result<Option<(String, KktpSessionBinding)>, String> {
    if !runtime.hydra.has_contact(&session.peer_hydra_id)? {
        return Ok(None);
    }
    decode_fixed_hex::<32>(&session.peer_hydra_id, "persistent transport peer HYDRA id")?;
    decode_fixed_hex::<16>(&session.sid, "persistent transport KKTP SID")?;
    let role = parse_persisted_role(&session.role)?;
    let mut binding = make_kktp_binding(
        &runtime.identity_id,
        &session.peer_hydra_id,
        session.sid,
        role,
        KktpSessionState::Active,
    )?;
    binding.send_seq = session.send_seq;
    binding.recv_next_seq = session.recv_next_seq;
    binding.persistent_transport = Some(PersistentTransportState {
        send_chain_key: decode_fixed_hex::<32>(
            &session.send_chain_key_hex,
            "persistent send chain key",
        )?,
        recv_chain_key: decode_fixed_hex::<32>(
            &session.recv_chain_key_hex,
            "persistent receive chain key",
        )?,
        realtime_send_key: decode_fixed_hex::<32>(
            &session.realtime_send_key_hex,
            "persistent realtime send key",
        )?,
        realtime_recv_key: decode_fixed_hex::<32>(
            &session.realtime_recv_key_hex,
            "persistent realtime receive key",
        )?,
    });
    Ok(Some((session.peer_hydra_id, binding)))
}

fn restore_transport_vault(
    runtime: &mut HydraProfileRuntime,
    vault: PersistedTransportVault,
) -> Result<usize, String> {
    let mut restored = HashMap::new();
    for session in vault.sessions {
        if let Some((peer, binding)) = restore_transport_session(runtime, session)? {
            restored.insert(peer, binding);
        }
    }
    runtime.retired_kktp_sids = vault
        .retired_sids
        .into_iter()
        .filter(|sid| decode_fixed_hex::<16>(sid, "persistent retired SID").is_ok())
        .take(MAX_RETIRED_KKTP_SIDS)
        .collect();
    runtime.pending_contact_request_sids = vault
        .pending_contact_request_sids
        .into_iter()
        .filter_map(normalized_persisted_sid)
        .take(4096)
        .collect();
    runtime.prepared_kktp_deliveries = vault
        .prepared_deliveries
        .into_iter()
        .filter(|(_, delivery)| restored.contains_key(&delivery.contact_id))
        .collect();
    let restored_count = restored.len();
    runtime.kktp_sessions = restored;
    Ok(restored_count)
}

fn normalized_persisted_sid(sid: String) -> Option<String> {
    let normalized = sid.to_ascii_lowercase();
    decode_fixed_hex::<16>(&normalized, "persistent pending contact-request SID")
        .ok()
        .map(|_| normalized)
}

pub(crate) fn load_transport_state(runtime: &mut HydraProfileRuntime) -> Result<(), String> {
    let Some(source) = transport_state_source(runtime) else {
        return Ok(());
    };
    let vault = decrypt_transport_vault(runtime, &source)?;
    validate_transport_vault(runtime, &vault)?;
    let restored_count = restore_transport_vault(runtime, vault)?;
    crate::debug_log::record(
        "info",
        "handshake",
        "persistent-transports-restored",
        format!("identity={} sessions={restored_count}", runtime.identity_id),
    );
    Ok(())
}
