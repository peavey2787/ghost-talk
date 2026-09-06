/**
 * voiceTestRunner.js - Test harness for voice: wires engine, lobby, VoiceFacade;
 * tracks sent/received voice txIds (with opusBytes for replay) and stats. No UI.
 */

import { VoiceFacade, VoiceFacadeEvent } from "../voiceFacade.js";

const KAS_PER_VOICE_TX = 0.00001;

/**
 * @typedef {{ type: 'rtt'|'audio', txId: string }} SentEntry
 * @typedef {{ type: 'rtt'|'audio', txId: string, receivedAt: number, opusBytes?: Uint8Array }} ReceivedEntry
 */

/**
 * Voice test runner: holds engine ref, creates VoiceFacade when joining voice,
 * tracks sent/received txIds and exposes getSentTxIds, getReceivedTxIds, getStats.
 */
export class VoiceTestRunner {
  constructor() {
    /** @type {import("../../../kkGameEngine.js").KKGameEngine | null} */
    this._engine = null;
    /** @type {VoiceFacade | null} */
    this._voiceFacade = null;
    /** @type {SentEntry[]} */
    this._sentRtt = [];
    /** @type {SentEntry[]} */
    this._sentAudio = [];
    /** @type {ReceivedEntry[]} */
    this._receivedRtt = [];
    /** @type {ReceivedEntry[]} */
    this._receivedAudio = [];
    /** @type {number | null} */
    this._lastReceivedAt = null;
  }

  /**
   * Set the game engine (must be initialized with lobby active for voice).
   * @param {import("../../../kkGameEngine.js").KKGameEngine} engine
   */
  setEngine(engine) {
    this._engine = engine;
  }

  /**
   * Join voice chat: create VoiceFacade, bootstrap over lobby, register with engine.
   * Requires engine initialized and lobby state HOSTING or MEMBER.
   * @returns {Promise<void>}
   */
  async joinVoice() {
    const engine = this._engine;
    if (!engine) throw new Error("VoiceTestRunner: setEngine(engine) required");
    const adapter = engine._adapter;
    const portal = adapter?._portal;
    const lobby = engine._lobby;
    if (!adapter || !portal) throw new Error("VoiceTestRunner: engine must be initialized (adapter, portal)");
    if (!lobby || !lobby._manager) throw new Error("VoiceTestRunner: join a lobby first");

    if (this._voiceFacade) {
      this.leaveVoice();
    }

    const facade = new VoiceFacade();
    const handler = lobby._manager.handler;

    const sendEncrypted = (obj) => {
      const text = typeof obj === "string" ? obj : JSON.stringify(obj);
      return engine.sendLobbyMessage(text);
    };
    const registerIncomingHandler = (fn) => {
      handler.setOnVoiceDhMessage((_mailboxId, msg) => fn(msg));
    };

    facade.on(VoiceFacadeEvent.RTT_SENT, ({ txId }) => {
      this._sentRtt.push({ type: "rtt", txId });
    });
    facade.on(VoiceFacadeEvent.AUDIO_CHUNK_SENT, ({ txId }) => {
      this._sentAudio.push({ type: "audio", txId });
    });
    facade.on(VoiceFacadeEvent.RTT, (data) => {
      const txid = data?.txid ?? data?.transactionId;
      if (txid) {
        this._receivedRtt.push({ type: "rtt", txId: String(txid), receivedAt: Date.now() });
        this._lastReceivedAt = Date.now();
      }
    });
    facade.on(VoiceFacadeEvent.VOICE_CHUNK, (data) => {
      const txid = data?.txid ?? data?.transactionId;
      if (txid) {
        this._receivedAudio.push({
          type: "audio",
          txId: String(txid),
          receivedAt: Date.now(),
          opusBytes: data.opusBytes ?? null,
        });
        this._lastReceivedAt = Date.now();
      }
    });

    const groupKeyBytes = lobby.getVoiceGroupKey?.() ?? null;
    const groupMailboxId = lobby.getGroupMailboxId?.() ?? null;
    await facade.init({
      adapter,
      portal,
      groupMailboxId,
      ...(groupKeyBytes ? { groupKeyBytes } : { bootstrapVoiceSession: { sendEncrypted, registerIncomingHandler } }),
    });

    // Register the inbound handler with the engine so KKTP:V: matches route to it
    engine.registerVoiceHandler(facade._inbound);

    this._voiceFacade = facade;
  }

