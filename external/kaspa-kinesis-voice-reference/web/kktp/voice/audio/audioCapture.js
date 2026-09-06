/**
 * audioCapture.js - Get MediaStream from mic and feed to encoder (or return stream)
 * Single responsibility: capture only; no encode logic.
 */

import { Logger, LogModule } from "../../core/logger.js";

const log = Logger.create(LogModule.voice.audio);

/**
 * Request microphone access and return MediaStream.
 * @param {Object} [constraints] - MediaTrackConstraints (e.g. { audio: true })
 * @returns {Promise<MediaStream>}
 */
export async function getMicrophoneStream(constraints = { audio: true }) {
  const stream = await navigator.mediaDevices.getUserMedia(constraints);
  return stream;
}

/**
 * Create an AudioContext and MediaStreamSourceNode from a MediaStream.
 * Caller must manage context lifecycle (resume/close).
 * @param {MediaStream} stream
 * @param {AudioContext} [context] - Optional existing context
 * @returns {{ context: AudioContext, sourceNode: MediaStreamAudioSourceNode }}
 */
export function createSourceNode(stream, context = null) {
  const ctx = context || new (window.AudioContext || window.webkitAudioContext)();
  const sourceNode = ctx.createMediaStreamSource(stream);
  return { context: ctx, sourceNode };
}

/**
 * Stop all tracks on a stream.
 * @param {MediaStream} stream
 */
export function stopStream(stream) {
  if (!stream) return;
  try {
    for (const track of stream.getTracks()) track.stop();
  } catch (e) {
    log.warn("audioCapture stopStream", e?.message ?? e);
  }
}
