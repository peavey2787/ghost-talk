import { useEffect, useRef, useState } from "react";
import { captureKasSignerLuminance, KASSIGNER_SCAN_INTERVAL_MS, openKasSignerCamera } from "../kassignerCamera";
import { scanKasSignerAccountQr } from "../native";

interface Props {
  onScan: (accountPayload: string) => void;
  onClose: () => void;
}

export function KasSignerKpubScanner({ onScan, onClose }: Props) {
  const [status, setStatus] = useState("Hold the KasSigner account-kpub QR inside the camera view.");
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const timerRef = useRef<number | null>(null);
  const busyRef = useRef(false);
  const cameraGenerationRef = useRef(0);

  useEffect(() => {
    void startCamera();
    return stopCamera;
  }, []);

  function teardownCamera() {
    if (timerRef.current !== null) {
      window.clearInterval(timerRef.current);
      timerRef.current = null;
    }
    for (const track of streamRef.current?.getTracks() ?? []) track.stop();
    streamRef.current = null;
    if (videoRef.current) videoRef.current.srcObject = null;
    busyRef.current = false;
  }

  function stopCamera() {
    cameraGenerationRef.current += 1;
    teardownCamera();
  }

  async function scanFrame() {
    const video = videoRef.current;
    const canvas = canvasRef.current;
    if (!video || !canvas || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || busyRef.current) return;
    busyRef.current = true;
    const generation = cameraGenerationRef.current;
    try {
      const captured = captureKasSignerLuminance(video, canvas);
      if (!captured) return;
      const { width, height, luminanceBase64 } = captured;
      const result = await scanKasSignerAccountQr({ width, height, luminanceBase64 });
      if (generation !== cameraGenerationRef.current) return;
      if (result?.account_payload) {
        stopCamera();
        onScan(result.account_payload);
      }
    } catch (error) {
      if (generation !== cameraGenerationRef.current) return;
      setStatus(String(error));
      stopCamera();
    } finally {
      busyRef.current = false;
    }
  }

  async function startCamera() {
    const generation = cameraGenerationRef.current + 1;
    cameraGenerationRef.current = generation;
    teardownCamera();
    if (!navigator.mediaDevices?.getUserMedia) {
      setStatus("Camera access is unavailable. Close this scanner and paste the kpub instead.");
      return;
    }
    try {
      const stream = await openKasSignerCamera();
      if (generation !== cameraGenerationRef.current) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }
      streamRef.current = stream;
      if (!videoRef.current) throw new Error("KasSigner camera preview is unavailable.");
      videoRef.current.srcObject = stream;
      await videoRef.current.play();
      timerRef.current = window.setInterval(() => { void scanFrame(); }, KASSIGNER_SCAN_INTERVAL_MS);
    } catch (error) {
      if (generation !== cameraGenerationRef.current) return;
      stopCamera();
      setStatus(`Camera scan failed: ${String(error)}`);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="Scan KasSigner account kpub">
      <div className="modal kassigner-modal">
        <div className="modal-head">
          <div>
            <h3>Scan KasSigner account</h3>
            <p className="muted">Export the account kpub on KasSigner and point it at this camera.</p>
          </div>
          <button onClick={() => { stopCamera(); onClose(); }}>Close</button>
        </div>
        <video ref={videoRef} className="kassigner-camera active" muted playsInline />
        <canvas ref={canvasRef} hidden />
        <div className="status">{status}</div>
      </div>
    </div>
  );
}
