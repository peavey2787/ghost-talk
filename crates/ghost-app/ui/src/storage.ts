import type { KasSignerIdentityOwnershipProof, KasSignerWalletRecord, MailboxFragmentBucket, PendingDeliveryAck, Profile, WalletRecord } from "./model";

const PROFILE_KEY = "ghost-talk/profiles/v2";
const MAX_PENDING_PACKETS = 128;
const MAX_FRAGMENTS = 128;
const MAX_PENDING_ACKS = 128;
const MAX_SEEN_TXIDS = 2048;
const MAX_PUBLIC_DIRECTORY = 100;

export function parseProfiles(raw: string | null | undefined): Profile[] {
  try {
    const parsed = JSON.parse(raw ?? "[]") as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isProfile).map(normalizeProfile);
  } catch {
    return [];
  }
}

export function loadProfiles(): Profile[] {
  return parseProfiles(localStorage.getItem(PROFILE_KEY));
}

export function saveProfiles(profiles: Profile[]): void {
  localStorage.setItem(PROFILE_KEY, JSON.stringify(profiles));
}

export function serializeProfiles(profiles: Profile[]): string {
  return JSON.stringify(profiles);
}

export function newProfile(label: string): Profile {
  return {
    id: randomId(),
    label: label.trim() || "User",
    autoLogin: false,
    recoveryBackupConfirmed: false,
    securityPolicyRevision: 0,
    contacts: [],
    chats: [],
    publicDirectory: [],
    settings: {
      route: "Auto",
      stego: "Off",
      contactsBackupKaspa: true,
      backupMessagesKaspa: false,
      requireUnlockPassword: true,
      requireSendPassword: false,
      autoIgnoreUnknownChats: false,
      debugLogging: false,
      publicUsername: "",
      publicDescription: "",
      publicInterests: "",
    },
  };
}

export function enforceAutoLogin(profiles: Profile[]): Profile[] {
  if (profiles.length !== 1 || !profiles[0].recoveryBackupConfirmed) {
    return profiles.map(profile => ({ ...profile, autoLogin: false }));
  }
  return profiles;
}


function normalizeRoute(value: unknown): "Auto" | "Kaspa only" {
  // r74 exposed WebRTC/DCUtR as manual choices even though routing is a policy,
  // not an implementation detail. Preserve old profiles by migrating either
  // legacy direct-only value to Auto; Kaspa only remains an explicit opt-out.
  return value === "Kaspa only" ? "Kaspa only" : "Auto";
}

function normalizeProfile(profile: Profile): Profile {
  return {
    ...profile,
    autoLogin:
      Boolean(profile.autoLogin)
      && Boolean(profile.recoveryBackupConfirmed),
    recoveryBackupConfirmed: Boolean(profile.recoveryBackupConfirmed),
    securityPolicyRevision: normalizeRevision(profile.securityPolicyRevision),
    contacts: Array.isArray(profile.contacts) ? profile.contacts : [],
    chats: normalizeChats(Array.isArray(profile.chats) ? profile.chats : []),
    publicDirectory: normalizePublicDirectory((profile as Profile & { publicDirectory?: unknown }).publicDirectory),
    wallet: normalizeWallet(profile.wallet),
    kasSignerWallet: normalizeKasSignerWallet((profile as Profile & { kasSignerWallet?: unknown }).kasSignerWallet),
    settings: {
      route: normalizeRoute(profile.settings?.route),
      stego: profile.settings?.stego ?? "Off",
      contactsBackupKaspa: profile.settings?.contactsBackupKaspa !== false,
      backupMessagesKaspa: Boolean(profile.settings?.backupMessagesKaspa),
      requireUnlockPassword: profile.settings?.requireUnlockPassword !== false,
      requireSendPassword: Boolean(profile.settings?.requireSendPassword),
      autoIgnoreUnknownChats: Boolean(profile.settings?.autoIgnoreUnknownChats),
      debugLogging: Boolean(profile.settings?.debugLogging),
      publicUsername: typeof profile.settings?.publicUsername === "string" ? profile.settings.publicUsername : "",
      publicDescription: typeof profile.settings?.publicDescription === "string" ? profile.settings.publicDescription : "",
      publicInterests: typeof profile.settings?.publicInterests === "string" ? profile.settings.publicInterests : "",
    },
  };
}


