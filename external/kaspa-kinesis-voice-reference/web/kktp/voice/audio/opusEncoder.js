/**
 * opusEncoder.js - Encode PCM/stream to Opus (via MediaRecorder or injectable)
 * Single responsibility: encode only; configurable bitrate.
 */

import { Logger, LogModule } from "../../core/logger.js";

const log = Logger.create(LogModule.voice.audio);

const MIME_OPUS = "audio/webm;codecs=opus";
const FALLBACK_MIME = "audio/webm";

/**
 * Create an encoder that captures from stream and emits Opus (WebM) chunks.
 * Uses MediaRecorder when available; bitrate is a hint.
 * @param {Object} options
 * @param {MediaStream} options.stream - Microphone (or other) stream
 * @param {number} [options.bitrate=24000] - Target bitrate (bits/sec)
 * @param {function(Uint8Array): void} options.onData - Called with encoded chunk
 * @returns {{ start: function(), stop: function(), close: function() }}
 */
export function createOpusEncoder({ stream, bitrate = 24000, onData }) {
  if (!stream || typeof onData !== "function") {
    throw new Error("opusEncoder: stream and onData required");
  }

  let recorder = null;
  /** @type {Promise<void>|null} - Last ondataavailable delivery so stop() can flush. */
  let _lastDataPromise = null;

  const mimeType = MediaRecorder.isTypeSupported(MIME_OPUS) ? MIME_OPUS : FALLBACK_MIME;

  function start() {
    if (recorder) return;
    _lastDataPromise = null;
    try {
      recorder = new MediaRecorder(stream, {
        mimeType,
        audioBitsPerSecond: bitrate,
      });
      recorder.ondataavailable = (e) => {
        if (e.data?.size) {
          _lastDataPromise = e.data.arrayBuffer().then((buf) => {
            onData(new Uint8Array(buf));
          });
        }
      };
      recorder.start(100);
    } catch (e) {
      log.error("opusEncoder start", e?.message ?? e);
      throw e;
    }
  }

  function stop() {
    if (!recorder) return Promise.resolve();
    return new Promise((resolve) => {
      recorder.onstop = async () => {
        if (_lastDataPromise) await _lastDataPromise;
        _lastDataPromise = null;
        resolve();
      };
      recorder.stop();
    });
  }

  function close() {
    if (recorder && recorder.state !== "inactive") {
      try {
        recorder.stop();
      } catch (_) {}
    }
    recorder = null;
  }

  return { start, stop, close };
}

/**
 * Check if encoding is supported in this environment.
 */
export function isEncodingSupported() {
  return typeof MediaRecorder !== "undefined" && (MediaRecorder.isTypeSupported(MIME_OPUS) || MediaRecorder.isTypeSupported(FALLBACK_MIME));
}
