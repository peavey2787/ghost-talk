use super::wallet_commands::WalletRuntimeState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_kaspa::wallet::WalletPublic;
use kassigner_sdk::{AddressBranch, KasSigner, Network, SigningRequest};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::{collections::HashMap, sync::Mutex, time::{SystemTime, UNIX_EPOCH}};
use tauri::State;

const MAX_CAMERA_PIXELS: usize = 1_280 * 960;


fn decode_camera_luminance(width: usize, height: usize, encoded: &str) -> Result<Vec<u8>, String> {
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| "KasSigner camera dimensions overflow".to_string())?;
    if width < 64 || height < 64 || pixels > MAX_CAMERA_PIXELS {
        return Err("KasSigner camera frame dimensions are invalid".into());
    }
    let max_encoded = pixels
        .checked_add(2)
        .and_then(|value| value.checked_div(3))
        .and_then(|value| value.checked_mul(4))
        .and_then(|value| value.checked_add(4))
        .ok_or_else(|| "KasSigner camera payload length overflow".to_string())?;
    if encoded.len() > max_encoded {
        return Err("KasSigner camera frame payload is too large".into());
    }
    let luminance = BASE64
        .decode(encoded)
        .map_err(|_| "KasSigner camera frame payload is not valid base64".to_string())?;
    if luminance.len() != pixels {
        return Err("KasSigner camera frame dimensions do not match its luminance payload".into());
    }
    Ok(luminance)
}

fn parse_portal_derivation_index(value: &serde_json::Value) -> Result<u32, String> {
    match value {
        serde_json::Value::String(text) => text
            .parse::<u32>()
            .map_err(|error| format!("invalid Kaspa Portal derivation index: {error}")),
        serde_json::Value::Number(number) => number
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| "Kaspa Portal derivation index exceeds u32".to_string()),
        _ => Err("Kaspa Portal derivation index must be an unsigned integer".to_string()),
    }
}

fn portal_output_derivations(pskt_hex: &str) -> Result<Vec<(usize, AddressBranch, u32)>, String> {
    let wire = hex::decode(pskt_hex).map_err(|error| format!("PSKT outer hex: {error}"))?;
    if wire.len() < 4 {
        return Err("PSKT payload is too short".into());
    }
    let is_pskb = &wire[..4] == b"PSKB";
    let is_pskt = &wire[..4] == b"PSKT";
    if !is_pskb && !is_pskt {
        return Err("KasSigner input is not a PSKT/PSKB payload".into());
    }
    let json = hex::decode(&wire[4..]).map_err(|error| format!("PSKT inner hex: {error}"))?;
    let root: serde_json::Value =
        serde_json::from_slice(&json).map_err(|error| format!("PSKT JSON: {error}"))?;
    let document = if is_pskb {
        let entries = root
            .as_array()
            .ok_or_else(|| "PSKB root must be an array".to_string())?;
        if entries.len() != 1 {
            return Err(format!("PSKB must contain exactly one transaction, got {}", entries.len()));
        }
        &entries[0]
    } else {
        &root
    };
    let outputs = document
        .get("outputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "PSKT outputs are missing".to_string())?;
    let mut derivations = Vec::new();
    for (output_index, output) in outputs.iter().enumerate() {
        let Some(raw_hint) = output
            .get("proprietaries")
            .and_then(|value| value.get("kaspaPortalDerivation"))
        else {
            continue;
        };
        let hint = raw_hint
            .as_object()
            .ok_or_else(|| format!("output[{output_index}] Kaspa Portal derivation must be an object"))?;
        let branch_code = hint
            .get("branch")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| format!("output[{output_index}] Kaspa Portal derivation branch is invalid"))?;
        let branch = AddressBranch::from_code(branch_code)
            .map_err(|error| format!("output[{output_index}] Kaspa Portal derivation branch: {error}"))?;
        let index = parse_portal_derivation_index(
            hint.get("index")
                .ok_or_else(|| format!("output[{output_index}] Kaspa Portal derivation index is missing"))?,
        )?;
        derivations.push((output_index, branch, index));
    }
    Ok(derivations)
}