  /**
   * Leave voice chat: unregister handler from engine, destroy facade, reset stats.
   */
  leaveVoice() {
    if (this._engine) {
      this._engine.unregisterVoiceHandler();
    }
    if (this._voiceFacade) {
      this._voiceFacade.destroy();
      this._voiceFacade = null;
    }
    this._sentRtt = [];
    this._sentAudio = [];
    this._receivedRtt = [];
    this._receivedAudio = [];
    this._lastReceivedAt = null;
  }

  /**
   * Request to talk (send RTT tx, wait confirmation, start lease).
   * @returns {Promise<{ blockHash: string, blueScore: number, txIndexInBlock: number }>}
   */
  async requestToTalk() {
    if (!this._voiceFacade) throw new Error("VoiceTestRunner: joinVoice() first");
    return await this._voiceFacade.requestToTalk();
  }

  /** Start recording (mic -> Opus). */
  async startRecording() {
    if (!this._voiceFacade) throw new Error("VoiceTestRunner: joinVoice() first");
    await this._voiceFacade.startRecording();
  }

  /** Stop recording and send audio chunks. */
  async stopRecording() {
    if (!this._voiceFacade) throw new Error("VoiceTestRunner: joinVoice() first");
    await this._voiceFacade.stopRecording();
  }

  /** Start full-duplex live mode. */
  async startLiveMode() {
    if (!this._voiceFacade) throw new Error("VoiceTestRunner: joinVoice() first");
    await this._voiceFacade.startLiveMode();
  }

  /** Stop full-duplex live mode. */
  stopLiveMode() {
    this._voiceFacade?.stopLiveMode();
  }

  /** Mute in live mode. */
  mute() { this._voiceFacade?.mute(); }

  /** Unmute in live mode. */
  unmute() { this._voiceFacade?.unmute(); }

  /** @returns {boolean} */
  get isLiveMode() { return this._voiceFacade?.isLiveMode ?? false; }

  /** @returns {boolean} */
  get isMuted() { return this._voiceFacade?.isMuted ?? false; }

  /** @returns {{ type: 'rtt'|'audio', txId: string }[]} */
  getSentTxIds() {
    return [...this._sentRtt.map((e) => ({ type: e.type, txId: e.txId })), ...this._sentAudio.map((e) => ({ type: e.type, txId: e.txId }))];
  }

  /** @returns {ReceivedEntry[]} */
  getReceivedTxIds() {
    return [
      ...this._receivedRtt.map((e) => ({ type: e.type, txId: e.txId })),
      ...this._receivedAudio.map((e) => ({ type: e.type, txId: e.txId, opusBytes: e.opusBytes })),
    ];
  }

  /** @returns {{ rttSent: number, audioSent: number, rttReceived: number, audioReceived: number, kasSent: number, lastReceivedAt: number|null }} */
  getStats() {
    const rttSent = this._sentRtt.length;
    const audioSent = this._sentAudio.length;
    const rttReceived = this._receivedRtt.length;
    const audioReceived = this._receivedAudio.length;
    const totalSent = rttSent + audioSent;
    const kasSent = totalSent * KAS_PER_VOICE_TX;
    return { rttSent, audioSent, rttReceived, audioReceived, kasSent, lastReceivedAt: this._lastReceivedAt };
  }

  /** @returns {ReceivedEntry[]} Raw received audio entries with opusBytes for replay. */
  getReceivedAudioEntries() {
    return this._receivedAudio;
  }

  /**
   * Subscribe to facade events (e.g. error, leaseExpired). Call after joinVoice().
   * @param {string} event
   * @param {(...args: any[]) => void} fn
   */
  on(event, fn) {
    if (this._voiceFacade) this._voiceFacade.on(event, fn);
  }

  /** Tear down voice facade and unregister from engine. */
  destroy() {
    this.leaveVoice();
  }
}
