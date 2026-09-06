import { useEffect, useMemo, useRef, useState } from "react";
import type { ChatThread, Message, Route, StegoProfile } from "../model";
import {
  LIVE_VOICE_MAX_BYTES,
  LIVE_VOICE_WINDOW_MS,
  LiveVoicePlaybackQueue,
  LiveVoiceWindowRecorder,
  VOICE_MESSAGE_MAX_MS,
  blobToBase64,
  decodeLiveVoicePacket,
  decodeVoiceMessage,
  encodeLiveVoicePacket,
  encodeVoiceMessage,
  preferredOpusMime,
} from "../voice";
import {
  GhostPeerCarrier,
  base64ToBytes,
  bytesToBase64,
  decodeDirectRouteSignal,
  encodeDirectRouteSignal,
  type DirectRouteSignalPacket,
  type RealtimeControlEnvelope,
  type Signal,
} from "../webrtc";

interface Props {
  chats: ChatThread[];
  mask: boolean;
  onMask: (value: boolean) => void;
  route: Route;
  stego: StegoProfile;
  onRoute: (route: Route) => void;
  onStego: (stego: StegoProfile) => void;
  onSend: (chatId: string, body: string) => Promise<void>;
  onSendControl: (chatId: string, body: string, reuseChange?: boolean) => Promise<void>;
  onSealDirect: (chatId: string, body: string) => Promise<string>;
  onOpenDirect: (chatId: string, envelopeB64: string) => Promise<string | null>;
  localHydraId?: string;
  realtimeControls: RealtimeControlEnvelope[];
  onRealtimeControlHandled: (id: string) => void;
  onStartChat: (target: string) => Promise<string>;
  onAcceptRequest: (chatId: string) => Promise<void>;
  onIgnoreRequest: (chatId: string) => void;
  onDeleteRequest: (chatId: string) => void;
  onArchive: (chatId: string, archived: boolean) => void;
  onAddContact: (chatId: string) => void;
  onLeave: (chatId: string) => Promise<void>;
  /** Optional external selection, used by incoming-message notifications and Discover. */
  focusChatId?: string;
  onFocusHandled?: () => void;
}

const stegoProfiles: Array<{ profile: StegoProfile; available: boolean; note: string }> = [
  { profile: "Off", available: true, note: "Normal padded HYDRA envelope" },
  { profile: "Deterministic", available: true, note: "Instant model-free HYDRA technical-telemetry carrier" },
  { profile: "Fast Unicode", available: false, note: "Requires a configured local HYDRA language-model backend" },
  { profile: "Fast Hybrid", available: false, note: "Requires a configured local HYDRA language-model backend" },
  { profile: "Arithmetic", available: false, note: "Requires a configured local HYDRA language-model backend" },
];

type CallStatus = "calling" | "incoming" | "connected" | "error";
type DirectRouteState = "kaspa" | "connecting" | "webrtc";

interface LiveCallState {
  chatId: string;
  callId: string;
  label: string;
  status: CallStatus;
  muted: boolean;
  error?: string;
}