fn canonical_kassigner_public(
    public: &WalletPublic,
    expected_account_fingerprint: &str,
    network: Network,
) -> Result<WalletPublic, String> {
    if !public.watch_only {
        return Err("KasSigner financial wallet must remain watch-only".into());
    }
    if public.network != network.to_string() {
        return Err("KasSigner wallet network does not match signing network".into());
    }
    let kpub = public
        .kpub
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "KasSigner watch-only wallet is missing its account kpub".to_string())?;
    let paired = kassigner_sdk::pair_normal(kpub, network).map_err(|error| error.to_string())?;
    if paired.account_fingerprint != expected_account_fingerprint {
        return Err("KasSigner profile fingerprint does not match its stored account kpub".into());
    }
    let canonical_kpub = paired
        .account_kpub
        .ok_or_else(|| "KasSigner pairing did not return an account kpub".to_string())?;
    let receive_addresses = paired
        .receive_addresses
        .into_iter()
        .map(|item| item.address)
        .collect::<Vec<_>>();
    let change_addresses = paired
        .change_addresses
        .into_iter()
        .map(|item| item.address)
        .collect::<Vec<_>>();
    if public.next_receive_index >= receive_addresses.len() {
        return Err("KasSigner receive-address cursor exceeds the authoritative wallet-derived range".into());
    }
    if public.next_change_index >= change_addresses.len() {
        return Err("KasSigner change-address cursor exceeds the authoritative wallet-derived range".into());
    }
    Ok(WalletPublic {
        network: network.to_string(),
        account_path: "KasSigner account kpub".to_string(),
        kpub: Some(canonical_kpub),
        watch_only: true,
        receive_addresses,
        change_addresses,
        next_receive_index: public.next_receive_index,
        next_change_index: public.next_change_index,
    })
}

fn canonical_ghost_public_from_secret(
    secret: &ghost_kaspa::wallet::WalletSecret,
    public: &WalletPublic,
) -> Result<WalletPublic, String> {
    super::wallet_commands::validate_public_projection(secret, public)?;
    let mut canonical = ghost_kaspa::wallet::derive_public(secret)?;
    if public.next_receive_index >= canonical.receive_addresses.len() {
        return Err("Ghost Talk receive-address cursor exceeds the seed-derived watch range".into());
    }
    if public.next_change_index >= canonical.change_addresses.len() {
        return Err("Ghost Talk change-address cursor exceeds the seed-derived watch range".into());
    }
    canonical.next_receive_index = public.next_receive_index;
    canonical.next_change_index = public.next_change_index;
    Ok(canonical)
}

fn canonical_ghost_public(
    wallet_state: &WalletRuntimeState,
    profile_id: &str,
    password: &str,
    sealed: &[u8],
    public: &WalletPublic,
) -> Result<WalletPublic, String> {
    if public.watch_only {
        return Err("Ghost-created KasSigner signing requires a software-backed wallet projection".into());
    }
    let secret = wallet_state.secret_or_open(profile_id, password, sealed, public)?;
    canonical_ghost_public_from_secret(&secret, public)
}

fn pskt_output_script(pskt_hex: &str, output_index: usize) -> Result<Vec<u8>, String> {
    let wire = hex::decode(pskt_hex).map_err(|error| format!("PSKT outer hex: {error}"))?;
    if wire.len() < 4 {
        return Err("PSKT payload is too short".into());
    }
    let is_pskb = &wire[..4] == b"PSKB";
    let is_pskt = &wire[..4] == b"PSKT";
    if !is_pskb && !is_pskt {
        return Err("KasSigner input is not a PSKT/PSKB payload".into());
    }
    let json = hex::decode(&wire[4..]).map_err(|error| format!("PSKT inner hex: {error}"))?;
    let root: serde_json::Value =
        serde_json::from_slice(&json).map_err(|error| format!("PSKT JSON: {error}"))?;
    let document = if is_pskb {
        let entries = root
            .as_array()
            .ok_or_else(|| "PSKB root must be an array".to_string())?;
        if entries.len() != 1 {
            return Err(format!("PSKB must contain exactly one transaction, got {}", entries.len()));
        }
        &entries[0]
    } else {
        &root
    };
    let output = document
        .get("outputs")
        .and_then(serde_json::Value::as_array)
        .and_then(|outputs| outputs.get(output_index))
        .ok_or_else(|| format!("PSKT output[{output_index}] is missing"))?;
    let encoded = output
        .get("scriptPublicKey")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("PSKT output[{output_index}] scriptPublicKey is missing"))?;
    if encoded.len() < 4 {
        return Err(format!("PSKT output[{output_index}] scriptPublicKey is too short"));
    }
    let version = u16::from_str_radix(&encoded[..4], 16)
        .map_err(|error| format!("PSKT output[{output_index}] script version: {error}"))?;
    if version != 0 {
        return Err(format!(
            "KasSigner wallet-owned output[{output_index}] uses unsupported script version {version}"
        ));
    }
    hex::decode(&encoded[4..])
        .map_err(|error| format!("PSKT output[{output_index}] script hex: {error}"))
}

