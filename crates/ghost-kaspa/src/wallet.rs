use kaspa_portal::{
    primitives::NetworkId,
    transaction::{
        interchange::{kspt, pskt},
        model::{SigHashType, Transaction},
    },
    wallet::{
        account::derivation::WalletData,
        derivation::bip32::{derive_address_key, derive_change_key, derive_path, ExtendedPrivKey},
        mnemonic::bip39,
    },
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Instant;
use zeroize::{Zeroize, ZeroizeOnDrop};

const ADDRESS_LOOKAHEAD: u32 = 64;
const HARDENED: u32 = 0x8000_0000;

pub const MAILBOX_OUTPUT_SOMPI: u64 = 10_000_000;

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct WalletSecret {
    pub mnemonic: String,
    #[serde(default)]
    pub passphrase: String,
    pub account_path: String,
    pub network: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletPublic {
    pub network: String,
    pub account_path: String,
    #[serde(default)]
    pub kpub: Option<String>,
    #[serde(default)]
    pub watch_only: bool,
    pub receive_addresses: Vec<String>,
    pub change_addresses: Vec<String>,
    pub next_receive_index: usize,
    pub next_change_index: usize,
}

impl WalletPublic {
    pub fn receive_address(&self) -> Result<&str, String> {
        self.receive_addresses
            .get(self.next_receive_index)
            .map(String::as_str)
            .ok_or_else(|| "receive-address lookahead exhausted".to_string())
    }

    pub fn change_address(&self) -> Result<&str, String> {
        self.change_addresses
            .get(self.next_change_index)
            .map(String::as_str)
            .ok_or_else(|| "change-address lookahead exhausted".to_string())
    }

    pub fn all_addresses(&self) -> impl Iterator<Item = &String> {
        self.receive_addresses.iter().chain(self.change_addresses.iter())
    }

    pub fn advance_receive(&mut self) -> Result<(), String> {
        if self.next_receive_index + 1 >= self.receive_addresses.len() {
            return Err("receive-address lookahead exhausted".into());
        }
        self.next_receive_index += 1;
        Ok(())
    }

    pub fn advance_change(&mut self) -> Result<(), String> {
        if self.next_change_index + 1 >= self.change_addresses.len() {
            return Err("change-address lookahead exhausted".into());
        }
        self.next_change_index += 1;
        Ok(())
    }

    pub fn portal_wallet(&self) -> WalletData {
        WalletData {
            kpub: self.kpub.clone().unwrap_or_default(),
            receive_addresses: self.receive_addresses.clone(),
            change_addresses: self.change_addresses.clone(),
            next_receive_index: self.next_receive_index,
            next_change_index: self.next_change_index,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreatedWallet {
    pub mnemonic: String,
    pub public: WalletPublic,
}

pub fn generate_wallet(
    passphrase: &str,
    account_path: &str,
    network: &str,
) -> Result<(WalletSecret, CreatedWallet), String> {
    let mut entropy = [0u8; 32];
    OsRng.fill_bytes(&mut entropy);
    let mnemonic = bip39::mnemonic_from_entropy_24(&entropy);
    entropy.zeroize();
    let mnemonic = mnemonic
        .indices
        .iter()
        .map(|index| bip39::index_to_word(*index))
        .collect::<Vec<_>>()
        .join(" ");
    import_wallet(&mnemonic, passphrase, account_path, network).map(|(secret, public)| {
        (
            secret,
            CreatedWallet {
                mnemonic,
                public,
            },
        )
    })
}

/// Derive the Ghost Talk HYDRA identity root from the exact BIP39 seed that
/// also feeds the Kaspa wallet. BLAKE3 derive-key mode provides explicit
/// domain separation so this 32-byte seed is independent from every BIP32
/// Kaspa child key while remaining deterministically recoverable from the
/// same 24 words and optional BIP39 passphrase.
pub fn hydra_identity_seed(secret: &WalletSecret) -> Result<[u8; 32], String> {
    let mut seed = mnemonic_seed(&secret.mnemonic, &secret.passphrase)?;
    let derived = blake3::derive_key("GhostTalk/HYDRA-ID/v1", &seed);
    seed.zeroize();
    Ok(derived)
}

pub fn import_wallet(
    mnemonic: &str,
    passphrase: &str,
    account_path: &str,
    network: &str,
) -> Result<(WalletSecret, WalletPublic), String> {
    let normalized_mnemonic = mnemonic.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut seed = mnemonic_seed(&normalized_mnemonic, passphrase)?;
    let path = parse_account_path(account_path)?;
    let account_result =
        derive_path(&seed, &path).map_err(|error| format!("BIP32 derivation: {error:?}"));
    seed.zeroize();
    let account = account_result?;
    let network_id = NetworkId::parse(network)?;
    let public = project_account(&account, account_path, network_id)?;
    Ok((
        WalletSecret {
            mnemonic: normalized_mnemonic,
            passphrase: passphrase.to_owned(),
            account_path: account_path.trim().to_owned(),
            network: network_id.canonical_name(),
        },
        public,
    ))
}

pub fn derive_public(secret: &WalletSecret) -> Result<WalletPublic, String> {
    import_wallet(
        &secret.mnemonic,
        &secret.passphrase,
        &secret.account_path,
        &secret.network,
    )
    .map(|(_, public)| public)
}

pub fn account_key(secret: &WalletSecret) -> Result<ExtendedPrivKey, String> {
    let mut seed = mnemonic_seed(&secret.mnemonic, &secret.passphrase)?;
    let path = parse_account_path(&secret.account_path)?;
    let result = derive_path(&seed, &path).map_err(|error| format!("BIP32 derivation: {error:?}"));
    seed.zeroize();
    result
}

/// Derive one external-chain private key for signing a GTCD bound to the
/// corresponding Kaspa P2PK address. The master/account key never leaves this
/// Rust boundary.
pub fn receive_private_key(secret: &WalletSecret, index: usize) -> Result<[u8; 32], String> {
    let index = u32::try_from(index).map_err(|_| "receive derivation index exceeds u32".to_string())?;
    if index >= HARDENED {
        return Err("receive derivation index must be non-hardened".into());
    }
    let account = account_key(secret)?;
    let child = derive_address_key(&account, index)
        .map_err(|error| format!("receive private-key derivation: {error:?}"))?;
    Ok(*child.private_key_bytes())
}

/// Derive the private key corresponding to one address already present in the
/// validated wallet projection. This is used to prove control of the exact
/// Kaspa address that received a private Ghost Talk bootstrap request.
pub fn private_key_for_address(
    secret: &WalletSecret,
    public: &WalletPublic,
    address: &str,
) -> Result<[u8; 32], String> {
    let account = account_key(secret)?;
    if let Some(index) = public.receive_addresses.iter().position(|value| value == address) {
        let index = u32::try_from(index).map_err(|_| "receive derivation index exceeds u32".to_string())?;
        let child = derive_address_key(&account, index)
            .map_err(|error| format!("receive private-key derivation: {error:?}"))?;
        return Ok(*child.private_key_bytes());
    }
    if let Some(index) = public.change_addresses.iter().position(|value| value == address) {
        let index = u32::try_from(index).map_err(|_| "change derivation index exceeds u32".to_string())?;
        let child = derive_change_key(&account, index)
            .map_err(|error| format!("change private-key derivation: {error:?}"))?;
        return Ok(*child.private_key_bytes());
    }
    Err("address is not part of the validated Ghost Talk wallet projection".into())
}

/// Derive a deterministic, domain-separated key for encrypted Ghost Talk
/// recovery snapshots. This key is independent of the local vault password,
/// so restoring the same Kaspa mnemonic/passphrase/account path can recover
/// contacts and optional message archives from Kaspa.
pub fn profile_backup_key(secret: &WalletSecret) -> Result<[u8; 32], String> {
    let account = account_key(secret)?;
    let mut hasher = Sha256::new();
    hasher.update(b"GhostTalk/KaspaProfileBackup/v1\0");
    hasher.update(account.private_key_bytes());
    Ok(hasher.finalize().into())
}

pub fn parse_account_path(path: &str) -> Result<Vec<u32>, String> {
    let value = path.trim();
    if !value.starts_with("m/") {
        return Err("derivation path must start with m/".into());
    }
    if value.len() > 160 {
        return Err("derivation path is too long".into());
    }
    let mut parsed = Vec::new();
    for segment in value[2..].split('/') {
        let hardened = segment.ends_with('\'');
        let digits = segment.trim_end_matches('\'');
        if digits.is_empty() {
            return Err("empty derivation path segment".into());
        }
        let index = digits
            .parse::<u32>()
            .map_err(|_| "invalid derivation path segment".to_string())?;
        if index >= HARDENED {
            return Err("derivation index must be below 2^31".into());
        }
        parsed.push(if hardened { index | HARDENED } else { index });
    }
    if parsed.is_empty() {
        return Err("derivation path must contain at least one segment".into());
    }
    Ok(parsed)
}

fn mnemonic_seed(mnemonic: &str, passphrase: &str) -> Result<[u8; 64], String> {
    let words = mnemonic.split_whitespace().collect::<Vec<_>>();
    if words.len() != 24 {
        return Err("Ghost Talk recovery mnemonic must contain exactly 24 words".into());
    }
    let mut indices = [0u16; 24];
    for (slot, word) in indices.iter_mut().zip(words) {
        *slot = bip39::word_to_index(word)
            .map_err(|_| format!("unknown BIP39 word: {word}"))?;
    }
    let mnemonic = bip39::Mnemonic24 { indices };
    bip39::validate_mnemonic_24(&mnemonic)
        .map_err(|error| format!("invalid 24-word mnemonic: {error:?}"))?;
    Ok(bip39::seed_from_mnemonic_24(&mnemonic, passphrase).bytes)
}

fn project_account(
    account: &ExtendedPrivKey,
    account_path: &str,
    network: NetworkId,
) -> Result<WalletPublic, String> {
    let prefix = network.address_prefix();
    let mut receive_addresses = Vec::with_capacity(ADDRESS_LOOKAHEAD as usize);
    let mut change_addresses = Vec::with_capacity(ADDRESS_LOOKAHEAD as usize);
    for index in 0..ADDRESS_LOOKAHEAD {
        let receive = derive_address_key(account, index)
            .map_err(|error| format!("receive derivation: {error:?}"))?;
        receive_addresses.push(kaspa_portal::primitives::address::encode_p2pk_address(
            &receive
                .public_key_x_only()
                .map_err(|error| format!("receive public key: {error:?}"))?,
            prefix,
        ));
        let change = derive_change_key(account, index)
            .map_err(|error| format!("change derivation: {error:?}"))?;
        change_addresses.push(kaspa_portal::primitives::address::encode_p2pk_address(
            &change
                .public_key_x_only()
                .map_err(|error| format!("change public key: {error:?}"))?,
            prefix,
        ));
    }
    Ok(WalletPublic {
        network: network.canonical_name(),
        account_path: account_path.trim().to_owned(),
        kpub: None,
        watch_only: false,
        receive_addresses,
        change_addresses,
        next_receive_index: 0,
        next_change_index: 0,
    })
}

pub fn sign_pskb(
    pskb_hex: &str,
    network: &str,
    account: &ExtendedPrivKey,
) -> Result<String, String> {
    let kspt_hex = pskt::relay_pskb_as_kspt_hex_for_network(pskb_hex, network)?;
    let wire = hex::decode(kspt_hex).map_err(|error| error.to_string())?;
    let mut transaction = Transaction::new();
    kspt::parse_compact_kspt(&wire, &mut transaction)
        .map_err(|error| format!("KSPT parse: {error:?}"))?;
    let mut entropy = [0u8; 32];
    OsRng.fill_bytes(&mut entropy);
    let signed_result = kspt::sign_transaction_account_multi_addr_with_entropy(
        &mut transaction,
        account,
        SigHashType::All,
        &entropy,
    )
    .map_err(|error| format!("KSPT signing: {error:?}"));
    entropy.zeroize();
    let signed = signed_result?;
    if signed == 0 || !kspt::is_fully_signed(&transaction) {
        return Err("wallet did not sign every transaction input".into());
    }
    let signed_wire = kspt::serialize_compact_kspt_vec(&transaction)
        .map_err(|error| format!("KSPT serialization: {error:?}"))?;
    pskt::merge_signed_kspt_into_pskb(&hex::encode(signed_wire), pskb_hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_paths_support_named_and_custom_roots() {
        assert_eq!(
            parse_account_path("m/44'/111111'/0'").unwrap(),
            vec![0x8000_002c, 0x8001_b207, 0x8000_0000]
        );
        assert!(parse_account_path("44'/111111'/0'").is_err());
        assert!(parse_account_path("m/44'/x/0'").is_err());
    }

    #[test]
    fn generated_wallet_has_independent_receive_and_change_branches() {
        let (_, created) = generate_wallet("", "m/44'/111111'/0'", "mainnet").unwrap();
        assert_eq!(created.public.receive_addresses.len(), ADDRESS_LOOKAHEAD as usize);
        assert_eq!(created.public.change_addresses.len(), ADDRESS_LOOKAHEAD as usize);
        assert_ne!(
            created.public.receive_addresses[0],
            created.public.change_addresses[0]
        );
    }

    #[test]
    fn hydra_seed_is_deterministic_and_domain_separated_from_wallet_material() {
        let (secret, _) = generate_wallet("", "m/44'/111111'/0'", "mainnet").unwrap();
        let first = hydra_identity_seed(&secret).unwrap();
        let second = hydra_identity_seed(&secret).unwrap();
        assert_eq!(first, second);

        let mut with_passphrase = secret.clone();
        with_passphrase.passphrase = "different".into();
        assert_ne!(first, hydra_identity_seed(&with_passphrase).unwrap());

        let mut other_account = secret.clone();
        other_account.account_path = "m/44'/111111'/1'".into();
        other_account.network = "testnet-10".into();
        assert_eq!(first, hydra_identity_seed(&other_account).unwrap());
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastTimings {
    pub utxo_plan_ms: u64,
    pub signed_analysis_ms: u64,
    pub submit_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastResult {
    pub transaction_id: String,
    pub fee_sompi: String,
    pub public: WalletPublic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<BroadcastTimings>,
}

pub async fn send(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    amount_sompi: u64,
    requested_fee_sompi: u64,
) -> Result<BroadcastResult, String> {
    let total_started = Instant::now();
    crate::validate_destination(destination)?;
    let account = account_key(secret)?;
    let mut public = public.clone();
    let wallet = public.portal_wallet();

    // Portal 1.0.1's payload-aware planner also handles an empty payload, so
    // ordinary KAS sends use the same fee/mass-aware UTXO selection as chat.
    let plan_started = Instant::now();
    let wire = portal
        .plan_send_with_payload(
            wallet,
            destination,
            amount_sompi,
            requested_fee_sompi,
            &[],
        )
        .await?;
    let utxo_plan_ms = plan_started.elapsed().as_millis() as u64;

    let analysis_started = Instant::now();
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal
        .analyze(&signed)
        .await?;
    let signed_analysis_ms = analysis_started.elapsed().as_millis() as u64;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err(
            "Portal 1.0.1 payload-aware planner produced a transaction that failed final fee/mass policy"
                .into(),
        );
    }

    let submit_started = Instant::now();
    let transaction_id = portal.broadcast_signed_pskb(&signed).await?;
    let submit_ms = submit_started.elapsed().as_millis() as u64;
    public.advance_change()?;
    Ok(BroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public,
        timings: Some(BroadcastTimings {
            utxo_plan_ms,
            signed_analysis_ms,
            submit_ms,
            total_ms: total_started.elapsed().as_millis() as u64,
        }),
    })
}

pub async fn send_payload(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    requested_fee_sompi: u64,
    payload: &[u8],
) -> Result<BroadcastResult, String> {
    send_payload_with_change_policy(
        portal,
        secret,
        public,
        destination,
        requested_fee_sompi,
        payload,
        true,
    )
    .await
}

/// High-frequency voice media deliberately reuses the current change address.
/// Rotating one HD change address per ~350 ms audio window would exhaust the
/// bounded lookahead during an ordinary call and continually restart frontend
/// monitoring. The caller still serializes broadcasts; ordinary wallet/message
/// sends retain fresh-change rotation through `send_payload`.
pub async fn send_payload_reuse_change(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    requested_fee_sompi: u64,
    payload: &[u8],
) -> Result<BroadcastResult, String> {
    send_payload_with_change_policy(
        portal,
        secret,
        public,
        destination,
        requested_fee_sompi,
        payload,
        false,
    )
    .await
}

async fn send_payload_with_change_policy(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    destination: &str,
    requested_fee_sompi: u64,
    payload: &[u8],
    advance_change: bool,
) -> Result<BroadcastResult, String> {
    crate::validate_destination(destination)?;
    if payload.is_empty() {
        return Err("mailbox payload must not be empty".into());
    }
    let account = account_key(secret)?;
    let mut public = public.clone();
    let wire = portal
        .plan_send_with_payload(
            public.portal_wallet(),
            destination,
            MAILBOX_OUTPUT_SOMPI,
            requested_fee_sompi,
            payload,
        )
        .await?;
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal
        .analyze(&signed)
        .await?;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err(
            "Portal 1.0.1 payload-aware planner produced a mailbox transaction that failed final fee/mass policy"
                .into(),
        );
    }
    let transaction_id = portal.broadcast_signed_pskb(&signed).await?;
    if advance_change {
        public.advance_change()?;
    }
    Ok(BroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public,
        timings: None,
    })
}

pub async fn consolidate(
    portal: &crate::PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    requested_fee_sompi: u64,
) -> Result<BroadcastResult, String> {
    let account = account_key(secret)?;
    let mut public = public.clone();
    let wallet = public.portal_wallet();
    let wire = portal.plan_consolidation(wallet, requested_fee_sompi).await?;
    let signed = sign_pskb(&wire, &public.network, &account)?;
    let analysis = portal
        .analyze(&signed)
        .await?;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err("Portal rejected consolidation fee/mass policy".into());
    }
    let transaction_id = portal.broadcast_signed_pskb(&signed).await?;
    public.advance_receive()?;
    Ok(BroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public,
        timings: None,
    })
}

/// Construct an unsigned PSKB using Ghost Talk's existing wallet policy. This
/// is the host-wallet half of the KasSigner integration boundary: the hardware
/// SDK never chooses UTXOs, outputs, fees, change or providers.
pub async fn plan_send_unsigned(
    portal: &crate::PortalFacade,
    public: &WalletPublic,
    destination: &str,
    amount_sompi: u64,
    requested_fee_sompi: u64,
) -> Result<String, String> {
    crate::validate_destination(destination)?;
    portal
        .plan_send_with_payload(
            public.portal_wallet(),
            destination,
            amount_sompi,
            requested_fee_sompi,
            &[],
        )
        .await
}

/// Construct an unsigned consolidation PSKB for external KasSigner signing.
pub async fn plan_consolidation_unsigned(
    portal: &crate::PortalFacade,
    public: &WalletPublic,
    requested_fee_sompi: u64,
) -> Result<String, String> {
    portal
        .plan_consolidation(public.portal_wallet(), requested_fee_sompi)
        .await
}

/// Analyze an externally signed PSKB. Exact signature-script size is present at
/// this point, so Portal's fee/mass result is authoritative.
pub async fn analyze_signed_pskb(
    portal: &crate::PortalFacade,
    signed_pskb: &str,
) -> Result<crate::PortalMassAnalysis, String> {
    portal
        .analyze(signed_pskb)
        .await
}

/// Final policy/broadcast boundary after KasSigner has validated and merged its
/// signed KSPT response back into the exact host-created PSKB.
pub async fn broadcast_external_signed_pskb(
    portal: &crate::PortalFacade,
    signed_pskb: &str,
    public: &WalletPublic,
    advance_change: bool,
) -> Result<BroadcastResult, String> {
    let analysis = portal
        .analyze(signed_pskb)
        .await?;
    if !analysis.mass_valid || !analysis.fee_sufficient {
        return Err("externally signed transaction failed final Kaspa fee/mass policy".into());
    }
    let transaction_id = portal.broadcast_signed_pskb(signed_pskb).await?;
    let mut next_public = public.clone();
    if advance_change {
        next_public.advance_change()?;
    } else {
        next_public.advance_receive()?;
    }
    Ok(BroadcastResult {
        transaction_id,
        fee_sompi: analysis.fee_sompi.to_string(),
        public: next_public,
        timings: None,
    })
}
