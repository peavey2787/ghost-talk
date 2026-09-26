use kaspa_portal::{primitives::utxo::UtxoEntry, KaspaPortal};

use crate::dotk_deed::{DerivedDeed, DOTK_REGISTRY};

const MAX_DEED_UTXOS: usize = 64;

pub(crate) async fn verify_live_deed_wrpc(
    endpoint: &str,
    derived: &DerivedDeed,
) -> Result<(String, u64), String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err("dot.k verification requires the active Kaspa wRPC endpoint".into());
    }
    let portal = KaspaPortal::builder()
        .network(kaspa_portal::primitives::NetworkId::Mainnet)
        .endpoint(endpoint)
        .connect()
        .await
        .map_err(|error| {
            format!("could not connect dot.k verifier to the active Kaspa node: {error}")
        })?;
    let result = verify_connected_deed(&portal, derived).await;
    let _ = portal.disconnect();
    result
}

async fn verify_connected_deed(
    portal: &KaspaPortal,
    derived: &DerivedDeed,
) -> Result<(String, u64), String> {
    let first = get_deed_utxos(portal, derived, "deed UTXO").await?;

    for entry in first.iter().take(MAX_DEED_UTXOS) {
        let Some((txid, index)) = candidate_outpoint(entry, derived) else {
            continue;
        };
        let fresh = get_deed_utxos(portal, derived, "deed freshness").await?;
        if live_outpoint_exists(&fresh, &txid, index, derived) {
            return Ok((txid, index));
        }
    }
    Err("dot.k ownership could not be confirmed by the active Kaspa node. No recipient was selected; retry after refresh.".into())
}

async fn get_deed_utxos(
    portal: &KaspaPortal,
    derived: &DerivedDeed,
    label: &str,
) -> Result<Vec<UtxoEntry>, String> {
    portal
        .chain()
        .map_err(|error| error.to_string())?
        .utxos(&derived.deed_address)
        .await
        .map_err(|error| format!("dot.k {label} query failed: {error}"))
}

/// A deed outpoint must carry the registry covenant id, the exact bond, and
/// the derived P2SH lock. Covenant-tagged outputs cannot be coinbase outputs,
/// so the registry id alone rules those out.
fn candidate_outpoint(entry: &UtxoEntry, derived: &DerivedDeed) -> Option<(String, u64)> {
    let covenant = entry.covenant_id.as_deref()?;
    if entry.amount != derived.bond
        || !covenant.eq_ignore_ascii_case(DOTK_REGISTRY)
        || hex::encode(&entry.script_public_key) != derived.script_public_key
    {
        return None;
    }
    Some((entry.tx_id.clone(), u64::from(entry.index)))
}

fn live_outpoint_exists(
    entries: &[UtxoEntry],
    txid: &str,
    index: u64,
    derived: &DerivedDeed,
) -> bool {
    entries.iter().any(|entry| {
        candidate_outpoint(entry, derived).is_some_and(|(candidate_txid, candidate_index)| {
            candidate_txid == txid && candidate_index == index
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dotk_deed::derive_deed;

    fn deed() -> DerivedDeed {
        derive_deed(
            "21millioncoven",
            0,
            "079ab96f3b42f1b3667010e6d855172bb8e905e3369fe5ad57b662c4bc365449",
        )
        .unwrap()
    }

    fn entry(derived: &DerivedDeed, covenant: Option<&str>) -> UtxoEntry {
        UtxoEntry {
            tx_id: "11".repeat(32),
            index: 2,
            amount: derived.bond,
            script_public_key: hex::decode(&derived.script_public_key).unwrap(),
            block_daa_score: 1,
            covenant_id: covenant.map(str::to_owned),
        }
    }

    #[test]
    fn deed_outpoint_requires_registry_covenant_bond_and_script() {
        let derived = deed();
        assert_eq!(derived.bond, 100_000_000);
        let good = entry(&derived, Some(DOTK_REGISTRY));
        assert_eq!(
            candidate_outpoint(&good, &derived),
            Some(("11".repeat(32), 2))
        );
        assert!(candidate_outpoint(&entry(&derived, None), &derived).is_none());
        assert!(candidate_outpoint(&entry(&derived, Some(&"00".repeat(32))), &derived).is_none());
        let mut wrong_bond = good.clone();
        wrong_bond.amount -= 1;
        assert!(candidate_outpoint(&wrong_bond, &derived).is_none());
        assert!(live_outpoint_exists(&[good], &"11".repeat(32), 2, &derived));
    }
}
