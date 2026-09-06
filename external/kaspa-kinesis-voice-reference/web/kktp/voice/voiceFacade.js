/**
 * voiceFacade.js - Single entry for KKTP voice (RTT + Opus in Kaspa payloads)
 * Orchestrates RTT, lease, capture, encode, chunk, send, inbound, playback.
 * Supports half-duplex (walkie-talkie) and full-duplex (live chat) modes.
 */

import { EventEmitter } from "../core/eventEmitter.js";
import { Logger, LogModule } from "../core/logger.js";
import { getVoiceConfig } from "./config/voiceConstants.js";
import { createVoiceSession, createVoiceSessionFromGroupKey } from "./crypto/voiceSession.js";
import { requestToTalk } from "./rtt/requestToTalk.js";
import { canonicalTxId } from "./tx/voiceTxSender.js";
import { waitForConfirmation } from "./rtt/confirmationWatcher.js";
import { LeaseManager, LeaseEvent } from "./lease/leaseManager.js";
import { getMicrophoneStream, stopStream } from "./audio/audioCapture.js";
import { createOpusEncoder } from "./audio/opusEncoder.js";
import { chunkOpusBytes } from "./audio/audioChunker.js";
import { serializeAudioChunkPayload } from "./payload/voicePayloadFormat.js";
import { sendAudioChunk } from "./tx/voiceTxSender.js";
import { AudioPlayback } from "./playback/audioPlayback.js";
import { VoiceInboundHandler } from "./inbound/voiceInboundHandler.js";

const log = Logger.create(LogModule.voice.voiceFacade);

/** Full-duplex: chunk interval baseline (ms) */
const LIVE_CHUNK_INTERVAL_MS = 350;
/** Full-duplex: RTT heartbeat period (ms) — presence check-in */
const LIVE_RTT_HEARTBEAT_MS = 60_000;
/** VAD: RMS threshold below which a chunk is considered silence */
const VAD_SILENCE_RMS_THRESHOLD = 0.01;

export const VoiceFacadeEvent = Object.freeze({
  RTT: "rtt",
  RTT_SENT: "rttSent",
  AUDIO_CHUNK_SENT: "audioChunkSent",
  LEASE_EXPIRED: "leaseExpired",
  VOICE_CHUNK: "voiceChunk",
  ERROR: "error",
});

export class VoiceFacade extends EventEmitter {
  constructor() {
    super();
    this._adapter = null;
    this._portal = null;
    this._config = null;
    this._lease = new LeaseManager();
    this._playback = null;
    this._inbound = null;
    this._stream = null;
    this._encoder = null;
    this._recordedChunks = [];
    this._queuePosition = null;
    this._voiceSession = null;
    /** @type {Set<string>} */
    this._sentTxIds = new Set();

    // Full-duplex state
    this._liveMode = false;
    this._liveSending = false;
    this._liveStopRequested = false;
    this._liveHeartbeatTimer = null;
    this._muted = false;
  }

  /**
   * @param {Object} options
   * @param {Object} options.adapter - KaspaAdapter
   * @param {Object} options.portal - KaspaPortal (for scanner / confirmation)
   * @param {string} [options.groupMailboxId] - Lobby group mailbox ID for session-scoped prefixes
   * @param {Object} [options.config] - Overrides for leaseSeconds, opusBitrate, jitterBufferMs, etc.
   * @param {{ encryptPayload: (s: string) => string, decryptPayload: (s: string) => string }} [options.voiceSession]
   * @param {{ sendEncrypted: (obj: object) => void, registerIncomingHandler: (handler: (msg: object) => void) => void, voiceKeyIndex?: number }} [options.bootstrapVoiceSession]
   * @param {Uint8Array} [options.groupKeyBytes]
   */
  async init(options = {}) {
    this._adapter = options.adapter ?? this._adapter;
    this._portal = options.portal ?? this._portal;
    this._config = getVoiceConfig({
      ...options.config,
      groupMailboxId: options.groupMailboxId ?? options.config?.groupMailboxId,
    });

    if (options.voiceSession) {
      this._voiceSession = options.voiceSession;
    } else if (options.groupKeyBytes) {
      this._voiceSession = createVoiceSessionFromGroupKey(options.groupKeyBytes);
    } else if (options.bootstrapVoiceSession) {
      const { sendEncrypted, registerIncomingHandler, voiceKeyIndex } = options.bootstrapVoiceSession;
      this._voiceSession = await createVoiceSession(this._adapter, { voiceKeyIndex });
      if (typeof registerIncomingHandler === "function") {
        registerIncomingHandler(this._voiceSession.handleIncoming);
      }
      this._voiceSession.establishViaLobby(sendEncrypted);
    }

    this._lease.setLeaseSeconds(this._config.leaseSeconds);
    this._lease.on(LeaseEvent.EXPIRED, () => this.emit(VoiceFacadeEvent.LEASE_EXPIRED));

    // Register voice prefixes with the adapter so the scanner catches them
    this._adapter?.addPrefix?.(this._config.prefixRtt);
    this._adapter?.addPrefix?.(this._config.prefixAudio);

    this._playback = new AudioPlayback({ jitterBufferMs: this._config.jitterBufferMs });
    this._inbound = new VoiceInboundHandler({
      prefixRtt: this._config.prefixRtt,
      prefixAudio: this._config.prefixAudio,
      playback: this._playback,
      voiceSession: this._voiceSession,
      isOwnSentTx: (id) => this._sentTxIds.has(canonicalTxId(id)),
      onRtt: (_event, data) => this.emit(VoiceFacadeEvent.RTT, data),
      onVoiceChunk: (_event, data) => this.emit(VoiceFacadeEvent.VOICE_CHUNK, data),
    });

    log.info("VoiceFacade: joined voice chat", {
      groupMailboxId: options.groupMailboxId ?? null,
      prefixRtt: this._config.prefixRtt,
      prefixAudio: this._config.prefixAudio,
    });
  }

