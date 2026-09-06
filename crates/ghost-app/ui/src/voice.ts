const VOICE_MESSAGE_PREFIX = "\u001eGHOST-VOICE-MESSAGE-V1:";
const LIVE_VOICE_PREFIX = "\u001eGHOST-LIVE-VOICE-V1:";
export const LIVE_VOICE_WINDOW_MS = 350;
export const LIVE_VOICE_JITTER_MS = 400;
export const LIVE_VOICE_GAP_WAIT_MS = 1_200;
export const LIVE_VOICE_MAX_BYTES = 32 * 1024;
export const VOICE_MESSAGE_MAX_MS = 10_000;

const LIVE_VOICE_RECORDER_TIMESLICE_MS = 100;
const LIVE_VOICE_PLAYOUT_LEAD_SECONDS = 0.02;

export type LiveVoiceKind = "request" | "accept" | "decline" | "hangup" | "audio";

export interface LiveVoicePacket {
  version: 1;
  callId: string;
  kind: LiveVoiceKind;
  sequence?: number;
  mime?: string;
  data?: string;
}

export interface VoiceMessagePayload {
  mime: string;
  data: string;
}

export interface LiveVoiceWindow {
  mime: string;
  blob: Blob;
}

interface ActiveVoiceRecorder {
  recorder: MediaRecorder;
  chunks: BlobPart[];
  mime: string;
}

interface QueuedVoiceWindow {
  mime: string;
  base64: string;
}

export function encodeVoiceMessage(payload: VoiceMessagePayload): string {
  return `${VOICE_MESSAGE_PREFIX}${JSON.stringify(payload)}`;
}

export function decodeVoiceMessage(body: string): VoiceMessagePayload | null {
  if (!body.startsWith(VOICE_MESSAGE_PREFIX)) return null;
  try {
    const parsed = JSON.parse(body.slice(VOICE_MESSAGE_PREFIX.length)) as Partial<VoiceMessagePayload>;
    if (typeof parsed.mime !== "string" || !parsed.mime.startsWith("audio/")) return null;
    if (typeof parsed.data !== "string" || !/^[A-Za-z0-9+/]*={0,2}$/.test(parsed.data)) return null;
    return { mime: parsed.mime, data: parsed.data };
  } catch {
    return null;
  }
}

export function encodeLiveVoicePacket(packet: LiveVoicePacket): string {
  return `${LIVE_VOICE_PREFIX}${JSON.stringify(packet)}`;
}

export function decodeLiveVoicePacket(body: string): LiveVoicePacket | null {
  if (!body.startsWith(LIVE_VOICE_PREFIX)) return null;
  try {
    const value = JSON.parse(body.slice(LIVE_VOICE_PREFIX.length)) as Partial<LiveVoicePacket>;
    if (value.version !== 1 || typeof value.callId !== "string" || !/^[0-9a-f]{32}$/i.test(value.callId)) return null;
    if (!("request,accept,decline,hangup,audio".split(",") as string[]).includes(String(value.kind))) return null;
    if (value.kind === "audio") {
      if (!Number.isSafeInteger(value.sequence) || (value.sequence ?? -1) < 0) return null;
      if (typeof value.mime !== "string" || !value.mime.startsWith("audio/")) return null;
      if (typeof value.data !== "string" || !/^[A-Za-z0-9+/]*={0,2}$/.test(value.data)) return null;
    }
    return value as LiveVoicePacket;
  } catch {
    return null;
  }
}

export function isLiveVoiceBody(body: string): boolean {
  return body.startsWith(LIVE_VOICE_PREFIX);
}

export function preferredOpusMime(): string {
  if (typeof MediaRecorder === "undefined") return "";
  return ["audio/webm;codecs=opus", "audio/ogg;codecs=opus"].find(type => MediaRecorder.isTypeSupported(type)) ?? "";
}

export async function blobToBase64(blob: Blob): Promise<string> {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  const step = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += step) {
    binary += String.fromCharCode(...bytes.subarray(offset, Math.min(offset + step, bytes.length)));
  }
  return btoa(binary);
}

