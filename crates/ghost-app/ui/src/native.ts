import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DerivationPreset,
  DirectoryLiveEvent,
  KasSignerIdentityOwnershipProof,
  ProfileBackupContact,
  ProfileBackupMessage,
  ProfileBackupRestoreResult,
  PublicGhostProfile,
  NetworkStatusEvent,
  WalletLiveEvent,
  WalletPublic,
  WalletRecovery,
  WalletSnapshot,
} from "./model";

export const isTauri = "__TAURI_INTERNALS__" in window;

export async function loadNativeProfileState(): Promise<string | null> {
  if (!isTauri) return null;
  return invoke<string | null>("profile_state_load");
}

export async function saveNativeProfileState(json: string): Promise<void> {
  if (!isTauri) return;
  await invoke("profile_state_save", { json });
}


export async function initializeHydraFromWallet(args: {
  profileId: string;
  password: string;
  sealed: number[];
  public: WalletPublic;
}): Promise<{ identity_id: string; label: string }> {
  if (!isTauri) throw new Error("HYDRA identity initialization requires the native Ghost Talk app.");
  return invoke("hydra_initialize_from_wallet", args);
}

export async function ensureHydra(args: {
  profileId: string;
  password: string;
  identityId?: string;
}): Promise<{ identity_id: string; label: string }> {
  if (!isTauri) throw new Error("HYDRA identity access requires the native Ghost Talk app.");
  return invoke("hydra_ensure", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId || null,
  });
}

export interface PeerRouteRegistration {
  contactId: string;
  kaspaAddress: string;
  displayName: string;
  sessionSid?: string;
}

export async function registerHydraPeerRoutes(args: {
  profileId: string;
  identityId: string;
  routes: PeerRouteRegistration[];
}): Promise<void> {
  if (!isTauri || args.routes.length === 0) return;
  await invoke("hydra_register_peer_routes", {
    profileId: args.profileId,
    identityId: args.identityId,
    routes: args.routes.map(route => ({
      contact_id: route.contactId,
      kaspa_address: route.kaspaAddress,
      display_name: route.displayName,
      session_sid: route.sessionSid || null,
    })),
  });
}

export async function lockHydraProfile(profileId: string): Promise<void> {
  if (!isTauri) return;
  await invoke("hydra_lock_profile", { profileId });
}

export interface ResolvedGhostPeer {
  kaspa_address: string;
  display_name: string;
  hydra_handle?: string;
  descriptor_blue_score?: string;
  kns_name?: string;
  verified_public: boolean;
  username: string;
  description: string;
  interests: string[];
}

export async function resolveGhostPeer(args: {
  profileId: string;
  password: string;
  identityId: string;
  target: string;
  network: string;
  restEndpoint?: string;
}): Promise<ResolvedGhostPeer> {
  if (!isTauri) throw new Error("Ghost Talk peer discovery requires the native app.");
  return invoke("resolve_ghost_peer", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    target: args.target,
    network: args.network,
    restEndpoint: args.restEndpoint || null,
  });
}

export async function lookupGhostProfile(args: {
  target: string;
  network: string;
  restEndpoint?: string;
}): Promise<PublicGhostProfile | null> {
  if (!isTauri) throw new Error("Ghost Talk public-profile lookup requires the native app.");
  return invoke("lookup_ghost_profile", {
    target: args.target,
    network: args.network,
    restEndpoint: args.restEndpoint || null,
  });
}