fn validate_portal_output_derivation(
    pskt_hex: &str,
    public: &WalletPublic,
    output_index: usize,
    branch: AddressBranch,
    index: u32,
) -> Result<(), String> {
    let addresses = match branch {
        AddressBranch::Receive => &public.receive_addresses,
        AddressBranch::Change => &public.change_addresses,
        _ => return Err("KasSigner output derivation branch is unsupported".into()),
    };
    let address = addresses
        .get(usize::try_from(index).map_err(|_| "KasSigner derivation index exceeds usize".to_string())?)
        .ok_or_else(|| format!(
            "Kaspa Portal output[{output_index}] derivation {}/{} exceeds the authoritative wallet-derived range",
            branch.code(),
            index
        ))?;
    let expected_script = kassigner_protocol::address_to_script_pubkey(address)
        .map_err(|error| format!("KasSigner wallet-derived output script: {error}"))?;
    let actual_script = pskt_output_script(pskt_hex, output_index)?;
    if actual_script != expected_script {
        return Err(format!(
            "Kaspa Portal output[{output_index}] derivation {}/{} does not match the authoritative wallet derivation; signing request refused",
            branch.code(),
            index
        ));
    }
    Ok(())
}

fn prepare_kassigner_request(
    pskt_hex: &str,
    public: &WalletPublic,
    network: Network,
) -> Result<SigningRequest, String> {
    let mut decorated = pskt_hex.to_owned();
    for (output_index, branch, index) in portal_output_derivations(pskt_hex)? {
        validate_portal_output_derivation(pskt_hex, public, output_index, branch, index)?;
        decorated = kassigner_sdk::attach_output_derivation(&decorated, output_index, branch, index)
            .map_err(|error| error.to_string())?;
    }
    kassigner_sdk::prepare(&decorated, network).map_err(|error| error.to_string())
}

#[derive(Clone)]
enum PendingKind {
    Send { destination: String, amount_sompi: u64 },
    Consolidate,
}

struct PendingHardwareTx {
    request: SigningRequest,
    decoder: KasSigner,
    public: WalletPublic,
    kind: PendingKind,
    wrpc_endpoint: Option<String>,
}

#[derive(Clone)]
struct PendingIdentityProof {
    canonical_kpub: String,
    account_fingerprint: String,
    network: String,
    hydra_identity_id: String,
    challenge: String,
    created_at_ms: u64,
}

