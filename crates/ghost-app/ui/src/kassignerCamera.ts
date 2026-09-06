const MAX_SCAN_WIDTH = 1280;
const MAX_SCAN_HEIGHT = 960;

export const KASSIGNER_SCAN_INTERVAL_MS = 120;

function shouldRetryCameraConstraints(error: unknown): boolean {
  const name = typeof error === "object" && error !== null && "name" in error
    ? String((error as { name?: unknown }).name ?? "")
    : "";
  return name === "OverconstrainedError"
    || name === "ConstraintNotSatisfiedError"
    || name === "TypeError";
}

async function preferContinuousFocus(stream: MediaStream): Promise<void> {
  const track = stream.getVideoTracks()[0];
  if (!track?.getCapabilities || !track.applyConstraints) return;
  try {
    const capabilities = track.getCapabilities() as MediaTrackCapabilities & { focusMode?: string[] };
    if (!capabilities.focusMode?.includes("continuous")) return;
    const advanced = [{ focusMode: "continuous" }] as unknown as MediaTrackConstraintSet[];
    await track.applyConstraints({ advanced });
  } catch {
    // Focus hints are optional. Camera capture remains usable when a WebView
    // exposes the camera but not the Image Capture focus extensions.
  }
}

export async function openKasSignerCamera(): Promise<MediaStream> {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new Error("Camera access is unavailable.");
  }
  const preferred: MediaStreamConstraints = {
    video: {
      facingMode: { ideal: "environment" },
      width: { ideal: 1280 },
      height: { ideal: 960 },
    },
    audio: false,
  };
  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia(preferred);
  } catch (error) {
    if (!shouldRetryCameraConstraints(error)) throw error;
    stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
  }
  await preferContinuousFocus(stream);
  return stream;
}

export interface KasSignerLuminanceFrame {
  width: number;
  height: number;
  luminanceBase64: string;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, Math.min(bytes.length, offset + chunkSize)));
  }
  return btoa(binary);
}

export function captureKasSignerLuminance(
  video: HTMLVideoElement,
  canvas: HTMLCanvasElement,
): KasSignerLuminanceFrame | null {
  if (video.readyState < HTMLMediaElement.HAVE_ENOUGH_DATA || video.videoWidth <= 0 || video.videoHeight <= 0) {
    return null;
  }
  const sourceWidth = video.videoWidth;
  const sourceHeight = video.videoHeight;
  // Preserve substantially more QR-module detail than the old 640x480 path.
  // 1280x960 is also the native command's explicit allocation ceiling.
  const scale = Math.min(1, MAX_SCAN_WIDTH / sourceWidth, MAX_SCAN_HEIGHT / sourceHeight);
  const width = Math.max(64, Math.floor(sourceWidth * scale));
  const height = Math.max(64, Math.floor(sourceHeight * scale));
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) throw new Error("Camera canvas is unavailable.");
  context.drawImage(video, 0, 0, width, height);
  const rgba = context.getImageData(0, 0, width, height).data;
  const luminance = new Uint8Array(width * height);
  for (let source = 0, target = 0; source < rgba.length; source += 4, target += 1) {
    luminance[target] = (rgba[source] * 77 + rgba[source + 1] * 150 + rgba[source + 2] * 29) >> 8;
  }
  return { width, height, luminanceBase64: bytesToBase64(luminance) };
}