/**
 * Kinesis-style full-duplex recorder. A complete independently decodable
 * WebM/Ogg window is stopped and finalized, then the next MediaRecorder is
 * started immediately *before* the previous window is encrypted/broadcast.
 * That keeps microphone capture continuous while the carrier is busy.
 */
export class LiveVoiceWindowRecorder {
  private active: ActiveVoiceRecorder | null = null;
  private closed = false;

  constructor(private readonly stream: MediaStream) {}

  start(): void {
    if (this.closed || this.active) return;
    this.active = this.createRecorder();
  }

  async flushAndRestart(): Promise<LiveVoiceWindow> {
    if (this.closed) throw new Error("Live voice recorder is closed.");
    if (!this.active) this.start();
    const active = this.active;
    if (!active) throw new Error("Live voice recorder failed to start.");

    this.active = null;
    await stopRecorder(active.recorder);
    const blob = new Blob(active.chunks, { type: active.mime });
    const mime = active.recorder.mimeType || active.mime || "audio/webm";

    // Critical ordering: restart capture before the caller awaits encryption,
    // WebRTC delivery, or a Kaspa transaction broadcast for this completed blob.
    if (!this.closed) this.active = this.createRecorder();
    return { mime, blob };
  }

  close(): void {
    this.closed = true;
    const active = this.active;
    this.active = null;
    if (active && active.recorder.state !== "inactive") {
      try { active.recorder.stop(); } catch { /* recorder already stopping */ }
    }
  }

  private createRecorder(): ActiveVoiceRecorder {
    const preferred = preferredOpusMime();
    const recorder = new MediaRecorder(
      this.stream,
      preferred ? { mimeType: preferred, audioBitsPerSecond: 24_000 } : { audioBitsPerSecond: 24_000 },
    );
    const chunks: BlobPart[] = [];
    recorder.ondataavailable = event => {
      if (event.data.size) chunks.push(event.data);
    };
    recorder.start(LIVE_VOICE_RECORDER_TIMESLICE_MS);
    return { recorder, chunks, mime: recorder.mimeType || preferred || "audio/webm" };
  }
}

function stopRecorder(recorder: MediaRecorder): Promise<void> {
  if (recorder.state === "inactive") return Promise.resolve();
  return new Promise<void>((resolve, reject) => {
    recorder.onstop = () => resolve();
    recorder.onerror = () => reject(new Error("Microphone encoder failed."));
    recorder.stop();
  });
}

function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value);
  const output = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) output[index] = binary.charCodeAt(index);
  return output;
}

/**
 * Ordered, jitter-buffered live playout modeled on Kaspa Kinesis.
 * Complete independent Opus/WebM windows are decoded to PCM and scheduled on
 * one AudioContext timeline. AudioBufferSourceNode start times are contiguous,
 * so a later window is never started by an HTMLAudio `ended` callback and no
 * artificial inter-window silence is inserted by DOM media startup latency.
 */
export class LiveVoicePlaybackQueue {
  private pending = new Map<number, QueuedVoiceWindow>();
  private expectedSequence: number | null = null;
  private started = false;
  private startTimer: number | null = null;
  private gapTimer: number | null = null;
  private generation = 0;
  private draining = false;
  private drainRequested = false;
  private context: AudioContext | null = null;
  private nextStartTime = 0;
  private activeSources = new Set<AudioBufferSourceNode>();

  push(sequence: number, mime: string, base64: string): void {
    if (!Number.isSafeInteger(sequence) || sequence < 0) return;
    if (this.expectedSequence !== null && sequence < this.expectedSequence) return;
    if (this.pending.has(sequence)) return;
    this.pending.set(sequence, { mime, base64 });

    if (!this.started) {
      if (this.startTimer === null) {
        const generation = this.generation;
        this.startTimer = window.setTimeout(() => {
          this.startTimer = null;
          if (generation !== this.generation || this.pending.size === 0) return;
          this.started = true;
          this.expectedSequence = 0;
          void this.drain(generation);
        }, LIVE_VOICE_JITTER_MS);
      }
      return;
    }

    void this.drain(this.generation);
  }

