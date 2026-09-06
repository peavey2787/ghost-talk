/**
 * dashboardController.js - Dashboard logic for KKTP Voice Live Test.
 * Handles init, lobby, voice join/leave, unified voice action button (3-state),
 * half-duplex/full-duplex mode toggle, audio replay with latency profiler, stats.
 */
import { KKGameEngine } from "../../kkGameEngine.js";
import { getInstructions } from "../../lobby/tests/liveTestSteps.js";
import { VoiceTestRunner } from "./voiceTestRunner.js";
import { decodeOpusToPcm } from "../audio/opusDecoder.js";

const FAUCET_URL = "https://faucet-tn10.kaspanet.io/";
const EXPLORER_BASE = "https://explorer-tn10.kaspa.org/txs/";
const BALANCE_POLL_MS = 6000;

const VOICE_BTN = Object.freeze({ REQUEST: "request", RECORD: "record", STOP: "stop" });
const VOICE_BTN_LABELS = Object.freeze({
  [VOICE_BTN.REQUEST]: "Request to Talk",
  [VOICE_BTN.RECORD]: "Start Recording",
  [VOICE_BTN.STOP]: "Stop & Send",
});

let engine = null;
let runner = new VoiceTestRunner();
let balancePollTimer = null;
let lastJoinCode = null;
let lastLoggedState = null;
let statsInterval = null;
let searchUnsub = null;
let voiceBtnState = VOICE_BTN.REQUEST;
let voiceJoined = false;

const $ = (id) => document.getElementById(id);
let dom = {};

export function boot() {
  dom = {
    logContainer: $("log-container"), fundingSection: $("funding-section"),
    instructionsSection: $("instructions-section"), stateSection: $("state-section"),
    actionsSection: $("actions-section"), voiceSection: $("voice-section"),
    statsSection: $("stats-section"), explorerSection: $("explorer-section"),
    balanceDisplay: $("balance-display"), addressDisplay: $("address-display"),
    instructionsText: $("instructions-text"), stateBadge: $("state-badge"),
    hostActions: $("host-actions"), joinerActions: $("joiner-actions"),
    joinCodeArea: $("join-code-area"), joinCodeDisplay: $("join-code-display"),
    joinCodeInput: $("join-code-input"), leaveArea: $("leave-area"),
    btnCloseLobby: $("btn-close-lobby"), loadingActions: $("loading-actions"),
    loadingActionsText: $("loading-actions-text"),
    btnJoinVoice: $("btn-join-voice"), btnLeaveVoice: $("btn-leave-voice"),
    btnVoiceAction: $("btn-voice-action"), btnMute: $("btn-mute"),
    voiceLoading: $("voice-loading"), voiceLoadingText: $("voice-loading-text"),
    sentTxList: $("sent-tx-list"), receivedTxList: $("received-tx-list"),
    lobbySearchSection: $("lobby-search-section"), btnToggleSearch: $("btn-toggle-search"),
    lobbySearchResults: $("lobby-search-results"), lobbyMaxAgeMinsInput: $("lobby-max-age-mins"),
    modeHalfDuplex: $("mode-half-duplex"), modeFullDuplex: $("mode-full-duplex"),
  };
  loadWalletsIntoSelect();
  bindEvents();
  setInterval(() => { if (engine) { updateStateBadge(); updateActionVisibility(); } }, 1500);
}

function uiLog(msg) {
  const el = document.createElement("div");
  el.className = "log-entry";
  el.textContent = msg;
  dom.logContainer.appendChild(el);
  dom.logContainer.scrollTop = dom.logContainer.scrollHeight;
}

function getRole() { return document.querySelector('input[name="role"]:checked')?.value ?? "host"; }
function isFullDuplex() { return dom.modeFullDuplex?.checked ?? false; }

function setUIReady(ready) {
  const show = [dom.fundingSection, dom.instructionsSection, dom.stateSection, dom.actionsSection, dom.lobbySearchSection];
  const hide = [dom.voiceSection, dom.statsSection, dom.explorerSection];
  if (ready) { show.forEach((s) => s?.classList.remove("hidden")); updateInstructions(); updateStateBadge(); updateActionVisibility(); startBalancePoll(); }
  else { [...show, ...hide].forEach((s) => s?.classList.add("hidden")); stopBalancePoll(); if (statsInterval) { clearInterval(statsInterval); statsInterval = null; } }
}

