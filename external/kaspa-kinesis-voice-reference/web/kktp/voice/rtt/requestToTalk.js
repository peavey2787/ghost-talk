/**
 * requestToTalk.js - Build and send RTT transaction; return txId
 * Single responsibility: RTT payload + send; no confirmation.
 */

import { serializeRttPayload } from "../payload/voicePayloadFormat.js";
import { sendRtt } from "../tx/voiceTxSender.js";

/**
 * Send Request-to-Talk transaction.
 * @param {Object} adapter - KaspaAdapter
 * @param {string} prefix - RTT prefix (e.g. PREFIX_RTT from voiceConstants)
 * @param {Object} [meta] - Optional { ts?, id? }
 * @param {{ encryptPayload: (s: string) => string } | null} [voiceSession] - When set, RTT body is encrypted
 * @returns {Promise<string>} Transaction id
 */
export async function requestToTalk(adapter, prefix, meta = {}, voiceSession = null) {
  const payload = serializeRttPayload(prefix, meta, voiceSession);
  return await sendRtt(adapter, payload);
}
