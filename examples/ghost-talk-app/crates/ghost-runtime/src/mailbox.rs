use crate::{PendingFrameBucket, WalletRecord};
use ghost_api::MailboxEvent;

const GHST_MAGIC: &str = "47485354";
const GHST_HEADER_BYTES: usize = 30;
const MAX_FRAGMENTS: usize = 128;
const MAX_PENDING_PACKETS: usize = 128;
const MAX_SEEN_TXIDS: usize = 2048;

#[derive(Clone, Debug, PartialEq)]
pub struct ReadyEnvelope {
    pub packet_id: String,
    pub envelope_hex: String,
    pub transaction_id: String,
    pub block_time: Option<u64>,
}

fn absorb_mailbox_events(wallet: &mut WalletRecord, events: &[MailboxEvent]) {
    for event in events {
        absorb_mailbox_event(wallet, event);
    }
    trim_pending_packets(wallet);
}

fn absorb_mailbox_event(wallet: &mut WalletRecord, event: &MailboxEvent) {
    let txid = event.transaction_id.to_lowercase();
    if wallet.mailbox_seen_txids.iter().any(|seen| seen == &txid) {
        return;
    }
    let Some(frame) = parse_frame(&event.payload_hex) else {
        return;
    };
    if !merge_frame(wallet, event, &frame) {
        wallet.mailbox_pending.remove(&frame.packet_id);
        return;
    }
    remember_mailbox_tx(wallet, txid);
}

fn merge_frame(wallet: &mut WalletRecord, event: &MailboxEvent, frame: &Frame) -> bool {
    let bucket = wallet
        .mailbox_pending
        .entry(frame.packet_id.clone())
        .or_insert_with(|| new_frame_bucket(frame, event));
    if bucket.count != frame.count {
        return false;
    }
    let key = frame.index.to_string();
    if bucket
        .parts
        .get(&key)
        .is_some_and(|old| old != &frame.data_hex)
    {
        return false;
    }
    bucket.parts.insert(key.clone(), frame.data_hex.clone());
    bucket.txids.insert(key, event.transaction_id.clone());
    bucket.block_time = newest_block_time(bucket.block_time, event.block_time);
    true
}

fn new_frame_bucket(frame: &Frame, event: &MailboxEvent) -> PendingFrameBucket {
    PendingFrameBucket {
        count: frame.count,
        parts: Default::default(),
        txids: Default::default(),
        block_time: event.block_time,
    }
}