export function ChatView(props: Props) {
  const [selectedId, setSelectedId] = useState(props.chats[0]?.id ?? "");
  const [draft, setDraft] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [stegoOpen, setStegoOpen] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState("");
  const [requestBusy, setRequestBusy] = useState(false);
  const [newChatOpen, setNewChatOpen] = useState(false);
  const [newChatTarget, setNewChatTarget] = useState("");
  const [newChatBusy, setNewChatBusy] = useState(false);
  const [newChatError, setNewChatError] = useState("");
  const [showIgnored, setShowIgnored] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const [voiceMessageRecording, setVoiceMessageRecording] = useState(false);
  const [voiceMessageBusy, setVoiceMessageBusy] = useState(false);
  const voiceRecorderRef = useRef<MediaRecorder | null>(null);
  const voiceMessageStreamRef = useRef<MediaStream | null>(null);
  const voiceMessageChunksRef = useRef<BlobPart[]>([]);
  const voiceMessageChatRef = useRef("");
  const voiceMessageTimerRef = useRef<number | null>(null);
  const [call, setCall] = useState<LiveCallState | null>(null);
  const callRef = useRef<LiveCallState | null>(null);
  const messagesRef = useRef<HTMLDivElement | null>(null);
  const liveStreamRef = useRef<MediaStream | null>(null);
  const liveGenerationRef = useRef(0);
  const liveSequenceRef = useRef(0);
  const liveMutedRef = useRef(false);
  const playbackRef = useRef(new LiveVoicePlaybackQueue());
  const directCarrierRef = useRef<GhostPeerCarrier | null>(null);
  const directChatIdRef = useRef("");
  const directNegotiationIdRef = useRef("");
  const directReadyRef = useRef(false);
  const directAttemptKeyRef = useRef("");
  const directNegotiationTimerRef = useRef<number | null>(null);
  const [directRoute, setDirectRoute] = useState<DirectRouteState>("kaspa");
  const ignoredRequests = useMemo(
    () => props.chats.filter(isHiddenIgnoredRequest),
    [props.chats],
  );
  const archivedChats = useMemo(
    () => props.chats.filter(chat => chat.archived && !isHiddenIgnoredRequest(chat)),
    [props.chats],
  );
  const visibleChats = useMemo(
    () => showIgnored
      ? ignoredRequests
      : showArchived
        ? props.chats.filter(chat => chat.archived && !isHiddenIgnoredRequest(chat))
        : props.chats.filter(chat => !chat.archived && !isHiddenIgnoredRequest(chat)),
    [props.chats, ignoredRequests, showIgnored, showArchived],
  );
  const selected = useMemo(
    () => visibleChats.find(chat => chat.id === selectedId) ?? visibleChats[0],
    [visibleChats, selectedId],
  );

  useEffect(() => {
    if (!selectedId && visibleChats[0]) setSelectedId(visibleChats[0].id);
    if (selectedId && !visibleChats.some(chat => chat.id === selectedId)) {
      setSelectedId(visibleChats[0]?.id ?? "");
    }
  }, [visibleChats, selectedId]);

  useEffect(() => {
    const node = messagesRef.current;
    if (!node || !selected) return;
    // Incoming Kaspa mailbox messages are appended asynchronously. Keep the
    // newest message visible without requiring the user to manually scroll.
    const frame = window.requestAnimationFrame(() => {
      node.scrollTop = node.scrollHeight;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [selected?.id, selected?.messages.length]);

  useEffect(() => {
    const id = props.focusChatId;
    if (!id) return;
    const chat = props.chats.find(candidate => candidate.id === id);
    if (!chat) return;
    // A notification/Discover jump may target an archived or ignored thread.
    // Move to the appropriate list and select the exact chat; ordinary chat-row
    // clicks continue to use the same selectedId path.
    setShowIgnored(isHiddenIgnoredRequest(chat));
    setShowArchived(Boolean(chat.archived) && !isHiddenIgnoredRequest(chat));
    setSelectedId(id);
    props.onFocusHandled?.();
  }, [props.focusChatId, props.chats, props.onFocusHandled]);

  useEffect(() => {
    callRef.current = call;
    liveMutedRef.current = Boolean(call?.muted);
  }, [call]);

  useEffect(() => () => {
    liveGenerationRef.current += 1;
    playbackRef.current.clear();
    directCarrierRef.current?.close();
    directCarrierRef.current = null;
    directChatIdRef.current = "";
    directNegotiationIdRef.current = "";
    directReadyRef.current = false;
    if (directNegotiationTimerRef.current !== null) window.clearTimeout(directNegotiationTimerRef.current);
    liveStreamRef.current?.getTracks().forEach(track => track.stop());
    voiceMessageStreamRef.current?.getTracks().forEach(track => track.stop());
    if (voiceMessageTimerRef.current !== null) window.clearTimeout(voiceMessageTimerRef.current);
  }, []);

  useEffect(() => {
    for (const envelope of props.realtimeControls) {
      const direct = decodeDirectRouteSignal(envelope.body);
      if (direct) {
        props.onRealtimeControlHandled(envelope.id);
        void handleDirectRouteControl(envelope, direct);
        continue;
      }
      const packet = decodeLiveVoicePacket(envelope.body);
      if (!packet) {
        props.onRealtimeControlHandled(envelope.id);
        continue;
      }
      const current = callRef.current;
      if (packet.kind === "request") {
        if (current && current.callId !== packet.callId) {
          void props.onSendControl(envelope.chatId, encodeLiveVoicePacket({
            version: 1,
            callId: packet.callId,
            kind: "decline",
          }), true);
        } else if (!current) {
          const chat = props.chats.find(candidate => candidate.id === envelope.chatId);
          setSelectedId(envelope.chatId);
          setShowArchived(Boolean(chat?.archived));
          setShowIgnored(false);
          const incoming: LiveCallState = {
            chatId: envelope.chatId,
            callId: packet.callId,
            label: chat?.label ?? "Ghost Talk peer",
            status: "incoming",
            muted: false,
          };
          callRef.current = incoming;
          setCall(incoming);
          if (chat) void ensureDirectUpgrade(chat, true);
        }
      } else if (current?.callId === packet.callId && current.chatId === envelope.chatId) {
        if (packet.kind === "accept" && current.status === "calling") {
          const connected = { ...current, status: "connected" as const, error: undefined };
          callRef.current = connected;
          setCall(connected);
          void startLiveSender(current.chatId, current.callId);
        } else if (packet.kind === "decline" || packet.kind === "hangup") {
          stopLiveCallLocal(packet.kind === "decline" ? "Call declined." : undefined);
        } else if (packet.kind === "audio" && current.status === "connected" && packet.mime && packet.data) {
          playbackRef.current.push(packet.sequence!, packet.mime, packet.data);
        }
      }
      props.onRealtimeControlHandled(envelope.id);
    }
  }, [props.realtimeControls]);

  useEffect(() => {
    if (call?.status !== "calling") return;
    const callId = call.callId;
    const timer = window.setTimeout(() => {
      if (callRef.current?.callId !== callId || callRef.current.status !== "calling") return;
      void props.onSendControl(call.chatId, encodeLiveVoicePacket({ version: 1, callId, kind: "hangup" }), true)
        .catch(() => undefined);
      stopLiveCallLocal("No answer.");
    }, 30_000);
    return () => window.clearTimeout(timer);
  }, [call?.callId, call?.status]);

  useEffect(() => {
    if (props.route !== "Auto" || !selected?.bootstrapComplete || !selected.sessionSid || selected.left || selected.peerLeft) {
      if (directChatIdRef.current) closeDirectCarrier();
      return;
    }
    if (directChatIdRef.current && directChatIdRef.current !== selected.id) closeDirectCarrier();
    void ensureDirectUpgrade(selected);
  }, [props.route, props.localHydraId, selected?.id, selected?.sessionSid, selected?.bootstrapComplete, selected?.peerHydraHandle, selected?.left, selected?.peerLeft]);

  function closeDirectCarrier(fallback = true): void {
    const carrier = directCarrierRef.current;
    directCarrierRef.current = null;
    directChatIdRef.current = "";
    directNegotiationIdRef.current = "";
    directReadyRef.current = false;
    if (directNegotiationTimerRef.current !== null) {
      window.clearTimeout(directNegotiationTimerRef.current);
      directNegotiationTimerRef.current = null;
    }
    if (carrier) carrier.close();
    setDirectRoute(fallback ? "kaspa" : "connecting");
  }

  function configureDirectCarrier(chatId: string, negotiationId: string): GhostPeerCarrier {
    const previous = directCarrierRef.current;
    directCarrierRef.current = null;
    directChatIdRef.current = "";
    directNegotiationIdRef.current = "";
    if (previous) previous.close();

    const carrier = new GhostPeerCarrier();
    directCarrierRef.current = carrier;
    directChatIdRef.current = chatId;
    directNegotiationIdRef.current = negotiationId;
    directReadyRef.current = false;
    setDirectRoute("connecting");
    carrier.onOpen = () => {
      if (directCarrierRef.current !== carrier || directChatIdRef.current !== chatId || directNegotiationIdRef.current !== negotiationId) return;
      directReadyRef.current = true;
      if (directNegotiationTimerRef.current !== null) {
        window.clearTimeout(directNegotiationTimerRef.current);
        directNegotiationTimerRef.current = null;
      }
      setDirectRoute("webrtc");
    };
    carrier.onClose = () => {
      if (directCarrierRef.current !== carrier) return;
      directReadyRef.current = false;
      setDirectRoute("kaspa");
    };
    carrier.onError = () => {
      if (directCarrierRef.current !== carrier) return;
      directReadyRef.current = false;
      setDirectRoute("kaspa");
    };
    carrier.onPacket = packet => {
      void handleDirectPacket(chatId, carrier, packet);
    };
    return carrier;
  }

  async function handleDirectPacket(chatId: string, carrier: GhostPeerCarrier, packet: Uint8Array): Promise<void> {
    if (directCarrierRef.current !== carrier || directChatIdRef.current !== chatId) return;
    try {
      const body = await props.onOpenDirect(chatId, bytesToBase64(packet));
      if (!body) return;
      const voice = decodeLiveVoicePacket(body);
      const current = callRef.current;
      if (!voice || !current || current.chatId !== chatId || current.callId !== voice.callId || current.status !== "connected") return;
      if (voice.kind === "audio" && voice.mime && voice.data && Number.isSafeInteger(voice.sequence)) {
        playbackRef.current.push(voice.sequence!, voice.mime, voice.data);
      } else if (voice.kind === "hangup") {
        stopLiveCallLocal();
      }
    } catch {
      // Direct media is an optimization. Authentication/decryption failures or a
      // broken peer channel close only the direct path; Kaspa remains available.
      closeDirectCarrier();
    }
  }

  async function ensureDirectUpgrade(chat: ChatThread, force = false): Promise<void> {
    if (props.route !== "Auto" || typeof RTCPeerConnection === "undefined") return;
    if (!chat.bootstrapComplete || !chat.sessionSid || chat.left || chat.peerLeft) return;
    const localHydraId = props.localHydraId?.toLowerCase();
    const peerHydraId = chat.peerHydraHandle?.toLowerCase();
    if (!localHydraId || !peerHydraId || localHydraId === peerHydraId) return;

    if (directChatIdRef.current === chat.id && directCarrierRef.current) {
      if (directReadyRef.current || directRoute === "connecting") return;
    }

    const attemptKey = `${chat.id}:${chat.sessionSid}`;
    if (!force && directAttemptKeyRef.current === attemptKey) return;

    // Exactly one side automatically originates an offer, preventing WebRTC glare.
    // The other side answers the authenticated SID-bound offer when it arrives.
    if (localHydraId.localeCompare(peerHydraId) > 0) return;
    directAttemptKeyRef.current = attemptKey;
    const negotiationId = randomCallId();
    try {
      const carrier = configureDirectCarrier(chat.id, negotiationId);
      const offer = await carrier.createOffer();
      await props.onSendControl(chat.id, encodeDirectRouteSignal({
        version: 1,
        negotiationId,
        kind: "offer",
        sdp: normalizeDirectSignal(offer, "offer"),
      }), true);
      if (directNegotiationTimerRef.current !== null) window.clearTimeout(directNegotiationTimerRef.current);
      directNegotiationTimerRef.current = window.setTimeout(() => {
        if (directChatIdRef.current === chat.id && directNegotiationIdRef.current === negotiationId && !directReadyRef.current) {
          closeDirectCarrier();
        }
      }, 12_000);
    } catch {
      closeDirectCarrier();
    }
  }

  async function handleDirectRouteControl(envelope: RealtimeControlEnvelope, packet: DirectRouteSignalPacket): Promise<void> {
    if (props.route !== "Auto" || typeof RTCPeerConnection === "undefined") return;
    const chat = props.chats.find(candidate => candidate.id === envelope.chatId);
    if (!chat?.bootstrapComplete || chat.left || chat.peerLeft || chat.sessionSid?.toLowerCase() !== envelope.sessionSid.toLowerCase()) return;

    if (packet.kind === "offer") {
      try {
        const carrier = configureDirectCarrier(chat.id, packet.negotiationId);
        const answer = await carrier.acceptOffer({ type: "offer", sdp: packet.sdp });
        await props.onSendControl(chat.id, encodeDirectRouteSignal({
          version: 1,
          negotiationId: packet.negotiationId,
          kind: "answer",
          sdp: normalizeDirectSignal(answer, "answer"),
        }), true);
      } catch {
        closeDirectCarrier();
      }
      return;
    }

    const carrier = directCarrierRef.current;
    if (!carrier || directChatIdRef.current !== chat.id || directNegotiationIdRef.current !== packet.negotiationId) return;
    try {
      await carrier.acceptAnswer({ type: "answer", sdp: packet.sdp });
    } catch {
      closeDirectCarrier();
    }
  }

  async function ensureLiveMicrophone(): Promise<MediaStream> {
    const existing = liveStreamRef.current;
    if (existing?.getAudioTracks().some(track => track.readyState === "live")) return existing;
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      throw new Error("Live voice requires microphone/MediaRecorder support on this device.");
    }
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
      video: false,
    });
    liveStreamRef.current = stream;
    return stream;
  }

  async function startCall(): Promise<void> {
    if (!selected || blocksComposer(selected) || voiceMessageRecording || !selected.bootstrapComplete || callRef.current) return;
    setSendError("");
    const callId = randomCallId();
    const next: LiveCallState = {
      chatId: selected.id,
      callId,
      label: selected.label,
      status: "calling",
      muted: false,
    };
    callRef.current = next;
    setCall(next);
    try {
      await ensureLiveMicrophone();
      void ensureDirectUpgrade(selected, true);
      await props.onSendControl(selected.id, encodeLiveVoicePacket({ version: 1, callId, kind: "request" }), true);
    } catch (error) {
      stopLiveCallLocal(String(error));
    }
  }

  async function acceptCall(): Promise<void> {
    const current = callRef.current;
    if (!current || current.status !== "incoming") return;
    setSendError("");
    try {
      await ensureLiveMicrophone();
      const chat = props.chats.find(candidate => candidate.id === current.chatId);
      if (chat) void ensureDirectUpgrade(chat, true);
      await props.onSendControl(current.chatId, encodeLiveVoicePacket({
        version: 1,
        callId: current.callId,
        kind: "accept",
      }), true);
      const connected = { ...current, status: "connected" as const, error: undefined };
      callRef.current = connected;
      setCall(connected);
      void startLiveSender(current.chatId, current.callId);
    } catch (error) {
      stopLiveCallLocal(String(error));
    }
  }

  async function declineCall(): Promise<void> {
    const current = callRef.current;
    if (!current) return;
    try {
      await props.onSendControl(current.chatId, encodeLiveVoicePacket({ version: 1, callId: current.callId, kind: "decline" }), true);
    } finally {
      stopLiveCallLocal();
    }
  }

  async function hangupCall(): Promise<void> {
    const current = callRef.current;
    if (!current) return;
    try {
      await props.onSendControl(current.chatId, encodeLiveVoicePacket({ version: 1, callId: current.callId, kind: "hangup" }), true);
    } catch {
      // A local hangup must still stop microphone/call state if the carrier is unavailable.
    } finally {
      stopLiveCallLocal();
    }
  }

  function stopLiveCallLocal(error?: string): void {
    liveGenerationRef.current += 1;
    liveSequenceRef.current = 0;
    playbackRef.current.clear();
    liveStreamRef.current?.getTracks().forEach(track => track.stop());
    liveStreamRef.current = null;
    if (error) {
      const current = callRef.current;
      if (current) {
        const failed = { ...current, status: "error" as const, error };
        callRef.current = failed;
        setCall(failed);
        return;
      }
      setSendError(error);
    }
    callRef.current = null;
    setCall(null);
  }

  async function startLiveSender(chatId: string, callId: string): Promise<void> {
    const generation = ++liveGenerationRef.current;
    liveSequenceRef.current = 0;
    let consecutiveSendFailures = 0;
    let recorder: LiveVoiceWindowRecorder | null = null;
    try {
      const stream = await ensureLiveMicrophone();
      recorder = new LiveVoiceWindowRecorder(stream);
      recorder.start();
      let windowStartedAt = performance.now();

      while (generation === liveGenerationRef.current && callRef.current?.callId === callId) {
        // The recorder is already capturing while the previous window is being
        // encrypted/broadcast. Wait only the remainder of the target 350 ms.
        const remaining = Math.max(0, LIVE_VOICE_WINDOW_MS - (performance.now() - windowStartedAt));
        if (remaining > 0) await new Promise(resolve => window.setTimeout(resolve, remaining));
        if (generation !== liveGenerationRef.current || callRef.current?.callId !== callId) break;

        const { mime, blob } = await recorder.flushAndRestart();
        windowStartedAt = performance.now();
        if (generation !== liveGenerationRef.current || callRef.current?.callId !== callId) break;
        if (liveMutedRef.current || blob.size === 0) continue;
        if (blob.size > LIVE_VOICE_MAX_BYTES) {
          throw new Error(`Live Opus window exceeded ${LIVE_VOICE_MAX_BYTES / 1024} KiB; call stopped rather than sending an oversized media packet.`);
        }

        const data = await blobToBase64(blob);
        // Sequence numbers describe successfully handed-off media windows. If a
        // carrier attempt fails and this window is dropped, reuse the number for
        // the next window so the receiver never waits on an intentionally absent slot.
        const sequence = liveSequenceRef.current;
        try {
          const body = encodeLiveVoicePacket({
            version: 1,
            callId,
            kind: "audio",
            sequence,
            mime,
            data,
          });
          const carrier = directCarrierRef.current;
          if (props.route === "Auto" && directReadyRef.current && carrier?.ready) {
            try {
              const encrypted = await props.onSealDirect(chatId, body);
              carrier.send(base64ToBytes(encrypted));
            } catch {
              closeDirectCarrier();
              await props.onSendControl(chatId, body, true);
            }
          } else {
            await props.onSendControl(chatId, body, true);
          }
          liveSequenceRef.current = sequence + 1;
          consecutiveSendFailures = 0;
        } catch (error) {
          consecutiveSendFailures += 1;
          if (consecutiveSendFailures >= 5) throw error;
        }
      }
    } catch (error) {
      if (generation === liveGenerationRef.current && callRef.current?.callId === callId) {
        stopLiveCallLocal(String(error));
      }
    } finally {
      recorder?.close();
    }
  }

  function toggleCallMute(): void {
    const current = callRef.current;
    if (!current || current.status !== "connected") return;
    const muted = !current.muted;
    liveMutedRef.current = muted;
    liveStreamRef.current?.getAudioTracks().forEach(track => { track.enabled = !muted; });
    setCall({ ...current, muted });
  }

  async function toggleVoiceMessage(): Promise<void> {
    if (!selected || composerBlocked || voiceMessageBusy) return;
    const activeRecorder = voiceRecorderRef.current;
    if (activeRecorder && activeRecorder.state !== "inactive") {
      activeRecorder.stop();
      return;
    }
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      setSendError("Voice messages require microphone/MediaRecorder support on this device.");
      return;
    }
    setSendError("");
    setVoiceMessageBusy(true);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
        video: false,
      });
      voiceMessageStreamRef.current = stream;
      voiceMessageChunksRef.current = [];
      voiceMessageChatRef.current = selected.id;
      const preferred = preferredOpusMime();
      const recorder = new MediaRecorder(
        stream,
        preferred ? { mimeType: preferred, audioBitsPerSecond: 24_000 } : { audioBitsPerSecond: 24_000 },
      );
      voiceRecorderRef.current = recorder;
      recorder.ondataavailable = event => {
        if (event.data.size) voiceMessageChunksRef.current.push(event.data);
      };
      recorder.onerror = () => {
        setSendError("Voice-message encoder failed.");
      };
      recorder.onstop = () => {
        void finishVoiceMessage(recorder.mimeType || preferred || "audio/webm");
      };
      recorder.start();
      setVoiceMessageRecording(true);
      setVoiceMessageBusy(false);
      voiceMessageTimerRef.current = window.setTimeout(() => {
        if (recorder.state !== "inactive") recorder.stop();
      }, VOICE_MESSAGE_MAX_MS);
    } catch (error) {
      setVoiceMessageBusy(false);
      setVoiceMessageRecording(false);
      setSendError(String(error));
    }
  }

  async function finishVoiceMessage(mime: string): Promise<void> {
    if (voiceMessageTimerRef.current !== null) {
      window.clearTimeout(voiceMessageTimerRef.current);
      voiceMessageTimerRef.current = null;
    }
    voiceMessageStreamRef.current?.getTracks().forEach(track => track.stop());
    voiceMessageStreamRef.current = null;
    voiceRecorderRef.current = null;
    setVoiceMessageRecording(false);
    setVoiceMessageBusy(true);
    try {
      const blob = new Blob(voiceMessageChunksRef.current, { type: mime });
      voiceMessageChunksRef.current = [];
      if (!blob.size) throw new Error("No microphone audio was recorded.");
      if (blob.size > 48 * 1024) throw new Error("Voice message is too large for one atomic Ghost Talk transaction. Record a shorter clip.");
      const body = encodeVoiceMessage({ mime, data: await blobToBase64(blob) });
      await props.onSend(voiceMessageChatRef.current, body);
    } catch (error) {
      setSendError(String(error));
    } finally {
      setVoiceMessageBusy(false);
    }
  }

  async function submitMessage(): Promise<void> {
    const body = draft.trim();
    if (!selected || !body || sending || blocksComposer(selected)) return;
    setSending(true);
    setSendError("");
    setDraft("");
    try {
      await props.onSend(selected.id, body);
    } catch (error) {
      setSendError(String(error));
    } finally {
      setSending(false);
    }
  }

  async function startChat(): Promise<void> {
    const target = newChatTarget.trim();
    if (!target || newChatBusy) return;
    setNewChatBusy(true);
    setNewChatError("");
    try {
      const id = await props.onStartChat(target);
      setShowIgnored(false);
      setShowArchived(false);
      setSelectedId(id);
      setNewChatTarget("");
      setNewChatOpen(false);
    } catch (error) {
      setNewChatError(String(error));
    } finally {
      setNewChatBusy(false);
    }
  }

  async function acceptRequest(): Promise<void> {
    if (!selected || requestBusy) return;
    setRequestBusy(true);
    setSendError("");
    try {
      await props.onAcceptRequest(selected.id);
    } catch (error) {
      setSendError(String(error));
    } finally {
      setRequestBusy(false);
    }
  }

  if (!selected) {
    if (showIgnored) {
      return (
        <section className="page empty-state">
          <div className="empty-icon">◌</div>
          <h2>No ignored requests</h2>
          <p>Automatically or manually ignored first-contact requests will be retained here until you accept or delete them.</p>
          <button onClick={() => setShowIgnored(false)}>Back to chats</button>
        </section>
      );
    }
    return (
      <section className="page empty-state">
        <div className="empty-icon">◉</div>
        <h2>{props.chats.length ? "No active chats" : "No chats yet"}</h2>
        <p>{props.chats.length
          ? "Your active chat list is empty. Archived conversations remain available locally and can be restored at any time."
          : "Start a one-off encrypted chat with any valid Kaspa address or KNS name. The recipient does not need to publish a public Ghost Talk profile."}</p>
        <div className="direct-chat-form">
          <input value={newChatTarget} onChange={event => setNewChatTarget(event.target.value)} placeholder="kaspa:… or alice.kas" />
          <button className="primary" disabled={!newChatTarget.trim() || newChatBusy} onClick={() => void startChat()}>
            {newChatBusy ? "Checking address…" : "Start chat"}
          </button>
        </div>
        <div className="button-row">
          {archivedChats.length > 0 && <button onClick={() => setShowArchived(true)}>Archived chats ({archivedChats.length})</button>}
          {ignoredRequests.length > 0 && <button onClick={() => setShowIgnored(true)}>Ignored requests ({ignoredRequests.length})</button>}
        </div>
        {newChatError && <div className="status">{newChatError}</div>}
      </section>
    );
  }

  const request = selected.incomingRequest;
  const requestBlocksComposer = blocksComposer(selected);
  const composerBlocked = requestBlocksComposer || Boolean(call);

  return (
    <section className={`chat-layout ${advanced ? "with-drawer" : ""}`}>
      <aside className="thread-list">
        <div className="thread-list-head">
          <h3>{showIgnored ? "Ignored" : showArchived ? "Archived" : "Chats"}</h3>
          <div className="thread-actions">
            {ignoredRequests.length > 0 && (
              <button title={showIgnored ? "Back to normal chats" : "View ignored requests"} onClick={() => setShowIgnored(value => !value)}>
                {showIgnored ? "←" : `◌ ${ignoredRequests.length}`}
              </button>
            )}
            {!showIgnored && (
              <button title={showArchived ? "Back to active chats" : "View archived chats"} onClick={() => setShowArchived(value => !value)}>
                {showArchived ? "←" : "▣"}
              </button>
            )}
            {!showIgnored && !showArchived && <button title="New direct chat" onClick={() => setNewChatOpen(true)}>＋</button>}
          </div>
        </div>
        {visibleChats.map(chat => (
          <button
            type="button"
            key={chat.id}
            className={`thread-row ${chat.id === selected.id ? "active" : ""}`}
            aria-pressed={chat.id === selected.id}
            aria-label={`Open chat with ${compactKaspaLabel(threadLabelSource(chat))}`}
            title={compactKaspaLabel(threadLabelSource(chat))}
            onClick={() => setSelectedId(chat.id)}
          >
            <span className="avatar">{chat.label.slice(0, 1).toUpperCase()}</span>
            <span className="thread-row-text">
              <b>{compactKaspaLabel(threadLabelSource(chat))}{chat.verifiedPublic && <span className="verified-mark" title="Verified public Ghost Talk profile">✓</span>}</b>
              <small>{chat.incomingRequest?.state === "pending" ? "New chat request" : chat.incomingRequest?.state === "ignored" ? "Ignored request" : lastMessage(chat, props.mask)}</small>
            </span>
            {chat.incomingRequest?.state === "pending" && <span className="request-dot" title="Incoming chat request" />}
          </button>
        ))}
      </aside>
      <section className="chat">
        <header>
          <div>
            <strong title={compactKaspaLabel(threadLabelSource(selected))}>{compactKaspaLabel(threadLabelSource(selected))}{selected.verifiedPublic && <span className="verified-mark" title="Verified public Ghost Talk profile">✓</span>}</strong>
            <small>{selected.contactId ? "Saved contact" : "Direct chat · not saved as contact"}</small>
          </div>
          <div className="thread-actions">
            <button
              className={call?.chatId === selected.id ? "on" : ""}
              title="1:1 live Opus voice over Auto direct WebRTC with Kaspa fallback, or Kaspa only"
              disabled={requestBlocksComposer || request?.state === "accepted" || voiceMessageRecording || !selected.bootstrapComplete || selected.messages.some(message => message.direction === "out" && message.pending && message.pendingStage === "handshake") || Boolean(call && call.chatId !== selected.id)}
              onClick={() => call?.chatId === selected.id ? void hangupCall() : void startCall()}
            >{call?.chatId === selected.id ? "Hang up" : "☎ Call"}</button>
            {!selected.contactId && selected.peerKaspaAddress && (
              <button title="Save this direct-chat peer as a contact" onClick={() => props.onAddContact(selected.id)}>＋ Contact</button>
            )}
            <button onClick={() => props.onArchive(selected.id, !selected.archived)}>{selected.archived ? "Unarchive" : "Archive"}</button>
            {!selected.left && <button onClick={() => void props.onLeave(selected.id)}>Leave</button>}
            <button className={advanced ? "on" : ""} onClick={() => setAdvanced(value => !value)}>Advanced ▾</button>
          </div>
        </header>
        {request?.state === "pending" && (
          <div className="chat-request-banner compact">
            <code className="request-address" title={request.peerAddress}>{request.peerAddress}</code>
            <div className="button-row">
              <button disabled={requestBusy} onClick={() => props.onIgnoreRequest(selected.id)}>Ignore</button>
              <button className="primary" disabled={requestBusy} onClick={() => void acceptRequest()}>{requestBusy ? "Accepting…" : "Accept"}</button>
            </div>
          </div>
        )}
        {request?.state === "ignored" && (
          <div className="chat-request-banner compact ignored">
            <code className="request-address" title={request.peerAddress}>{request.peerAddress}</code>
            <div className="button-row">
              <button disabled={requestBusy} onClick={() => props.onDeleteRequest(selected.id)}>Delete</button>
              <button className="primary" disabled={requestBusy} onClick={() => void acceptRequest()}>{requestBusy ? "Accepting…" : "Accept"}</button>
            </div>
          </div>
        )}
        {selected.left && (
          <div className="chat-request-banner ignored">
            <b>You left this chat</b>
            <small>The local HYDRA session is closed. Start a new chat with this address if you want to establish a fresh secure session.</small>
          </div>
        )}
        {!selected.left && selected.peerLeft && (
          <div className="chat-request-banner compact ignored">
            <b>The other participant left this chat</b>
            <small>This KKTP session is closed. Start a new chat to create a fresh session.</small>
          </div>
        )}
        <div className="messages" ref={messagesRef}>
          {selected.messages.length === 0 ? (
            <div className="inline-empty">
              {request?.state === "accepted" && !selected.bootstrapComplete
                ? "Secure chat accepted. Waiting for the initiator's encrypted handshake…"
                : request
                  ? "No encrypted messages yet."
                  : "No messages yet."}
            </div>
          ) : selected.messages.map(message => <MessageBubble key={message.id} message={message} mask={props.mask} />)}
        </div>
        <footer className="composer">
          <button title="Local privacy mask" className={props.mask ? "on" : ""} onClick={() => props.onMask(!props.mask)}>***</button>
          <div className="stego-wrap">
            <button title="Wire steganography" className={props.stego !== "Off" ? "on" : ""} onClick={() => setStegoOpen(value => !value)}>S</button>
            {stegoOpen && (
              <div className="popover stego-menu">
                <b>Wire steganography</b>
                {stegoProfiles.map(option => (
                  <button
                    key={option.profile}
                    className={props.stego === option.profile ? "active" : ""}
                    disabled={!option.available}
                    title={option.note}
                    onClick={() => {
                      if (!option.available) return;
                      props.onStego(option.profile);
                      setStegoOpen(false);
                    }}
                  >
                    {option.profile}{!option.available ? " · local model required" : ""}
                  </button>
                ))}
              </div>
            )}
          </div>
          <input
            type={props.mask ? "password" : "text"}
            autoComplete="off"
            value={draft}
            disabled={composerBlocked}
            onChange={event => setDraft(event.target.value)}
            onKeyDown={event => {
              if (event.key === "Enter" && draft.trim() && !event.shiftKey) {
                event.preventDefault();
                void submitMessage();
              }
            }}
            placeholder={selected.archived ? "Archived chat · Unarchive to reply…" : selected.peerLeft ? "Chat ended by peer · start a new chat to reply…" : requestBlocksComposer ? "Finish the incoming chat request first…" : call ? "Live voice call active…" : "Message…"}
          />
          <button
            title={voiceMessageRecording ? "Stop and send voice message" : "Record voice message"}
            className={voiceMessageRecording ? "on voice-recording" : ""}
            disabled={composerBlocked || voiceMessageBusy}
            onClick={() => void toggleVoiceMessage()}
          >{voiceMessageRecording ? "■" : "🎤"}</button>
          <button className="primary" disabled={composerBlocked || sending || !draft.trim()} onClick={() => void submitMessage()}>
            {sending ? "…" : "➤"}
          </button>
        </footer>
        {sendError && <div className="status composer-status">{sendError}</div>}
      </section>
      {advanced && (
        <aside className="drawer">
          <div className="drawer-title"><h2>Advanced</h2><button onClick={() => setAdvanced(false)}>×</button></div>
          <label>Transport<select value={props.route} onChange={event => props.onRoute(event.target.value as Route)}>{["Auto", "Kaspa only"].map(value => <option key={value}>{value}</option>)}</select></label>
          {props.route === "Auto" && selected.bootstrapComplete && (
            <small className="direct-route-status">Direct route: {directRoute === "webrtc" ? "connected" : directRoute === "connecting" ? "negotiating" : "Kaspa fallback"}</small>
          )}
          <label>Stego<select value={props.stego} onChange={event => props.onStego(event.target.value as StegoProfile)}>{stegoProfiles.map(option => <option key={option.profile} value={option.profile} disabled={!option.available}>{option.profile}{!option.available ? " (local model required)" : ""}</option>)}</select></label>
          <hr />
          <small>
            Auto automatically negotiates a direct WebRTC realtime connection as soon as the authenticated chat session is active. Kaspa carries the authenticated offer/answer signaling and remains the durable/offline fallback if direct ICE connectivity is unavailable or drops. Kaspa only never opens a direct peer connection. Each Kaspa mailbox transaction transfers 0.1 KAS to the recipient address plus the network fee, and public transaction timing/amount metadata remain visible on-chain.
          </small>
        </aside>
      )}
      {call && (
        <div className="modal-backdrop voice-call-backdrop">
          <div className="modal voice-call-modal">
            <div className="voice-call-icon">☎</div>
            <h3>{call.status === "incoming" ? "Incoming voice call" : call.status === "calling" ? "Calling…" : call.status === "connected" ? "Live voice" : "Voice call ended"}</h3>
            <p className="voice-call-peer"><b>{call.label}</b></p>
            {call.status === "connected" && <p className="muted">{directRoute === "webrtc"
              ? "Full-duplex Opus · direct WebRTC media · authenticated/encrypted by the existing HYDRA session. Kaspa remains signaling/fallback."
              : directRoute === "connecting"
                ? "Full-duplex Opus · attempting direct WebRTC now; Kaspa carries media until the direct channel opens."
                : props.route === "Auto"
                  ? "Full-duplex Opus · direct route unavailable, using authenticated HYDRA/Kaspa fallback."
                  : "Full-duplex Opus · authenticated/encrypted by the existing HYDRA session and carried by Kaspa only."}</p>}
            {call.status === "calling" && <p className="muted">Waiting for the recipient to answer…</p>}
            {call.status === "incoming" && <p className="muted">Accept to start microphone capture. No microphone audio is sent before you answer.</p>}
            {call.error && <div className="status">{call.error}</div>}
            <div className="button-row voice-call-actions">
              {call.status === "incoming" ? (
                <>
                  <button onClick={() => void declineCall()}>Decline</button>
                  <button className="primary" onClick={() => void acceptCall()}>Accept</button>
                </>
              ) : call.status === "connected" ? (
                <>
                  <button className={call.muted ? "on" : ""} onClick={toggleCallMute}>{call.muted ? "Unmute" : "Mute"}</button>
                  <button onClick={() => void hangupCall()}>Hang up</button>
                </>
              ) : call.status === "error" ? (
                <button onClick={() => stopLiveCallLocal()}>Close</button>
              ) : (
                <button onClick={() => void hangupCall()}>Cancel</button>
              )}
            </div>
            {call.status === "connected" && <small>{directRoute === "webrtc"
              ? "Direct media windows do not create Kaspa transactions or chat bubbles. If the peer connection drops, Auto falls back to the normal Kaspa mailbox carrier."
              : "Kaspa fallback media windows do not create chat bubbles or per-window delivery-ack transactions; each outbound window is a mailbox transaction and incurs the normal network fee/transfer policy."}</small>}
          </div>
        </div>
      )}
      {newChatOpen && (
        <div className="modal-backdrop">
          <div className="modal direct-chat-modal">
            <h3>Start a chat</h3>
            <p className="muted">Enter any valid Kaspa address or KNS name. A public Ghost Talk profile is optional. If no verified public profile is available, your first message stays local while Ghost Talk sends a direct Kaspa chat request containing only the public material needed to establish HYDRA encryption. No directory listing is required, but that bootstrap metadata is still visible on the public Kaspa chain. The person is not added to Contacts.</p>
            <input autoFocus value={newChatTarget} onChange={event => setNewChatTarget(event.target.value)} placeholder="kaspa:… or alice.kas" onKeyDown={event => { if (event.key === "Enter") void startChat(); }} />
            {newChatError && <div className="status">{newChatError}</div>}
            <div className="button-row">
              <button onClick={() => { setNewChatOpen(false); setNewChatError(""); }}>Cancel</button>
              <button className="primary" disabled={!newChatTarget.trim() || newChatBusy} onClick={() => void startChat()}>
                {newChatBusy ? "Checking address…" : "Start chat"}
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

function blocksComposer(chat: ChatThread): boolean {
  return Boolean(chat.archived)
    || Boolean(chat.left)
    || Boolean(chat.peerLeft)
    || Boolean(chat.incomingRequest && chat.incomingRequest.state !== "accepted");
}

function threadLabelSource(chat: ChatThread): string {
  const label = chat.label.trim();
  const peer = chat.peerKaspaAddress?.trim();
  if (peer && /^(kaspa(?:test)?):/i.test(label) && peer.startsWith(label) && peer.length > label.length) {
    return peer;
  }
  return chat.label;
}

function compactKaspaLabel(value: string): string {
  const match = /^(kaspa(?:test)?):(.+)$/i.exec(value.trim());
  if (!match) return value;
  const payload = match[2];
  if (payload.length <= 12) return payload;
  return `${payload.slice(0, 6)}...${payload.slice(-4)}`;
}

function isHiddenIgnoredRequest(chat: ChatThread): boolean {
  return chat.incomingRequest?.state === "ignored" && !chat.contactId && chat.messages.length === 0;
}

function MessageBubble({ message, mask }: { message: Message; mask: boolean }) {
  const voice = decodeVoiceMessage(message.body);
  const body = mask ? "*".repeat(Math.max(1, voice ? 13 : message.body.length)) : message.body;
  const className = message.direction === "out" ? "me" : message.direction === "system" ? "system" : "them";
  return (
    <p className={`${className}${message.pending ? " pending-message" : ""}${message.sendState === "failed" ? " failed-message" : ""}`}>
      {voice ? (mask ? <span>Voice message hidden</span> : <span className="voice-message"><b>Voice message</b><audio controls preload="none" src={`data:${voice.mime};base64,${voice.data}`} /></span>) : body}
      {message.direction === "out" && message.sendState && (
        <small className={`message-state ${message.sendState}`} title={message.sendError || message.sendState}>
          {message.sendState === "sending" ? "◷ Sending" : message.sendState === "sent" ? "✓ Sent" : message.sendState === "delivered" ? "✓✓ Delivered" : "⚠ Not sent"}
        </small>
      )}
      {message.pending && message.sendState !== "failed" && <small>{pendingLabel(message)}</small>}
    </p>
  );
}

function pendingLabel(message: Message): string {
  switch (message.pendingStage) {
    case "request": return "Waiting for recipient to accept secure chat…";
    case "handshake": return "Establishing secure session…";
    case "delivery": return "Broadcasting to Kaspa…";
    default: return message.pendingId ? "Establishing secure session…" : "Broadcasting to Kaspa…";
  }
}

function lastMessage(chat: ChatThread, mask: boolean): string {
  const body = chat.messages.at(-1)?.body ?? "No messages";
  if (decodeVoiceMessage(body)) return mask ? "*************" : "Voice message";
  return mask ? "*".repeat(Math.min(Math.max(body.length, 1), 24)) : body;
}

function normalizeDirectSignal(signal: Signal, expected: "offer" | "answer"): DirectRouteSignalPacket["sdp"] {
  if (signal.type !== expected || !signal.sdp.sdp) throw new Error(`Expected WebRTC ${expected}`);
  return { type: expected, sdp: signal.sdp.sdp };
}

function randomCallId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
}
