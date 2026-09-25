use super::*;

fn wallet() -> WalletPublic {
    WalletPublic {
        network: "mainnet".into(),
        account_path: "m/44'/111111'/0'".into(),
        receive_addresses: vec!["kaspa:qtest".into()],
        change_addresses: vec!["kaspa:qchange".into()],
        next_receive_index: 0,
        next_change_index: 0,
    }
}

#[test]
fn resume_metadata_must_match_exact_media_and_next_chunk() {
    let bytes = b"abcdefgh";
    let chunks = [b"abcd".as_slice(), b"efgh".as_slice()];
    let mut progress = ArchiveProgress::new(
        "profile",
        &content_hash(bytes),
        "audio/webm",
        "kaspa:qtest",
        bytes.len() as u64,
        chunks.len() as u32,
        wallet(),
    );
    assert!(validate(
        &progress,
        "profile",
        "audio/webm",
        "kaspa:qtest",
        bytes,
        &chunks
    )
    .is_ok());
    progress.mark_in_flight(0, chunks[0], 10);
    assert!(validate(
        &progress,
        "profile",
        "audio/webm",
        "kaspa:qtest",
        bytes,
        &chunks
    )
    .is_ok());
    progress.in_flight.as_mut().unwrap().index = 1;
    assert!(validate(
        &progress,
        "profile",
        "audio/webm",
        "kaspa:qtest",
        bytes,
        &chunks
    )
    .is_err());
}

#[test]
fn resume_metadata_rejects_different_media() {
    let bytes = b"archive";
    let chunks = [bytes.as_slice()];
    let progress = ArchiveProgress::new(
        "profile",
        &content_hash(bytes),
        "audio/ogg",
        "kaspa:qtest",
        bytes.len() as u64,
        1,
        wallet(),
    );
    assert!(validate(
        &progress,
        "other",
        "audio/ogg",
        "kaspa:qtest",
        bytes,
        &chunks
    )
    .is_err());
    assert!(validate(
        &progress,
        "profile",
        "audio/ogg",
        "kaspa:qtest",
        b"changed",
        &[b"changed"]
    )
    .is_err());
}