  // ---------------------------------------------------------------------------
  // Half-duplex: RTT → Record → Stop → Send
  // ---------------------------------------------------------------------------

  /**
   * Send RTT tx, wait for block confirmation, start lease, return queue position.
   * @returns {Promise<{ blockHash: string, blueScore: number, txIndexInBlock: number }>}
   */
  async requestToTalk() {
    if (!this._adapter || !this._portal) {
      throw new Error("VoiceFacade: init({ adapter, portal }) required");
    }

    const txId = await requestToTalk(this._adapter, this._config.prefixRtt, {}, this._voiceSession);
    if (txId) this._sentTxIds.add(canonicalTxId(txId));
    log.info("VoiceFacade: RTT sent", { txId });
    this.emit(VoiceFacadeEvent.RTT_SENT, { txId });

    const position = await waitForConfirmation(this._portal, txId, this._config.rttConfirmTimeoutMs);
    log.info("VoiceFacade: RTT confirmed", { position });
    this._queuePosition = position;
    this._lease.start();
    return position;
  }

  /**
   * Start recording (mic -> Opus). Call stopRecording() to finalize and send.
   */
  async startRecording() {
    if (this._encoder) return;
    if (!this._adapter) {
      this.emit(VoiceFacadeEvent.ERROR, new Error("VoiceFacade: adapter not set"));
      return;
    }

    try {
      this._recordedChunks = [];
      this._stream = await getMicrophoneStream({ audio: true });
      this._encoder = createOpusEncoder({
        stream: this._stream,
        bitrate: this._config.opusBitrate,
        onData: (u8) => this._recordedChunks.push(u8),
      });
      this._encoder.start();
      log.info("VoiceFacade: recording started");
    } catch (e) {
      log.error("VoiceFacade startRecording", e?.message ?? e);
      this.emit(VoiceFacadeEvent.ERROR, e);
      if (this._stream) stopStream(this._stream);
      this._stream = null;
    }
  }

