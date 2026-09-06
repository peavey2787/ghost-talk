/**
 * voiceInboundHandler.js - Parse inbound voice matches; route RTT / push audio to playback.
 * Single responsibility: match processing only. No scanner subscription; the engine routes matches here.
 */

import { Logger, LogModule } from "../../core/logger.js";
import { getPayloadType, parseRttPayload, parseAudioChunkPayload } from "../payload/voicePayloadFormat.js";

const log = Logger.create(LogModule.voice.inbound);

export const VoiceInboundEvent = Object.freeze({
  RTT: "rtt",
  VOICE_CHUNK: "voiceChunk",
});

/**
 * @param {Object} options
 * @param {string} options.prefixRtt - RTT payload prefix
 * @param {string} options.prefixAudio - Audio chunk payload prefix
 * @param {InstanceType<import('../playback/audioPlayback.js').AudioPlayback>} options.playback - AudioPlayback instance
 * @param {{ decryptPayload: (s: string) => string } | null} [options.voiceSession]
 * @param {function(string): boolean} [options.isOwnSentTx] - Return true to skip own-sent chunks
 * @param {function(string, any): void} [options.onRtt] - Callback (event, matchData)
 * @param {function(string, any): void} [options.onVoiceChunk] - Callback (event, chunkData)
 */
export class VoiceInboundHandler {
  constructor(options = {}) {
    this._prefixRtt = options.prefixRtt ?? "KKTP:V:RTT:";
    this._prefixAudio = options.prefixAudio ?? "KKTP:V:AUDIO:";
    this._playback = options.playback;
    this._voiceSession = options.voiceSession ?? null;
    this._isOwnSentTx = options.isOwnSentTx ?? (() => false);
    this._onRtt = options.onRtt;
    this._onVoiceChunk = options.onVoiceChunk;
  }

  /**
   * Process a single scanner match routed by the engine.
   * @param {Object} data - Dehydrated match object from scanner/indexer
   */
  handleMatch(data) {
    const payload = data?.decodedPayload ?? data?.payload;
    if (typeof payload !== "string") return;

    const type = getPayloadType(payload, this._prefixRtt, this._prefixAudio);
    if (type === "rtt") {
      const parsed = parseRttPayload(payload, this._prefixRtt, this._voiceSession);
      if (parsed.ok) {
        const txid = data?.txid ?? data?.transactionId;
        log.info("VoiceInbound: RTT received", { txid });
        if (this._onRtt) this._onRtt(VoiceInboundEvent.RTT, { ...data, meta: parsed.meta });
      }
      return;
    }

    if (type === "audio") {
      const parsed = parseAudioChunkPayload(payload, this._prefixAudio, this._voiceSession);
      if (!parsed.ok) return;
      const txId = data?.txid ?? data?.transactionId;
      if (txId && this._isOwnSentTx(txId)) {
        log.debug("VoiceInbound: skipping own tx", { txId });
        return;
      }
      log.info("VoiceInbound: audio chunk received", { txId, chunkIndex: parsed.chunkIndex, totalChunks: parsed.totalChunks });
      if (this._playback) {
        this._playback.pushChunk(parsed.queuePosition, parsed.chunkIndex, parsed.totalChunks, parsed.opusBytes);
      }
      if (this._onVoiceChunk) {
        this._onVoiceChunk(VoiceInboundEvent.VOICE_CHUNK, {
          ...data,
          queuePosition: parsed.queuePosition,
          chunkIndex: parsed.chunkIndex,
          totalChunks: parsed.totalChunks,
          opusBytes: parsed.opusBytes,
        });
      }
    }
  }

  /**
   * Set or clear the voice session (e.g. after bootstrap).
   * @param {{ decryptPayload: (s: string) => string } | null} voiceSession
   */
  setVoiceSession(voiceSession) {
    this._voiceSession = voiceSession ?? null;
  }
}