  clear(): void {
    this.generation += 1;
    if (this.startTimer !== null) window.clearTimeout(this.startTimer);
    if (this.gapTimer !== null) window.clearTimeout(this.gapTimer);
    this.startTimer = null;
    this.gapTimer = null;
    this.pending.clear();
    this.expectedSequence = null;
    this.started = false;
    this.draining = false;
    this.drainRequested = false;
    this.nextStartTime = 0;
    for (const source of this.activeSources) {
      try { source.stop(); } catch { /* already ended */ }
    }
    this.activeSources.clear();
    const context = this.context;
    this.context = null;
    if (context && context.state !== "closed") void context.close().catch(() => undefined);
  }

  private async drain(generation: number): Promise<void> {
    if (generation !== this.generation || !this.started || this.expectedSequence === null) return;
    if (this.draining) {
      this.drainRequested = true;
      return;
    }

    this.draining = true;
    try {
      do {
        this.drainRequested = false;
        while (generation === this.generation && this.expectedSequence !== null) {
          const sequence: number = this.expectedSequence;
          const packet = this.pending.get(sequence);
          if (!packet) {
            this.armGapTimer(generation);
            break;
          }

          if (this.gapTimer !== null) {
            window.clearTimeout(this.gapTimer);
            this.gapTimer = null;
          }
          this.pending.delete(sequence);
          this.expectedSequence = sequence + 1;
          await this.decodeAndSchedule(packet, generation);
        }
      } while (this.drainRequested && generation === this.generation);
    } finally {
      this.draining = false;
    }
  }

  private armGapTimer(generation: number): void {
    if (this.gapTimer !== null || this.pending.size === 0 || this.expectedSequence === null) return;
    const expected = this.expectedSequence;
    const lowest = Math.min(...this.pending.keys());
    if (lowest <= expected) return;

    this.gapTimer = window.setTimeout(() => {
      this.gapTimer = null;
      if (generation !== this.generation || this.expectedSequence !== expected || this.pending.size === 0) return;
      const next = Math.min(...this.pending.keys());
      if (next > expected) this.expectedSequence = next;
      void this.drain(generation);
    }, LIVE_VOICE_GAP_WAIT_MS);
  }

  private async decodeAndSchedule(packet: QueuedVoiceWindow, generation: number): Promise<void> {
    if (generation !== this.generation) return;
    const context = this.getContext();
    if (context.state === "suspended") {
      try { await context.resume(); } catch { /* autoplay policy may retry on next window */ }
    }
    if (generation !== this.generation) return;

    try {
      const bytes = base64ToBytes(packet.base64);
      // `base64ToBytes` is typed as Uint8Array<ArrayBufferLike> under modern
      // TypeScript. Copy into a concrete ArrayBuffer before decodeAudioData so
      // the DOM API cannot receive a SharedArrayBuffer-backed view and no
      // BlobPart generic mismatch leaks into the production build.
      const encoded = new ArrayBuffer(bytes.byteLength);
      new Uint8Array(encoded).set(bytes);
      const decoded = await context.decodeAudioData(encoded);
      if (generation !== this.generation || decoded.duration <= 0) return;

      const source = context.createBufferSource();
      source.buffer = decoded;
      source.connect(context.destination);
      this.activeSources.add(source);
      source.onended = () => this.activeSources.delete(source);

      const startAt = Math.max(
        this.nextStartTime,
        context.currentTime + LIVE_VOICE_PLAYOUT_LEAD_SECONDS,
      );
      source.start(startAt);
      this.nextStartTime = startAt + decoded.duration;
    } catch {
      // A malformed/unsupported window cannot be repaired here. Sequence still
      // advances so one bad carrier cannot deadlock the remainder of the call.
    }
  }

  private getContext(): AudioContext {
    if (!this.context || this.context.state === "closed") {
      this.context = new AudioContext({ sampleRate: 48_000 });
      this.nextStartTime = this.context.currentTime;
    }
    return this.context;
  }
}
