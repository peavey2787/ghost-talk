use kaspa_portal::{
    transaction::{
        interchange::{kspt, pskt},
        model::{SigHashType, Transaction},
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
    let kspt_hex = pskt::relay_pskb_as_kspt_hex_for_network(pskb_hex, network)?;
    let wire = hex::decode(kspt_hex).map_err(|error| error.to_string())?;
    let mut transaction =
        Transaction::try_new().map_err(|error| format!("transaction storage: {error:?}"))?;
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
