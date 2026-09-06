export type Signal =
  | { type: "offer"; sdp: RTCSessionDescriptionInit }
  | { type: "answer"; sdp: RTCSessionDescriptionInit };

const DIRECT_ROUTE_PREFIX = "\u001eGHOST-DIRECT-ROUTE-V1:";

export interface RealtimeControlEnvelope {
  id: string;
  chatId: string;
  sessionSid: string;
  body: string;
  receivedAt: number;
}

export interface DirectRouteSignalPacket {
  version: 1;
  negotiationId: string;
  kind: "offer" | "answer";
  sdp: { type: RTCSdpType; sdp: string };
}

export function encodeDirectRouteSignal(packet: DirectRouteSignalPacket): string {
  return `${DIRECT_ROUTE_PREFIX}${JSON.stringify(packet)}`;
}

export function decodeDirectRouteSignal(body: string): DirectRouteSignalPacket | null {
  if (!body.startsWith(DIRECT_ROUTE_PREFIX)) return null;
  try {
    const value = JSON.parse(body.slice(DIRECT_ROUTE_PREFIX.length)) as Partial<DirectRouteSignalPacket>;
    if (value.version !== 1 || !/^[0-9a-f]{32}$/i.test(String(value.negotiationId))) return null;
    if (value.kind !== "offer" && value.kind !== "answer") return null;
    if (!value.sdp || value.sdp.type !== value.kind || typeof value.sdp.sdp !== "string") return null;
    if (value.sdp.sdp.length === 0 || value.sdp.sdp.length > 64 * 1024) return null;
    return value as DirectRouteSignalPacket;
  } catch {
    return null;
  }
}

export function isDirectRouteSignalBody(body: string): boolean {
  return body.startsWith(DIRECT_ROUTE_PREFIX);
}

const DEFAULT_ICE_SERVERS: RTCIceServer[] = [
  { urls: "stun:stun.cloudflare.com:3478" },
  { urls: "stun:stun.l.google.com:19302" },
];

const ICE_GATHER_TIMEOUT_MS = 3_000;

/**
 * Direct peer carrier used only for already-authenticated Ghost Talk sessions.
 * Signaling is carried by the current SID-bound Kaspa/HYDRA realtime control
 * channel. Application bytes handed to send() must already be HYDRA-encrypted.
 */
export class GhostPeerCarrier {
  private pc: RTCPeerConnection;
  private channel?: RTCDataChannel;
  onPacket?: (packet: Uint8Array) => void;
  onOpen?: () => void;
  onClose?: () => void;
  onError?: (error: string) => void;

  constructor(configuration?: RTCConfiguration) {
    this.pc = new RTCPeerConnection(configuration ?? { iceServers: DEFAULT_ICE_SERVERS });
    this.pc.ondatachannel = event => this.attach(event.channel);
    this.pc.onconnectionstatechange = () => {
      const state = this.pc.connectionState;
      if (state === "failed") this.onError?.("WebRTC peer connection failed");
      if (state === "closed" || state === "failed" || state === "disconnected") this.onClose?.();
    };
  }

  get ready(): boolean {
    return this.channel?.readyState === "open" && this.pc.connectionState !== "failed";
  }

  private attach(channel: RTCDataChannel): void {
    this.channel = channel;
    channel.binaryType = "arraybuffer";
    channel.onopen = () => this.onOpen?.();
    channel.onclose = () => this.onClose?.();
    channel.onerror = () => this.onError?.("WebRTC data channel failed");
    channel.onmessage = event => {
      const bytes = event.data instanceof ArrayBuffer ? new Uint8Array(event.data) : undefined;
      if (bytes) this.onPacket?.(bytes);
    };
  }

  async createOffer(): Promise<Signal> {
    this.attach(this.pc.createDataChannel("ghost-media", { ordered: true }));
    const offer = await this.pc.createOffer();
    await this.pc.setLocalDescription(offer);
    await waitForIceGathering(this.pc);
    const local = this.pc.localDescription;
    if (!local?.sdp) throw new Error("WebRTC offer did not produce a local SDP");
    return { type: "offer", sdp: { type: local.type, sdp: local.sdp } };
  }

  async acceptOffer(signal: Signal): Promise<Signal> {
    if (signal.type !== "offer") throw new Error("Expected a WebRTC offer");
    await this.pc.setRemoteDescription(signal.sdp);
    const answer = await this.pc.createAnswer();
    await this.pc.setLocalDescription(answer);
    await waitForIceGathering(this.pc);
    const local = this.pc.localDescription;
    if (!local?.sdp) throw new Error("WebRTC answer did not produce a local SDP");
    return { type: "answer", sdp: { type: local.type, sdp: local.sdp } };
  }

  async acceptAnswer(signal: Signal): Promise<void> {
    if (signal.type !== "answer") throw new Error("Expected a WebRTC answer");
    await this.pc.setRemoteDescription(signal.sdp);
  }

  send(packet: Uint8Array): void {
    if (!this.ready || !this.channel) throw new Error("WebRTC carrier is not open");

    let payload: ArrayBuffer;
    if (packet.buffer instanceof ArrayBuffer) {
      payload =
        packet.byteOffset === 0 && packet.byteLength === packet.buffer.byteLength
          ? packet.buffer
          : packet.buffer.slice(packet.byteOffset, packet.byteOffset + packet.byteLength);
    } else {
      payload = new ArrayBuffer(packet.byteLength);
      new Uint8Array(payload).set(packet);
    }
    this.channel.send(payload);
  }

  close(): void {
    this.channel?.close();
    this.pc.close();
  }
}

function waitForIceGathering(pc: RTCPeerConnection): Promise<void> {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise(resolve => {
    let settled = false;
    const done = () => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      pc.removeEventListener("icegatheringstatechange", changed);
      resolve();
    };
    const changed = () => {
      if (pc.iceGatheringState === "complete") done();
    };
    const timer = window.setTimeout(done, ICE_GATHER_TIMEOUT_MS);
    pc.addEventListener("icegatheringstatechange", changed);
  });
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const step = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += step) {
    binary += String.fromCharCode(...bytes.subarray(offset, Math.min(offset + step, bytes.length)));
  }
  return btoa(binary);
}

export function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value);
  const output = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) output[index] = binary.charCodeAt(index);
  return output;
}

export async function openOpusMicrophone(onChunk: (bytes: Uint8Array) => void) {
  const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
  const preferred = ["audio/webm;codecs=opus", "audio/ogg;codecs=opus"].find(MediaRecorder.isTypeSupported);
  const recorder = new MediaRecorder(stream, preferred ? { mimeType: preferred, audioBitsPerSecond: 24000 } : undefined);
  recorder.ondataavailable = async event => {
    if (event.data.size) onChunk(new Uint8Array(await event.data.arrayBuffer()));
  };
  recorder.start(250);
  return () => { recorder.stop(); stream.getTracks().forEach(track => track.stop()); };
}
