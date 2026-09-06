export type Tab = "Chats" | "Contacts" | "Discover" | "Rooms" | "Games" | "Kaspa" | "Settings";
export type NetworkConnectionStatus = "connected" | "disconnected" | "connecting" | "reconnecting";
export type Route = "Auto" | "Kaspa only";
export type StegoProfile = "Off" | "Deterministic" | "Fast Unicode" | "Fast Hybrid" | "Arithmetic";

export interface Contact {
  id: string;
  label: string;
  kaspaAddress: string;
  knsName?: string;
  /** Internal authenticated HYDRA peer handle resolved from a signed descriptor/bootstrap. Never user-entered. */
  hydraHandle?: string;
  /** True only when the peer's latest self-published discoverable descriptor verifies. */
  verifiedPublic?: boolean;
  publicUsername?: string;
}

export interface Message {
  id: string;
  direction: "in" | "out" | "system";
  body: string;
  createdAt: number;
  txid?: string;
  /** Stable 128-bit logical message id used for dedupe and exact failed-submit retry. */
  wireId?: string;
  /** Exact KKTP conversation SID that authenticated this message, when known. */
  sessionSid?: string;
  pending?: boolean;
  /** In-memory HYDRA first-contact handshake correlation. */
  pendingId?: string;
  /** Retry only after a recovery handshake established a replacement live ratchet session. */
  retryAfter?: number;
  /** Number of automatic carrier retries already attempted for this logical message. */
  retryCount?: number;
  /** Correlates a private address-only contact request before the peer shares HYDRA public material. */
  contactRequestId?: string;
  pendingStage?: "request" | "handshake" | "finish" | "delivery";
  /** Immediate local delivery lifecycle. A successful Kaspa SubmitTransaction is Delivered. */
  // "sent" is reserved for bootstrap/control progress before this logical message itself is broadcast.
  sendState?: "sending" | "sent" | "delivered" | "failed";
  sendError?: string;
}

export interface ChatThread {
  id: string;
  contactId?: string;
  label: string;
  messages: Message[];
  /** Direct-chat routing may exist without a saved contact. */
  peerKaspaAddress?: string;
  peerKnsName?: string;
  peerHydraHandle?: string;
  verifiedPublic?: boolean;
  /** User-level consent/bootstrap completed for this peer; public discoverability alone never implies consent. */
  bootstrapComplete?: boolean;
  /** Persisted KKTP session id for this exact logical conversation. */
  sessionSid?: string;
  /** Local-only organization; on-chain history is immutable. */
  archived?: boolean;
  /** Local session exit. A manual restart/reaccept is required before sending again. */
  left?: boolean;
  /** Authenticated on-chain session_end received from the peer for this exact SID. */
  peerLeft?: boolean;
  incomingRequest?: {
    requestId: string;
    peerHydraId: string;
    peerAddress: string;
    localAddress: string;
    /** Exact signed GTCR public bootstrap evidence, re-verified natively on Accept. */
    signedRequestHex: string;
    state: "pending" | "accepted" | "ignored";
  };
}

export interface WalletPublic {
  network: string;
  account_path: string;
  kpub?: string;
  watch_only?: boolean;
  receive_addresses: string[];
  change_addresses: string[];
  next_receive_index: number;
  next_change_index: number;
}


export interface KasSignerIdentityOwnershipProof {
  version: number;
  account_fingerprint: string;
  network: string;
  hydra_identity_id: string;
  challenge: string;
  signature_hex: string;
  message_hash_hex: string;
  verified_at_ms: number;
}

export interface KasSignerWalletRecord {
  accountFingerprint: string;
  public: WalletPublic;
  /** Hardware Sign Message proof that the imported public account was controlled at ID creation. */
  ownershipProof?: KasSignerIdentityOwnershipProof;
  restEndpoint?: string;
  wrpcEndpoint?: string;
}

export interface MailboxFragmentBucket {
  count: number;
  parts: Record<string, string>;
  txids: Record<string, string>;
  blockTime?: number;
}