#[derive(Default)]
pub struct KasSignerRuntimeState {
    pending: Mutex<HashMap<String, PendingHardwareTx>>,
    identity_proofs: Mutex<HashMap<String, PendingIdentityProof>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerWatchOnlyAccount {
    account_fingerprint: String,
    public: WalletPublic,
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerIdentityProofPrompt {
    proof_id: String,
    challenge: String,
    account_fingerprint: String,
    expires_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerIdentityOwnershipProof {
    version: u16,
    account_fingerprint: String,
    network: String,
    hydra_identity_id: String,
    challenge: String,
    signature_hex: String,
    message_hash_hex: String,
    verified_at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerIdentityProofQrScan {
    response_hex: String,
}

#[tauri::command]
pub fn kassigner_import_kpub(
    kpub: String,
    network: String,
) -> Result<KasSignerWatchOnlyAccount, String> {
    let network_value = parse_network(&network)?;
    let paired = kassigner_sdk::pair_normal(kpub.trim(), network_value)
        .map_err(|error| error.to_string())?;
    let canonical_kpub = paired
        .account_kpub
        .ok_or_else(|| "KasSigner descriptor pairing did not return an account kpub".to_string())?;
    let public = WalletPublic {
        network: network_value.to_string(),
        account_path: "KasSigner account kpub".to_string(),
        kpub: Some(canonical_kpub),
        watch_only: true,
        receive_addresses: paired.receive_addresses.into_iter().map(|item| item.address).collect(),
        change_addresses: paired.change_addresses.into_iter().map(|item| item.address).collect(),
        next_receive_index: 0,
        next_change_index: 0,
    };
    Ok(KasSignerWatchOnlyAccount {
        account_fingerprint: paired.account_fingerprint,
        public,
    })
}


#[tauri::command]
pub fn kassigner_begin_identity_proof(
    state: State<'_, KasSignerRuntimeState>,
    kpub: String,
    network: String,
    profile_id: String,
    hydra_identity_id: String,
) -> Result<KasSignerIdentityProofPrompt, String> {
    if profile_id.trim().is_empty() || profile_id.len() > 160 {
        return Err("Ghost Talk profile id is invalid for KasSigner ownership proof".into());
    }
    if hydra_identity_id.len() != 64 || !hydra_identity_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("HYDRA identity id must be exactly 64 hexadecimal characters".into());
    }
    let network_value = parse_network(&network)?;
    let paired = kassigner_sdk::pair_normal(kpub.trim(), network_value)
        .map_err(|error| error.to_string())?;
    let canonical_kpub = paired
        .account_kpub
        .ok_or_else(|| "KasSigner descriptor pairing did not return an account kpub".to_string())?;
    let created_at_ms = unix_time_ms()?;
    let mut nonce = [0u8; 16];
    OsRng.fill_bytes(&mut nonce);
    let challenge = format!(
        "Ghost Talk Kaspa identity ownership proof v1\nNetwork: {}\nAccount fingerprint: {}\nGhost Talk profile: {}\nHYDRA identity: {}\nNonce: {}\nCreated ms: {}\nOnly sign if you are creating this exact Ghost Talk identity.",
        network_value,
        paired.account_fingerprint,
        profile_id,
        hydra_identity_id.to_ascii_lowercase(),
        hex::encode(nonce),
        created_at_ms,
    );
    if challenge.len() > 1_024 {
        return Err("KasSigner ownership challenge exceeds the hardware message limit".into());
    }
    let proof_id = new_request_id();
    let expires_at_ms = created_at_ms.saturating_add(10 * 60 * 1_000);
    state
        .identity_proofs
        .lock()
        .map_err(|_| "KasSigner identity-proof state is poisoned".to_string())?
        .insert(
            proof_id.clone(),
            PendingIdentityProof {
                canonical_kpub,
                account_fingerprint: paired.account_fingerprint.clone(),
                network: network_value.to_string(),
                hydra_identity_id: hydra_identity_id.to_ascii_lowercase(),
                challenge: challenge.clone(),
                created_at_ms,
            },
        );
    Ok(KasSignerIdentityProofPrompt {
        proof_id,
        challenge,
        account_fingerprint: paired.account_fingerprint,
        expires_at_ms,
    })
}

#[tauri::command]
pub fn kassigner_scan_identity_proof_qr(
    state: State<'_, KasSignerRuntimeState>,
    proof_id: String,
    width: usize,
    height: usize,
    luminance_base64: String,
) -> Result<Option<KasSignerIdentityProofQrScan>, String> {
    if !state
        .identity_proofs
        .lock()
        .map_err(|_| "KasSigner identity-proof state is poisoned".to_string())?
        .contains_key(&proof_id)
    {
        return Err("KasSigner identity ownership proof is no longer pending".into());
    }
    let luminance = decode_camera_luminance(width, height, &luminance_base64)?;
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
        luminance[y * width + x]
    });
    for grid in image.detect_grids() {
        let mut raw = Vec::new();
        if grid.decode_to(&mut raw).is_err() || raw.len() != 96 {
            continue;
        }
        return Ok(Some(KasSignerIdentityProofQrScan { response_hex: hex::encode(raw) }));
    }
    Ok(None)
}

#[tauri::command]
pub fn kassigner_complete_identity_proof(
    state: State<'_, KasSignerRuntimeState>,
    proof_id: String,
    response_hex: String,
) -> Result<KasSignerIdentityOwnershipProof, String> {
    let pending = state
        .identity_proofs
        .lock()
        .map_err(|_| "KasSigner identity-proof state is poisoned".to_string())?
        .get(&proof_id)
        .cloned()
        .ok_or_else(|| "KasSigner identity ownership proof is no longer pending".to_string())?;
    let now = unix_time_ms()?;
    if now.saturating_sub(pending.created_at_ms) > 10 * 60 * 1_000 {
        state
            .identity_proofs
            .lock()
            .map_err(|_| "KasSigner identity-proof state is poisoned".to_string())?
            .remove(&proof_id);
        return Err("KasSigner identity ownership proof expired; start the proof again".into());
    }
    let response = hex::decode(response_hex.trim())
        .map_err(|_| "KasSigner ownership response must be hexadecimal".to_string())?;
    if response.len() != 96 {
        return Err("KasSigner Sign Message response must be exactly 96 bytes".into());
    }
    let signature: [u8; 64] = response[..64]
        .try_into()
        .map_err(|_| "KasSigner signature length is invalid".to_string())?;
    let message_hash: [u8; 32] = response[64..]
        .try_into()
        .map_err(|_| "KasSigner message hash length is invalid".to_string())?;
    let expected_hash = ghost_kaspa::kassigner_message_digest(pending.challenge.as_bytes());
    if message_hash != expected_hash {
        return Err("KasSigner signed a different message than the Ghost Talk ownership challenge".into());
    }
    let pubkey_x = account_xonly_from_kpub(&pending.canonical_kpub)?;
    ghost_kaspa::verify_kassigner_message_signature(
        &pubkey_x,
        pending.challenge.as_bytes(),
        &signature,
    )
    .map_err(|_| "KasSigner ownership signature does not match the imported account kpub".to_string())?;
    state
        .identity_proofs
        .lock()
        .map_err(|_| "KasSigner identity-proof state is poisoned".to_string())?
        .remove(&proof_id);
    Ok(KasSignerIdentityOwnershipProof {
        version: 1,
        account_fingerprint: pending.account_fingerprint,
        network: pending.network,
        hydra_identity_id: pending.hydra_identity_id,
        challenge: pending.challenge,
        signature_hex: hex::encode(signature),
        message_hash_hex: hex::encode(message_hash),
        verified_at_ms: now,
    })
}

#[tauri::command]
pub fn kassigner_cancel_identity_proof(
    state: State<'_, KasSignerRuntimeState>,
    proof_id: String,
) -> Result<(), String> {
    state
        .identity_proofs
        .lock()
        .map_err(|_| "KasSigner identity-proof state is poisoned".to_string())?
        .remove(&proof_id);
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerAccountQrScan {
    account_payload: String,
}

#[tauri::command]
pub fn kassigner_scan_account_qr(
    width: usize,
    height: usize,
    luminance_base64: String,
) -> Result<Option<KasSignerAccountQrScan>, String> {
    let luminance = decode_camera_luminance(width, height, &luminance_base64)?;
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
        luminance[y * width + x]
    });
    for grid in image.detect_grids() {
        let mut raw = Vec::new();
        if grid.decode_to(&mut raw).is_err() || raw.is_empty() {
            continue;
        }
        // pair_normal accepts canonical kpub text, raw 78-byte account payload encoded
        // as hex, or hex containing the textual kpub. Returning hex preserves the
        // exact binary QR payload without an unsafe UTF-8 assumption.
        return Ok(Some(KasSignerAccountQrScan { account_payload: hex::encode(raw) }));
    }
    Ok(None)
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerQrFrame {
    index: u8,
    total: u8,
    payload_hex: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerSigningPrompt {
    request_id: String,
    sdk_version: &'static str,
    qr_frames: Vec<KasSignerQrFrame>,
    max_inputs: u16,
    note: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerScanProgress {
    received: u8,
    total: u8,
    bits: Vec<bool>,
    response_hex: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KasSignerCompleteResult {
    status: &'static str,
    resign: Option<KasSignerSigningPrompt>,
    broadcast: Option<ghost_kaspa::wallet::BroadcastResult>,
    message: String,
}

fn parse_network(network: &str) -> Result<Network, String> {
    Network::parse(network).map_err(|error| error.to_string())
}

fn new_request_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn unix_time_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_string())?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "system clock exceeds supported range".to_string())
}

fn account_xonly_from_kpub(kpub: &str) -> Result<[u8; 32], String> {
    let payload = kassigner_protocol::decode_kpub_text(kpub).map_err(|error| error.to_string())?;
    if !matches!(payload[45], 0x02 | 0x03) {
        return Err("KasSigner account kpub has an invalid compressed public key".into());
    }
    payload[46..78]
        .try_into()
        .map_err(|_| "KasSigner account kpub public key length is invalid".to_string())
}

fn prompt(request_id: &str, request: &SigningRequest) -> KasSignerSigningPrompt {
    let limits = kassigner_sdk::limits();
    KasSignerSigningPrompt {
        request_id: request_id.to_owned(),
        sdk_version: kassigner_sdk::SDK_VERSION,
        qr_frames: request
            .qr_frames()
            .iter()
            .map(|frame| KasSignerQrFrame {
                index: frame.index,
                total: frame.total,
                payload_hex: hex::encode(&frame.payload),
            })
            .collect(),
        max_inputs: limits.max_inputs,
        note: "Ghost Talk maps Portal-owned output derivation hints into KasSigner metadata; the hardware independently verifies change ownership before signing.",
    }
}

fn install_pending(
    state: &KasSignerRuntimeState,
    request_id: String,
    request: SigningRequest,
    public: WalletPublic,
    kind: PendingKind,
    wrpc_endpoint: Option<String>,
) -> Result<KasSignerSigningPrompt, String> {
    let response = prompt(&request_id, &request);
    state
        .pending
        .lock()
        .map_err(|_| "KasSigner pending state is poisoned".to_string())?
        .insert(
            request_id,
            PendingHardwareTx {
                request,
                decoder: KasSigner::new(),
                public,
                kind,
                wrpc_endpoint,
            },
        );
    Ok(response)
}

#[tauri::command]
pub async fn kassigner_prepare_send(
    state: State<'_, KasSignerRuntimeState>,
    wallet_state: State<'_, WalletRuntimeState>,
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    public: WalletPublic,
    profile_id: Option<String>,
    password: Option<String>,
    sealed: Option<Vec<u8>>,
    account_fingerprint: Option<String>,
    destination: String,
    amount_sompi: String,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<KasSignerSigningPrompt, String> {
    let amount = super::wallet_commands::parse_u64_decimal(&amount_sompi, "amount")?;
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "fee")?;
    let network = parse_network(&public.network)?;
    let public = if public.watch_only {
        let fingerprint = account_fingerprint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "KasSigner watch-only signing requires the paired account fingerprint".to_string())?;
        canonical_kassigner_public(&public, fingerprint.trim(), network)?
    } else {
        let profile_id = profile_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Ghost-created KasSigner signing requires the Ghost Talk profile id".to_string())?;
        let sealed = sealed
            .as_deref()
            .ok_or_else(|| "Ghost-created KasSigner signing requires the encrypted wallet vault".to_string())?;
        canonical_ghost_public(
            &wallet_state,
            profile_id,
            password.as_deref().unwrap_or(""),
            sealed,
            &public,
        )?
    };
    let portal = gateway.portal(&public, wrpc_endpoint.as_deref()).await?;
    let pskb = match ghost_kaspa::wallet::plan_send_unsigned(&portal, &public, &destination, amount, fee).await {
        Ok(pskb) => pskb,
        Err(error) => {
            gateway.note_operation_error(&error).await;
            return Err(error);
        }
    };
    let request = prepare_kassigner_request(&pskb, &public, network)?;
    let request_id = new_request_id();
    install_pending(
        &state,
        request_id,
        request,
        public,
        PendingKind::Send { destination, amount_sompi: amount },
        wrpc_endpoint,
    )
}

