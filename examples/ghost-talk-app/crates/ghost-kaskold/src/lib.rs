#![forbid(unsafe_code)]

mod backup;
mod signing;

use ghost_api::{KasKoldInventoryResult, KasKoldWalletSummary};
use rand::{rngs::OsRng, RngCore};
use vault_runtime::VaultRuntime;
use zeroize::Zeroize;

pub use backup::{backup, import_bytes, import_text};
pub use signing::{review_pskt, sign_pskt};

const WRAP_MAGIC: &[u8; 4] = b"GTK1";

pub fn import_text_inventory(
    password: &str,
    sealed_inventory: &[u8],
    kind: &str,
    value: &str,
    passphrase: &str,
) -> Result<KasKoldInventoryResult, String> {
    let mut runtime = open_runtime(password, sealed_inventory)?;
    import_text(&mut runtime, kind, value, passphrase)?;
    inventory_result(password, &runtime)
}

pub fn import_bytes_inventory(
    password: &str,
    sealed_inventory: &[u8],
    kind: &str,
    data: &[u8],
    credential: &str,
) -> Result<KasKoldInventoryResult, String> {
    let mut runtime = open_runtime(password, sealed_inventory)?;
    import_bytes(&mut runtime, kind, data, credential)?;
    inventory_result(password, &runtime)
}

pub fn backup_inventory(
    password: &str,
    sealed_inventory: &[u8],
    kind: &str,
    credential: &str,
    carrier: &[u8],
) -> Result<ghost_api::KasKoldBackupResult, String> {
    let runtime = open_runtime(password, sealed_inventory)?;
    backup(&runtime, kind, credential, carrier)
}

pub(crate) fn runtime_error(error: vault_runtime::VaultRuntimeError) -> String {
    format!("KasKold: {error:?}")
}

pub(crate) fn open_runtime(password: &str, sealed: &[u8]) -> Result<VaultRuntime, String> {
    if sealed.is_empty() {
        return Ok(VaultRuntime::new());
    }
    let mut plaintext =
        ghost_storage::open(password, &ghost_storage::SealedVault(sealed.to_vec()))?;
    if plaintext.len() < 36 || &plaintext[..4] != WRAP_MAGIC {
        plaintext.zeroize();
        return Err("KasKold compatibility inventory is invalid".into());
    }
    let mut wrapping_key = [0u8; 32];
    wrapping_key.copy_from_slice(&plaintext[4..36]);
    let mut runtime = VaultRuntime::new();
    let result = runtime
        .unlock_native_inventory(&plaintext[36..], &wrapping_key)
        .map_err(runtime_error);
    wrapping_key.zeroize();
    plaintext.zeroize();
    result?;
    Ok(runtime)
}

fn seal_runtime(password: &str, runtime: &VaultRuntime) -> Result<Vec<u8>, String> {
    let mut wrapping_key = [0u8; 32];
    OsRng.fill_bytes(&mut wrapping_key);
    let inventory = runtime
        .seal_native_inventory(&wrapping_key)
        .map_err(runtime_error)?;
    let mut plaintext = Vec::with_capacity(36 + inventory.len());
    plaintext.extend_from_slice(WRAP_MAGIC);
    plaintext.extend_from_slice(&wrapping_key);
    plaintext.extend_from_slice(&inventory);
    wrapping_key.zeroize();
    let result = ghost_storage::seal(password, &plaintext).map(|sealed| sealed.0);
    plaintext.zeroize();
    result
}

fn inventory_result(
    password: &str,
    runtime: &VaultRuntime,
) -> Result<KasKoldInventoryResult, String> {
    Ok(KasKoldInventoryResult {
        sealed_inventory: seal_runtime(password, runtime)?,
        wallets: summaries(runtime)?,
    })
}

fn summaries(runtime: &VaultRuntime) -> Result<Vec<KasKoldWalletSummary>, String> {
    Ok(runtime
        .wallet_summaries()
        .map_err(runtime_error)?
        .into_iter()
        .map(|wallet| KasKoldWalletSummary {
            index: wallet.index,
            name: wallet.name,
            active: wallet.active,
            kind: format!("{:?}", wallet.kind),
            fingerprint: wallet.fingerprint,
            kpub: wallet.kpub,
        })
        .collect())
}
