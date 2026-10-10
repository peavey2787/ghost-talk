use kaspa_portal::{
    primitives::address::KaspaNetwork,
    transaction::{
        interchange::{
            kspt::{self, wire::Limits},
            pskt::pipeline,
        },
        model::SigHashType,
    },
    wallet::derivation::bip32::ExtendedPrivKey,
};
use rand::{rngs::OsRng, RngCore};
use zeroize::Zeroize;

pub fn sign_pskb(
    pskb_hex: &str,
    network: &str,
    account: &ExtendedPrivKey,
) -> Result<String, String> {
    let network =
        KaspaNetwork::from_name(network).ok_or_else(|| format!("unknown network {network}"))?;
    let mut entropy = [0u8; 32];
    OsRng.fill_bytes(&mut entropy);
    let merged = pipeline::sign_pskt(pskb_hex, network, Limits::grammar(), |transaction| {
        let signed = kspt::sign_transaction_account_multi_addr_with_entropy(
            transaction,
            account,
            SigHashType::All,
            &entropy,
        )
        .map_err(|error| format!("KSPT signing: {error:?}"))?;
        if signed == 0 || !kspt::is_fully_signed(transaction) {
            return Err("wallet did not sign every transaction input".into());
        }
        Ok(())
    });
    entropy.zeroize();
    merged
}
