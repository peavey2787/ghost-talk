use super::*;

#[test]
fn ghost_payload_filter_is_explicit() {
    assert!(is_ghost_payload(b"KKTP:ANCHOR:{\"type\":\"discovery\"}"));
    assert!(is_ghost_payload(b"KKTP:deadbeef:{}"));
    assert!(is_ghost_payload(b"GHSTpayload"));
    assert!(is_ghost_payload(b"GTCDpayload"));
    assert!(is_ghost_payload(b"GTCRpayload"));
    assert!(is_ghost_payload(b"GTCApayload"));
    assert!(is_ghost_payload(b"GTAKpayload"));
    assert!(is_ghost_payload(b"GTVApayload"));
    assert!(is_ghost_payload(b"GTBKpayload"));
    assert!(!is_ghost_payload(b"other"));
}

#[test]
fn history_u64_fields_accept_numbers_and_decimal_strings() {
    assert_eq!(optional_u64(Some(&serde_json::json!(42))), Some(42));
    assert_eq!(optional_u64(Some(&serde_json::json!("42"))), Some(42));
    assert_eq!(optional_u64(Some(&serde_json::json!("not-a-number"))), None);
}

#[test]
fn testnet_rest_endpoints_are_network_specific() {
    assert!(rest_base_for_network("testnet-10").contains("tn10"));
    assert!(rest_base_for_network("testnet-11").contains("tn11"));
    assert_eq!(rest_base_for_network("mainnet"), "https://api.kaspa.org");
}
