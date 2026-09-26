//! Kaspa Portal builds the PSKBs Ghost Talk hands to KasKold for signing.
//! Both libraries must agree on the standard PSKT shape (format version 0,
//! per-input sighashType, object-typed maps); this pins that contract.
#![cfg(not(target_arch = "wasm32"))]

use kaspa_portal::primitives::utxo::UtxoEntry;
use kaspa_portal::transaction::builder::{PskbApi, PskbGlobalPlan, SweepInputPolicy};

fn p2pk_script(key_byte: u8) -> Vec<u8> {
    let mut script = vec![0x20];
    script.extend_from_slice(&[key_byte; 32]);
    script.push(0xac);
    script
}

#[test]
fn kaskold_accepts_a_portal_built_pskb() {
    let source = p2pk_script(0x11);
    let utxo = UtxoEntry {
        tx_id: "ab".repeat(32),
        index: 0,
        amount: 100_000_000,
        script_public_key: source.clone(),
        block_daa_score: 1,
        covenant_id: None,
    };
    let plan = PskbApi.plan_sweep(
        &[utxo],
        &source,
        &p2pk_script(0x22),
        99_990_000,
        PskbGlobalPlan::standard(),
        &SweepInputPolicy::p2pk(serde_json::json!({})),
    );
    let wire = PskbApi.encode(&plan).expect("Portal encodes the PSKB");
    let request = kaskold_sdk::prepare(&wire, kaskold_sdk::Network::Mainnet)
        .expect("KasKold accepts Portal's PSKB");
    assert!(!request.kspt_hex.is_empty());
}
