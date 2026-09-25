use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use ghost_api::{
    BackupContact, BackupMessage, ProfileBackupArchive, ProfileBackupPublishResult,
    ProfileBackupRestoreResult,
};
use ghost_kaspa::wallet::{WalletPublic, WalletSecret};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use zeroize::Zeroize;

use super::{
    kaspa::profile_portal,
    support::util::{decimal, open_wallet_secret, required, required_str, to_value},
};

const MAGIC: &[u8; 4] = b"GTBK";
const VERSION: u8 = 1;
const HEADER: usize = 29;
const AAD: &[u8] = b"GhostTalk/KaspaProfileBackup/v1\0";
const MAX_PLAINTEXT: usize = 1024 * 1024;
const MAX_FRAGMENTS: usize = 16;

pub(super) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "profile_backup_publish" => publish(args).await,
        "profile_backup_restore" => restore(args).await,
        _ => Err(format!("unknown browser profile-backup command: {command}")),
    }
}

async fn publish(args: &Value) -> Result<Value, String> {
    let profile_id = required_str(args, "profileId")?;
    let password = required_str(args, "password")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let projection: crate::model::WalletProjection = required(args, "public")?;
    let mut public = WalletPublic::from_projection(&projection);
    let contacts: Vec<BackupContact> = required(args, "contacts")?;
    let messages: Vec<BackupMessage> = required(args, "messages")?;
    ghost_api::validate_profile_backup_inputs(&contacts, &messages)?;
    let secret = open_wallet_secret(password, &sealed, &public)?;
    let archive = ProfileBackupArchive {
        version: VERSION,
        saved_at_ms: js_sys::Date::now().max(0.0) as u64,
        contacts,
        messages,
    };
    let plaintext = serde_json::to_vec(&archive).map_err(|error| format!("profile backup encode: {error}"))?;
    if plaintext.len() > MAX_PLAINTEXT { return Err("profile backup exceeds the 1 MiB encrypted archive limit".into()); }
    let content_hash = hex::encode(Sha256::digest(&plaintext));
    let encrypted = encrypt(&secret, &plaintext)?;
    let frames = fragment(&encrypted)?;
    let fee = decimal(required_str(args, "feeSompi")?, "backup fee")?;
    let endpoint = args.get("wrpcEndpoint").and_then(Value::as_str).filter(|value| !value.trim().is_empty());
    let portal = profile_portal(profile_id, &public, endpoint).await?;
    let destination = public.receive_address()?.to_owned();
    let mut transaction_ids = Vec::with_capacity(frames.len());
    for frame in frames {
        let sent = ghost_kaspa::wallet::send_payload(&portal, &secret, &public, &destination, fee, &frame).await?;
        public = sent.public;
        transaction_ids.push(sent.transaction_id);
    }
    to_value(ProfileBackupPublishResult { transaction_ids, content_hash, public: public.projection() })
}

async fn restore(args: &Value) -> Result<Value, String> {
    let password = required_str(args, "password")?;
    let sealed: Vec<u8> = required(args, "sealed")?;
    let projection: crate::model::WalletProjection = required(args, "public")?;
    let public = WalletPublic::from_projection(&projection);
    let secret = open_wallet_secret(password, &sealed, &public)?;
    let addresses = public.all_addresses().cloned().collect::<Vec<_>>();
    let now = js_sys::Date::now().max(0.0) as u64;
    let cutoff = now.saturating_sub(48 * 60 * 60 * 1_000);
    let transactions = super::runtime::history::backup_payloads(&public.network, &addresses, cutoff).await?;
    to_value(restore_newest(&secret, collect_candidates(transactions)))
}

fn encrypt(secret: &WalletSecret, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let mut key = ghost_kaspa::wallet::profile_backup_key(secret)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "profile backup key initialization failed")?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher.encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: AAD })
        .map_err(|_| "profile backup encryption failed".to_string());
    key.zeroize();
    let mut out = nonce.to_vec();
    out.extend_from_slice(&encrypted?);
    Ok(out)
}

fn decrypt(secret: &WalletSecret, encrypted: &[u8]) -> Result<Vec<u8>, String> {
    if encrypted.len() < 28 || encrypted.len() > MAX_PLAINTEXT + 28 { return Err("profile backup ciphertext length is invalid".into()); }
    let mut key = ghost_kaspa::wallet::profile_backup_key(secret)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "profile backup key initialization failed")?;
    let result = cipher.decrypt(Nonce::from_slice(&encrypted[..12]), Payload { msg: &encrypted[12..], aad: AAD })
        .map_err(|_| "profile backup authentication failed".to_string());
    key.zeroize();
    result
}