function normalizeChats(chats: Profile["chats"]): Profile["chats"] {
  const output: Profile["chats"] = [];
  const requestIndex = new Map<string, number>();
  for (const rawChat of chats) {
    const chat: Profile["chats"][number] = {
      ...rawChat,
      messages: normalizeChatMessages(rawChat.messages),
      incomingRequest: rawChat.incomingRequest ? {
        requestId: rawChat.incomingRequest.requestId,
        peerHydraId: rawChat.incomingRequest.peerHydraId,
        peerAddress: rawChat.incomingRequest.peerAddress,
        localAddress: rawChat.incomingRequest.localAddress,
        signedRequestHex: rawChat.incomingRequest.signedRequestHex,
        state: rawChat.incomingRequest.state,
      } : undefined,
    };
    const requestId = chat.incomingRequest?.requestId;
    if (!requestId) {
      output.push(chat);
      continue;
    }
    const existingIndex = requestIndex.get(requestId);
    if (existingIndex === undefined) {
      requestIndex.set(requestId, output.length);
      output.push(chat);
      continue;
    }
    const existing = output[existingIndex];
    const rank = { pending: 0, ignored: 1, accepted: 2 } as const;
    const incomingRequest = rank[chat.incomingRequest!.state] > rank[existing.incomingRequest!.state]
      ? chat.incomingRequest
      : existing.incomingRequest;
    const messages = [...existing.messages];
    const seen = new Set(messages.map(message => message.id));
    for (const message of chat.messages) {
      if (!seen.has(message.id)) {
        messages.push(message);
        seen.add(message.id);
      }
    }
    output[existingIndex] = {
      ...existing,
      contactId: existing.contactId ?? chat.contactId,
      peerKaspaAddress: existing.peerKaspaAddress ?? chat.peerKaspaAddress,
      peerKnsName: existing.peerKnsName ?? chat.peerKnsName,
      peerHydraHandle: existing.peerHydraHandle ?? chat.peerHydraHandle,
      verifiedPublic: Boolean(existing.verifiedPublic || chat.verifiedPublic),
      bootstrapComplete: Boolean(existing.bootstrapComplete || chat.bootstrapComplete),
      archived: Boolean(existing.archived || chat.archived),
      left: Boolean(existing.left || chat.left),
      peerLeft: Boolean(existing.peerLeft || chat.peerLeft),
      sessionSid: existing.sessionSid ?? chat.sessionSid ?? requestId,
      incomingRequest,
      messages,
    };
  }
  return output;
}

function normalizeChatMessages(messages: Profile["chats"][number]["messages"]): Profile["chats"][number]["messages"] {
  return messages.map(message => {
    let normalized = message;
    const legacySuccessReplay = Boolean(
      normalized.pending
      && normalized.retryAfter !== undefined
      && !normalized.sendError
      && (normalized.pendingStage === "request" || normalized.pendingStage === "handshake" || normalized.pendingStage === "finish")
    );
    if (legacySuccessReplay) {
      // r43 scheduled timers even after Kaspa had accepted bootstrap carriers. r44
      // consumes the durable on-chain carrier locally instead of rebroadcasting it.
      normalized = { ...normalized, retryAfter: undefined, retryCount: undefined };
    }
    if (
      normalized.direction === "out"
      && normalized.txid
      && normalized.sendState === "sent"
      && (normalized.pendingStage === "finish" || normalized.pendingStage === "delivery")
    ) {
      // r73 and earlier kept an already-broadcast message in "Awaiting delivery"
      // until a peer ACK. Kaspa acceptance is the delivery boundary for Kaspa-only
      // transport; bootstrap ACKs may still activate the ratchet independently.
      normalized = {
        ...normalized,
        pending: false,
        pendingId: undefined,
        pendingStage: undefined,
        retryAfter: undefined,
        retryCount: undefined,
        sendState: "delivered",
        sendError: undefined,
      };
    }
    return normalized;
  });
}

