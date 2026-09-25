use super::model::ADDRESS_LOOKAHEAD;
use super::{generate_wallet, hydra_identity_seed, parse_account_path};

#[test]
fn account_paths_support_named_and_custom_roots() {
    assert_eq!(
        parse_account_path("m/44'/111111'/0'").unwrap(),
        vec![0x8000_002c, 0x8001_b207, 0x8000_0000]
    );
    assert!(parse_account_path("44'/111111'/0'").is_err());
    assert!(parse_account_path("m/44'/x/0'").is_err());
}

#[test]
fn generated_wallet_has_independent_receive_and_change_branches() {
    let (_, created) = generate_wallet("", "m/44'/111111'/0'", "mainnet").unwrap();
    assert_eq!(
        created.public.receive_addresses.len(),
        ADDRESS_LOOKAHEAD as usize
    );
    assert_eq!(
        created.public.change_addresses.len(),
        ADDRESS_LOOKAHEAD as usize
    );
    assert_ne!(
        created.public.receive_addresses[0],
        created.public.change_addresses[0]
    );
}

#[test]
fn receive_chain_rotates_without_changing_earlier_addresses() {
    let (_, mut created) = generate_wallet("", "m/44'/111111'/0'", "mainnet").unwrap();
    let first = created.public.receive_addresses[0].clone();
    let second = created.public.receive_addresses[1].clone();
    assert_eq!(created.public.receive_address().unwrap(), first);
    created.public.advance_receive().unwrap();
    assert_eq!(created.public.receive_address().unwrap(), second);
    assert_eq!(created.public.receive_addresses[0], first);
}

#[test]
fn final_prederived_change_address_is_reused_after_rotation_window() {
    let (_, mut created) = generate_wallet("", "m/44'/111111'/0'", "mainnet").unwrap();
    created.public.next_change_index = created.public.change_addresses.len() - 1;
    let final_address = created.public.change_address().unwrap().to_string();
    assert!(!created.public.advance_change_if_available());
    assert_eq!(created.public.change_address().unwrap(), final_address);
}

#[test]
fn hydra_seed_is_deterministic_and_domain_separated_from_wallet_material() {
    let (secret, _) = generate_wallet("", "m/44'/111111'/0'", "mainnet").unwrap();
    let first = hydra_identity_seed(&secret).unwrap();
    let second = hydra_identity_seed(&secret).unwrap();
    assert_eq!(first, second);

    let mut with_passphrase = secret.clone();
    with_passphrase.passphrase = "different".into();
    assert_ne!(first, hydra_identity_seed(&with_passphrase).unwrap());

    let mut other_account = secret.clone();
    other_account.account_path = "m/44'/111111'/1'".into();
    other_account.network = "testnet-10".into();
    assert_eq!(first, hydra_identity_seed(&other_account).unwrap());
}
