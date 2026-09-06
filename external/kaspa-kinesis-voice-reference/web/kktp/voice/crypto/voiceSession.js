/**
 * voiceSession.js - Voice DH session: establish over lobby, encrypt/decrypt voice payloads.
 * Reuses DHSession and deriveSessionKey; key derived with voice-specific salt and info
 * including first RTT queue position for replay binding.
 */

import { DHSession } from "../../engine/kaspa/crypto/dh_encryption.js";
import { deriveSessionKey } from "../../protocol/utils/sessionKeyDerivation.js";
import { hexToBytes, bytesToHex } from "../../core/conversions.js";
import { getRandomBytesSync } from "../../core/randomBytes.js";

const VOICE_SALT = new TextEncoder().encode("KKTP:voice:1");

/**
 * Encode first RTT queue position as fixed-width bytes: 8-byte big-endian blueScore, 4-byte big-endian txIndexInBlock.
 * @param {{ blueScore: number, txIndexInBlock: number }} pos
 * @returns {Uint8Array}
 */
function encodeQueuePosition(pos) {
  if (!pos || typeof pos.blueScore !== "number" || typeof pos.txIndexInBlock !== "number") {
    return new Uint8Array(0);
  }
  const buf = new ArrayBuffer(12);
  const view = new DataView(buf);
  view.setBigUint64(0, BigInt(pos.blueScore), false);
  view.setUint32(8, pos.txIndexInBlock >>> 0, false);
  return new Uint8Array(buf);
}

/**
 * Create a voice DH session. Uses adapter.startSession(voiceKeyIndex) when available, otherwise ephemeral keys.
 *
 * @param {{ startSession?(index: number): Promise<DHSession> }} adapter - Adapter with startSession (e.g. KaspaAdapter)
 * @param {{ voiceKeyIndex?: number }} [options]
 * @returns {Promise<{ establishViaLobby: Function, handleIncoming: Function, encryptPayload: Function, decryptPayload: Function, isReady: () => boolean }>}
 */