function updateInstructions() { dom.instructionsText.textContent = getInstructions(getRole(), engine ? engine.lobbyState : "IDLE", !!engine); }
function updateStateBadge() { if (engine) dom.stateBadge.textContent = engine.lobbyState || "IDLE"; }

function updateActionVisibility() {
  const role = getRole(), state = engine ? engine.lobbyState : "IDLE", inLobby = state === "HOSTING" || state === "MEMBER";
  dom.hostActions?.classList.toggle("hidden", role !== "host");
  dom.joinerActions?.classList.toggle("hidden", role !== "joiner");
  dom.leaveArea?.classList.toggle("hidden", !inLobby);
  dom.btnCloseLobby?.classList.toggle("hidden", role !== "host" || state !== "HOSTING");
  dom.voiceSection?.classList.toggle("hidden", !inLobby);
  if (inLobby) { dom.statsSection?.classList.remove("hidden"); dom.explorerSection?.classList.remove("hidden"); }
  dom.btnJoinVoice.disabled = !inLobby || voiceJoined;
  dom.btnLeaveVoice.disabled = !voiceJoined;
  dom.btnLeaveVoice.classList.toggle("hidden", !voiceJoined);
  dom.btnVoiceAction.disabled = !voiceJoined;
  dom.btnMute.classList.toggle("hidden", !voiceJoined || !isFullDuplex());
  updateVoiceActionButton();
}

function updateVoiceActionButton() {
  if (!dom.btnVoiceAction) return;
  if (isFullDuplex()) { dom.btnVoiceAction.classList.add("hidden"); return; }
  dom.btnVoiceAction.classList.remove("hidden");
  dom.btnVoiceAction.textContent = VOICE_BTN_LABELS[voiceBtnState] ?? "Request to Talk";
  dom.btnVoiceAction.disabled = !voiceJoined;
}

function resetVoiceBtnState() { voiceBtnState = VOICE_BTN.REQUEST; updateVoiceActionButton(); }

function renderExplorerLinks() {
  const sent = runner.getSentTxIds(), received = runner.getReceivedTxIds();
  const shortId = (id) => id.length > 12 ? id.slice(0, 10) + "\u2026" : id;
  dom.sentTxList.innerHTML = sent.length === 0 ? "<span style='color:#666'>None yet</span>"
    : sent.map((e) => `<a class="explorer-link" href="${EXPLORER_BASE}${e.txId}" target="_blank" rel="noopener noreferrer">${e.type}: ${shortId(e.txId)}</a>`).join("");

  const audioEntries = runner.getReceivedAudioEntries();
  dom.receivedTxList.innerHTML = received.length === 0 ? "<span style='color:#666'>None yet</span>"
    : received.map((e) => {
        const link = `<a class="explorer-link" href="${EXPLORER_BASE}${e.txId}" target="_blank" rel="noopener noreferrer">${e.type}: ${shortId(e.txId)}</a>`;
        if (e.type !== "audio" || !e.opusBytes) return link;
        const entry = audioEntries.find((a) => a.txId === e.txId);
        const ago = entry?.receivedAt ? ` <span style="color:#888;font-size:0.8em;">(${((Date.now() - entry.receivedAt) / 1000).toFixed(1)}s ago)</span>` : "";
        return `<div style="display:flex;align-items:center;gap:4px;">${link}${ago}<button class="btn-play-audio" data-txid="${e.txId}" title="Play audio" style="background:none;border:1px solid #49eacb;border-radius:4px;padding:2px 6px;color:#49eacb;cursor:pointer;font-size:0.85em;">&#9654;</button></div>`;
      }).join("");
  dom.receivedTxList.querySelectorAll(".btn-play-audio").forEach((btn) => { btn.onclick = () => playReceivedAudio(btn.dataset.txid); });
}

async function playReceivedAudio(txId) {
  const entry = runner.getReceivedAudioEntries().find((e) => e.txId === txId);
  if (!entry?.opusBytes) { uiLog("No audio data for tx " + txId); return; }
  let ctx;
  try {
    ctx = new (window.AudioContext || window.webkitAudioContext)();
    const { channelData, sampleRate } = await decodeOpusToPcm(entry.opusBytes, ctx);
    if (!channelData?.length) { uiLog("Decode returned empty audio."); return; }
    const buf = ctx.createBuffer(channelData.length, channelData[0].length, sampleRate);
    for (let c = 0; c < channelData.length; c++) buf.copyToChannel(channelData[c], c);
    const src = ctx.createBufferSource();
    src.buffer = buf; src.connect(ctx.destination); src.start(0);
    src.onended = () => ctx.close();
    uiLog("Playing audio: " + txId.slice(0, 10) + "\u2026");
  } catch (e) { uiLog("Play failed: " + (e?.message || e)); ctx?.close(); }
}

