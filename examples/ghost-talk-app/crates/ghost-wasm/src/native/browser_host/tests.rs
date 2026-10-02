use serde_json::json;
use wasm_bindgen_test::*;

use super::invoke;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test(async)]
async fn standalone_browser_creates_wallet_without_tauri() {
    let response = invoke(
        "wallet_create",
        json!({
            "password": "browser-test-password",
            "passphrase": "",
            "accountPath": "m/44'/111111'/0'",
            "network": "testnet-10",
        }),
    )
    .await
    .expect("browser wallet creation must not require Tauri");
    assert_eq!(response["public"]["network"], "testnet-10");
    assert_eq!(
        response["mnemonic"]
            .as_str()
            .expect("mnemonic string")
            .split_whitespace()
            .count(),
        24
    );
}

#[wasm_bindgen_test(async)]
async fn standalone_browser_profile_state_round_trips() {
    let state = r#"[{"id":"browser-test"}]"#;
    invoke(
        "profile_state_save",
        json!({"json": state, "changedProfileIds": ["browser-test"]}),
    )
    .await
    .expect("save browser profile state");
    let loaded = invoke("profile_state_load", json!({}))
        .await
        .expect("load browser profile state");
    assert_eq!(loaded.as_str(), Some(state));
}
#[wasm_bindgen_test]
fn standalone_browser_accepts_kaspa_resolver_node_descriptor() {
    let endpoint = super::kaspa::parse_resolver_endpoint(
        r#"{"uid":"public-node","url":"wss://node.example.org/wrpc/borsh"}"#,
    )
    .expect("Kaspa resolver NodeDescriptor URL");
    assert_eq!(endpoint, "wss://node.example.org/wrpc/borsh");
}

#[wasm_bindgen_test]
fn standalone_browser_rejects_non_tls_resolver_endpoints() {
    // A resolver may only hand out TLS nodes; plaintext ws:// or a bare URL
    // body would let a network attacker steer the wallet to a hostile node.
    for body in [
        r#"{"uid":"public-node","url":"ws://node.example.org/wrpc/borsh"}"#,
        "wss://node.example.org/wrpc/borsh",
        r#"{"uid":"public-node"}"#,
    ] {
        assert!(
            super::kaspa::parse_resolver_endpoint(body).is_err(),
            "{body}"
        );
    }
}