fn fragment(encrypted: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let chunk = ghost_core::MAX_GHOST_TX_PAYLOAD.checked_sub(HEADER).ok_or("Ghost Talk payload ceiling is smaller than backup header")?;
    let count = encrypted.len().div_ceil(chunk);
    if count == 0 || count > MAX_FRAGMENTS || count > usize::from(u16::MAX) { return Err("encrypted profile backup requires too many Kaspa transactions".into()); }
    let total = u32::try_from(encrypted.len()).map_err(|_| "encrypted profile backup is too large")?;
    let mut snapshot = [0u8; 16]; OsRng.fill_bytes(&mut snapshot);
    Ok(encrypted.chunks(chunk).enumerate().map(|(index, part)| {
        let mut frame = Vec::with_capacity(HEADER + part.len());
        frame.extend_from_slice(MAGIC); frame.push(VERSION); frame.extend_from_slice(&snapshot);
        frame.extend_from_slice(&(index as u16).to_le_bytes()); frame.extend_from_slice(&(count as u16).to_le_bytes());
        frame.extend_from_slice(&total.to_le_bytes()); frame.extend_from_slice(part); frame
    }).collect())
}

#[derive(Default)]
struct Candidate { count: usize, total: usize, parts: HashMap<usize, Vec<u8>>, max_blue: u64 }
struct Frame<'a> { snapshot: [u8;16], index: usize, count: usize, total: usize, data: &'a [u8] }

fn parse_frame(payload: &[u8]) -> Option<Frame<'_>> {
    if payload.len() < HEADER || &payload[..4] != MAGIC || payload[4] != VERSION { return None; }
    let snapshot = payload[5..21].try_into().ok()?;
    let index = usize::from(u16::from_le_bytes(payload[21..23].try_into().ok()?));
    let count = usize::from(u16::from_le_bytes(payload[23..25].try_into().ok()?));
    let total = usize::try_from(u32::from_le_bytes(payload[25..29].try_into().ok()?)).ok()?;
    if count == 0 || count > MAX_FRAGMENTS || index >= count || !(28..=MAX_PLAINTEXT+28).contains(&total) { return None; }
    Some(Frame { snapshot, index, count, total, data: &payload[HEADER..] })
}

fn collect_candidates(transactions: Vec<(u64, Vec<u8>)>) -> Vec<Candidate> {
    let mut map = HashMap::<[u8;16], Candidate>::new();
    for (blue, payload) in transactions {
        let Some(frame) = parse_frame(&payload) else { continue };
        let conflict = map.get(&frame.snapshot).is_some_and(|c| c.count != 0 && (c.count != frame.count || c.total != frame.total));
        if conflict { map.remove(&frame.snapshot); continue; }
        let c = map.entry(frame.snapshot).or_default(); c.count=frame.count; c.total=frame.total; c.max_blue=c.max_blue.max(blue); c.parts.entry(frame.index).or_insert(frame.data.to_vec());
    }
    let mut values = map.into_values().collect::<Vec<_>>(); values.sort_by_key(|c| std::cmp::Reverse(c.max_blue)); values
}

fn restore_newest(secret: &WalletSecret, candidates: Vec<Candidate>) -> Option<ProfileBackupRestoreResult> {
    candidates.into_iter().find_map(|candidate| {
        if candidate.count == 0 || candidate.parts.len() != candidate.count { return None; }
        let mut encrypted = Vec::with_capacity(candidate.total);
        for index in 0..candidate.count { encrypted.extend_from_slice(candidate.parts.get(&index)?); }
        if encrypted.len() != candidate.total { return None; }
        let mut plaintext = decrypt(secret, &encrypted).ok()?;
        let decoded = serde_json::from_slice::<ProfileBackupArchive>(&plaintext).ok();
        let content_hash = hex::encode(Sha256::digest(&plaintext)); plaintext.zeroize();
        let archive = decoded?;
        (archive.version == VERSION && ghost_api::validate_profile_backup_inputs(&archive.contacts, &archive.messages).is_ok())
            .then_some(ProfileBackupRestoreResult { content_hash, archive })
    })
}