function updateStats() {
  const s = runner.getStats();
  $("stat-rtt-sent").textContent = s.rttSent; $("stat-rtt-received").textContent = s.rttReceived;
  $("stat-audio-sent").textContent = s.audioSent; $("stat-audio-received").textContent = s.audioReceived;
  $("stat-kas-sent").textContent = s.kasSent.toFixed(5);
  $("stat-last-received").textContent = s.lastReceivedAt == null ? "\u2014" : Math.round((Date.now() - s.lastReceivedAt) / 1000) + " s ago";
  renderExplorerLinks();
}

async function refreshBalance() {
  if (!engine) return;
  try { const kas = await engine.getBalance(); dom.balanceDisplay.textContent = (typeof kas === "number" ? kas.toFixed(8) : String(kas)) + " KAS"; }
  catch (e) { uiLog("Balance refresh: " + (e?.message || e)); }
}
function startBalancePoll() { stopBalancePoll(); refreshBalance(); balancePollTimer = setInterval(refreshBalance, BALANCE_POLL_MS); }
function stopBalancePoll() { if (balancePollTimer) { clearInterval(balancePollTimer); balancePollTimer = null; } }
function showLoadingActions(show, text = "Working...") { dom.loadingActions?.classList.toggle("hidden", !show); if (dom.loadingActionsText) dom.loadingActionsText.textContent = text; }
function copyFeedback(id, ms = 1500) { const fb = $(id); if (fb) { fb.classList.add("show"); setTimeout(() => fb.classList.remove("show"), ms); } }

async function loadWalletsIntoSelect() {
  const sel = $("wallet-select");
  try {
    sel.innerHTML = '<option value="">Loading wallets\u2026</option>'; sel.disabled = true;
    if (!engine) engine = new KKGameEngine();
    const wallets = await engine.getAllWallets();
    sel.innerHTML = '<option value="">-- Select wallet --</option>';
    (wallets || []).forEach((w) => { const n = w.filename ?? w.title ?? w.name ?? "?"; const o = document.createElement("option"); o.value = n; o.textContent = n; sel.appendChild(o); });
    uiLog("Wallets loaded: " + (wallets?.length ?? 0));
  } catch (e) { sel.innerHTML = '<option value="">-- Load wallets first --</option>'; uiLog("Load wallets failed: " + (e?.message || e)); }
  finally { sel.disabled = false; }
}

async function doInit(walletName, password, loadingEl, label) {
  if (!walletName || !password) return false;
  if (!engine) engine = new KKGameEngine();
  runner.setEngine(engine);
  if (loadingEl) loadingEl.classList.add("show");
  try {
    await engine.init({ password, walletName, network: "testnet-10", gameId: "kktp-voice-test", gameName: "KKTP Voice Test" });
    dom.addressDisplay.textContent = engine.address || "--";
    $("faucet-link")?.setAttribute("href", FAUCET_URL);
    setUIReady(true);
    if (engine.on) {
      engine.on("lobbyUpdated", (d) => { const st = d?.state; if (st === "MEMBER" && lastLoggedState !== "MEMBER") { uiLog("Joined lobby."); lastLoggedState = "MEMBER"; } else if (st) lastLoggedState = st; updateInstructions(); updateStateBadge(); updateActionVisibility(); });
      engine.on("lobbyClosed", () => { lastLoggedState = null; updateInstructions(); updateStateBadge(); updateActionVisibility(); });
      engine.on("messageReceived", (d) => { uiLog("Message from " + (d?.senderName || d?.senderId || "?") + ": " + (typeof d?.text === "string" ? d.text : String(d?.text ?? ""))); });
    }
    uiLog("Initialized. Address: " + (engine.address ? engine.address.slice(0, 20) + "..." : "--"));
    return true;
  } catch (e) { uiLog((label || "Init") + " failed: " + (e?.message || e)); return false; }
  finally { if (loadingEl) loadingEl.classList.remove("show"); }
}

