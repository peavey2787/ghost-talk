use super::{WalletRecord, WalletStateService};
use ghost_domain::wallet::WalletProjection;

fn projection(receive_index: usize, change_index: usize, receive_len: usize) -> WalletProjection {
    WalletProjection {
        network: "mainnet".into(),
        account_path: "m/44'/111111'/0'".into(),
        receive_addresses: (0..receive_len)
            .map(|index| format!("receive-{index}"))
            .collect(),
        change_addresses: vec!["change-0".into(), "change-1".into(), "change-2".into()],
        next_receive_index: receive_index,
        next_change_index: change_index,
    }
}

fn wallet(public: WalletProjection) -> WalletRecord {
    WalletRecord {
        public,
        ..Default::default()
    }
}

#[test]
fn merge_progress_requires_wallet_and_never_rolls_indices_back() {
    let mut state = None;
    assert!(!WalletStateService::merge_progress(
        &mut state,
        projection(1, 1, 2)
    ));
    WalletStateService::install(&mut state, wallet(projection(1, 1, 2)));
    assert!(WalletStateService::merge_progress(
        &mut state,
        projection(2, 2, 3)
    ));
    assert!(!WalletStateService::merge_progress(
        &mut state,
        projection(0, 0, 1)
    ));
    let current = state.as_ref().unwrap();
    assert_eq!(current.public.next_receive_index, 2);
    assert_eq!(current.public.next_change_index, 2);
    assert_eq!(current.public.receive_addresses.len(), 3);
}

#[test]
fn reconcile_preserves_concurrent_local_fields_and_monotonic_progress() {
    let mut baseline = wallet(projection(0, 0, 3));
    baseline.mailbox_checkpoint = "10".into();
    baseline.directory_checkpoint = "20".into();
    baseline.rest_endpoint = Some("https://baseline.example".into());
    let mut local = baseline.clone();
    local.public.next_receive_index = 2;
    local.public.next_change_index = 1;
    local.rest_endpoint = Some("https://local.example".into());
    local.mailbox_seen_txids.push("a".repeat(64));
    local.used_addresses.push("receive-2".into());
    local.mailbox_checkpoint = "30".into();
    let mut remote = baseline.clone();
    remote.public.next_receive_index = 1;
    remote.wrpc_endpoint = Some("wss://remote.example".into());
    remote.directory_checkpoint = "40".into();
    let mut state = Some(local);
    WalletStateService::reconcile(&mut state, Some(&baseline), Some(remote));
    let merged = state.as_ref().unwrap();
    assert_eq!(
        merged.rest_endpoint.as_deref(),
        Some("https://local.example")
    );
    assert_eq!(
        merged.wrpc_endpoint.as_deref(),
        Some("wss://remote.example")
    );
    assert_eq!(merged.mailbox_checkpoint, "30");
    assert_eq!(merged.directory_checkpoint, "40");
    assert_eq!(merged.public.next_receive_index, 2);
    assert_eq!(merged.public.next_change_index, 1);
    assert_eq!(merged.mailbox_seen_txids, vec!["a".repeat(64)]);
    assert_eq!(merged.used_addresses, vec!["receive-2".to_string()]);
}
