/**
 * voiceTxSender.js - Send RTT and audio-chunk transactions via adapter
 * Single responsibility: submit voice txs; payload built by voicePayloadFormat.
 */

/** Minimal KAS amount for voice txs (dust) */
const VOICE_TX_AMOUNT_KAS = "0.5";

/**
 * Send RTT transaction; returns transaction id.
 * @param {Object} adapter - KaspaAdapter (or { send })
 * @param {string} payload - Full RTT payload string
 * @param {string} [toAddress] - Optional; adapter may use own address
 * @returns {Promise<string>} Transaction id
 */
export async function sendRtt(adapter, payload, toAddress) {
  const result = await adapter.send({
    amount: VOICE_TX_AMOUNT_KAS,
    toAddress: toAddress || adapter.address,
    payload,
  });
  return normalizeTxId(result);
}

/**
 * Send one audio chunk transaction.
 * @param {Object} adapter - KaspaAdapter
 * @param {string} payload - Full audio chunk payload string
 * @param {string} [toAddress]
 * @returns {Promise<string>} Transaction id
 */
export async function sendAudioChunk(adapter, payload, toAddress) {
  const result = await adapter.send({
    amount: VOICE_TX_AMOUNT_KAS,
    toAddress: toAddress || adapter.address,
    payload,
  });
  return normalizeTxId(result);
}

/**
 * Canonical form for tx id so send-path and match-path compare equal.
 * @param {string|undefined|null} id - Raw transaction id
 * @returns {string}
 */
export function canonicalTxId(id) {
  const s = String(id ?? "").trim().replace(/^0x/i, "").toLowerCase();
  return s;
}

function normalizeTxId(result) {
  if (!result) throw new Error("Voice tx: no result from send");
  const id =
    result.transactionId ?? result.txId ?? result.transactionIds?.[0] ?? result.txid;
  if (id) return canonicalTxId(id);
  throw new Error("Voice tx: result has no transaction id");
}