fn newest_block_time(current: Option<u64>, incoming: Option<u64>) -> Option<u64> {
    match (current, incoming) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

fn remember_mailbox_tx(wallet: &mut WalletRecord, txid: String) {
    if !valid_txid(&txid) {
        return;
    }
    wallet.mailbox_seen_txids.push(txid);
    let excess = wallet
        .mailbox_seen_txids
        .len()
        .saturating_sub(MAX_SEEN_TXIDS);
    if excess > 0 {
        wallet.mailbox_seen_txids.drain(0..excess);
    }
}

fn valid_txid(txid: &str) -> bool {
    txid.len() == 64 && txid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn trim_pending_packets(wallet: &mut WalletRecord) {
    while wallet.mailbox_pending.len() > MAX_PENDING_PACKETS {
        let Some(key) = wallet.mailbox_pending.keys().next().cloned() else {
            break;
        };
        wallet.mailbox_pending.remove(&key);
    }
}

fn ready_envelopes(wallet: &WalletRecord) -> Vec<ReadyEnvelope> {
    let mut output = Vec::new();
    for (packet_id, bucket) in &wallet.mailbox_pending {
        if bucket.count == 0 || bucket.count > MAX_FRAGMENTS {
            continue;
        }
        let mut envelope_hex = String::new();
        let mut transaction_id = String::new();
        let mut complete = true;
        for index in 0..bucket.count {
            let key = index.to_string();
            let Some(part) = bucket.parts.get(&key) else {
                complete = false;
                break;
            };
            envelope_hex.push_str(part);
            if let Some(txid) = bucket.txids.get(&key) {
                transaction_id = txid.clone();
            }
        }
        if complete {
            output.push(ReadyEnvelope {
                packet_id: packet_id.clone(),
                envelope_hex,
                transaction_id,
                block_time: bucket.block_time,
            });
        }
    }
    output
}

fn remove_envelope(wallet: &mut WalletRecord, packet_id: &str) {
    wallet.mailbox_pending.remove(packet_id);
}

/// Sole owner of durable mailbox frame reassembly and envelope lifecycle.
pub struct MailboxService;

impl MailboxService {
    pub fn absorb(wallet: &mut WalletRecord, events: &[MailboxEvent]) {
        absorb_mailbox_events(wallet, events);
    }
    pub fn ready(wallet: &WalletRecord) -> Vec<ReadyEnvelope> {
        ready_envelopes(wallet)
    }
    pub fn remove(wallet: &mut WalletRecord, packet_id: &str) {
        remove_envelope(wallet, packet_id);
    }
}

struct Frame {
    packet_id: String,
    index: usize,
    count: usize,
    data_hex: String,
}

fn parse_frame(payload: &str) -> Option<Frame> {
    let hex = payload.trim().to_ascii_lowercase();
    if hex.len() < GHST_HEADER_BYTES * 2
        || !hex.bytes().all(|b| b.is_ascii_hexdigit())
        || !hex.starts_with(GHST_MAGIC)
    {
        return None;
    }
    if u8::from_str_radix(&hex[8..10], 16).ok()? != 1 {
        return None;
    }
    let packet_id = hex[12..44].to_string();
    let index = little_endian(&hex[44..48])?;
    let count = little_endian(&hex[48..52])?;
    let declared = little_endian(&hex[52..60])?;
    let data_hex = hex[60..].to_string();
    if count == 0 || count > MAX_FRAGMENTS || index >= count || declared != data_hex.len() / 2 {
        return None;
    }
    Some(Frame {
        packet_id,
        index,
        count,
        data_hex,
    })
}

fn little_endian(hex: &str) -> Option<usize> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut value = 0usize;
    for (index, offset) in (0..hex.len()).step_by(2).enumerate() {
        let byte = usize::from_str_radix(&hex[offset..offset + 2], 16).ok()?;
        value = value.checked_add(byte.checked_shl((index * 8) as u32)?)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::MailboxService;
    use crate::WalletRecord;
    use ghost_api::MailboxEvent;

    fn le_hex(value: u32, bytes: usize) -> String {
        (0..bytes)
            .map(|index| format!("{:02x}", (value >> (index * 8)) & 0xff))
            .collect()
    }
    fn frame(id: &str, index: u16, count: u16, data: &str) -> String {
        format!(
            "474853540100{id}{}{}{}{data}",
            le_hex(index as u32, 2),
            le_hex(count as u32, 2),
            le_hex((data.len() / 2) as u32, 4)
        )
    }

    #[test]
    fn reassembles_two_frames() {
        let id = "00112233445566778899aabbccddeeff";
        let events = vec![
            MailboxEvent {
                transaction_id: "1".repeat(64),
                payload_hex: frame(id, 0, 2, "aabb"),
                block_time: Some(1),
                ..Default::default()
            },
            MailboxEvent {
                transaction_id: "2".repeat(64),
                payload_hex: frame(id, 1, 2, "ccdd"),
                block_time: Some(2),
                ..Default::default()
            },
        ];
        let mut wallet = WalletRecord::default();
        MailboxService::absorb(&mut wallet, &events);
        let ready = MailboxService::ready(&wallet);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].envelope_hex, "aabbccdd");
        MailboxService::remove(&mut wallet, id);
        assert!(MailboxService::ready(&wallet).is_empty());
    }

    #[test]
    fn rejects_malformed_or_replayed_frames_without_duplicate_state() {
        let mut wallet = WalletRecord::default();
        let id = "00112233445566778899aabbccddeeff";
        let valid = MailboxEvent {
            transaction_id: "3".repeat(64),
            payload_hex: frame(id, 0, 1, "aabb"),
            block_time: Some(1),
            ..Default::default()
        };
        let malformed = MailboxEvent {
            payload_hex: "not-hex".into(),
            ..valid.clone()
        };
        MailboxService::absorb(&mut wallet, &[malformed]);
        assert!(MailboxService::ready(&wallet).is_empty());

        MailboxService::absorb(&mut wallet, &[valid.clone(), valid]);
        assert_eq!(MailboxService::ready(&wallet).len(), 1);
        assert_eq!(wallet.mailbox_seen_txids.len(), 1);
    }
}