#[tauri::command]
pub async fn kassigner_prepare_consolidation(
    state: State<'_, KasSignerRuntimeState>,
    wallet_state: State<'_, WalletRuntimeState>,
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    public: WalletPublic,
    profile_id: Option<String>,
    password: Option<String>,
    sealed: Option<Vec<u8>>,
    account_fingerprint: Option<String>,
    fee_sompi: String,
    wrpc_endpoint: Option<String>,
) -> Result<KasSignerSigningPrompt, String> {
    let fee = super::wallet_commands::parse_u64_decimal(&fee_sompi, "fee")?;
    let network = parse_network(&public.network)?;
    let public = if public.watch_only {
        let fingerprint = account_fingerprint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "KasSigner watch-only signing requires the paired account fingerprint".to_string())?;
        canonical_kassigner_public(&public, fingerprint.trim(), network)?
    } else {
        let profile_id = profile_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "Ghost-created KasSigner signing requires the Ghost Talk profile id".to_string())?;
        let sealed = sealed
            .as_deref()
            .ok_or_else(|| "Ghost-created KasSigner signing requires the encrypted wallet vault".to_string())?;
        canonical_ghost_public(
            &wallet_state,
            profile_id,
            password.as_deref().unwrap_or(""),
            sealed,
            &public,
        )?
    };
    let portal = gateway.portal(&public, wrpc_endpoint.as_deref()).await?;
    let pskb = match ghost_kaspa::wallet::plan_consolidation_unsigned(&portal, &public, fee).await {
        Ok(pskb) => pskb,
        Err(error) => {
            gateway.note_operation_error(&error).await;
            return Err(error);
        }
    };
    let request = prepare_kassigner_request(&pskb, &public, network)?;
    let request_id = new_request_id();
    install_pending(
        &state,
        request_id,
        request,
        public,
        PendingKind::Consolidate,
        wrpc_endpoint,
    )
}