export function reconcilePersistedHistory(primary: Profile[], cached: Profile[]): Profile[] {
  // Native app-data is the durable cross-build mirror, while WebView localStorage
  // is synchronous and can contain a newer chat snapshot if the process exits
  // before the queued native fsync completes. Never choose one wholesale: merge
  // chat/message history by stable IDs so either copy can repair the other.
  const cachedById = new Map(cached.map(profile => [profile.id, profile]));
  const merged = primary.map(profile => {
    const local = cachedById.get(profile.id);
    if (!local) return profile;
    cachedById.delete(profile.id);
    return {
      ...profile,
      chats: mergePersistedChats(profile.chats, local.chats),
    };
  });
  // A profile created immediately before shutdown may exist only in the
  // synchronous WebView cache. Preserve it instead of erasing it on next boot.
  merged.push(...cachedById.values());
  return merged;
}

function mergePersistedChats(primary: Profile["chats"], cached: Profile["chats"]): Profile["chats"] {
  const cachedRemaining = new Map(cached.map(chat => [chat.id, chat]));
  const cachedBySid = new Map(
    cached.filter(chat => chat.sessionSid).map(chat => [chat.sessionSid!, chat]),
  );
  const merged = primary.map(chat => {
    const local = cachedRemaining.get(chat.id) ?? (chat.sessionSid ? cachedBySid.get(chat.sessionSid) : undefined);
    if (!local) return chat;
    cachedRemaining.delete(local.id);
    if (local.sessionSid) cachedBySid.delete(local.sessionSid);
    const requestRank = { pending: 0, ignored: 1, accepted: 2 } as const;
    const incomingRequest = !chat.incomingRequest
      ? local.incomingRequest
      : !local.incomingRequest
        ? chat.incomingRequest
        : requestRank[local.incomingRequest.state] > requestRank[chat.incomingRequest.state]
          ? local.incomingRequest
          : chat.incomingRequest;
    return {
      ...local,
      ...chat,
      contactId: chat.contactId ?? local.contactId,
      label: chat.label || local.label,
      peerKaspaAddress: chat.peerKaspaAddress ?? local.peerKaspaAddress,
      peerKnsName: chat.peerKnsName ?? local.peerKnsName,
      peerHydraHandle: chat.peerHydraHandle ?? local.peerHydraHandle,
      verifiedPublic: Boolean(chat.verifiedPublic || local.verifiedPublic),
      bootstrapComplete: Boolean(chat.bootstrapComplete || local.bootstrapComplete),
      sessionSid: chat.sessionSid ?? local.sessionSid,
      archived: Boolean(chat.archived || local.archived),
      left: Boolean(chat.left || local.left),
      peerLeft: Boolean(chat.peerLeft || local.peerLeft),
      incomingRequest,
      messages: mergePersistedMessages(chat.messages, local.messages),
    };
  });
  // These are chats that the native mirror had not committed yet. There is no
  // destructive chat-delete feature, so retaining them is the fail-safe choice.
  merged.push(...cachedRemaining.values());
  return normalizeChats(merged);
}

function mergePersistedMessages(
  primary: Profile["chats"][number]["messages"],
  cached: Profile["chats"][number]["messages"],
): Profile["chats"][number]["messages"] {
  const key = (message: Profile["chats"][number]["messages"][number]) =>
    `${message.direction}:${message.wireId ?? message.id}`;
  const cachedByKey = new Map(cached.map(message => [key(message), message]));
  const merged = primary.map(message => {
    const local = cachedByKey.get(key(message));
    if (!local) return message;
    cachedByKey.delete(key(message));
    const stateRank = { sending: 0, sent: 1, failed: 2, delivered: 3 } as const;
    const primaryRank = message.sendState ? stateRank[message.sendState] : -1;
    const cachedRank = local.sendState ? stateRank[local.sendState] : -1;
    const preferredState = cachedRank > primaryRank ? local : message;
    return {
      ...local,
      ...message,
      wireId: message.wireId ?? local.wireId,
      sessionSid: message.sessionSid ?? local.sessionSid,
      txid: message.txid ?? local.txid,
      sendState: preferredState.sendState,
      sendError: preferredState.sendError,
      pending: preferredState.pending,
      pendingId: preferredState.pendingId,
      pendingStage: preferredState.pendingStage,
      retryAfter: preferredState.retryAfter,
      retryCount: preferredState.retryCount,
    };
  });
  merged.push(...cachedByKey.values());
  return merged.sort((left, right) => left.createdAt - right.createdAt);
}

