import type { MailboxEvent, WalletRecord } from "./model";

const GHST_MAGIC = "47485354";
const GHST_VERSION = 1;
const GHST_HEADER_BYTES = 30;
const KSPT_V1_MAX_PAYLOAD_BYTES = 65_535;
const GHST_DATA_MAX_BYTES = KSPT_V1_MAX_PAYLOAD_BYTES - GHST_HEADER_BYTES;
const MAX_FRAGMENTS = 128;
const MAX_PENDING_PACKETS = 128;
const MAX_ENVELOPE_BYTES = MAX_FRAGMENTS * GHST_DATA_MAX_BYTES;
const MAX_PENDING_BYTES = 2 * 1024 * 1024;
const MAX_SEEN_TXIDS = 2048;

export interface ReadyEnvelope {
  packetId: string;
  envelopeHex: string;
  transactionId: string;
  blockTime?: number;
}

export function absorbMailboxEvents(
  wallet: WalletRecord,
  events: MailboxEvent[],
): WalletRecord {
  const pending = structuredClone(wallet.mailboxPending ?? {});
  const seen = [...(wallet.mailboxSeenTxids ?? [])];
  const seenSet = new Set(seen);
  for (const event of events) {
    const txid = event.transaction_id.toLowerCase();
    if (/^[0-9a-f]{64}$/.test(txid) && seenSet.has(txid)) continue;
    const frame = parseFrame(event.payload_hex);
    if (!frame) continue;
    const existing = pending[frame.packetId];
    if (existing && existing.count !== frame.count) {
      delete pending[frame.packetId];
      continue;
    }
    const bucket = existing
      ? { ...existing, parts: { ...existing.parts }, txids: { ...(existing.txids ?? {}) } }
      : { count: frame.count, parts: {}, txids: {}, blockTime: event.block_time };
    const key = frame.index.toString();
    const previous = bucket.parts[key];
    if (previous && previous !== frame.dataHex) {
      delete pending[frame.packetId];
      continue;
    }
    bucket.parts[key] = frame.dataHex;
    bucket.txids[key] = event.transaction_id;
    bucket.blockTime = Math.max(bucket.blockTime ?? 0, event.block_time ?? 0) || undefined;
    if (bucketBytes(bucket.parts) > MAX_ENVELOPE_BYTES) {
      delete pending[frame.packetId];
      continue;
    }
    pending[frame.packetId] = bucket;
    if (/^[0-9a-f]{64}$/.test(txid)) {
      seen.push(txid);
      seenSet.add(txid);
      while (seen.length > MAX_SEEN_TXIDS) {
        const removed = seen.shift();
        if (removed) seenSet.delete(removed);
      }
    }
  }

  trimPending(pending);
  return { ...wallet, mailboxPending: pending, mailboxSeenTxids: seen.length ? seen : undefined };
}

export function readyEnvelopes(wallet: WalletRecord): ReadyEnvelope[] {
  const ready: ReadyEnvelope[] = [];
  for (const [packetId, bucket] of Object.entries(wallet.mailboxPending ?? {})) {
    if (bucket.count < 1 || bucket.count > MAX_FRAGMENTS) continue;
    const parts: string[] = [];
    let transactionId = "";
    for (let index = 0; index < bucket.count; index += 1) {
      const part = bucket.parts[index.toString()];
      if (part === undefined) {
        parts.length = 0;
        break;
      }
      parts.push(part);
      transactionId = bucket.txids?.[index.toString()] ?? transactionId;
    }
    if (!parts.length) continue;
    const envelopeHex = parts.join("");
    if (envelopeHex.length / 2 > MAX_ENVELOPE_BYTES) continue;
    ready.push({ packetId, envelopeHex, transactionId, blockTime: bucket.blockTime });
  }
  return ready;
}

export function removeEnvelope(wallet: WalletRecord, packetId: string): WalletRecord {
  const pending = { ...(wallet.mailboxPending ?? {}) };
  delete pending[packetId];
  return { ...wallet, mailboxPending: pending };
}

function parseFrame(payloadHex: string): {
  packetId: string;
  index: number;
  count: number;
  dataHex: string;
} | null {
  const hex = payloadHex.toLowerCase();
  if (
    !/^[0-9a-f]*$/.test(hex)
    || hex.length < GHST_HEADER_BYTES * 2
    || hex.length / 2 > KSPT_V1_MAX_PAYLOAD_BYTES
    || hex.slice(0, 8) !== GHST_MAGIC
  ) {
    return null;
  }
  const version = Number.parseInt(hex.slice(8, 10), 16);
  if (version !== GHST_VERSION) return null;
  const packetId = hex.slice(12, 44);
  const index = littleEndianNumber(hex.slice(44, 48));
  const count = littleEndianNumber(hex.slice(48, 52));
  const declaredLength = littleEndianNumber(hex.slice(52, 60));
  const dataHex = hex.slice(60);
  if (
    count < 1
    || count > MAX_FRAGMENTS
    || index >= count
    || declaredLength !== dataHex.length / 2
    || declaredLength > GHST_DATA_MAX_BYTES
  ) {
    return null;
  }
  return { packetId, index, count, dataHex };
}

function littleEndianNumber(hex: string): number {
  const bytes = hex.match(/../g) ?? [];
  let value = 0;
  for (let index = 0; index < bytes.length; index += 1) {
    value += Number.parseInt(bytes[index], 16) * (2 ** (8 * index));
  }
  return value;
}

function trimPending(
  pending: NonNullable<WalletRecord["mailboxPending"]>,
): void {
  const keys = Object.keys(pending);
  while (keys.length > MAX_PENDING_PACKETS || pendingBytes(pending) > MAX_PENDING_BYTES) {
    const key = keys.shift();
    if (key === undefined) break;
    delete pending[key];
  }
}

function pendingBytes(pending: NonNullable<WalletRecord["mailboxPending"]>): number {
  return Object.values(pending).reduce((total, bucket) => total + bucketBytes(bucket.parts), 0);
}

function bucketBytes(parts: Record<string, string>): number {
  return Object.values(parts).reduce((total, part) => total + part.length / 2, 0);
}
