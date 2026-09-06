/**
 * voiceConstants.js - Voice module configuration (RTT, Opus, jitter, payload size)
 * Single responsibility: export prefixes, defaults, and limits for kktp/voice.
 */

/** Payload prefixes for scanner (RTT and audio chunk txs) — global fallback when no groupMailboxId */
export const PREFIX_RTT = "KKTP:V:RTT:";
export const PREFIX_AUDIO = "KKTP:V:AUDIO:";

/**
 * Build session-scoped voice prefixes (same pattern as lobby group: KKTP:GROUP:{groupMailboxId}:).
 * Everyone in the same lobby subscribes to these; scanner then catches only this session's voice txs.
 * @param {string} groupMailboxId - Lobby group mailbox ID (e.g. from lobby.getGroupMailboxId())
 * @returns {{ prefixRtt: string, prefixAudio: string }}
 */
export function buildVoicePrefixesForSession(groupMailboxId) {
  if (!groupMailboxId || typeof groupMailboxId !== "string") {
    return { prefixRtt: PREFIX_RTT, prefixAudio: PREFIX_AUDIO };
  }
  const base = `KKTP:V:${groupMailboxId}:`;
  return {
    prefixRtt: `${base}RTT:`,
    prefixAudio: `${base}AUDIO:`,
  };
}

/** Default lease duration in seconds (configurable by downstream devs) */
export const DEFAULT_LEASE_SECONDS = 20;

/** Opus: bitrate range 6–510 kbps; default for voice */
export const OPUS_BITRATE_MIN = 6000;
export const OPUS_BITRATE_MAX = 510000;
export const OPUS_BITRATE_DEFAULT = 24000;

/** Kaspa payload limit (bytes); keep chunks under this */
export const MAX_PAYLOAD_BYTES = 32 * 1024;

/** Chunk size for audio (bytes) — each chunk fits in one tx payload with metadata */
export const DEFAULT_CHUNK_SIZE = Math.min(MAX_PAYLOAD_BYTES - 256, 32 * 1024 - 256);

/** Jitter buffer: wait 300–500 ms before starting playback to avoid DAG propagation stutter */
export const JITTER_BUFFER_MS_MIN = 300;
export const JITTER_BUFFER_MS_MAX = 500;
export const JITTER_BUFFER_MS_DEFAULT = 400;

/** RTT confirmation timeout (ms) */
export const RTT_CONFIRM_TIMEOUT_MS_DEFAULT = 30_000;

/**
 * Merge user config with defaults.
 * When groupMailboxId is set, prefixes are session-scoped (like lobby KKTP:GROUP:{id}:) so all peers in the session catch each other's voice txs.
 * @param {Object} [user] - User overrides; user.groupMailboxId triggers session-scoped prefixes
 * @returns {Object} Full config
 */
export function getVoiceConfig(user = {}) {
  const scoped = user.groupMailboxId
    ? buildVoicePrefixesForSession(user.groupMailboxId)
    : { prefixRtt: PREFIX_RTT, prefixAudio: PREFIX_AUDIO };
  return {
    prefixRtt: user.prefixRtt ?? scoped.prefixRtt,
    prefixAudio: user.prefixAudio ?? scoped.prefixAudio,
    leaseSeconds: user.leaseSeconds ?? DEFAULT_LEASE_SECONDS,
    opusBitrate: user.opusBitrate ?? OPUS_BITRATE_DEFAULT,
    maxPayloadBytes: user.maxPayloadBytes ?? MAX_PAYLOAD_BYTES,
    chunkSize: user.chunkSize ?? DEFAULT_CHUNK_SIZE,
    jitterBufferMs: Math.max(
      JITTER_BUFFER_MS_MIN,
      Math.min(JITTER_BUFFER_MS_MAX, user.jitterBufferMs ?? JITTER_BUFFER_MS_DEFAULT)
    ),
    rttConfirmTimeoutMs: user.rttConfirmTimeoutMs ?? RTT_CONFIRM_TIMEOUT_MS_DEFAULT,
  };
}