#[tauri::command]
pub fn kassigner_scan_response_frame(
    state: State<'_, KasSignerRuntimeState>,
    request_id: String,
    width: usize,
    height: usize,
    luminance_base64: String,
) -> Result<KasSignerScanProgress, String> {
    let luminance = decode_camera_luminance(width, height, &luminance_base64)?;
    let mut pending = state
        .pending
        .lock()
        .map_err(|_| "KasSigner pending state is poisoned".to_string())?;
    let pending = pending
        .get_mut(&request_id)
        .ok_or_else(|| "KasSigner signing request is no longer pending".to_string())?;
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
        luminance[y * width + x]
    });
    let grids = image.detect_grids();
    for grid in grids {
        let mut raw = Vec::new();
        if grid.decode_to(&mut raw).is_err() || raw.is_empty() {
            continue;
        }
        if let Some(response) = pending
            .decoder
            .accept_qr_frame(&raw)
            .map_err(|error| error.to_string())?
        {
            let progress = pending.decoder.qr_decoder_progress();
            return Ok(KasSignerScanProgress {
                received: progress.received,
                total: progress.total,
                bits: progress.bits,
                response_hex: Some(hex::encode(response)),
            });
        }
    }
    let progress = pending.decoder.qr_decoder_progress();
    Ok(KasSignerScanProgress {
        received: progress.received,
        total: progress.total,
        bits: progress.bits,
        response_hex: None,
    })
}