export async function createVoiceSession(adapter, options = {}) {
  const voiceKeyIndex = options.voiceKeyIndex ?? 200;
  let session;

  if (adapter?.startSession && typeof adapter.startSession === "function") {
    session = await adapter.startSession(voiceKeyIndex);
  } else {
    const privHex = bytesToHex(getRandomBytesSync(32));
    session = new DHSession();
    session.initiateHandshake(privHex, undefined);
  }

  let ready = false;
  /** @type {string | null} */
  let _peerPubDerived = null;
  /** @type {((obj: object) => void) | null} */
  let _sendEncrypted = null;
  /** @type {Set<string>} Peer pub hexes we've already sent a voice_dh reply to (canonical lowercased). */
  const _repliedToPeerPubs = new Set();

  /**
   * Bootstrap voice key over the lobby. Sends our voice_dh pub; call handleIncoming for each decrypted lobby message.
   * When solo (no other peer yet), derives with our own pub so session becomes ready; when a peer joins and sends voice_dh we re-derive with them.
   *
   * @param {(plaintextJson: object) => void} sendEncrypted - Send a JSON object over the lobby secure channel
   */
  function establishViaLobby(sendEncrypted) {
    _sendEncrypted = sendEncrypted;
    if (!session?.myPublicKeyHex) return;
    sendEncrypted({ type: "voice_dh", pub: session.myPublicKeyHex });
    handleIncoming({ type: "voice_dh", pub: session.myPublicKeyHex });
  }

  /**
   * Process an incoming decrypted lobby message. When type is voice_dh with peer pub, derive voice key and mark ready.
   * Re-derives when a new peer pub is received (e.g. joiner joins after solo host). After deriving, sends our voice_dh so the other side can derive.
   *
   * @param {{ type?: string, pub?: string }} msg - Decrypted message from lobby
   * @param {{ blueScore?: number, txIndexInBlock?: number }} [firstRttQueuePosition] - Optional; when provided, included in HKDF info for replay binding
   */
  function handleIncoming(msg, firstRttQueuePosition = null) {
    if (!msg || msg.type !== "voice_dh" || !msg.pub) return;
    const peerPubHex = String(msg.pub).trim();
    if (!peerPubHex) return;
    // Ignore self-echo (group message delivered back to sender); prevents reply loop. When not yet ready, we allow own pub for solo bootstrap in establishViaLobby.
    if (ready && peerPubHex.toLowerCase() === session.myPublicKeyHex.toLowerCase()) return;
    if (ready && peerPubHex === _peerPubDerived) return;

    session.deriveSharedSecret(peerPubHex);
    const myPubBytes = hexToBytes(session.myPublicKeyHex);
    const peerPubBytes = hexToBytes(peerPubHex);
    const queuePosBytes = encodeQueuePosition(
      firstRttQueuePosition ?? {}
    );
    // Canonical order so both sides derive the same voice key (HKDF info must match).
    const myHex = session.myPublicKeyHex.toLowerCase();
    const peerHex = peerPubHex.toLowerCase();
    const [firstBytes, secondBytes] =
      myHex <= peerHex ? [myPubBytes, peerPubBytes] : [peerPubBytes, myPubBytes];
    const info = new Uint8Array(firstBytes.length + secondBytes.length + queuePosBytes.length);
    info.set(firstBytes, 0);
    info.set(secondBytes, firstBytes.length);
    info.set(queuePosBytes, firstBytes.length + secondBytes.length);

    const voiceKey = deriveSessionKey(
      VOICE_SALT,
      session.sharedSecretBytes,
      info,
      32
    );
    session.setSessionKey(voiceKey);
    ready = true;
    _peerPubDerived = peerPubHex;
    // Only send our voice_dh reply once per peer (idempotent for duplicate deliveries).
    const peerKey = peerPubHex.toLowerCase();
    if (_sendEncrypted && !_repliedToPeerPubs.has(peerKey)) {
      _repliedToPeerPubs.add(peerKey);
      _sendEncrypted({ type: "voice_dh", pub: session.myPublicKeyHex });
    }
  }

  /**
   * Encrypt a string payload (e.g. JSON for RTT or base64 for compact audio). Uses DHSession string-in/string-out.
   *
   * @param {string} plaintext
   * @returns {string}
   */
  function encryptPayload(plaintext) {
    if (!ready || !session) throw new Error("Voice session not established");
    return session.encryptMessage(plaintext);
  }

  /**
   * Decrypt a string payload.
   *
   * @param {string} ciphertext
   * @returns {string}
   */
  function decryptPayload(ciphertext) {
    if (!ready || !session) throw new Error("Voice session not established");
    return session.decryptMessage(ciphertext);
  }

  function isReady() {
    return !!ready;
  }

  return {
    establishViaLobby,
    handleIncoming,
    encryptPayload,
    decryptPayload,
    isReady,
  };
}

/**
 * Create a voice session from the lobby's derived group key.
 * All peers in the lobby use the same key; no DH handshake.
 *
 * @param {Uint8Array} groupKeyBytes - 32-byte voice group key (e.g. from lobby.getVoiceGroupKey())
 * @returns {{ establishViaLobby: Function, handleIncoming: Function, encryptPayload: (s: string) => string, decryptPayload: (s: string) => string, isReady: () => boolean }}
 */
export function createVoiceSessionFromGroupKey(groupKeyBytes) {
  if (!(groupKeyBytes instanceof Uint8Array) || groupKeyBytes.length !== 32) {
    throw new Error("createVoiceSessionFromGroupKey: 32-byte Uint8Array required");
  }
  const session = new DHSession();
  session.setSessionKey(groupKeyBytes);

  function encryptPayload(plaintext) {
    return session.encryptMessage(plaintext);
  }

  function decryptPayload(ciphertext) {
    return session.decryptMessage(ciphertext);
  }

  function isReady() {
    return true;
  }

  function establishViaLobby(_sendEncrypted) {
    // No-op: key already set
  }

  function handleIncoming(_msg, _firstRttQueuePosition = null) {
    // No-op: key already set
  }

  return {
    establishViaLobby,
    handleIncoming,
    encryptPayload,
    decryptPayload,
    isReady,
  };
}