export async function publishGhostDescriptor(args: {
  profileId: string;
  password: string;
  identityId: string;
  displayName: string;
  username: string;
  description: string;
  interests: string[];
  discoverable: boolean;
  sealed: number[];
  public: WalletPublic;
  restEndpoint?: string;
  wrpcEndpoint?: string;
}): Promise<{ kaspa_address: string; transaction_id?: string; already_current: boolean; discoverable: boolean; public: WalletPublic }> {
  if (!isTauri) throw new Error("Ghost Talk descriptor publication requires the native app.");
  return invoke("publish_ghost_descriptor", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    displayName: args.displayName,
    username: args.username,
    description: args.description,
    interests: args.interests,
    discoverable: args.discoverable,
    sealed: args.sealed,
    public: args.public,
    restEndpoint: args.restEndpoint || null,
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export interface HydraControlProjection {
  destination: string;
  payloads_hex: string[];
  completes_pending_id?: string;
}

export interface HydraRecoveryProjection {
  destination: string;
  offer_hex: string;
  sid: string;
  peer_hydra_id: string;
}

export interface HydraIncomingRequestProjection {
  request_id: string;
  peer_address: string;
  local_address: string;
  peer_label: string;
  peer_hydra_id: string;
  signed_request_hex: string;
}

export interface HydraContactAcceptedProjection {
  request_id: string;
  peer_address: string;
  acceptor_address: string;
  peer_label: string;
  peer_hydra_id: string;
}

export interface HydraSessionEndedProjection {
  peer_hydra_id: string;
  peer_address: string;
  sid: string;
  reason: string;
}

export interface HydraMailboxResult {
  received?: { from: string; plaintext: string; session_sid?: string };
  control?: HydraControlProjection;
  recovery?: HydraRecoveryProjection;
  incoming_request?: HydraIncomingRequestProjection;
  contact_accepted?: HydraContactAcceptedProjection;
  peer_address?: string;
  peer_label?: string;
  message_id?: string;
  delivery_ack?: string;
  delivery_ack_peer?: string;
  session_established_peer?: string;
  session_ended?: HydraSessionEndedProjection;
  discard: boolean;
}

export async function leaveHydraPeer(profileId: string, contactId: string): Promise<void> {
  if (!isTauri) return;
  await invoke("hydra_leave_peer", { profileId, contactId });
}

export async function rejoinHydraPeer(profileId: string, contactId: string): Promise<void> {
  if (!isTauri) return;
  await invoke("hydra_rejoin_peer", { profileId, contactId });
}

export async function previewHydraContactRequest(args: {
  envelopeHex: string;
  localKaspaAddresses: string[];
}): Promise<HydraIncomingRequestProjection | null> {
  if (!isTauri) return null;
  return invoke<HydraIncomingRequestProjection | null>("hydra_preview_contact_request", args);
}

export async function receiveHydraMailbox(args: {
  profileId: string;
  password: string;
  identityId: string;
  envelopeHex: string;
  localKaspaAddresses: string[];
  activeSessionSids: string[];
}): Promise<HydraMailboxResult> {
  if (!isTauri) return { discard: false };
  return invoke("hydra_receive_mailbox", args);
}

export async function sealHydraDirect(args: {
  profileId: string;
  contactId: string;
  sessionSid: string;
  body: string;
}): Promise<string> {
  if (!isTauri) throw new Error("Direct HYDRA transport requires the native Ghost Talk app.");
  const result = await invoke<{ envelope_b64: string }>("hydra_seal_direct", args);
  return result.envelope_b64;
}

export async function openHydraDirect(args: {
  profileId: string;
  contactId: string;
  sessionSid: string;
  envelopeB64: string;
}): Promise<string | null> {
  if (!isTauri) throw new Error("Direct HYDRA transport requires the native Ghost Talk app.");
  return invoke<string | null>("hydra_open_direct", args);
}

export async function presets(): Promise<DerivationPreset[]> {
  if (!isTauri) return fallbackPresets;
  return invoke<DerivationPreset[]>("derivation_presets");
}

export async function createWallet(args: {
  password: string;
  passphrase: string;
  accountPath: string;
  network: string;
}): Promise<{ sealed: number[]; mnemonic: string; public: WalletPublic }> {
  return invoke("wallet_create", args);
}

export async function importWallet(args: {
  password: string;
  mnemonic: string;
  passphrase: string;
  accountPath: string;
  network: string;
}): Promise<{ sealed: number[]; public: WalletPublic }> {
  return invoke("wallet_import", args);
}

export async function unlockWallet(args: {
  profileId: string;
  password: string;
  sealed: number[];
  public: WalletPublic;
}): Promise<WalletPublic> {
  if (!isTauri) throw new Error("Wallet unlock requires the native Ghost Talk app.");
  return invoke("wallet_unlock", args);
}

export async function lockWallet(profileId: string): Promise<void> {
  if (!isTauri) return;
  await invoke("wallet_lock", { profileId });
}

export async function setRememberedUnlock(args: {
  profileId: string;
  enabled: boolean;
  password: string;
  sealed: number[];
  public: WalletPublic;
}): Promise<void> {
  if (!isTauri) throw new Error("Remembered unlock requires the native Ghost Talk app.");
  return invoke("remembered_unlock_set", {
    profileId: args.profileId,
    enabled: args.enabled,
    password: args.password,
    sealed: args.sealed,
    public: args.public,
  });
}

export async function loadRememberedUnlock(profileId: string): Promise<string | null> {
  if (!isTauri) return null;
  return invoke<string | null>("remembered_unlock_load", { profileId });
}

export async function revealWalletRecovery(args: {
  password: string;
  sealed: number[];
  public: WalletPublic;
}): Promise<WalletRecovery> {
  if (!isTauri) throw new Error("Wallet recovery reveal requires the native Ghost Talk app.");
  return invoke("wallet_reveal_recovery", args);
}

export async function nextReceive(publicWallet: WalletPublic): Promise<WalletPublic> {
  return invoke("wallet_next_receive", { public: publicWallet });
}

export async function refreshWallet(
  publicWallet: WalletPublic,
  restEndpoint?: string,
  wrpcEndpoint?: string,
): Promise<WalletSnapshot> {
  return invoke("wallet_refresh", {
    public: publicWallet,
    options: { restEndpoint: restEndpoint || null, wrpcEndpoint: wrpcEndpoint || null },
  });
}

export async function sendKaspa(args: {
  profileId: string;
  reuseUnlocked: boolean;
  password: string;
  sealed: number[];
  public: WalletPublic;
  destination: string;
  amountSompi: string;
  feeSompi: string;
  restEndpoint?: string;
  wrpcEndpoint?: string;
}) {
  return invoke<{ transaction_id: string; fee_sompi: string; public: WalletPublic }>("wallet_send", {
    profileId: args.profileId,
    reuseUnlocked: args.reuseUnlocked,
    password: args.password,
    sealed: args.sealed,
    public: args.public,
    destination: args.destination,
    amountSompi: args.amountSompi,
    feeSompi: args.feeSompi,
    options: { restEndpoint: args.restEndpoint || null, wrpcEndpoint: args.wrpcEndpoint || null },
  });
}

export async function consolidateKaspa(args: {
  profileId: string;
  reuseUnlocked: boolean;
  password: string;
  sealed: number[];
  public: WalletPublic;
  feeSompi: string;
  restEndpoint?: string;
  wrpcEndpoint?: string;
}) {
  return invoke<{ transaction_id: string; fee_sompi: string; public: WalletPublic }>(
    "wallet_consolidate",
    {
      profileId: args.profileId,
      reuseUnlocked: args.reuseUnlocked,
      password: args.password,
      sealed: args.sealed,
      public: args.public,
      feeSompi: args.feeSompi,
      options: { restEndpoint: args.restEndpoint || null, wrpcEndpoint: args.wrpcEndpoint || null },
    },
  );
}

export interface MailboxSendResult {
  transaction_id: string;
  fee_sompi: string;
  mailbox_output_sompi: string;
  public: WalletPublic;
  pending_handshake: boolean;
  pending_id?: string;
}

export async function sendMailboxMessage(args: {
  profileId: string;
  password: string;
  identityId: string;
  senderDisplayName: string;
  contactId: string;
  destination: string;
  body: string;
  messageId: string;
  stegoProfile: string;
  sealed: number[];
  public: WalletPublic;
  feeSompi?: string;
  wrpcEndpoint?: string;
  reuseChange?: boolean;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa mailbox send requires the native Ghost Talk app.");
  return invoke("mailbox_send_message", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    senderDisplayName: args.senderDisplayName,
    contactId: args.contactId,
    destination: args.destination,
    body: args.body,
    messageId: args.messageId,
    stegoProfile: args.stegoProfile,
    sealed: args.sealed,
    public: args.public,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
    reuseChange: args.reuseChange ?? false,
  });
}


export async function sendMailboxContactRequest(args: {
  profileId: string;
  password: string;
  identityId: string;
  senderDisplayName: string;
  sealed: number[];
  public: WalletPublic;
  destination: string;
  requestId: string;
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa private chat request requires the native Ghost Talk app.");
  return invoke("mailbox_send_contact_request", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    senderDisplayName: args.senderDisplayName,
    sealed: args.sealed,
    public: args.public,
    destination: args.destination,
    requestId: args.requestId,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function sendMailboxContactAccept(args: {
  profileId: string;
  password: string;
  identityId: string;
  senderDisplayName: string;
  sealed: number[];
  public: WalletPublic;
  signedRequestHex: string;
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa private chat acceptance requires the native Ghost Talk app.");
  return invoke("mailbox_send_contact_accept", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    senderDisplayName: args.senderDisplayName,
    sealed: args.sealed,
    public: args.public,
    signedRequestHex: args.signedRequestHex,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function sendMailboxSessionEnd(args: {
  profileId: string;
  password: string;
  identityId: string;
  contactId: string;
  destination: string;
  sealed: number[];
  public: WalletPublic;
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa session_end requires the native Ghost Talk app.");
  return invoke("mailbox_send_session_end", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    contactId: args.contactId,
    destination: args.destination,
    sealed: args.sealed,
    public: args.public,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function sendMailboxControl(args: {
  profileId: string;
  password: string;
  sealed: number[];
  public: WalletPublic;
  destination: string;
  payloadsHex: string[];
  completesPendingId?: string;
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa mailbox control send requires the native Ghost Talk app.");
  return invoke("mailbox_send_control", {
    profileId: args.profileId,
    password: args.password,
    sealed: args.sealed,
    public: args.public,
    destination: args.destination,
    payloadsHex: args.payloadsHex,
    completesPendingId: args.completesPendingId || null,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function retryMailboxHandshakeFinish(args: {
  profileId: string;
  password: string;
  identityId: string;
  contactId: string;
  messageId: string;
  sealed: number[];
  public: WalletPublic;
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa mailbox FINISH retry requires the native Ghost Talk app.");
  return invoke("mailbox_retry_handshake_finish", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    contactId: args.contactId,
    messageId: args.messageId,
    sealed: args.sealed,
    public: args.public,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function sendMailboxRecoveryOffer(args: {
  profileId: string;
  password: string;
  identityId: string;
  senderDisplayName: string;
  sealed: number[];
  public: WalletPublic;
  destination: string;
  offerHex: string;
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa mailbox recovery requires the native Ghost Talk app.");
  return invoke("mailbox_send_recovery_offer", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    senderDisplayName: args.senderDisplayName,
    sealed: args.sealed,
    public: args.public,
    destination: args.destination,
    offerHex: args.offerHex,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function sendMailboxDeliveryAck(args: {
  profileId: string;
  password: string;
  identityId: string;
  sealed: number[];
  public: WalletPublic;
  destination: string;
  destinationHydraId: string;
  messageId: string;
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<MailboxSendResult> {
  if (!isTauri) throw new Error("Kaspa mailbox delivery acknowledgement requires the native Ghost Talk app.");
  return invoke("mailbox_send_delivery_ack", {
    profileId: args.profileId,
    password: args.password,
    identityId: args.identityId,
    sealed: args.sealed,
    public: args.public,
    destination: args.destination,
    destinationHydraId: args.destinationHydraId,
    messageId: args.messageId,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function publishProfileBackup(args: {
  password: string;
  sealed: number[];
  public: WalletPublic;
  contacts: ProfileBackupContact[];
  messages: ProfileBackupMessage[];
  feeSompi?: string;
  wrpcEndpoint?: string;
}): Promise<{ transaction_ids: string[]; content_hash: string; public: WalletPublic }> {
  if (!isTauri) throw new Error("Kaspa profile backup requires the native Ghost Talk app.");
  return invoke("profile_backup_publish", {
    password: args.password,
    sealed: args.sealed,
    public: args.public,
    contacts: args.contacts,
    messages: args.messages,
    feeSompi: args.feeSompi ?? "0",
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function restoreProfileBackup(args: {
  password: string;
  sealed: number[];
  public: WalletPublic;
  restEndpoint?: string;
}): Promise<ProfileBackupRestoreResult | null> {
  if (!isTauri) throw new Error("Kaspa profile restore requires the native Ghost Talk app.");
  return invoke("profile_backup_restore", {
    password: args.password,
    sealed: args.sealed,
    public: args.public,
    restEndpoint: args.restEndpoint || null,
  });
}

export async function qrSvg(text: string): Promise<string> {
  if (!isTauri) return "";
  const response = await invoke<{ svg: string }>("qr_svg", { text });
  return response.svg;
}

export interface BroadcastResult {
  transaction_id: string;
  fee_sompi: string;
  public: WalletPublic;
  timings?: {
    utxo_plan_ms: number;
    signed_analysis_ms: number;
    submit_ms: number;
    total_ms: number;
  };
}


export interface KasSignerWatchOnlyAccount {
  account_fingerprint: string;
  public: WalletPublic;
}

export async function importKasSignerKpub(args: {
  kpub: string;
  network: string;
}): Promise<KasSignerWatchOnlyAccount> {
  if (!isTauri) throw new Error("KasSigner watch-only import requires the native Ghost Talk app.");
  return invoke("kassigner_import_kpub", { kpub: args.kpub, network: args.network });
}


export async function scanKasSignerAccountQr(args: {
  width: number;
  height: number;
  luminanceBase64: string;
}): Promise<{ account_payload: string } | null> {
  if (!isTauri) throw new Error("KasSigner account QR scanning requires the native Ghost Talk app.");
  return invoke("kassigner_scan_account_qr", args);
}

export interface KasSignerIdentityProofPrompt {
  proof_id: string;
  challenge: string;
  account_fingerprint: string;
  expires_at_ms: number;
}

export async function beginKasSignerIdentityProof(args: {
  kpub: string;
  network: string;
  profileId: string;
  hydraIdentityId: string;
}): Promise<KasSignerIdentityProofPrompt> {
  if (!isTauri) throw new Error("KasSigner identity proof requires the native Ghost Talk app.");
  return invoke("kassigner_begin_identity_proof", {
    kpub: args.kpub,
    network: args.network,
    profileId: args.profileId,
    hydraIdentityId: args.hydraIdentityId,
  });
}

export async function scanKasSignerIdentityProofQr(args: {
  proofId: string;
  width: number;
  height: number;
  luminanceBase64: string;
}): Promise<{ response_hex: string } | null> {
  if (!isTauri) throw new Error("KasSigner identity-proof scanning requires the native Ghost Talk app.");
  return invoke("kassigner_scan_identity_proof_qr", args);
}

export async function completeKasSignerIdentityProof(
  proofId: string,
  responseHex: string,
): Promise<KasSignerIdentityOwnershipProof> {
  if (!isTauri) throw new Error("KasSigner identity proof requires the native Ghost Talk app.");
  return invoke("kassigner_complete_identity_proof", { proofId, responseHex });
}

export async function cancelKasSignerIdentityProof(proofId: string): Promise<void> {
  if (!isTauri) return;
  await invoke("kassigner_cancel_identity_proof", { proofId });
}

export interface KasSignerQrFrame {
  index: number;
  total: number;
  payload_hex: string;
}

export interface KasSignerSigningPrompt {
  request_id: string;
  sdk_version: string;
  qr_frames: KasSignerQrFrame[];
  max_inputs: number;
  note: string;
}

export interface KasSignerScanProgress {
  received: number;
  total: number;
  bits: boolean[];
  response_hex?: string;
}

export interface KasSignerCompleteResult {
  status: "resign" | "broadcast";
  resign?: KasSignerSigningPrompt;
  broadcast?: BroadcastResult;
  message: string;
}

export async function qrSvgHex(payloadHex: string): Promise<string> {
  if (!isTauri) return "";
  const response = await invoke<{ svg: string }>("qr_svg_hex", { payloadHex });
  return response.svg;
}

export async function prepareKasSignerSend(args: {
  public: WalletPublic;
  profileId?: string;
  password?: string;
  sealed?: number[];
  accountFingerprint?: string;
  destination: string;
  amountSompi: string;
  feeSompi: string;
  wrpcEndpoint?: string;
}): Promise<KasSignerSigningPrompt> {
  if (!isTauri) throw new Error("KasSigner requires the native Ghost Talk app.");
  return invoke("kassigner_prepare_send", {
    public: args.public,
    profileId: args.profileId || null,
    password: args.password || null,
    sealed: args.sealed || null,
    accountFingerprint: args.accountFingerprint || null,
    destination: args.destination,
    amountSompi: args.amountSompi,
    feeSompi: args.feeSompi,
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function prepareKasSignerConsolidation(args: {
  public: WalletPublic;
  profileId?: string;
  password?: string;
  sealed?: number[];
  accountFingerprint?: string;
  feeSompi: string;
  wrpcEndpoint?: string;
}): Promise<KasSignerSigningPrompt> {
  if (!isTauri) throw new Error("KasSigner requires the native Ghost Talk app.");
  return invoke("kassigner_prepare_consolidation", {
    public: args.public,
    profileId: args.profileId || null,
    password: args.password || null,
    sealed: args.sealed || null,
    accountFingerprint: args.accountFingerprint || null,
    feeSompi: args.feeSompi,
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function scanKasSignerResponseFrame(args: {
  requestId: string;
  width: number;
  height: number;
  luminanceBase64: string;
}): Promise<KasSignerScanProgress> {
  if (!isTauri) throw new Error("KasSigner scanning requires the native Ghost Talk app.");
  return invoke("kassigner_scan_response_frame", args);
}

export async function completeKasSigner(
  requestId: string,
  responseHex: string,
): Promise<KasSignerCompleteResult> {
  if (!isTauri) throw new Error("KasSigner completion requires the native Ghost Talk app.");
  return invoke("kassigner_complete", { requestId, responseHex });
}

export async function cancelKasSigner(requestId: string): Promise<void> {
  if (!isTauri) return;
  await invoke("kassigner_cancel", { requestId });
}

export async function startWalletMonitor(args: {
  profileId: string;
  public: WalletPublic;
  checkpoint: string;
  directoryCheckpoint?: string;
  restEndpoint?: string;
  wrpcEndpoint?: string;
}): Promise<void> {
  if (!isTauri) return;
  await invoke("wallet_monitor_start", {
    profileId: args.profileId,
    public: args.public,
    checkpoint: args.checkpoint,
    directoryCheckpoint: args.directoryCheckpoint || null,
    restEndpoint: args.restEndpoint || null,
    wrpcEndpoint: args.wrpcEndpoint || null,
  });
}

export async function updateWalletMonitorPublic(profileId: string, publicState: WalletPublic): Promise<void> {
  if (!isTauri) return;
  await invoke("wallet_monitor_update_public", { profileId, public: publicState });
}

export async function stopWalletMonitor(profileId: string): Promise<void> {
  if (!isTauri) return;
  await invoke("wallet_monitor_stop", { profileId });
}

export async function onWalletLive(handler: (event: WalletLiveEvent) => void): Promise<UnlistenFn> {
  if (!isTauri) return () => undefined;
  return listen<WalletLiveEvent>("ghost://wallet-live", event => handler(event.payload));
}

export async function onDirectoryLive(handler: (event: DirectoryLiveEvent) => void): Promise<UnlistenFn> {
  if (!isTauri) return () => undefined;
  return listen<DirectoryLiveEvent>("ghost://directory-live", event => handler(event.payload));
}

export async function onNetworkStatus(handler: (event: NetworkStatusEvent) => void): Promise<UnlistenFn> {
  if (!isTauri) return () => undefined;
  return listen<NetworkStatusEvent>("ghost://network-status", event => handler(event.payload));
}


export interface DebugLogEntry {
  sequence: number;
  timestamp_ms: number;
  level: string;
  category: string;
  event: string;
  details: string;
}

export interface DebugLogSnapshot {
  enabled: boolean;
  latest_sequence: number;
  entries: DebugLogEntry[];
}

export interface HydraDebugSession {
  peer_hydra_id: string;
  sid: string;
  role: string;
  state: string;
  hydra_status: string;
  mailbox_id: string;
  send_seq: number;
  recv_next_seq: number;
  peer_kaspa_address?: string;
}

export interface HydraDebugState {
  available: boolean;
  identity_id?: string;
  sessions: HydraDebugSession[];
  pending_outbound?: string;
  prepared_completion?: string;
  pending_inbound?: string;
  pending_recovery?: string;
  prepared_recovery_finish?: string;
  retained_delivery_count: number;
  retired_sid_count: number;
}

export async function setNativeDebugLogging(enabled: boolean): Promise<DebugLogSnapshot> {
  if (!isTauri) return { enabled, latest_sequence: 0, entries: [] };
  return invoke("debug_log_set_enabled", { enabled });
}

export async function recordNativeDebugLog(
  level: string,
  category: string,
  event: string,
  details: string,
): Promise<void> {
  if (!isTauri) return;
  await invoke("debug_log_record", { level, category, event, details });
}

export async function getNativeDebugLogSnapshot(sinceSequence?: number): Promise<DebugLogSnapshot> {
  if (!isTauri) return { enabled: false, latest_sequence: 0, entries: [] };
  return invoke("debug_log_snapshot", { sinceSequence: sinceSequence ?? null });
}

export async function clearNativeDebugLog(): Promise<void> {
  if (!isTauri) return;
  await invoke("debug_log_clear");
}

export async function getHydraDebugState(profileId: string): Promise<HydraDebugState> {
  if (!isTauri) {
    return {
      available: false,
      sessions: [],
      retained_delivery_count: 0,
      retired_sid_count: 0,
    };
  }
  return invoke("hydra_debug_state", { profileId });
}

const fallbackPresets: DerivationPreset[] = [
  {
    label: "Kaspa Standard (CLI / Kaspium / KasWare / OneKey / Tangem)",
    path: "m/44'/111111'/0'",
    supported: true,
    note: "BIP44 account root; receive /0/index and change /1/index.",
  },
  { label: "Kaspa account #1", path: "m/44'/111111'/1'", supported: true, note: "Second account." },
  { label: "Custom account path", path: null, supported: true, note: "Custom BIP32 account root." },
];
