/**
 * opusDecoder.js - Decode Opus (WebM/Ogg) bytes to PCM for playback
 * Single responsibility: decode only.
 */

import { Logger, LogModule } from "../../core/logger.js";

const log = Logger.create(LogModule.voice.audio);

/**
 * Decode Opus/WebM bytes to PCM (Float32Array, mono or stereo).
 * Uses AudioContext.decodeAudioData when available.
 * @param {Uint8Array} opusBytes - Encoded audio (WebM/Opus or Ogg/Opus)
 * @param {AudioContext} [context] - Optional; creates offline context if not provided
 * @returns {Promise<{ channelData: Float32Array[], sampleRate: number }>}
 */
export async function decodeOpusToPcm(opusBytes, context = null) {
  if (!opusBytes?.length) {
    return { channelData: [], sampleRate: 44100 };
  }

  const ctx = context || new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 48000 });
  const blob = new Blob([opusBytes], { type: "audio/webm" });

  try {
    const arrayBuffer = await blob.arrayBuffer();
    const audioBuffer = await ctx.decodeAudioData(arrayBuffer);

    const channelData = [];
    for (let i = 0; i < audioBuffer.numberOfChannels; i++) {
      channelData.push(audioBuffer.getChannelData(i).slice(0));
    }

    if (!context) ctx.close();

    return {
      channelData,
      sampleRate: audioBuffer.sampleRate,
    };
  } catch (e) {
    if (!context) ctx.close();
    log.warn("opusDecoder decode failed", e?.message ?? e);
    throw e;
  }
}

/**
 * Check if decoding is supported (decodeAudioData for WebM/Opus).
 */
export function isDecodingSupported() {
  return typeof AudioContext !== "undefined" || typeof webkitAudioContext !== "undefined";
}
