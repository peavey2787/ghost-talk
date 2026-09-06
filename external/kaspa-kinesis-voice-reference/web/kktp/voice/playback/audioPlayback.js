/**
 * audioPlayback.js - Play decoded PCM in order; jitter buffer to avoid DAG stutter
 * Single responsibility: buffer by queue position + chunk index, jitter delay, then play.
 *
 * Half-duplex: one queue position per recording → jitter once, play all chunks, done.
 * Full-duplex: same queue position reused across many 350 ms windows → jitter only for the
 * first batch; every subsequent batch with the same key plays immediately at _nextStartTime
 * for gapless continuous playback.
 */

import { Logger, LogModule } from "../../core/logger.js";
import { queuePositionKey } from "../queue/queuePosition.js";
import { decodeOpusToPcm } from "../audio/opusDecoder.js";

const log = Logger.create(LogModule.voice.playback);

/** Idle timeout before a live-continuation stream entry is cleaned up (ms). */
const STALE_STREAM_MS = 5_000;

/**
 * @param {Object} options
 * @param {number} [options.jitterBufferMs=400] - Delay before starting playback (300-500ms)
 */
export class AudioPlayback {
  constructor(options = {}) {
    this._jitterMs = options.jitterBufferMs ?? 400;
    this._context = null;
    /**
     * @type {Map<string, {
     *   chunks: Map<number, Uint8Array>,
     *   totalChunks: number|null,
     *   jitterTimer: number|null,
     *   started: boolean,
     *   playing: boolean,
     *   pendingChunks: Map<number, Uint8Array>|null,
     *   pendingTotal: number|null,
     *   staleTimer: number|null
     * }>}
     */
    this._streams = new Map();
    this._nextStartTime = 0;
  }

  /**
   * Get or create AudioContext for playback.
   */
  _getContext() {
    if (this._context) return this._context;
    this._context = new (window.AudioContext || window.webkitAudioContext)();
    return this._context;
  }

  /**
   * Push an audio chunk for a given queue position.
   * First batch for a key gets the jitter delay; subsequent batches (same key,
   * i.e. full-duplex continuation windows) play immediately for gapless audio.
   * @param {{ blueScore: number, txIndexInBlock: number }} queuePosition
   * @param {number} chunkIndex
   * @param {number} totalChunks
   * @param {Uint8Array} opusBytes
   */
  pushChunk(queuePosition, chunkIndex, totalChunks, opusBytes) {
    const key = queuePositionKey(queuePosition);
    let stream = this._streams.get(key);
    if (!stream) {
      stream = {
        chunks: new Map(),
        totalChunks: null,
        jitterTimer: null,
        started: false,
        playing: false,
        pendingChunks: null,
        pendingTotal: null,
        staleTimer: null,
      };
      this._streams.set(key, stream);
    }

    this._resetStaleTimer(key, stream);

    // Chunks arriving while _playBatch is mid-decode go into a pending slot
    // so their indices (which restart at 0 per window) don't collide.
    if (stream.playing) {
      if (!stream.pendingChunks) stream.pendingChunks = new Map();
      stream.pendingChunks.set(chunkIndex, new Uint8Array(opusBytes));
      if (totalChunks != null) stream.pendingTotal = totalChunks;
      return;
    }

    stream.chunks.set(chunkIndex, new Uint8Array(opusBytes));
    if (totalChunks != null) stream.totalChunks = totalChunks;

    // Continuation: stream already played at least once — skip jitter
    if (stream.started) {
      this._tryPlayBatch(key, stream);
      return;
    }

    // First batch: apply jitter buffer
    const tryStart = () => {
      if (stream.started) return;
      if (this._isBatchReady(stream)) {
        stream.started = true;
        if (stream.jitterTimer != null) clearTimeout(stream.jitterTimer);
        this._playBatch(key, stream);
      }
    };

    if (!stream.jitterTimer && !stream.started) {
      stream.jitterTimer = setTimeout(() => {
        stream.jitterTimer = null;
        tryStart();
      }, this._jitterMs);
    }
    tryStart();
  }

  /** @returns {boolean} Whether chunk 0 (and chunk 1 or single-chunk) are present. */
  _isBatchReady(stream) {
    const has0 = stream.chunks.has(0);
    const has1 = stream.chunks.has(1);
    return has0 && (has1 || stream.totalChunks === 1);
  }

  /** Play a continuation batch immediately if ready. */
  _tryPlayBatch(key, stream) {
    if (this._isBatchReady(stream)) {
      this._playBatch(key, stream);
    }
  }

  /**
   * Decode and schedule all current chunks, then reset the stream for the next
   * batch instead of deleting it, enabling gapless full-duplex continuation.
   */
  async _playBatch(key, stream) {
    stream.playing = true;

    const ctx = this._getContext();
    if (ctx.state === "suspended") await ctx.resume();

    const indices = Array.from(stream.chunks.keys()).sort((a, b) => a - b);
    log.info("AudioPlayback: playing batch", { key, chunks: indices.length });
    let currentTime = Math.max(this._nextStartTime, ctx.currentTime);

    for (const idx of indices) {
      const opusBytes = stream.chunks.get(idx);
      if (!opusBytes?.length) continue;

      try {
        const { channelData, sampleRate } = await decodeOpusToPcm(opusBytes, ctx);
        if (!channelData?.length) continue;

        const numChannels = channelData.length;
        const length = channelData[0].length;
        const buffer = ctx.createBuffer(numChannels, length, sampleRate);
        for (let c = 0; c < numChannels; c++) buffer.copyToChannel(channelData[c], c);

        const source = ctx.createBufferSource();
        source.buffer = buffer;
        source.connect(ctx.destination);
        source.start(currentTime);
        currentTime += buffer.duration;
      } catch (e) {
        log.warn("audioPlayback decode/play chunk failed", e?.message ?? e);
      }
    }

    this._nextStartTime = currentTime;

    // Reset for the next batch but keep the stream entry alive for continuation.
    stream.chunks.clear();
    stream.totalChunks = null;
    stream.playing = false;

    // Promote any chunks that arrived while we were decoding/playing.
    if (stream.pendingChunks && stream.pendingChunks.size > 0) {
      stream.chunks = stream.pendingChunks;
      stream.totalChunks = stream.pendingTotal;
      stream.pendingChunks = null;
      stream.pendingTotal = null;
      this._tryPlayBatch(key, stream);
    } else {
      stream.pendingChunks = null;
      stream.pendingTotal = null;
    }
  }

  /**
   * Reset the idle cleanup timer. Fires after STALE_STREAM_MS of inactivity
   * to free the stream entry (prevents unbounded memory growth).
   */
  _resetStaleTimer(key, stream) {
    if (stream.staleTimer != null) clearTimeout(stream.staleTimer);
    stream.staleTimer = setTimeout(() => {
      const s = this._streams.get(key);
      if (s && !s.playing) {
        if (s.jitterTimer != null) clearTimeout(s.jitterTimer);
        this._streams.delete(key);
        log.debug("AudioPlayback: stale stream cleaned up", { key });
      }
    }, STALE_STREAM_MS);
  }

  setJitterBufferMs(ms) {
    this._jitterMs = Math.max(300, Math.min(500, ms));
  }

  close() {
    for (const stream of this._streams.values()) {
      if (stream.jitterTimer) clearTimeout(stream.jitterTimer);
      if (stream.staleTimer) clearTimeout(stream.staleTimer);
    }
    this._streams.clear();
    if (this._context) this._context.close();
    this._context = null;
  }
}
