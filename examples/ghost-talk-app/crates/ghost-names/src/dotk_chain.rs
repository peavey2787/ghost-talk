use std::str::FromStr;

use kaspa_wrpc_client::prelude::*;

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
    let network_id = NetworkId::from_str("mainnet").map_err(|error| error.to_string())?;
    let client = KaspaRpcClient::new_with_args(
        WrpcEncoding::Borsh,
        Some(endpoint),
        None,
        Some(network_id),
        None,
    )
    .map_err(|error| format!("could not create dot.k Kaspa verifier: {error}"))?;
    let options = ConnectOptions {
        block_async_connect: true,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|error| {
        format!("could not connect dot.k verifier to the active Kaspa node: {error}")
    })?;

    let result = verify_connected_deed(&client, derived).await;
    let _ = client.disconnect().await;
    result
}

async fn verify_connected_deed(
    client: &KaspaRpcClient,
    derived: &DerivedDeed,
) -> Result<(String, u64), String> {
    let address = RpcAddress::try_from(derived.deed_address.as_str())
        .map_err(|error| format!("derived dot.k deed address is invalid: {error}"))?;
    let first = get_deed_utxos(client, &address, "deed UTXO").await?;

    for entry in first.iter().take(MAX_DEED_UTXOS) {
        let Some((txid, index)) = candidate_outpoint(entry, derived) else {
            continue;
        };
        let fresh = get_deed_utxos(client, &address, "deed freshness").await?;
        if live_outpoint_exists(&fresh, &txid, index, derived) {
            return Ok((txid, index));
        }
    }
    Err("dot.k ownership could not be confirmed by the active Kaspa node. No recipient was selected; retry after refresh.".into())
}

async fn get_deed_utxos(
    client: &KaspaRpcClient,
    address: &RpcAddress,
    label: &str,
) -> Result<Vec<RpcUtxosByAddressesEntry>, String> {
    client
        .get_utxos_by_addresses(vec![address.clone()])
        .await
        .map_err(|error| format!("dot.k {label} query failed: {error}"))
}

fn candidate_outpoint(
    row: &RpcUtxosByAddressesEntry,
    derived: &DerivedDeed,
) -> Option<(String, u64)> {
    let entry = &row.utxo_entry;
    let covenant = entry.covenant_id.as_ref()?.to_string();
    let script_hex = hex::encode(entry.script_public_key.script());
    let txid = row.outpoint.transaction_id.to_string();
    let index = u64::from(row.outpoint.index);
    let address_matches = row
        .address
        .as_ref()
        .is_none_or(|address| address.to_string() == derived.deed_address);

    if entry.amount != derived.bond
        || entry.is_coinbase
        || covenant != DOTK_REGISTRY
        || script_hex != derived.script_public_key
        || !address_matches
    {
        return None;
    }
    Some((txid, index))
}

fn live_outpoint_exists(
    entries: &[RpcUtxosByAddressesEntry],
    txid: &str,
    index: u64,
    derived: &DerivedDeed,
) -> bool {
    entries.iter().any(|row| {
        candidate_outpoint(row, derived).is_some_and(|(candidate_txid, candidate_index)| {
            candidate_txid == txid && candidate_index == index
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dotk_deed::derive_deed;

    #[test]
    fn v2_rpc_model_is_required_for_covenant_proof() {
        let deed = derive_deed(
            "21millioncoven",
            0,
            "079ab96f3b42f1b3667010e6d855172bb8e905e3369fe5ad57b662c4bc365449",
        )
        .unwrap();
        assert_eq!(deed.bond, 100_000_000);
        assert_eq!(DOTK_REGISTRY.len(), 64);
    }
}
