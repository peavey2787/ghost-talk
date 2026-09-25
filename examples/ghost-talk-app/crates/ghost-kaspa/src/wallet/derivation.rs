use super::model::{CreatedWallet, WalletPublic, WalletSecret, ADDRESS_LOOKAHEAD, HARDENED};
use kaspa_portal::{
    primitives::NetworkId,
    wallet::{
        derivation::bip32::{derive_address_key, derive_change_key, derive_path, ExtendedPrivKey},
        mnemonic::bip39,
    },
};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

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
    import_wallet(&mnemonic, passphrase, account_path, network)
        .map(|(secret, public)| (secret, CreatedWallet { mnemonic, public }))
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


/// Verify that a public wallet view belongs to the encrypted/derived secret and
/// that both derivation cursors remain inside the committed lookahead window.
pub fn validate_public_projection(
    secret: &WalletSecret,
    public: &WalletPublic,
) -> Result<(), String> {
    let expected = derive_public(secret)?;
    if expected.network != public.network
        || expected.account_path != public.account_path
        || expected.receive_addresses != public.receive_addresses
        || expected.change_addresses != public.change_addresses
        || public.next_receive_index >= public.receive_addresses.len()
        || public.next_change_index >= public.change_addresses.len()
    {
        return Err("wallet public projection does not match encrypted wallet secret".into());
    }
    Ok(())
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
    let index =
        u32::try_from(index).map_err(|_| "receive derivation index exceeds u32".to_string())?;
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
    if let Some(index) = public
        .receive_addresses
        .iter()
        .position(|value| value == address)
    {
        let index =
            u32::try_from(index).map_err(|_| "receive derivation index exceeds u32".to_string())?;
        let child = derive_address_key(&account, index)
            .map_err(|error| format!("receive private-key derivation: {error:?}"))?;
        return Ok(*child.private_key_bytes());
    }
    if let Some(index) = public
        .change_addresses
        .iter()
        .position(|value| value == address)
    {
        let index =
            u32::try_from(index).map_err(|_| "change derivation index exceeds u32".to_string())?;
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
    validate_account_path_prefix(value)?;
    let parsed = value[2..]
        .split('/')
        .map(parse_account_segment)
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.is_empty() {
        return Err("derivation path must contain at least one segment".into());
    }
    Ok(parsed)
}

fn validate_account_path_prefix(value: &str) -> Result<(), String> {
    if !value.starts_with("m/") {
        return Err("derivation path must start with m/".into());
    }
    if value.len() > 160 {
        return Err("derivation path is too long".into());
    }
    Ok(())
}

fn parse_account_segment(segment: &str) -> Result<u32, String> {
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
    Ok(if hardened { index | HARDENED } else { index })
}

pub(crate) fn mnemonic_seed(mnemonic: &str, passphrase: &str) -> Result<[u8; 64], String> {
    let words = mnemonic.split_whitespace().collect::<Vec<_>>();
    if words.len() != 24 {
        return Err("Ghost Talk recovery mnemonic must contain exactly 24 words".into());
    }
    let mut indices = [0u16; 24];
    for (slot, word) in indices.iter_mut().zip(words) {
        *slot = bip39::word_to_index(word).map_err(|_| format!("unknown BIP39 word: {word}"))?;
    }
    let mnemonic = bip39::Mnemonic24 { indices };
    bip39::validate_mnemonic_24(&mnemonic)
        .map_err(|error| format!("invalid 24-word mnemonic: {error:?}"))?;
    Ok(bip39::seed_from_mnemonic_24(&mnemonic, passphrase).bytes)
}

pub(crate) fn project_account(
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
        receive_addresses,
        change_addresses,
        next_receive_index: 0,
        next_change_index: 0,
    })
}