export function reconcileSecurityPolicies(primary: Profile[], cached: Profile[]): Profile[] {
  const cachedById = new Map(cached.map(profile => [profile.id, profile]));
  return primary.map(profile => {
    const local = cachedById.get(profile.id);
    if (!local) return profile;
    const primaryRevision = normalizeRevision(profile.securityPolicyRevision);
    const cachedRevision = normalizeRevision(local.securityPolicyRevision);
    if (cachedRevision <= primaryRevision) return profile;
    return {
      ...profile,
      securityPolicyRevision: cachedRevision,
      settings: {
        ...profile.settings,
        requireUnlockPassword: local.settings.requireUnlockPassword,
        requireSendPassword: local.settings.requireSendPassword,
      },
    };
  });
}

function normalizeRevision(value: unknown): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : 0;
}

function normalizeKasSignerWallet(value: unknown): KasSignerWalletRecord | undefined {
  if (!value || typeof value !== "object") return undefined;
  const wallet = value as Partial<KasSignerWalletRecord>;
  if (typeof wallet.accountFingerprint !== "string" || !wallet.accountFingerprint || !isWalletPublic(wallet.public)) {
    return undefined;
  }
  if (!wallet.public.watch_only || typeof wallet.public.kpub !== "string" || !wallet.public.kpub) {
    return undefined;
  }
  return {
    accountFingerprint: wallet.accountFingerprint,
    public: wallet.public,
    ownershipProof: normalizeKasSignerOwnershipProof(wallet.ownershipProof),
    restEndpoint: optionalString(wallet.restEndpoint),
    wrpcEndpoint: optionalString(wallet.wrpcEndpoint),
  };
}

function normalizeKasSignerOwnershipProof(value: unknown): KasSignerIdentityOwnershipProof | undefined {
  if (!value || typeof value !== "object") return undefined;
  const proof = value as Partial<KasSignerIdentityOwnershipProof>;
  if (
    proof.version !== 1
    || typeof proof.account_fingerprint !== "string"
    || typeof proof.network !== "string"
    || typeof proof.hydra_identity_id !== "string"
    || typeof proof.challenge !== "string"
    || typeof proof.signature_hex !== "string"
    || typeof proof.message_hash_hex !== "string"
    || typeof proof.verified_at_ms !== "number"
    || !Number.isFinite(proof.verified_at_ms)
  ) return undefined;
  return proof as KasSignerIdentityOwnershipProof;
}

function normalizeWallet(value: unknown): WalletRecord | undefined {
  if (!value || typeof value !== "object") return undefined;
  const wallet = value as Partial<WalletRecord>;
  if (
    !Array.isArray(wallet.sealed)
    || !wallet.sealed.every(byte => Number.isInteger(byte) && byte >= 0 && byte <= 255)
    || !isWalletPublic(wallet.public)
  ) {
    return undefined;
  }
  return {
    sealed: wallet.sealed,
    public: wallet.public,
    mailboxCheckpoint:
      typeof wallet.mailboxCheckpoint === "string" ? wallet.mailboxCheckpoint : "0",
    mailboxScannerVersion: wallet.mailboxScannerVersion === 1 ? 1 : 0,
    directoryCheckpoint:
      typeof wallet.directoryCheckpoint === "string" ? wallet.directoryCheckpoint : "0",
    mailboxSeenTxids: normalizeSeenTxids(wallet.mailboxSeenTxids),
    mailboxPending: normalizePending(wallet.mailboxPending),
    deliveryAcksPending: normalizeDeliveryAcks(wallet.deliveryAcksPending),
    restEndpoint: optionalString(wallet.restEndpoint),
    wrpcEndpoint: optionalString(wallet.wrpcEndpoint),
    profileBackupHash: optionalString(wallet.profileBackupHash),
  };
}

function isWalletPublic(value: unknown): value is WalletRecord["public"] {
  if (!value || typeof value !== "object") return false;
  const wallet = value as Partial<WalletRecord["public"]>;
  return (
    typeof wallet.network === "string"
    && typeof wallet.account_path === "string"
    && Array.isArray(wallet.receive_addresses)
    && wallet.receive_addresses.every(address => typeof address === "string")
    && Array.isArray(wallet.change_addresses)
    && wallet.change_addresses.every(address => typeof address === "string")
    && validIndex(wallet.next_receive_index, wallet.receive_addresses.length)
    && validIndex(wallet.next_change_index, wallet.change_addresses.length)
  );
}