function bindEvents() {
  $("btn-init").onclick = async () => { const w = $("wallet-select").value?.trim(), p = $("password-input").value; if (!w || !p) { uiLog("Select a wallet and enter password."); return; } await doInit(w, p, $("init-loading"), "Init"); };
  $("btn-create-wallet").onclick = async () => { const w = $("new-wallet-name")?.value?.trim(), p = $("new-wallet-password")?.value; if (!w || !p) { uiLog("Enter wallet name and password."); return; } if (!/^[a-zA-Z0-9_-]+$/.test(w)) { uiLog("Wallet name: letters, numbers, hyphens, underscores only."); return; } if (await doInit(w, p, $("create-wallet-loading"), "Create wallet")) uiLog("Wallet created: " + w); };
  $("btn-copy-address").onclick = async () => { if (!engine?.address) return; try { await navigator.clipboard.writeText(engine.address); copyFeedback("copy-address-feedback"); } catch (e) { uiLog("Copy failed: " + (e?.message || e)); } };
  $("btn-copy-join-code").onclick = async () => { const c = dom.joinCodeDisplay.value || lastJoinCode; if (!c) return; try { await navigator.clipboard.writeText(c); copyFeedback("copy-join-feedback"); } catch (e) { uiLog("Copy failed: " + (e?.message || e)); } };
  document.querySelectorAll('input[name="role"]').forEach((el) => el.addEventListener("change", () => { updateInstructions(); updateActionVisibility(); }));

  $("btn-create-lobby").onclick = async () => {
    if (!engine) return; showLoadingActions(true, "Creating lobby...");
    try { const r = await engine.createLobby({ lobbyName: "Voice Test Lobby", maxMembers: 4, displayName: "Host", gameId: "kktp-voice-test", gameName: "KKTP Voice Test" }); lastJoinCode = r?.joinCode ?? ""; dom.joinCodeDisplay.value = lastJoinCode; dom.joinCodeArea?.classList.remove("hidden"); updateInstructions(); updateStateBadge(); updateActionVisibility(); uiLog("Lobby created. Share join code."); }
    catch (e) { uiLog("Create lobby failed: " + (e?.message || e)); } finally { showLoadingActions(false); }
  };
  $("btn-join-lobby").onclick = async () => {
    const code = dom.joinCodeInput.value?.trim(); if (!code) { uiLog("Paste the join code first."); return; } if (!engine) return;
    showLoadingActions(true, "Joining..."); uiLog("Joining...");
    try { await engine.joinLobby(code, "Joiner-" + Math.random().toString(36).slice(2, 8)); updateInstructions(); updateStateBadge(); updateActionVisibility(); }
    catch (e) { uiLog("Join failed: " + (e?.message || e)); } finally { showLoadingActions(false); }
  };

  $("btn-toggle-search").onclick = () => {
    if (!engine) return;
    if (searchUnsub) { searchUnsub(); searchUnsub = null; dom.btnToggleSearch.textContent = "Start searching for lobbies"; return; }
    const raw = dom.lobbyMaxAgeMinsInput?.value?.trim(), mins = raw ? Math.max(1, parseInt(raw, 10)) : undefined;
    const opts = mins != null && Number.isFinite(mins) ? { maxAgeMins: mins } : {};
    dom.lobbySearchResults.innerHTML = "";
    searchUnsub = engine.searchLobbies((info) => {
      uiLog("Lobby found: " + (info.lobbyName || info.lobbyId) + " \u2014 " + (info.joinCode || ""));
      const row = document.createElement("div"); row.style.marginBottom = "0.3em";
      const name = (info.lobbyName || info.lobbyId || "Lobby").slice(0, 24), code = info.joinCode || "";
      row.innerHTML = `<span class="address-text">${name}</span> <button type="button" class="use-join-code" data-code="${code.replace(/"/g, "&quot;")}">Use</button> <button type="button" class="copy-join-code" data-code="${code.replace(/"/g, "&quot;")}">Copy</button>`;
      dom.lobbySearchResults.appendChild(row);
      row.querySelector(".use-join-code")?.addEventListener("click", function () { const c = this.getAttribute("data-code"); if (c) { dom.joinCodeInput.value = c; uiLog("Join code filled."); } });
      row.querySelector(".copy-join-code")?.addEventListener("click", function () { const c = this.getAttribute("data-code"); if (c && navigator.clipboard) navigator.clipboard.writeText(c).then(() => uiLog("Join code copied.")); });
    }, opts);
    dom.btnToggleSearch.textContent = "Stop searching";
  };

  $("btn-leave-lobby").onclick = async () => { if (!engine) return; showLoadingActions(true, "Leaving..."); try { await engine.leaveLobby("user-left"); runner.destroy(); voiceJoined = false; resetVoiceBtnState(); updateInstructions(); updateStateBadge(); updateActionVisibility(); uiLog("Left lobby."); } catch (e) { uiLog("Leave failed: " + (e?.message || e)); } finally { showLoadingActions(false); } };
  $("btn-close-lobby").onclick = async () => { if (!engine) return; showLoadingActions(true, "Closing..."); try { await engine.closeLobby("host-closed"); updateInstructions(); updateStateBadge(); updateActionVisibility(); uiLog("Lobby closed."); } catch (e) { uiLog("Close failed: " + (e?.message || e)); } finally { showLoadingActions(false); } };

  dom.btnJoinVoice.onclick = handleJoinVoice;
  dom.btnLeaveVoice.onclick = handleLeaveVoice;
  dom.btnVoiceAction.onclick = handleVoiceAction;
  dom.btnMute.onclick = handleMuteToggle;
  dom.modeHalfDuplex?.addEventListener("change", handleModeChange);
  dom.modeFullDuplex?.addEventListener("change", handleModeChange);
}