#[tauri::command]
pub async fn kassigner_complete(
    state: State<'_, KasSignerRuntimeState>,
    gateway: State<'_, super::kaspa_gateway::KaspaGatewayState>,
    request_id: String,
    response_hex: String,
) -> Result<KasSignerCompleteResult, String> {
    let (request, public, kind, wrpc_endpoint) = {
        let pending = state
            .pending
            .lock()
            .map_err(|_| "KasSigner pending state is poisoned".to_string())?;
        let pending = pending
            .get(&request_id)
            .ok_or_else(|| "KasSigner signing request is no longer pending".to_string())?;
        (
            pending.request.clone(),
            pending.public.clone(),
            pending.kind.clone(),
            pending.wrpc_endpoint.clone(),
        )
    };
    let signed = kassigner_sdk::complete(&request, response_hex.trim())
        .map_err(|error| error.to_string())?;
    // Exercise the SDK finalization policy before handing the exact merged PSKB
    // back to Portal for typed broadcast. Portal remains the network owner.
    let _consensus_json = kassigner_sdk::finalize(&signed).map_err(|error| error.to_string())?;
    let portal = gateway.portal(&public, wrpc_endpoint.as_deref()).await?;
    let analysis = match ghost_kaspa::wallet::analyze_signed_pskb(&portal, &signed.pskt_hex).await {
        Ok(analysis) => analysis,
        Err(error) => {
            gateway.note_operation_error(&error).await;
            return Err(error);
        }
    };
    if !analysis.mass_valid {
        return Err("KasSigner-signed transaction exceeds Kaspa mass policy".into());
    }
    if !analysis.fee_sufficient {
        let fee = analysis.recommended_fee_sompi;
        let pskb = match &kind {
            PendingKind::Send { destination, amount_sompi } => {
                ghost_kaspa::wallet::plan_send_unsigned(
                    &portal,
                    &public,
                    destination,
                    *amount_sompi,
                    fee,
                )
                .await?
            }
            PendingKind::Consolidate => {
                ghost_kaspa::wallet::plan_consolidation_unsigned(&portal, &public, fee).await?
            }
        };
        let next_request = prepare_kassigner_request(&pskb, &public, parse_network(&public.network)?)?;
        let next_prompt = prompt(&request_id, &next_request);
        state
            .pending
            .lock()
            .map_err(|_| "KasSigner pending state is poisoned".to_string())?
            .insert(
                request_id,
                PendingHardwareTx {
                    request: next_request,
                    decoder: KasSigner::new(),
                    public,
                    kind,
                    wrpc_endpoint,
                },
            );
        return Ok(KasSignerCompleteResult {
            status: "resign",
            resign: Some(next_prompt),
            broadcast: None,
            message: format!(
                "Exact signed mass requires {} sompi fee. Review and sign the adjusted transaction once more on KasSigner.",
                fee
            ),
        });
    }
    let advance_change = matches!(kind, PendingKind::Send { .. });
    let result = match ghost_kaspa::wallet::broadcast_external_signed_pskb(
        &portal,
        &signed.pskt_hex,
        &public,
        advance_change,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            gateway.note_operation_error(&error).await;
            return Err(error);
        }
    };
    state
        .pending
        .lock()
        .map_err(|_| "KasSigner pending state is poisoned".to_string())?
        .remove(&request_id);
    Ok(KasSignerCompleteResult {
        status: "broadcast",
        resign: None,
        broadcast: Some(result),
        message: "KasSigner signature verified and transaction broadcast.".into(),
    })
}