  /**
   * Stop recording, chunk Opus, send each chunk as a tx.
   */
  async stopRecording() {
    if (!this._encoder) return;

    try {
      await this._encoder.stop();
      this._encoder.close();
      this._encoder = null;
      stopStream(this._stream);
      this._stream = null;
    } catch (e) {
      log.warn("VoiceFacade stopRecording encoder/stream", e?.message ?? e);
    }
    log.info("VoiceFacade: recording stopped");

    const position = this._queuePosition;
    if (!position || this._recordedChunks.length === 0) {
      this._recordedChunks = [];
      return;
    }

    const fullOpus = concatenateUint8Arrays(this._recordedChunks);
    this._recordedChunks = [];

    const maxChunkBytes = Math.min(this._config.chunkSize, this._config.maxPayloadBytes - 512);
    const chunks = chunkOpusBytes(fullOpus, maxChunkBytes);
    const totalChunks = chunks.length;
    log.info("VoiceFacade: sending chunks", { totalChunks });

    for (let i = 0; i < chunks.length; i++) {
      try {
        const payload = serializeAudioChunkPayload(
          this._config.prefixAudio, position, i, totalChunks, chunks[i], this._voiceSession,
        );
        const txId = await sendAudioChunk(this._adapter, payload);
        if (txId) this._sentTxIds.add(canonicalTxId(txId));
        log.info("VoiceFacade: audio chunk sent", { txId, chunkIndex: i, totalChunks });
        this.emit(VoiceFacadeEvent.AUDIO_CHUNK_SENT, { txId, chunkIndex: i, totalChunks });
      } catch (e) {
        log.error("VoiceFacade sendAudioChunk", e?.message ?? e);
        this.emit(VoiceFacadeEvent.ERROR, e);
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Full-duplex (live chat): always recording, sequential chunk send loop
  // ---------------------------------------------------------------------------

  /**
   * Enter full-duplex live mode: request mic with AEC, send a single RTT heartbeat,
   * then start a sequential recursive send loop that chunks and sends audio continuously.
   */
  async startLiveMode() {
    if (this._liveMode) return;
    if (!this._adapter || !this._portal) {
      throw new Error("VoiceFacade: init({ adapter, portal }) required");
    }

    this._liveMode = true;
    this._liveStopRequested = false;
    this._muted = false;

    // Request mic with AEC / noise suppression / AGC
    try {
      this._stream = await getMicrophoneStream({
        audio: {
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });
    } catch (e) {
      log.error("VoiceFacade: live mode mic request failed", e?.message ?? e);
      this._liveMode = false;
      this.emit(VoiceFacadeEvent.ERROR, e);
      return;
    }

    // Initial RTT heartbeat
    try {
      const txId = await requestToTalk(this._adapter, this._config.prefixRtt, {}, this._voiceSession);
      if (txId) this._sentTxIds.add(canonicalTxId(txId));
      log.info("VoiceFacade: live RTT heartbeat sent", { txId });
      this.emit(VoiceFacadeEvent.RTT_SENT, { txId });
      const position = await waitForConfirmation(this._portal, txId, this._config.rttConfirmTimeoutMs);
      this._queuePosition = position;
      log.info("VoiceFacade: live RTT confirmed", { position });
    } catch (e) {
      log.error("VoiceFacade: live RTT failed", e?.message ?? e);
      this.emit(VoiceFacadeEvent.ERROR, e);
      this._stopLiveInternal();
      return;
    }

    // Start recurring heartbeat RTT (every 60s)
    this._liveHeartbeatTimer = setTimeout(() => this._liveHeartbeat(), LIVE_RTT_HEARTBEAT_MS);

    // Start encoder (recreated each 350ms so each send is a complete WebM) and send loop
    this._recordedChunks = [];
    this._createLiveEncoder();
    log.info("VoiceFacade: live mode started");

    this._scheduleLiveSend();
  }

  /** Recursive heartbeat RTT for full-duplex presence check-in. */
  async _liveHeartbeat() {
    if (!this._liveMode || this._liveStopRequested) return;
    try {
      const txId = await requestToTalk(this._adapter, this._config.prefixRtt, {}, this._voiceSession);
      if (txId) this._sentTxIds.add(canonicalTxId(txId));
      log.info("VoiceFacade: live heartbeat RTT sent", { txId });
      this.emit(VoiceFacadeEvent.RTT_SENT, { txId });
      const position = await waitForConfirmation(this._portal, txId, this._config.rttConfirmTimeoutMs);
      this._queuePosition = position;
    } catch (e) {
      log.warn("VoiceFacade: live heartbeat RTT failed", e?.message ?? e);
    }
    if (this._liveMode && !this._liveStopRequested) {
      this._liveHeartbeatTimer = setTimeout(() => this._liveHeartbeat(), LIVE_RTT_HEARTBEAT_MS);
    }
  }

  /**
   * Sequential recursive timeout-based send loop.
   * Schedules the next send only after the current one resolves to prevent nonce/sequence conflicts.
   */
  _scheduleLiveSend() {
    if (this._liveStopRequested || !this._liveMode) return;
    setTimeout(async () => {
      if (this._liveStopRequested || !this._liveMode) return;
      await this._sendLiveChunk();
      this._scheduleLiveSend();
    }, LIVE_CHUNK_INTERVAL_MS);
  }

  /**
   * Flush one complete WebM window: stop encoder (so each send is valid WebM), send, recreate encoder.
   * Ensures every full-duplex chunk is decodable by the receiver (init + cluster per window).
   */
  async _sendLiveChunk() {
    if (this._liveSending) return; // in-flight guard
    if (!this._queuePosition || !this._encoder) return;

    this._liveSending = true;
    try {
      await this._encoder.stop();
      const fullOpus = concatenateUint8Arrays(this._recordedChunks);
      this._recordedChunks = [];

      this._encoder.close();
      this._encoder = null;
      this._createLiveEncoder();

      if (this._muted || fullOpus.length === 0) return;

      if (_isSilent(fullOpus)) {
        log.debug("VoiceFacade: VAD dropped silent chunk");
        return;
      }

      const maxChunkBytes = Math.min(this._config.chunkSize, this._config.maxPayloadBytes - 512);
      const chunks = chunkOpusBytes(fullOpus, maxChunkBytes);
      for (let i = 0; i < chunks.length; i++) {
        const payload = serializeAudioChunkPayload(
          this._config.prefixAudio, this._queuePosition, i, chunks.length, chunks[i], this._voiceSession,
        );
        const txId = await sendAudioChunk(this._adapter, payload);
        if (txId) this._sentTxIds.add(canonicalTxId(txId));
        this.emit(VoiceFacadeEvent.AUDIO_CHUNK_SENT, { txId, chunkIndex: i, totalChunks: chunks.length });
      }
    } catch (e) {
      log.error("VoiceFacade: live send failed", e?.message ?? e);
      this.emit(VoiceFacadeEvent.ERROR, e);
      if (!this._encoder && this._stream) this._createLiveEncoder();
    } finally {
      this._liveSending = false;
    }
  }

  /** Create and start the encoder for full-duplex (one WebM per window after stop). */
  _createLiveEncoder() {
    if (!this._stream || this._encoder) return;
    this._encoder = createOpusEncoder({
      stream: this._stream,
      bitrate: this._config.opusBitrate,
      onData: (u8) => this._recordedChunks.push(u8),
    });
    this._encoder.start();
  }

  /** Mute mic track (full-duplex). Chunks recorded while muted are discarded. */
  mute() {
    this._muted = true;
    if (this._stream) {
      for (const track of this._stream.getAudioTracks()) track.enabled = false;
    }
    log.info("VoiceFacade: muted");
  }

  /** Unmute mic track (full-duplex). */
  unmute() {
    this._muted = false;
    if (this._stream) {
      for (const track of this._stream.getAudioTracks()) track.enabled = true;
    }
    log.info("VoiceFacade: unmuted");
  }

  /** Stop full-duplex live mode. */
  stopLiveMode() {
    if (!this._liveMode) return;
    log.info("VoiceFacade: stopping live mode");
    this._stopLiveInternal();
  }

  _stopLiveInternal() {
    this._liveStopRequested = true;
    this._liveMode = false;
    if (this._liveHeartbeatTimer) {
      clearTimeout(this._liveHeartbeatTimer);
      this._liveHeartbeatTimer = null;
    }
    if (this._encoder) {
      this._encoder.close();
      this._encoder = null;
    }
    if (this._stream) {
      stopStream(this._stream);
      this._stream = null;
    }
    this._recordedChunks = [];
  }

  /** @returns {boolean} Whether live (full-duplex) mode is active. */
  get isLiveMode() { return this._liveMode; }

  /** @returns {boolean} Whether the mic is muted in live mode. */
  get isMuted() { return this._muted; }

  // ---------------------------------------------------------------------------
  // Shared
  // ---------------------------------------------------------------------------

  setAdapter(adapter) {
    this._adapter = adapter;
  }

  setPortal(portal) {
    this._portal = portal;
  }

  destroy() {
    log.info("VoiceFacade: left voice chat");
    this._stopLiveInternal();
    this._lease.cancel();
    this._playback?.close();

    // Remove voice prefixes from adapter
    if (this._config && this._adapter) {
      this._adapter.removePrefix?.(this._config.prefixRtt);
      this._adapter.removePrefix?.(this._config.prefixAudio);
    }

    if (this._stream) stopStream(this._stream);
    this._stream = null;
    this._encoder = null;
    this._adapter = null;
    this._portal = null;
    this._inbound = null;
    this._playback = null;
    this._voiceSession = null;
    this._sentTxIds.clear();
  }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

function concatenateUint8Arrays(arrays) {
  const total = arrays.reduce((s, a) => s + a.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const a of arrays) {
    out.set(a, offset);
    offset += a.length;
  }
  return out;
}

/**
 * Simple VAD: compute RMS of raw bytes and reject if below threshold.
 * Works on raw opus container bytes as a heuristic (byte-value variance).
 */
function _isSilent(bytes) {
  if (!bytes || bytes.length === 0) return true;
  let sum = 0;
  for (let i = 0; i < bytes.length; i++) {
    const v = (bytes[i] - 128) / 128;
    sum += v * v;
  }
  const rms = Math.sqrt(sum / bytes.length);
  return rms < VAD_SILENCE_RMS_THRESHOLD;
}
