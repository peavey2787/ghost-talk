use super::{parse_backup_frame, BACKUP_HEADER_BYTES, BACKUP_MAGIC, BACKUP_VERSION};

#[test]
fn backup_frame_parser_rejects_bad_counts_and_lengths() {
    assert!(parse_backup_frame(b"GTBK").is_none());
    let mut frame = vec![0u8; BACKUP_HEADER_BYTES];
    frame[..4].copy_from_slice(BACKUP_MAGIC);
    frame[4] = BACKUP_VERSION;
    frame[23..25].copy_from_slice(&1u16.to_le_bytes());
    frame[25..29].copy_from_slice(&28u32.to_le_bytes());
    assert!(parse_backup_frame(&frame).is_some());
    frame[23..25].copy_from_slice(&0u16.to_le_bytes());
    assert!(parse_backup_frame(&frame).is_none());
}