function normalizePending(value: unknown): Record<string, MailboxFragmentBucket> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const normalized: Record<string, MailboxFragmentBucket> = {};
  for (const [packetId, raw] of Object.entries(value).slice(-MAX_PENDING_PACKETS)) {
    if (!/^[0-9a-f]{32}$/i.test(packetId) || !raw || typeof raw !== "object") continue;
    const bucket = raw as Partial<MailboxFragmentBucket>;
    if (!Number.isInteger(bucket.count) || bucket.count! < 1 || bucket.count! > MAX_FRAGMENTS) {
      continue;
    }
    const parts = stringRecord(bucket.parts, true);
    const txids = stringRecord(bucket.txids, false);
    if (!parts || !txids) continue;
    const blockTime =
      typeof bucket.blockTime === "number" && Number.isFinite(bucket.blockTime)
        ? bucket.blockTime
        : undefined;
    normalized[packetId.toLowerCase()] = { count: bucket.count!, parts, txids, blockTime };
  }
  return Object.keys(normalized).length ? normalized : undefined;
}

function normalizeSeenTxids(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const seen: string[] = [];
  const unique = new Set<string>();
  for (const item of value.slice(-MAX_SEEN_TXIDS)) {
    if (typeof item !== "string" || !/^[0-9a-f]{64}$/i.test(item)) continue;
    const normalized = item.toLowerCase();
    if (unique.has(normalized)) continue;
    unique.add(normalized);
    seen.push(normalized);
  }
  return seen.length ? seen : undefined;
}

function normalizePublicDirectory(value: unknown): Profile["publicDirectory"] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(item => Boolean(item) && typeof item === "object")
    .filter(item => {
      const profile = item as Record<string, unknown>;
      return typeof profile.kaspa_address === "string"
        && typeof profile.hydra_identity_id === "string"
        && typeof profile.descriptor_blue_score === "string"
        && profile.verified === true;
    })
    .slice(0, MAX_PUBLIC_DIRECTORY) as Profile["publicDirectory"];
}

function normalizeDeliveryAcks(value: unknown): Record<string, PendingDeliveryAck> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const normalized: Record<string, PendingDeliveryAck> = {};
  for (const [messageId, raw] of Object.entries(value).slice(-MAX_PENDING_ACKS)) {
    if (!/^[0-9a-f]{32}$/i.test(messageId) || !raw || typeof raw !== "object") continue;
    const ack = raw as Partial<PendingDeliveryAck>;
    if (
      ack.purpose !== "handshake"
      || typeof ack.destination !== "string"
      || !ack.destination.trim()
      || typeof ack.destinationHydraId !== "string"
      || !/^[0-9a-f]{64}$/i.test(ack.destinationHydraId)
      || typeof ack.createdAt !== "number"
      || !Number.isFinite(ack.createdAt)
    ) {
      continue;
    }
    normalized[messageId.toLowerCase()] = {
      purpose: "handshake",
      destination: ack.destination,
      destinationHydraId: ack.destinationHydraId.toLowerCase(),
      createdAt: ack.createdAt,
    };
  }
  return Object.keys(normalized).length ? normalized : undefined;
}

function stringRecord(value: unknown, requireHex: boolean): Record<string, string> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const entries = Object.entries(value);
  if (entries.length > MAX_FRAGMENTS) return undefined;
  const output: Record<string, string> = {};
  for (const [key, item] of entries) {
    if (!/^\d{1,3}$/.test(key) || typeof item !== "string") return undefined;
    if (requireHex && (!/^[0-9a-f]*$/i.test(item) || item.length % 2 !== 0)) return undefined;
    output[key] = item;
  }
  return output;
}

function validIndex(value: unknown, length: number): value is number {
  return Number.isInteger(value) && Number(value) >= 0 && Number(value) < length;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function isProfile(value: unknown): value is Profile {
  if (!value || typeof value !== "object") return false;
  const profile = value as Partial<Profile>;
  return typeof profile.id === "string" && typeof profile.label === "string";
}

function randomId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
}
