import { useEffect, useRef, useState } from "react";
import { captureKasSignerLuminance, KASSIGNER_SCAN_INTERVAL_MS, openKasSignerCamera } from "../kassignerCamera";
import type { KasSignerIdentityOwnershipProof } from "../model";
import {
  cancelKasSignerIdentityProof,
  completeKasSignerIdentityProof,
  qrSvg,
  scanKasSignerIdentityProofQr,
  type KasSignerIdentityProofPrompt,
} from "../native";

interface Props {
  prompt: KasSignerIdentityProofPrompt;
  onVerified: (proof: KasSignerIdentityOwnershipProof) => void;
  onCancel: () => void;
}

export function KasSignerIdentityProofModal({ prompt, onVerified, onCancel }: Props) {
  const [svg, setSvg] = useState("");
  const [status, setStatus] = useState(
    "On KasSigner choose Sign Message → Scan QR, review this exact Ghost Talk ownership challenge, sign it, then tap SHOW QR.",
  );
  const [responseHex, setResponseHex] = useState("");
  const [scanning, setScanning] = useState(false);
  const [busy, setBusy] = useState(false);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const timerRef = useRef<number | null>(null);
  const scanBusyRef = useRef(false);
  const generationRef = useRef(0);

  useEffect(() => {
    let cancelled = false;
    qrSvg(prompt.challenge)
      .then(value => { if (!cancelled) setSvg(value); })
      .catch(error => { if (!cancelled) setStatus(String(error)); });
    return () => { cancelled = true; };
  }, [prompt.challenge]);

  useEffect(() => () => stopCamera(), []);

  function teardownCamera(): void {
    if (timerRef.current !== null) {
      window.clearInterval(timerRef.current);
      timerRef.current = null;
    }
    for (const track of streamRef.current?.getTracks() ?? []) track.stop();
    streamRef.current = null;
    if (videoRef.current) videoRef.current.srcObject = null;
    scanBusyRef.current = false;
    setScanning(false);
  }

  function stopCamera(): void {
    generationRef.current += 1;
    teardownCamera();
  }

  async function verify(response: string): Promise<void> {
    if (!response.trim() || busy) return;
    setBusy(true);
    setStatus("Verifying that this signature was produced by the imported KasSigner account…");
    try {
      const proof = await completeKasSignerIdentityProof(prompt.proof_id, response.trim());
      stopCamera();
      onVerified(proof);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function scanFrame(): Promise<void> {
    const video = videoRef.current;
    const canvas = canvasRef.current;
    if (!video || !canvas || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || scanBusyRef.current) return;
    scanBusyRef.current = true;
    const generation = generationRef.current;
    try {
      const captured = captureKasSignerLuminance(video, canvas);
      if (!captured) return;
      const { width, height, luminanceBase64 } = captured;
      const result = await scanKasSignerIdentityProofQr({
        proofId: prompt.proof_id,
        width,
        height,
        luminanceBase64,
      });
      if (generation !== generationRef.current) return;
      if (result?.response_hex) {
        stopCamera();
        setResponseHex(result.response_hex);
        await verify(result.response_hex);
      }
    } catch (error) {
      if (generation !== generationRef.current) return;
      setStatus(String(error));
      stopCamera();
    } finally {
      scanBusyRef.current = false;
    }
  }

  async function startCamera(): Promise<void> {
    if (scanning || busy) return;
    const generation = generationRef.current + 1;
    generationRef.current = generation;
    teardownCamera();
    if (!navigator.mediaDevices?.getUserMedia) {
      setStatus("Camera access is unavailable. Paste the 96-byte Sign Message result hex below instead.");
      return;
    }
    try {
      const stream = await openKasSignerCamera();
      if (generation !== generationRef.current) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }
      streamRef.current = stream;
      if (!videoRef.current) throw new Error("KasSigner camera preview is unavailable.");
      videoRef.current.srcObject = stream;
      await videoRef.current.play();
      setScanning(true);
      setStatus("Hold the KasSigner Sign Message result QR inside the camera view.");
      timerRef.current = window.setInterval(() => { void scanFrame(); }, KASSIGNER_SCAN_INTERVAL_MS);
    } catch (error) {
      if (generation !== generationRef.current) return;
      stopCamera();
      setStatus(`Camera scan failed: ${String(error)}. You can paste the result hex instead.`);
    }
  }

  async function cancel(): Promise<void> {
    stopCamera();
    await cancelKasSignerIdentityProof(prompt.proof_id).catch(() => undefined);
    onCancel();
  }

  return (
    <div className="modal-backdrop kassigner-backdrop" role="dialog" aria-modal="true" aria-label="Verify KasSigner account ownership">
      <div className="modal kassigner-modal ownership-proof-modal">
        <div className="modal-head">
          <div>
            <h3>Verify KasSigner ownership</h3>
            <p className="muted">A copied/stolen kpub alone cannot pass this step.</p>
          </div>
          <button disabled={busy} onClick={() => void cancel()}>Cancel</button>
        </div>
        <div className="kassigner-flow">
          <section className="kassigner-request">
            <h4>1. Sign this exact challenge on KasSigner</h4>
            <div className="kassigner-qr">
              {svg ? <div dangerouslySetInnerHTML={{ __html: svg }} /> : <span>Preparing QR…</span>}
            </div>
            <small>Account fingerprint: <code>{prompt.account_fingerprint}</code></small>
            <details>
              <summary>Read the exact challenge</summary>
              <pre className="ownership-challenge">{prompt.challenge}</pre>
            </details>
          </section>
          <section className="kassigner-response">
            <h4>2. Scan the signed proof</h4>
            <video ref={videoRef} className={scanning ? "kassigner-camera active" : "kassigner-camera"} muted playsInline />
            <canvas ref={canvasRef} hidden />
            <div className="button-row">
              {!scanning
                ? <button className="primary" disabled={busy} onClick={() => void startCamera()}>Scan proof QR</button>
                : <button onClick={stopCamera}>Stop camera</button>}
            </div>
            <details>
              <summary>Paste Sign Message result hex instead</summary>
              <textarea
                rows={5}
                value={responseHex}
                onChange={event => setResponseHex(event.target.value.replace(/\s/g, ""))}
                placeholder="96-byte signature + message-hash result hex"
              />
              <button disabled={!responseHex.trim() || busy} onClick={() => void verify(responseHex)}>Verify ownership</button>
            </details>
          </section>
        </div>
        <div className="status">{busy ? "Working… " : ""}{status}</div>
      </div>
    </div>
  );
}