async function handleJoinVoice() {
  if (!engine) return;
  dom.voiceLoading.classList.add("show"); dom.voiceLoadingText.textContent = "Joining voice...";
  try {
    await runner.joinVoice(); voiceJoined = true; resetVoiceBtnState(); updateActionVisibility();
    uiLog("Voice chat joined.");
    runner.on("error", (e) => uiLog("Voice error: " + (e?.message || e)));
    runner.on("leaseExpired", () => { uiLog("Lease expired."); resetVoiceBtnState(); });
    if (!statsInterval) statsInterval = setInterval(updateStats, 2000);
    updateStats();
    if (isFullDuplex()) await startFullDuplex();
  } catch (e) { uiLog("Join voice failed: " + (e?.message || e)); }
  finally { dom.voiceLoading.classList.remove("show"); }
}

function handleLeaveVoice() {
  runner.leaveVoice(); voiceJoined = false; resetVoiceBtnState(); updateActionVisibility(); uiLog("Left voice chat.");
}

async function handleVoiceAction() {
  if (!voiceJoined) return;
  if (voiceBtnState === VOICE_BTN.REQUEST) {
    dom.voiceLoading.classList.add("show"); dom.voiceLoadingText.textContent = "Waiting for RTT confirmation...";
    try { await runner.requestToTalk(); uiLog("Request to talk confirmed."); voiceBtnState = VOICE_BTN.RECORD; updateVoiceActionButton(); updateStats(); }
    catch (e) { uiLog("RTT failed: " + (e?.message || e)); }
    finally { dom.voiceLoading.classList.remove("show"); }
  } else if (voiceBtnState === VOICE_BTN.RECORD) {
    try { await runner.startRecording(); uiLog("Recording started."); voiceBtnState = VOICE_BTN.STOP; updateVoiceActionButton(); }
    catch (e) { uiLog("Start recording failed: " + (e?.message || e)); }
  } else if (voiceBtnState === VOICE_BTN.STOP) {
    dom.voiceLoading.classList.add("show"); dom.voiceLoadingText.textContent = "Sending chunks...";
    try { await runner.stopRecording(); uiLog("Chunks sent."); updateStats(); }
    catch (e) { uiLog("Stop/send failed: " + (e?.message || e)); }
    finally { dom.voiceLoading.classList.remove("show"); resetVoiceBtnState(); }
  }
}

function handleMuteToggle() {
  if (!voiceJoined || !isFullDuplex()) return;
  if (runner.isMuted) { runner.unmute(); dom.btnMute.textContent = "Mute"; uiLog("Unmuted."); }
  else { runner.mute(); dom.btnMute.textContent = "Unmute"; uiLog("Muted."); }
}

async function handleModeChange() {
  if (!voiceJoined) { updateActionVisibility(); return; }
  if (isFullDuplex()) { await startFullDuplex(); } else { runner.stopLiveMode(); resetVoiceBtnState(); uiLog("Switched to half-duplex."); }
  updateActionVisibility();
}

async function startFullDuplex() {
  dom.voiceLoading.classList.add("show"); dom.voiceLoadingText.textContent = "Starting live mode...";
  try { await runner.startLiveMode(); dom.btnMute.textContent = "Mute"; uiLog("Full-duplex live mode started."); }
  catch (e) { uiLog("Live mode failed: " + (e?.message || e)); }
  finally { dom.voiceLoading.classList.remove("show"); }
}
