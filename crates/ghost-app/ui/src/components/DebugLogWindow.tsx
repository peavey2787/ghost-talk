import { useEffect, useMemo, useRef, useState } from "react";
import {
  clearNativeDebugLog,
  getHydraDebugState,
  getNativeDebugLogSnapshot,
  type DebugLogEntry,
  type HydraDebugState,
} from "../native";

export function DebugLogWindow({
  profileId,
  open,
  onClose,
}: {
  profileId: string;
  open: boolean;
  onClose: () => void;
}) {
  const [entries, setEntries] = useState<DebugLogEntry[]>([]);
  const [state, setState] = useState<HydraDebugState>();
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const latest = useRef(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  async function refresh(full = false): Promise<void> {
    if (!open || !profileId) return;
    try {
      const snapshot = await getNativeDebugLogSnapshot(full ? undefined : latest.current);
      latest.current = snapshot.latest_sequence;
      setEntries(current => full ? snapshot.entries : appendUnique(current, snapshot.entries));
      setState(await getHydraDebugState(profileId));
      setError("");
    } catch (cause) {
      setError(String(cause));
    }
  }

  useEffect(() => {
    if (!open) return;
    latest.current = 0;
    setEntries([]);
    void refresh(true);
    const timer = window.setInterval(() => void refresh(false), 500);
    return () => window.clearInterval(timer);
  }, [open, profileId]);

  useEffect(() => {
    if (!open) return;
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [entries.length, open]);

  const transcript = useMemo(() => formatTranscript(entries, state), [entries, state]);
  if (!open) return null;

  return (
    <div className="modal-backdrop debug-window-backdrop" role="presentation">
      <section className="modal debug-window" role="dialog" aria-modal="true" aria-label="Protocol and Kaspa debug log">
        <header className="debug-window-head">
          <div>
            <h3>Protocol & Kaspa Debug</h3>
            <small>Redacted protocol metadata only. Message plaintext, passwords, private keys, seed words and ciphertext are not logged.</small>
          </div>
          <button onClick={onClose} aria-label="Close protocol debug window">×</button>
        </header>

        <div className="debug-state">
          <h4>Live protocol state</h4>
          <pre>{state ? JSON.stringify(state, null, 2) : "Waiting for native protocol state…"}</pre>
        </div>

        <div className="debug-log" ref={scrollRef}>
          {entries.length ? entries.map(entry => (
            <div className={`debug-log-entry ${entry.level}`} key={entry.sequence}>
              <code>
                {new Date(entry.timestamp_ms).toISOString()} #{entry.sequence} [{entry.level}] [{entry.category}] {entry.event}
              </code>
              {entry.details && <pre>{entry.details}</pre>}
            </div>
          )) : <div className="muted">No protocol/network events logged yet.</div>}
        </div>

        {error && <p className="status error">{error}</p>}
        <div className="button-row debug-actions">
          <button onClick={() => void refresh(true)}>Refresh</button>
          <button onClick={() => void navigator.clipboard.writeText(transcript).then(() => {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1200);
          })}>{copied ? "Copied" : "Copy log"}</button>
          <button onClick={() => void clearNativeDebugLog().then(() => {
            latest.current = 0;
            setEntries([]);
            return refresh(true);
          })}>Clear</button>
          <button className="primary" onClick={onClose}>Close</button>
        </div>
      </section>
    </div>
  );
}

function appendUnique(current: DebugLogEntry[], incoming: DebugLogEntry[]): DebugLogEntry[] {
  if (!incoming.length) return current;
  const seen = new Set(current.map(entry => entry.sequence));
  const next = [...current];
  for (const entry of incoming) {
    if (!seen.has(entry.sequence)) next.push(entry);
  }
  return next.sort((a, b) => a.sequence - b.sequence).slice(-4000);
}

function formatTranscript(entries: DebugLogEntry[], state?: HydraDebugState): string {
  const header = [
    "Ghost Talk Protocol Debug",
    `Exported: ${new Date().toISOString()}`,
    "Sensitive payloads are intentionally excluded.",
    "",
    "LIVE PROTOCOL STATE",
    state ? JSON.stringify(state, null, 2) : "unavailable",
    "",
    "EVENT LOG",
  ];
  return header.concat(entries.map(entry =>
    `${new Date(entry.timestamp_ms).toISOString()} #${entry.sequence} [${entry.level}] [${entry.category}] ${entry.event} ${entry.details}`
  )).join("\n");
}