export interface PendingDeliveryAck {
  purpose: "handshake";
  destination: string;
  destinationHydraId: string;
  createdAt: number;
}

export interface WalletRecord {
  sealed: number[];
  public: WalletPublic;
  mailboxCheckpoint: string;
  /** Scanner generation; missing/old values force one full address-history reconciliation. */
  mailboxScannerVersion?: number;
  /** Separate best-effort global GTCD discovery cursor; mailbox delivery never depends on it. */
  directoryCheckpoint?: string;
  /** Bounded accepted transaction ids used to make overlapping address-history scans replay-safe. */
  mailboxSeenTxids?: string[];
  mailboxPending?: Record<string, MailboxFragmentBucket>;
  deliveryAcksPending?: Record<string, PendingDeliveryAck>;
  restEndpoint?: string;
  wrpcEndpoint?: string;
  profileBackupHash?: string;
}

export interface ProfileSettings {
  route: Route;
  stego: StegoProfile;
  contactsBackupKaspa: boolean;
  backupMessagesKaspa: boolean;
  requireUnlockPassword: boolean;
  requireSendPassword: boolean;
  autoIgnoreUnknownChats: boolean;
  debugLogging: boolean;
  publicUsername: string;
  publicDescription: string;
  publicInterests: string;
}

export interface Profile {
  id: string;
  label: string;
  autoLogin: boolean;
  hydraIdentityId?: string;
  recoveryBackupConfirmed: boolean;
  /** Monotonic local/native conflict marker for wallet prompt policy changes. */
  securityPolicyRevision?: number;
  contacts: Contact[];
  chats: ChatThread[];
  /** Bounded local cache of latest verified public profiles observed on-chain. */
  publicDirectory?: PublicGhostProfile[];
  wallet?: WalletRecord;
  /** Optional watch-only financial account imported from KasSigner. Ghost Talk never stores its private key. */
  kasSignerWallet?: KasSignerWalletRecord;
  settings: ProfileSettings;
}

export interface PublicGhostProfile {
  kaspa_address: string;
  display_name: string;
  hydra_identity_id: string;
  descriptor_blue_score: string;
  kns_name?: string;
  username: string;
  description: string;
  interests: string[];
  verified: boolean;
  /** False is an authenticated latest opt-out record and removes the address from Discover. */
  discoverable?: boolean;
}

export interface NetworkStatusEvent {
  profile_id: string;
  status: NetworkConnectionStatus;
  reconnect_attempts: number;
}

export interface WalletHistoryEntry {
  transaction_id: string;
  blue_score: string;
  block_time?: number;
  ghost_payload: boolean;
}

export interface WalletRecovery {
  mnemonic: string;
  passphrase: string;
  account_path: string;
  network: string;
}

export interface WalletSnapshot {
  balance_sompi: string;
  utxo_count: string;
  blue_score: string;
  history: WalletHistoryEntry[];
  active_addresses: number;
  recommended_receive_index: number;
}

export interface MailboxEvent {
  transaction_id: string;
  blue_score: string;
  payload_hex: string;
  block_time?: number;
}

export interface WalletLiveEvent {
  profile_id: string;
  snapshot?: WalletSnapshot;
  checkpoint: string;
  mailbox: MailboxEvent[];
}

export interface DirectoryLiveEvent {
  profile_id: string;
  directory_checkpoint: string;
  public_profiles: PublicGhostProfile[];
}

export interface DerivationPreset {
  label: string;
  path: string | null;
  supported: boolean;
  note: string;
}

export interface ProfileBackupContact {
  id: string;
  label: string;
  kaspa_address: string;
  hydra_handle?: string;
}

export interface ProfileBackupMessage {
  chat_id: string;
  contact_id?: string;
  chat_label: string;
  id: string;
  direction: "in" | "out";
  body: string;
  created_at: number;
}

export interface ProfileBackupArchive {
  version: number;
  saved_at_ms: number;
  contacts: ProfileBackupContact[];
  messages: ProfileBackupMessage[];
}

export interface ProfileBackupRestoreResult {
  content_hash: string;
  archive: ProfileBackupArchive;
}
