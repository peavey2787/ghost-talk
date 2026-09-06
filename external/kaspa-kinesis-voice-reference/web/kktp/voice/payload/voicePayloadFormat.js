/**
 * voicePayloadFormat.js - Serialize/deserialize RTT and audio chunk payloads for Kaspa tx payloads
 * Single responsibility: wire-format contract only; no I/O.
 */

/**
 * Build RTT payload string (prefix + optional JSON metadata). When voiceSession is set, body is encrypted.
 * @param {string} prefix - RTT prefix
 * @param {Object} [meta] - Optional { ts?, id? }
 * @param {{ encryptPayload: (s: string) => string }} [voiceSession] - When set, encrypt RTT body
 * @returns {string}
 */
export function serializeRttPayload(prefix, meta = {}, voiceSession = null) {
  const body = meta.ts != null || meta.id != null ? JSON.stringify(meta) : "";
  if (voiceSession?.encryptPayload) {
    return prefix + voiceSession.encryptPayload(body || "{}");
  }
  return prefix + body;
}

/**
 * Parse RTT payload; returns metadata if present. When voiceSession is set and body does not look like JSON, decrypt first.
 * @param {string} payload - Full payload string
 * @param {string} prefix - RTT prefix
 * @param {{ decryptPayload: (s: string) => string }} [voiceSession] - When set, decrypt ciphertext body
 * @returns {{ ok: true, meta?: Object } | { ok: false }}
 */
export function parseRttPayload(payload, prefix, voiceSession = null) {
  if (typeof payload !== "string" || !payload.startsWith(prefix)) return { ok: false };
  let rest = payload.slice(prefix.length).trim();
  if (!rest) return { ok: true, meta: {} };
  if (voiceSession?.decryptPayload && rest[0] !== "{") {
    try {
      rest = voiceSession.decryptPayload(rest);
    } catch {
      return { ok: false };
    }
  }
  try {
    const meta = JSON.parse(rest);
    return { ok: true, meta };
  } catch {
    return { ok: true, meta: {} };
  }
}

/**
 * Build audio chunk payload (prefix + JSON with base64 Opus, or prefix + encrypted compact binary when voiceSession set).
 * @param {string} prefix - Audio prefix
 * @param {{ blueScore: number, txIndexInBlock: number }} queuePosition
 * @param {number} chunkIndex
 * @param {number} totalChunks
 * @param {Uint8Array} opusBytes
 * @param {{ encryptPayload: (s: string) => string }} [voiceSession] - When set, use compact binary then base64 once then encrypt
 * @returns {string}
 */
export function serializeAudioChunkPayload(prefix, queuePosition, chunkIndex, totalChunks, opusBytes, voiceSession = null) {
  if (voiceSession?.encryptPayload) {
    const header = new ArrayBuffer(20);
    const view = new DataView(header);
    view.setBigUint64(0, BigInt(queuePosition.blueScore), false);
    view.setUint32(8, queuePosition.txIndexInBlock >>> 0, false);
    view.setUint32(12, chunkIndex >>> 0, false);
    view.setUint32(16, totalChunks >>> 0, false);
    const binary = new Uint8Array(20 + opusBytes.length);
    binary.set(new Uint8Array(header), 0);
    binary.set(opusBytes, 20);
    const plaintext = uint8ArrayToBase64(binary);
    return prefix + voiceSession.encryptPayload(plaintext);
  }
  const b64 = uint8ArrayToBase64(opusBytes);
  const obj = {
    blueScore: queuePosition.blueScore,
    txIndexInBlock: queuePosition.txIndexInBlock,
    chunkIndex,
    totalChunks,
    b: b64,
  };
  return prefix + JSON.stringify(obj);
}

/**
 * Parse audio chunk payload. When voiceSession is set and body does not look like JSON, decrypt then parse compact binary (20-byte header + Opus).
 * @param {string} payload - Full payload string
 * @param {string} prefix - Audio prefix
 * @param {{ decryptPayload: (s: string) => string }} [voiceSession] - When set, decrypt then decode binary
 * @returns {{ ok: true, queuePosition: { blueScore: number, txIndexInBlock: number }, chunkIndex: number, totalChunks: number, opusBytes: Uint8Array } | { ok: false }}
 */
export function parseAudioChunkPayload(payload, prefix, voiceSession = null) {
  if (typeof payload !== "string" || !payload.startsWith(prefix)) return { ok: false };
  const rest = payload.slice(prefix.length);
  if (voiceSession?.decryptPayload && rest[0] !== "{") {
    try {
      const plaintext = voiceSession.decryptPayload(rest);
      const binary = base64ToUint8Array(plaintext);
      if (binary.length < 20) return { ok: false };
      const view = new DataView(binary.buffer, binary.byteOffset, binary.byteLength);
      const blueScore = Number(view.getBigUint64(0, false));
      const txIndexInBlock = view.getUint32(8, false);
      const chunkIndex = view.getUint32(12, false);
      const totalChunks = view.getUint32(16, false);
      const opusBytes = binary.slice(20);
      return {
        ok: true,
        queuePosition: { blueScore, txIndexInBlock },
        chunkIndex,
        totalChunks,
        opusBytes,
      };
    } catch {
      return { ok: false };
    }
  }
  try {
    const obj = JSON.parse(rest);
    const blueScore = Number(obj.blueScore);
    const txIndexInBlock = Number(obj.txIndexInBlock);
    const chunkIndex = Number(obj.chunkIndex);
    const totalChunks = Number(obj.totalChunks);
    const b = obj.b;
    if (
      !Number.isFinite(blueScore) ||
      !Number.isFinite(txIndexInBlock) ||
      !Number.isFinite(chunkIndex) ||
      !Number.isFinite(totalChunks) ||
      typeof b !== "string"
    ) {
      return { ok: false };
    }
    const opusBytes = base64ToUint8Array(b);
    return {
      ok: true,
      queuePosition: { blueScore, txIndexInBlock },
      chunkIndex,
      totalChunks,
      opusBytes,
    };
  } catch {
    return { ok: false };
  }
}

/**
 * Detect payload type by prefix.
 * @param {string} payload
 * @param {string} prefixRtt
 * @param {string} prefixAudio
 * @returns {'rtt'|'audio'|null}
 */
export function getPayloadType(payload, prefixRtt, prefixAudio) {
  if (typeof payload !== "string") return null;
  if (payload.startsWith(prefixRtt)) return "rtt";
  if (payload.startsWith(prefixAudio)) return "audio";
  return null;
}

function uint8ArrayToBase64(u8) {
  let binary = "";
  for (let i = 0; i < u8.length; i++) binary += String.fromCharCode(u8[i]);
  return btoa(binary);
}

function base64ToUint8Array(base64) {
  const bin = atob(base64);
  const u8 = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) u8[i] = bin.charCodeAt(i);
  return u8;
}
