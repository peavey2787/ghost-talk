import { useEffect, useRef, useState } from "react";
import { captureKasSignerLuminance, KASSIGNER_SCAN_INTERVAL_MS, openKasSignerCamera } from "../kassignerCamera";
import {
  cancelKasSigner,
  completeKasSigner,
  qrSvgHex,
  scanKasSignerResponseFrame,
  type BroadcastResult,
  type KasSignerSigningPrompt,
} from "../native";

interface Props {
  prompt: KasSignerSigningPrompt;
  onCancel: () => void;
  onBroadcast: (result: BroadcastResult, message: string) => void;
}

export function KasSignerSigningModal({ prompt, onCancel, onBroadcast }: Props) {
  const [current, setCurrent] = useState(prompt);
  const [frameIndex, setFrameIndex] = useState(0);
  const [qrPaused, setQrPaused] = useState(false);
  const [svg, setSvg] = useState("");
  const [status, setStatus] = useState("Scan this request with KasSigner, review it on the hardware screen, then scan the signed response.");
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState("");
  const [scanBits, setScanBits] = useState<boolean[]>([]);
  const [responseHex, setResponseHex] = useState("");
  const [busy, setBusy] = useState(false);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const scanTimerRef = useRef<number | null>(null);
  const scanBusyRef = useRef(false);
  const cameraGenerationRef = useRef(0);

  const frames = current.qr_frames;
  const frame = frames[Math.min(frameIndex, Math.max(0, frames.length - 1))];

  useEffect(() => {
    setCurrent(prompt);
    setFrameIndex(0);
    setQrPaused(false);
    setScanBits([]);
  }, [prompt]);

  useEffect(() => {
    if (frames.length <= 1 || qrPaused) return;
    const timer = window.setInterval(() => {
      setFrameIndex(index => (index + 1) % frames.length);
    }, 650);
    return () => window.clearInterval(timer);
  }, [frames.length, current.request_id, qrPaused]);

  useEffect(() => {
    if (!frame) {
      setSvg("");
      return;
    }
    let cancelled = false;
    qrSvgHex(frame.payload_hex)
      .then(value => { if (!cancelled) setSvg(value); })
      .catch(error => { if (!cancelled) setStatus(String(error)); });
    return () => { cancelled = true; };
  }, [frame?.payload_hex]);

  useEffect(() => () => stopCamera(), []);

  function teardownCamera() {
    if (scanTimerRef.current !== null) {
      window.clearInterval(scanTimerRef.current);
      scanTimerRef.current = null;
    }
    for (const track of streamRef.current?.getTracks() ?? []) track.stop();
    streamRef.current = null;
    if (videoRef.current) videoRef.current.srcObject = null;
    scanBusyRef.current = false;
    setScanning(false);
  }

  function stopCamera() {
    cameraGenerationRef.current += 1;
    teardownCamera();
  }

  async function finish(response: string) {
    if (!response.trim() || busy) return;
    setBusy(true);
    setStatus("Verifying KasSigner signature and transaction policy…");
    try {
      const result = await completeKasSigner(current.request_id, response.trim());
      if (result.status === "resign" && result.resign) {
        setCurrent(result.resign);
        setFrameIndex(0);
        setQrPaused(false);
        setResponseHex("");
        setScanProgress("");
        setScanBits([]);
        setStatus(result.message);
        return;
      }
      if (!result.broadcast) throw new Error("KasSigner completion did not return a broadcast transaction.");
      stopCamera();
      onBroadcast(result.broadcast, result.message);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function scanOneFrame() {
    const video = videoRef.current;
    const canvas = canvasRef.current;
    if (!video || !canvas || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || scanBusyRef.current) return;
    scanBusyRef.current = true;
    const generation = cameraGenerationRef.current;
    try {
      const captured = captureKasSignerLuminance(video, canvas);
      if (!captured) return;
      const { width, height, luminanceBase64 } = captured;
      const progress = await scanKasSignerResponseFrame({
        requestId: current.request_id,
        width,
        height,
        luminanceBase64,
      });
      if (generation !== cameraGenerationRef.current) return;
      if (progress.total > 0) {
        setScanBits(progress.bits.slice(0, progress.total));
        setScanProgress(`Signed response: ${progress.received}/${progress.total} QR frame${progress.total === 1 ? "" : "s"}`);
      }
      if (progress.response_hex) {
        stopCamera();
        setResponseHex(progress.response_hex);
        await finish(progress.response_hex);
      }
    } catch (error) {
      if (generation !== cameraGenerationRef.current) return;
      setStatus(String(error));
      stopCamera();
    } finally {
      scanBusyRef.current = false;
    }
  }

  async function startCamera() {
    if (scanning || busy) return;
    const generation = cameraGenerationRef.current + 1;
    cameraGenerationRef.current = generation;
    teardownCamera();
    if (!navigator.mediaDevices?.getUserMedia) {
      setStatus("Camera access is unavailable. Paste the signed KSPT response hex below instead.");
      return;
    }
    try {
      setScanBits([]);
      setScanProgress("");
      const stream = await openKasSignerCamera();
      if (generation !== cameraGenerationRef.current) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }
      streamRef.current = stream;
      if (!videoRef.current) throw new Error("KasSigner camera preview is unavailable.");
      videoRef.current.srcObject = stream;
      await videoRef.current.play();
      setScanning(true);
      setStatus("Hold the KasSigner signed-response QR inside the camera view.");
      scanTimerRef.current = window.setInterval(() => { void scanOneFrame(); }, KASSIGNER_SCAN_INTERVAL_MS);
    } catch (error) {
      if (generation !== cameraGenerationRef.current) return;
      stopCamera();
      setStatus(`Camera scan failed: ${String(error)}. You can paste the signed response hex instead.`);
    }
  }

  async function cancel() {
    stopCamera();
    await cancelKasSigner(current.request_id).catch(() => undefined);
    onCancel();
  }

  return (
    <div className="modal-backdrop kassigner-backdrop" role="dialog" aria-modal="true" aria-label="KasSigner transaction signing">
      <div className="modal kassigner-modal">
        <div className="modal-head">
          <div>
            <h3>Sign with KasSigner</h3>
            <p className="muted">SDK {current.sdk_version} · hardware limit {current.max_inputs} inputs</p>
          </div>
          <button onClick={() => void cancel()} disabled={busy}>Close</button>
        </div>

        <div className="kassigner-flow">
          <section className="kassigner-request">
            <h4>1. Scan request on the hardware signer</h4>
            <div className="kassigner-qr">
              {svg ? <div dangerouslySetInnerHTML={{ __html: svg }} /> : <span>Preparing QR…</span>}
            </div>
            <strong>Frame {frame ? frame.index + 1 : 0} / {frame?.total ?? frames.length}</strong>
            <div className="button-row kassigner-frame-controls">
              <button
                type="button"
                onClick={() => setFrameIndex(index => (index - 1 + frames.length) % frames.length)}
                disabled={frames.length <= 1}
              >
                ← Back
              </button>
              <button
                type="button"
                onClick={() => setQrPaused(paused => !paused)}
                disabled={frames.length <= 1}
              >
                {qrPaused ? "Resume" : "Pause"}
              </button>
              <button
                type="button"
                onClick={() => setFrameIndex(index => (index + 1) % frames.length)}
                disabled={frames.length <= 1}
              >
                Forward →
              </button>
            </div>
            <small>{current.note}</small>
          </section>

          <section className="kassigner-response">
            <h4>2. Scan the signed response</h4>
            <video ref={videoRef} className={scanning ? "kassigner-camera active" : "kassigner-camera"} muted playsInline />
            <canvas ref={canvasRef} hidden />
            {scanBits.some(Boolean) && (
              <div
                className="kassigner-frame-dots"
                role="status"
                aria-label={`${scanBits.filter(Boolean).length} of ${scanBits.length} QR frames scanned`}
              >
                {scanBits.map((filled, index) => (
                  <span
                    key={index}
                    className={filled ? "kassigner-frame-dot scanned" : "kassigner-frame-dot"}
                    title={`Frame ${index + 1}: ${filled ? "scanned" : "waiting"}`}
                  />
                ))}
              </div>
            )}
            <div className="button-row">
              {!scanning ? (
                <button className="primary" onClick={() => void startCamera()} disabled={busy}>Scan signed QR</button>
              ) : (
                <button onClick={stopCamera}>Stop camera</button>
              )}
            </div>
            {scanProgress && <small>{scanProgress}</small>}
            <details>
              <summary>Paste signed response hex instead</summary>
              <textarea
                rows={5}
                value={responseHex}
                onChange={event => setResponseHex(event.target.value.replace(/\s/g, ""))}
                placeholder="Signed KSPT response hex"
              />
              <button disabled={!responseHex.trim() || busy} onClick={() => void finish(responseHex)}>Verify and broadcast</button>
            </details>
          </section>
        </div>
        <div className="status">{busy ? "Working… " : ""}{status}</div>
      </div>
    </div>
  );
}