#[tauri::command]
pub fn kassigner_cancel(
    state: State<'_, KasSignerRuntimeState>,
    request_id: String,
) -> Result<(), String> {
    state
        .pending
        .lock()
        .map_err(|_| "KasSigner pending state is poisoned".to_string())?
        .remove(&request_id);
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    fn pskb_hex(document: serde_json::Value) -> String {
        let json = serde_json::to_vec(&serde_json::json!([document])).expect("json");
        let mut wire = b"PSKB".to_vec();
        wire.extend_from_slice(hex::encode(json).as_bytes());
        hex::encode(wire)
    }

    fn decode_document(pskb: &str) -> serde_json::Value {
        let wire = hex::decode(pskb).expect("outer");
        let json = hex::decode(&wire[4..]).expect("inner");
        serde_json::from_slice::<serde_json::Value>(&json).expect("json")[0].clone()
    }

    fn raw_test_account_hex() -> String {
        let mut payload = [0u8; 78];
        payload[..4].copy_from_slice(&[0x03, 0x8f, 0x33, 0x2e]);
        payload[4] = 3;
        payload[9..13].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        payload[13..45].fill(0x11);
        payload[45..78].copy_from_slice(&[
            0x02, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0,
            0x62, 0x95, 0xce, 0x87, 0x0b, 0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d,
            0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8, 0x17, 0x98,
        ]);
        hex::encode(payload)
    }

    fn test_hardware_public() -> (WalletPublic, String) {
        let paired = kassigner_sdk::pair_normal(&raw_test_account_hex(), Network::Mainnet)
            .expect("pair test account");
        let public = WalletPublic {
            network: "mainnet".into(),
            account_path: "stale frontend label".into(),
            kpub: paired.account_kpub.clone(),
            watch_only: true,
            receive_addresses: vec!["kaspa:stale".into()],
            change_addresses: vec!["kaspa:stale".into()],
            next_receive_index: 0,
            next_change_index: 0,
        };
        (public, paired.account_fingerprint)
    }

    #[test]
    fn portal_derivation_translation_preserves_change_hint() {
        let original = pskb_hex(serde_json::json!({
            "global": {},
            "inputs": [],
            "outputs": [
                {"proprietaries": {}},
                {"proprietaries": {"kaspaPortalDerivation": {"branch": 1, "index": "37"}}}
            ]
        }));
        let mut translated = original.clone();
        for (output, branch, index) in portal_output_derivations(&original).expect("hints") {
            translated = kassigner_sdk::attach_output_derivation(&translated, output, branch, index)
                .expect("attach");
        }
        let document = decode_document(&translated);
        assert!(document["outputs"][0]["proprietaries"].get("kassignerDerivation").is_none());
        assert_eq!(
            document["outputs"][1]["proprietaries"]["kassignerDerivation"],
            serde_json::json!({"branch": 1, "index": "37"})
        );
        assert_eq!(
            document["outputs"][1]["proprietaries"]["kaspaPortalDerivation"],
            serde_json::json!({"branch": 1, "index": "37"})
        );
    }


    #[test]
    fn kassigner_public_projection_is_rebuilt_from_kpub_before_signing() {
        let (stale, fingerprint) = test_hardware_public();
        let canonical = canonical_kassigner_public(&stale, &fingerprint, Network::Mainnet)
            .expect("canonical hardware wallet");
        assert_eq!(canonical.receive_addresses.len(), 20);
        assert_eq!(canonical.change_addresses.len(), 20);
        assert_ne!(canonical.receive_addresses[0], "kaspa:stale");
        assert_ne!(canonical.change_addresses[0], "kaspa:stale");
        assert_eq!(canonical.next_receive_index, 0);
        assert_eq!(canonical.next_change_index, 0);
        assert!(canonical.watch_only);
        assert!(canonical_kassigner_public(&stale, "00", Network::Mainnet).is_err());
    }

    #[test]
    fn portal_change_hint_must_match_kpub_derived_output_script() {
        let (stale, fingerprint) = test_hardware_public();
        let public = canonical_kassigner_public(&stale, &fingerprint, Network::Mainnet)
            .expect("canonical hardware wallet");
        let script = kassigner_protocol::address_to_script_pubkey(&public.change_addresses[0])
            .expect("change script");
        let matching = pskb_hex(serde_json::json!({
            "global": {},
            "inputs": [],
            "outputs": [{
                "scriptPublicKey": format!("0000{}", hex::encode(&script)),
                "proprietaries": {"kaspaPortalDerivation": {"branch": 1, "index": "0"}}
            }]
        }));
        validate_portal_output_derivation(&matching, &public, 0, AddressBranch::Change, 0)
            .expect("matching hint");

        let wrong_script = kassigner_protocol::address_to_script_pubkey(&public.receive_addresses[0])
            .expect("receive script");
        let mismatched = pskb_hex(serde_json::json!({
            "global": {},
            "inputs": [],
            "outputs": [{
                "scriptPublicKey": format!("0000{}", hex::encode(&wrong_script)),
                "proprietaries": {"kaspaPortalDerivation": {"branch": 1, "index": "0"}}
            }]
        }));
        let error = validate_portal_output_derivation(
            &mismatched,
            &public,
            0,
            AddressBranch::Change,
            0,
        )
        .expect_err("mismatched hint must fail closed");
        assert!(error.contains("does not match the KasSigner account kpub"));
    }

    #[test]
    fn camera_luminance_base64_validates_exact_dimensions() {
        let bytes = vec![0x7fu8; 64 * 64];
        let encoded = BASE64.encode(&bytes);
        assert_eq!(decode_camera_luminance(64, 64, &encoded).expect("decode"), bytes);
        assert!(decode_camera_luminance(64, 65, &encoded).is_err());
        assert!(decode_camera_luminance(63, 64, &encoded).is_err());
    }

    #[test]
    fn portal_derivation_translation_rejects_malformed_hint() {
        let malformed = pskb_hex(serde_json::json!({
            "global": {},
            "inputs": [],
            "outputs": [{
                "proprietaries": {"kaspaPortalDerivation": {"branch": 1, "index": "not-a-number"}}
            }]
        }));
        assert!(portal_output_derivations(&malformed).is_err());
    }
}
