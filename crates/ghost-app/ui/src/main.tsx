import React, { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";
import { ChatView } from "./components/ChatView";
import { ContactsView, DiscoverView, GamesView, RoomsView, SettingsView } from "./components/SimpleViews";
import { IdentityGate } from "./components/IdentityGate";
import { DebugLogWindow } from "./components/DebugLogWindow";
import { KaspaWallet } from "./components/KaspaWallet";
import { Sidebar } from "./components/Sidebar";
import { absorbMailboxEvents, readyEnvelopes, removeEnvelope } from "./mailbox";
import type {
  Contact,
  MailboxEvent,
  Message,
  NetworkConnectionStatus,
  Profile,
  PublicGhostProfile,
  Tab,
  WalletRecord,
  WalletSnapshot,
} from "./model";
import {
  ensureHydra,
  initializeHydraFromWallet,
  isTauri,
  loadNativeProfileState,
  loadRememberedUnlock,
  leaveHydraPeer,
  lockHydraProfile,
  lockWallet,
  lookupGhostProfile,
  onDirectoryLive,
  onNetworkStatus,
  onWalletLive,
  publishGhostDescriptor,
  publishProfileBackup,
  previewHydraContactRequest,
  receiveHydraMailbox,
  openHydraDirect,
  registerHydraPeerRoutes,
  sealHydraDirect,
  rejoinHydraPeer,
  retryMailboxHandshakeFinish,
  refreshWallet,
  resolveGhostPeer,
  saveNativeProfileState,
  sendMailboxContactAccept,
  sendMailboxContactRequest,
  sendMailboxControl,
  sendMailboxDeliveryAck,
  sendMailboxMessage,
  sendMailboxRecoveryOffer,
  sendMailboxSessionEnd,
  setRememberedUnlock,
  startWalletMonitor,
  stopWalletMonitor,
  updateWalletMonitorPublic,
  unlockWallet,
} from "./native";
import {
  enforceAutoLogin,
  loadProfiles,
  parseProfiles,
  reconcilePersistedHistory,
  reconcileSecurityPolicies,
  saveProfiles,
  serializeProfiles,
} from "./storage";
import { backupContacts, backupMessages, profileBackupFingerprint } from "./profileBackup";
import { decodeLiveVoicePacket, decodeVoiceMessage } from "./voice";
import { decodeDirectRouteSignal, type RealtimeControlEnvelope } from "./webrtc";
import { configureProtocolDebug, protocolDebug } from "./debug";

interface IncomingNotice {
  text: string;
  chatId?: string;
}

function App() {
  const [profiles, setProfiles] = useState<Profile[]>(() => enforceAutoLogin(loadProfiles()));
  const profilesRef = useRef(profiles);
  const nativeSaveChain = useRef<Promise<void>>(Promise.resolve());
  const mailboxApplyChain = useRef<Promise<void>>(Promise.resolve());
  const [profilesReady, setProfilesReady] = useState(!isTauri);
  const [activeId, setActiveId] = useState<string | null>(() =>
    isTauri ? null : autoLoginProfileId(enforceAutoLogin(loadProfiles()))
  );
  const [tab, setTab] = useState<Tab>("Chats");
  const [mask, setMask] = useState(false);
  const [snapshot, setSnapshot] = useState<WalletSnapshot>();
  const [sessionPassword, setSessionPassword] = useState("");
  const pendingUnlockRef = useRef<{ profileId: string; password: string } | null>(null);
  const [autoUnlockSettledId, setAutoUnlockSettledId] = useState("");
  const [sessionUnlockError, setSessionUnlockError] = useState("");
  const [networkStatus, setNetworkStatus] = useState<NetworkConnectionStatus>("connecting");
  const [reconnectAttempts, setReconnectAttempts] = useState(0);
  const [incomingNotice, setIncomingNotice] = useState<IncomingNotice | null>(null);
  const [chatFocusId, setChatFocusId] = useState("");
  const [contactPrefillTarget, setContactPrefillTarget] = useState("");
  const [realtimeControls, setRealtimeControls] = useState<RealtimeControlEnvelope[]>([]);
  const [debugOpen, setDebugOpen] = useState(false);
  const active = profiles.find(profile => profile.id === activeId);

  useEffect(() => {
    const enabled = Boolean(active?.settings.debugLogging);
    void configureProtocolDebug(enabled).catch(error => {
      console.error("Ghost Talk failed to configure protocol debugging", error);
    });
    if (!enabled) setDebugOpen(false);
  }, [activeId, active?.settings.debugLogging]);

  const queueNativeProfileSave = (snapshot: Profile[]): Promise<void> => {
    const serialized = serializeProfiles(snapshot);
    const pending = nativeSaveChain.current
      .catch(() => undefined)
      .then(() => saveNativeProfileState(serialized));
    // Keep the sequencing chain usable after an I/O failure, while callers that
    // explicitly await durability still receive the original rejection.
    nativeSaveChain.current = pending.catch(error => {
      console.error("Ghost Talk native profile persistence failed", error);
    });
    return pending;
  };

  const commitProfiles = (update: (current: Profile[]) => Profile[]) => {
    const updated = update(profilesRef.current);
    profilesRef.current = updated;
    setProfiles(updated);
    if (!profilesReady) return;
    // localStorage is synchronous: commit the visible chat snapshot immediately
    // instead of waiting for a post-render effect that can be skipped by shutdown.
    saveProfiles(updated);
    void queueNativeProfileSave(updated).catch(() => undefined);
  };

  const queueRealtimeControl = (profileId: string, fromHydraId: string, sessionSid: string, body: string) => {
    const profile = profilesRef.current.find(candidate => candidate.id === profileId);
    if (!profile || !validWireId(sessionSid)) return;
    const chat = profile.chats.find(candidate =>
      candidate.sessionSid?.toLowerCase() === sessionSid.toLowerCase()
      && routeForChat(profile, candidate)?.hydraHandle === fromHydraId
    );
    if (!chat) {
      protocolDebug("warn", "voice", "realtime-control-wrong-session-discarded", {
        profileId,
        fromHydraId,
        sessionSid,
      });
      return;
    }
    const voice = decodeLiveVoicePacket(body);
    const direct = decodeDirectRouteSignal(body);
    if (!voice && !direct) return;
    const correlation = voice
      ? `${voice.callId}:${voice.kind}:${voice.sequence ?? "control"}`
      : `${direct!.negotiationId}:${direct!.kind}`;
    const envelope: RealtimeControlEnvelope = {
      id: `${correlation}:${newWireId()}`,
      chatId: chat.id,
      sessionSid,
      body,
      receivedAt: Date.now(),
    };
    setRealtimeControls(current => [...current.slice(-255), envelope]);
    if (voice?.kind === "request") {
      setIncomingNotice({ text: `Incoming voice call from ${chat.label}.`, chatId: chat.id });
    }
  };

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    const cached = enforceAutoLogin(loadProfiles());
    loadNativeProfileState()
      .then(async raw => {
        if (cancelled) return;
        const nativeProfiles = raw === null ? [] : parseProfiles(raw);
        let loaded = raw === null
          ? cached
          : enforceAutoLogin(
              reconcileSecurityPolicies(
                reconcilePersistedHistory(nativeProfiles, cached),
                cached,
              ),
            );
        const reconciled = await reconcileRememberedUnlockPolicies(loaded);
        loaded = enforceAutoLogin(reconciled.profiles);
        profilesRef.current = loaded;
        commitProfiles(() => loaded);
        saveProfiles(loaded);
        const nativeChanged = raw === null || serializeProfiles(loaded) !== serializeProfiles(nativeProfiles);
        if (nativeChanged || reconciled.changed) {
          await saveNativeProfileState(serializeProfiles(loaded));
        }
        if (!cancelled) {
          setActiveId(autoLoginProfileId(loaded));
          setProfilesReady(true);
        }
      })
      .catch(() => {
        if (cancelled) return;
        profilesRef.current = cached;
        commitProfiles(() => cached);
        setActiveId(autoLoginProfileId(cached));
        setProfilesReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const monitorKey = active?.wallet
    ? [
        active.id,
        active.wallet.public.network,
        active.wallet.public.account_path,
        // HD receive/change cursors are wallet bookkeeping, not monitor identity.
        // Mailbox routing uses the stable receive address and the monitor already
        // owns the full derived address set. Restarting on either cursor can tear
        // down the cached Portal in the middle of GTCR -> GTCA -> HYDRA bootstrap.
        active.wallet.restEndpoint ?? "",
        active.wallet.wrpcEndpoint ?? "",
      ].join("|")
    : "";

  useEffect(() => {
    if (!active?.wallet) {
      setSnapshot(undefined);
      setNetworkStatus("disconnected");
      setReconnectAttempts(0);
      return;
    }
    let cancelled = false;
    let unlistenWallet: () => void = () => {};
    let unlistenDirectory: () => void = () => {};
    let unlistenNetwork: () => void = () => {};
    const profileId = active.id;
    const wallet = active.wallet;
    setSnapshot(undefined);
    setNetworkStatus("connecting");
    setReconnectAttempts(0);

    void (async () => {
      // Attach every native event listener before starting either the wallet
      // monitor or the first live wallet RPC. Otherwise a fast Portal connect can
      // emit connecting -> connected before the WebView listener exists, leaving
      // the sidebar permanently blinking yellow even though the cached Portal is
      // healthy and transactions are broadcasting successfully.
      const listeners = await Promise.allSettled([
        onNetworkStatus(event => {
          if (event.profile_id !== profileId || cancelled) return;
          setNetworkStatus(event.status);
          setReconnectAttempts(event.reconnect_attempts);
        }),
        onWalletLive(event => {
          if (event.profile_id !== profileId || cancelled) return;
          if (event.snapshot) setSnapshot(event.snapshot);
          if (event.mailbox.length) {
            protocolDebug("info", "mailbox", "wallet-live-carriers", {
              profileId,
              checkpoint: event.checkpoint,
              carrierCount: event.mailbox.length,
              transactionIds: event.mailbox.map(item => item.transaction_id),
            });
          }
          // Native monitor events can arrive faster than HYDRA processing. Serialize
          // each profile's mailbox application so an older async event can never
          // overwrite a newer checkpoint/request/chat state.
          mailboxApplyChain.current = mailboxApplyChain.current
            .catch(() => undefined)
            .then(async () => {
              if (event.profile_id !== profileId || cancelled) return;
              const current = profilesRef.current.find(profile => profile.id === profileId);
              if (!current) return;
              const pendingBefore = pendingRequestCount(current);
              const incomingBefore = incomingMessageCount(current);
              const next = await applyMailbox(
                current,
                event.checkpoint,
                event.mailbox,
                event.snapshot?.recommended_receive_index ?? current.wallet?.public.next_receive_index ?? 0,
                sessionPassword,
                (from, sid, body) => queueRealtimeControl(profileId, from, sid, body),
              );
              if (cancelled) return;
              if (pendingRequestCount(next) > pendingBefore) {
                setIncomingNotice({
                  text: "New encrypted chat request received.",
                  chatId: findNewPendingRequestChatId(current, next),
                });
              } else if (incomingMessageCount(next) > incomingBefore) {
                setIncomingNotice({
                  text: "New encrypted Ghost Talk message received.",
                  chatId: findNewIncomingMessageChatId(current, next),
                });
              }
              commitProfiles(all => all.map(profile => (profile.id === profileId ? mergeAppliedMailboxProfile(profile, next) : profile)));
            });
        }),
        onDirectoryLive(event => {
          if (event.profile_id !== profileId || cancelled) return;
          commitProfiles(all => all.map(profile =>
            profile.id === profileId
              ? mergePublicDirectory(profile, event.public_profiles ?? [], event.directory_checkpoint)
              : profile
          ));
        }),
      ]);

      if (cancelled) {
        for (const result of listeners) {
          if (result.status === "fulfilled") result.value();
        }
        return;
      }

      const [networkListener, walletListener, directoryListener] = listeners;
      if (networkListener.status === "fulfilled") unlistenNetwork = networkListener.value;
      else protocolDebug("error", "kaspa", "network-listener-start-failed", { profileId, error: String(networkListener.reason) });
      if (walletListener.status === "fulfilled") unlistenWallet = walletListener.value;
      else protocolDebug("error", "mailbox", "wallet-listener-start-failed", { profileId, error: String(walletListener.reason) });
      if (directoryListener.status === "fulfilled") unlistenDirectory = directoryListener.value;
      else protocolDebug("error", "directory", "directory-listener-start-failed", { profileId, error: String(directoryListener.reason) });

      try {
        await startWalletMonitor({
          profileId,
          public: wallet.public,
          checkpoint: wallet.mailboxScannerVersion === 1 ? (wallet.mailboxCheckpoint || "0") : "0",
          directoryCheckpoint: wallet.directoryCheckpoint || "0",
          restEndpoint: wallet.restEndpoint,
          wrpcEndpoint: wallet.wrpcEndpoint,
        });
      } catch (error) {
        protocolDebug("error", "mailbox", "monitor-start-failed", { profileId, error: String(error) });
      }

      if (cancelled) return;
      try {
        const value = await refreshWallet(wallet.public, wallet.restEndpoint, wallet.wrpcEndpoint);
        if (cancelled) return;
        setSnapshot(value);
        // refreshWallet obtains current UTXOs/DAA through the same live Portal
        // gateway used for broadcasts. A successful refresh is therefore an
        // authoritative connected signal and repairs any UI event that may have
        // been lost during WebView startup/resume.
        setNetworkStatus("connected");
        setReconnectAttempts(0);
        commitProfiles(all => all.map(profile =>
          profile.id === profileId ? rotateReceiveAddress(profile, value.recommended_receive_index) : profile
        ));
      } catch {
        if (!cancelled) setSnapshot(undefined);
      }
    })();

    return () => {
      cancelled = true;
      unlistenWallet();
      unlistenDirectory();
      unlistenNetwork();
      stopWalletMonitor(profileId).catch(() => undefined);
    };
  }, [activeId, monitorKey, sessionPassword]);

  useEffect(() => {
    if (!active?.wallet) return;
    // Keep the long-lived native monitor on the latest HD receive/change cursors
    // without restarting it or dropping the cached wRPC Portal. This closes the
    // r37/r38 gap where a fresh GTCR could target a newly advertised address while
    // the monitor continued directly probing an older receive index.
    void updateWalletMonitorPublic(active.id, active.wallet.public).catch(error => {
      console.error("Ghost Talk failed to update mailbox monitor address state", error);
    });
  }, [activeId, active?.wallet?.public.next_receive_index, active?.wallet?.public.next_change_index]);

  useEffect(() => {
    if (!active || !sessionPassword || !active.wallet || !active.hydraIdentityId) return;
    let cancelled = false;
    let queued = false;
    const profileId = active.id;

    // Every mailbox application path shares mailboxApplyChain. A prior r43 path
    // drained already-observed envelopes outside that chain, so the same signed
    // Discovery could be applied twice from the same pre-request snapshot and
    // create two UI threads. Keep one local drain instead of rebroadcasting
    // already-confirmed Kaspa bootstrap transactions.
    const drainObserved = () => {
      if (cancelled || queued) return;
      const current = profilesRef.current.find(profile => profile.id === profileId);
      if (!current?.wallet || !current.hydraIdentityId || readyEnvelopes(current.wallet).length === 0) return;
      queued = true;
      mailboxApplyChain.current = mailboxApplyChain.current
        .catch(() => undefined)
        .then(async () => {
          if (cancelled) return;
          const latest = profilesRef.current.find(profile => profile.id === profileId);
          if (!latest?.wallet || !latest.hydraIdentityId || readyEnvelopes(latest.wallet).length === 0) return;
          const pendingBefore = pendingRequestCount(latest);
          const incomingBefore = incomingMessageCount(latest);
          const next = await applyMailbox(
            latest,
            latest.wallet.mailboxCheckpoint,
            [],
            latest.wallet.public.next_receive_index,
            sessionPassword,
            (from, sid, body) => queueRealtimeControl(profileId, from, sid, body),
          );
          if (cancelled) return;
          if (pendingRequestCount(next) > pendingBefore) {
            setIncomingNotice({
              text: "New encrypted chat request received.",
              chatId: findNewPendingRequestChatId(latest, next),
            });
          } else if (incomingMessageCount(next) > incomingBefore) {
            setIncomingNotice({
              text: "New encrypted Ghost Talk message received.",
              chatId: findNewIncomingMessageChatId(latest, next),
            });
          }
          commitProfiles(all => all.map(profile => (
            profile.id === profileId ? mergeAppliedMailboxProfile(profile, next) : profile
          )));
        })
        .finally(() => {
          queued = false;
        });
    };

    drainObserved();
    const timer = window.setInterval(drainObserved, 500);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [activeId, sessionPassword]);

  useEffect(() => {
    if (!active?.wallet || sessionPassword) return;
    const pending = pendingUnlockRef.current?.profileId === active.id
      ? pendingUnlockRef.current
      : null;
    if (!pending && active.settings.requireUnlockPassword) return;
    if (!pending && autoUnlockSettledId === active.id) return;

    let cancelled = false;
    const profileId = active.id;
    void (async () => {
      try {
        const password = pending?.password ?? await loadRememberedUnlock(profileId);
        if (!password) {
          throw new Error(
            "Automatic unlock is enabled, but this device has no remembered unlock credential. Enter the ID password once to restore automatic unlock.",
          );
        }
        await unlockProfile(password);
      } catch (error) {
        if (!cancelled) {
          setSessionUnlockError(String(error));
          setTab("Kaspa");
        }
      } finally {
        if (!cancelled) {
          if (pendingUnlockRef.current?.profileId === profileId) pendingUnlockRef.current = null;
          setAutoUnlockSettledId(profileId);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeId, active?.settings.requireUnlockPassword, autoUnlockSettledId, sessionPassword]);

  const backupDependency = active
    ? JSON.stringify({
        contacts: active.settings.contactsBackupKaspa ? active.contacts : [],
        messages: active.settings.backupMessagesKaspa
          ? active.chats.flatMap(chat => chat.messages.filter(message => !message.txid && message.direction !== "system"))
          : [],
        contactsBackupKaspa: active.settings.contactsBackupKaspa,
        backupMessagesKaspa: active.settings.backupMessagesKaspa,
      })
    : "";

  useEffect(() => {
    if (!active?.wallet || !sessionPassword) return;
    if (!active.settings.contactsBackupKaspa && !active.settings.backupMessagesKaspa) return;
    const profileId = active.id;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void (async () => {
        const current = profilesRef.current.find(profile => profile.id === profileId);
        if (!current?.wallet) return;
        const fingerprint = await profileBackupFingerprint(current);
        if (fingerprint === current.wallet.profileBackupHash) return;
        const contacts = current.settings.contactsBackupKaspa ? backupContacts(current) : [];
        const messages = backupMessages(current);
        if (contacts.length === 0 && messages.length === 0) return;
        try {
          const result = await publishProfileBackup({
            password: sessionPassword,
            sealed: current.wallet.sealed,
            public: current.wallet.public,
            contacts,
            messages,
            wrpcEndpoint: current.wallet.wrpcEndpoint,
          });
          if (cancelled) return;
          commitProfiles(all => all.map(profile => profile.id === profileId && profile.wallet
            ? {
                ...profile,
                wallet: {
                  ...profile.wallet,
                  public: result.public,
                  profileBackupHash: fingerprint,
                },
              }
            : profile));
        } catch {
          // Backup stays dirty. Settings exposes an explicit retry/status path;
          // local contact/message state is never discarded because chain backup failed.
        }
      })();
    }, 1200);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [activeId, backupDependency, sessionPassword]);

  const updateProfile = (next: Profile) => {
    commitProfiles(current => current.map(profile => (profile.id === next.id ? next : profile)));
  };

  async function persistProfileUpdate(next: Profile): Promise<void> {
    // Security prompt policy must survive dev/build origin changes and abrupt app
    // restarts. Update the synchronous browser cache immediately, then await the
    // ordered native mirror write before reporting the setting change as saved.
    const updated = enforceAutoLogin(
      profilesRef.current.map(profile => profile.id === next.id ? next : profile),
    );
    profilesRef.current = updated;
    setProfiles(updated);
    saveProfiles(updated);
    await queueNativeProfileSave(updated);
  }

  if (!profilesReady) {
    return (
      <section className="gate-shell">
        <div className="gate-card"><h1>Ghost Talk</h1><p>Loading local Ghost Talk IDs…</p></div>
      </section>
    );
  }

  const startupUnlockPending = Boolean(
    active?.wallet
    && !sessionPassword
    && (
      pendingUnlockRef.current?.profileId === active.id
      || (!active.settings.requireUnlockPassword && autoUnlockSettledId !== active.id)
    )
  );

  if (startupUnlockPending) {
    return (
      <section className="gate-shell">
        <div className="gate-card loading-card" aria-live="polite" aria-busy="true">
          <img className="brand-mark" src="./ghost-talk-icon.png" alt="" />
          <div className="loading-spinner" aria-hidden="true" />
          <h1>Unlocking Ghost Talk ID</h1>
          <p className="muted">Opening the Kaspa wallet and HYDRA mailbox session…</p>
        </div>
      </section>
    );
  }

  if (!active) {
    return (
      <IdentityGate
        profiles={profiles}
        onSelect={(id, unlockPassword) => {
          setSessionPassword("");
          setSessionUnlockError("");
          setAutoUnlockSettledId("");
          pendingUnlockRef.current = unlockPassword ? { profileId: id, password: unlockPassword } : null;
          setActiveId(id);
        }}
        onAddProfile={profile => {
          commitProfiles(current => enforceAutoLogin([...current, profile]));
        }}
        onUpdateProfile={profile => {
          commitProfiles(current =>
            enforceAutoLogin(current.map(item => item.id === profile.id ? profile : item))
          );
        }}
        onAutoLogin={enabled => {
          if (profiles.length !== 1 || !profiles[0].recoveryBackupConfirmed) return;
          commitProfiles(() => [{ ...profiles[0], autoLogin: enabled }]);
        }}
      />
    );
  }

  async function unlockProfile(password: string, refreshRememberedCredential = false): Promise<void> {
    if (!active?.wallet) throw new Error("Create or import the Kaspa wallet first.");
    await unlockWallet({ profileId: active.id, password, sealed: active.wallet.sealed, public: active.wallet.public });
    const hydra = await ensureHydra({
      profileId: active.id,
      password,
      identityId: active.hydraIdentityId,
    });
    let currentProfile: Profile = { ...active, hydraIdentityId: hydra.identity_id };
    await registerHydraPeerRoutes({
      profileId: currentProfile.id,
      identityId: hydra.identity_id,
      routes: peerRouteRegistrations(currentProfile),
    });
    await Promise.all(
      currentProfile.chats
        .filter(chat => chat.left && chat.peerHydraHandle)
        .map(chat => leaveHydraPeer(currentProfile.id, chat.peerHydraHandle!)),
    );
    if (refreshRememberedCredential && !active.settings.requireUnlockPassword) {
      await setRememberedUnlock({
        profileId: active.id,
        enabled: true,
        password,
        sealed: active.wallet.sealed,
        public: active.wallet.public,
      });
    }
    try {
      currentProfile = await resumePendingMessage(currentProfile, password);
    } catch {
      // A previously interrupted first-contact send remains visibly pending and
      // can be retried on the next unlock/network opportunity.
    }
    updateProfile(currentProfile);
    setSessionPassword(password);
    setSessionUnlockError("");
    setSnapshot(undefined);
    try {
      const wallet = currentProfile.wallet;
      if (!wallet) throw new Error("Ghost Talk wallet state disappeared after unlock.");
      const fresh = await refreshWallet(
        wallet.public,
        wallet.restEndpoint,
        wallet.wrpcEndpoint,
      );
      setSnapshot(fresh);
      const rotated = rotateReceiveAddress(currentProfile, fresh.recommended_receive_index);
      if (rotated !== currentProfile) updateProfile(rotated);
    } catch {
      // Unlock remains valid when the public balance endpoint is temporarily unavailable.
      // The monitor retries; undefined snapshot deliberately renders as loading, never 0 KAS.
    }
  }

  async function installWallet(
    wallet: WalletRecord,
    password: string,
    backupConfirmed: boolean,
  ): Promise<void> {
    const hydra = await initializeHydraFromWallet({
      profileId: active!.id,
      password,
      sealed: wallet.sealed,
      public: wallet.public,
    });
    await unlockWallet({
      profileId: active!.id,
      password,
      sealed: wallet.sealed,
      public: wallet.public,
    });
    updateProfile({
      ...active!,
      wallet,
      hydraIdentityId: hydra.identity_id,
      recoveryBackupConfirmed: backupConfirmed,
    });
    setSessionPassword(password);
  }

  async function resolvePeer(target: string) {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current?.wallet || !current.hydraIdentityId) {
      throw new Error("This Ghost Talk ID is not fully initialized.");
    }
    if (!sessionPassword) {
      throw new Error("Unlock wallet + mailbox in Kaspa before resolving encrypted peers.");
    }
    return resolveGhostPeer({
      profileId: current.id,
      password: sessionPassword,
      identityId: current.hydraIdentityId,
      target,
      network: current.wallet.public.network,
      restEndpoint: current.wallet.restEndpoint,
    });
  }

  async function addContactTarget(label: string, target: string): Promise<void> {
    const peer = await resolvePeer(target);
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current) throw new Error("Select a Ghost Talk ID first.");
    const existing = current.contacts.find(contact => contact.kaspaAddress === peer.kaspa_address);
    const resolvedLabel = label.trim()
      || peer.username
      || peer.display_name
      || peer.kns_name
      || peer.kaspa_address;
    if (existing) {
      const updated: Contact = {
        ...existing,
        label: resolvedLabel || existing.label,
        kaspaAddress: peer.kaspa_address,
        knsName: peer.kns_name,
        hydraHandle: peer.hydra_handle ?? existing.hydraHandle,
        verifiedPublic: peer.verified_public,
        publicUsername: peer.username || undefined,
      };
      updateProfile({
        ...current,
        contacts: current.contacts.map(contact => contact.id === existing.id ? updated : contact),
        chats: current.chats.map(chat =>
          chat.contactId === existing.id || (!chat.contactId && chat.peerKaspaAddress === updated.kaspaAddress)
            ? {
                ...chat,
                contactId: existing.id,
                label: updated.label,
                peerKaspaAddress: updated.kaspaAddress,
                peerKnsName: updated.knsName,
                peerHydraHandle: updated.hydraHandle ?? chat.peerHydraHandle,
                verifiedPublic: updated.verifiedPublic,
              }
            : chat
        ),
      });
      return;
    }
    addContact(current, {
      id: crypto.randomUUID(),
      label: resolvedLabel,
      kaspaAddress: peer.kaspa_address,
      knsName: peer.kns_name,
      hydraHandle: peer.hydra_handle,
      verifiedPublic: peer.verified_public,
      publicUsername: peer.username || undefined,
    }, updateProfile);
  }

  async function startDirectChat(target: string): Promise<string> {
    const peer = await resolvePeer(target);
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current) throw new Error("Select a Ghost Talk ID first.");
    const contact = current.contacts.find(candidate => candidate.kaspaAddress === peer.kaspa_address);

    // Archive is a sticky local history boundary. Starting a chat with the same
    // peer must never revive or mutate an archived thread. Reuse only an already
    // active thread; otherwise create a separate active conversation with a new
    // local UUID. The first message in that new thread also gets a fresh 128-bit
    // GTCR request id from sendChatMessage(), so stale bootstrap state from the
    // archived conversation cannot become the identity of the new conversation.
    const existingActive = current.chats.find(chat =>
      !chat.left
      && !chat.peerLeft
      && !chat.archived
      && (
        (contact && chat.contactId === contact.id)
        || chat.peerKaspaAddress === peer.kaspa_address
      )
    );
    if (existingActive) {
      if (contact && existingActive.contactId !== contact.id) {
        updateProfile({
          ...current,
          chats: current.chats.map(chat => chat.id === existingActive.id ? {
            ...chat,
            contactId: contact.id,
            label: contact.label || chat.label,
            peerKaspaAddress: peer.kaspa_address,
            peerKnsName: peer.kns_name ?? chat.peerKnsName,
            peerHydraHandle: contact.hydraHandle ?? peer.hydra_handle ?? chat.peerHydraHandle,
            verifiedPublic: contact.verifiedPublic ?? peer.verified_public ?? chat.verifiedPublic,
          } : chat),
        });
      }
      setChatFocusId(existingActive.id);
      return existingActive.id;
    }

    // A left conversation closed its HYDRA peer state, so reopen transport state
    // before creating the new logical thread. Merely archived conversations do
    // not close HYDRA and are intentionally left untouched forever until the user
    // explicitly presses Unarchive on that exact historical thread.
    const departed = current.chats.find(chat =>
      (chat.left || chat.peerLeft)
      && (
        (contact && chat.contactId === contact.id)
        || chat.peerKaspaAddress === peer.kaspa_address
      )
    );
    const departedHandle = contact?.hydraHandle ?? departed?.peerHydraHandle;
    if (departedHandle) await rejoinHydraPeer(current.id, departedHandle);

    const id = crypto.randomUUID();
    const chat = {
      id,
      label: contact?.label || peer.username || peer.display_name || peer.kns_name || peer.kaspa_address,
      messages: [],
      contactId: contact?.id,
      peerKaspaAddress: peer.kaspa_address,
      peerKnsName: peer.kns_name,
      peerHydraHandle: contact?.hydraHandle ?? peer.hydra_handle,
      verifiedPublic: contact?.verifiedPublic ?? peer.verified_public,
      bootstrapComplete: false,
      archived: false,
      left: false,
      peerLeft: false,
    };
    updateProfile({ ...current, chats: [chat, ...current.chats] });
    setChatFocusId(id);
    return id;
  }

  async function startDirectChatFromDiscover(target: string): Promise<string> {
    const id = await startDirectChat(target);
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (current) {
      const selected = current.chats.find(chat => chat.id === id);
      if (selected && current.chats[0]?.id !== id) {
        updateProfile({
          ...current,
          chats: [selected, ...current.chats.filter(chat => chat.id !== id)],
        });
      }
    }
    setTab("Chats");
    return id;
  }

  async function resumePendingMessage(profile: Profile, password: string): Promise<Profile> {
    const wallet = profile.wallet;
    if (!wallet || !profile.hydraIdentityId) return profile;
    for (const chat of profile.chats) {
      const pending = chat.messages.find(message => message.direction === "out" && message.pending && !message.txid);
      if (!pending) continue;
      if (pending.pendingStage === "request" && pending.contactRequestId) {
        return {
          ...profile,
          chats: profile.chats.map(thread => thread.id === chat.id
            ? {
                ...thread,
                messages: thread.messages.map(message => message.id === pending.id
                  ? { ...message, retryAfter: Date.now(), retryCount: message.retryCount ?? 0 }
                  : message),
              }
            : thread),
        };
      }
      // The GTCR acceptor never initiates the first HYDRA session, including
      // after an app restart. Its queued replies are flushed only when the
      // requester's authenticated FINISH establishes the responder session.
      if (chat.incomingRequest?.state === "accepted") continue;
      const contact = chat.contactId
        ? profile.contacts.find(candidate => candidate.id === chat.contactId)
        : undefined;
      const hydraHandle = contact?.hydraHandle ?? chat.peerHydraHandle;
      const kaspaAddress = contact?.kaspaAddress ?? chat.peerKaspaAddress;
      if (!hydraHandle || !kaspaAddress) continue;
      const wireId = validWireId(pending.wireId) ? pending.wireId : newWireId();
      const result = await sendMailboxMessage({
        profileId: profile.id,
        password,
        identityId: profile.hydraIdentityId,
        senderDisplayName: profile.label,
        contactId: hydraHandle,
        destination: kaspaAddress,
        body: pending.body,
        messageId: wireId,
        stegoProfile: decodeVoiceMessage(pending.body) ? "Off" : profile.settings.stego,
        sealed: wallet.sealed,
        public: wallet.public,
        feeSompi: "0",
        wrpcEndpoint: wallet.wrpcEndpoint,
      });
      return {
        ...profile,
        wallet: { ...wallet, public: result.public },
        chats: profile.chats.map(thread => thread.id === chat.id
          ? {
              ...thread,
              messages: thread.messages.map(message => message.id === pending.id
                ? {
                    ...message,
                    wireId,
                    pending: result.pending_handshake,
                    pendingId: result.pending_id,
                    pendingStage: result.pending_handshake ? "handshake" : undefined,
                    txid: result.pending_handshake ? undefined : result.transaction_id,
                    retryCount: undefined,
                    retryAfter: undefined,
                    sendState: "sent",
                    sendError: undefined,
                  }
                : message),
            }
          : thread),
      };
    }
    return profile;
  }

  async function publishCurrentDescriptor(discoverable: boolean): Promise<string> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current?.wallet || !current.hydraIdentityId || !sessionPassword) {
      throw new Error("Unlock this Ghost Talk ID in Kaspa first.");
    }
    const interests = current.settings.publicInterests
      .split(",")
      .map(value => value.trim())
      .filter(Boolean);
    const result = await publishGhostDescriptor({
      profileId: current.id,
      password: sessionPassword,
      identityId: current.hydraIdentityId,
      displayName: current.label,
      username: current.settings.publicUsername,
      description: current.settings.publicDescription,
      interests,
      discoverable,
      sealed: current.wallet.sealed,
      public: current.wallet.public,
      restEndpoint: current.wallet.restEndpoint,
      wrpcEndpoint: current.wallet.wrpcEndpoint,
    });
    updateProfile({ ...current, wallet: { ...current.wallet, public: result.public } });
    if (result.already_current) {
      return discoverable
        ? `Your latest public Ghost Talk profile is already current: ${result.kaspa_address}`
        : `Your latest Ghost Talk profile is already unlisted: ${result.kaspa_address}`;
    }
    return discoverable
      ? `Public Ghost Talk profile published in ${result.transaction_id ?? "Kaspa transaction"}: ${result.kaspa_address}`
      : `Ghost Talk profile is now unlisted as of ${result.transaction_id ?? "the latest Kaspa transaction"}. Historical records remain on-chain.`;
  }

  async function lookupPublicProfile(target: string): Promise<PublicGhostProfile | null> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current?.wallet) throw new Error("Create or import the Kaspa wallet first.");
    const found = await lookupGhostProfile({
      target,
      network: current.wallet.public.network,
      restEndpoint: current.wallet.restEndpoint,
    });
    if (found) {
      commitProfiles(all => all.map(profile =>
        profile.id === current.id ? mergePublicDirectory(profile, [found]) : profile
      ));
    }
    return found;
  }

  async function acceptIncomingChat(chatId: string): Promise<void> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current?.wallet || !current.hydraIdentityId || !sessionPassword) {
      throw new Error("Unlock wallet + mailbox before accepting this chat request.");
    }
    const chat = current.chats.find(thread => thread.id === chatId);
    const request = chat?.incomingRequest;
    if (!chat || !request || (request.state !== "pending" && request.state !== "ignored")) {
      throw new Error("This chat request can no longer be accepted.");
    }
    protocolDebug("info", "handshake", "accept-click", {
      profileId: current.id,
      chatId,
      requestId: request.requestId,
      peerHydraId: request.peerHydraId,
      peerAddress: request.peerAddress,
    });
    const result = await sendMailboxContactAccept({
      profileId: current.id,
      password: sessionPassword,
      identityId: current.hydraIdentityId,
      senderDisplayName: current.label,
      sealed: current.wallet.sealed,
      public: current.wallet.public,
      signedRequestHex: request.signedRequestHex,
      wrpcEndpoint: current.wallet.wrpcEndpoint,
    });
    protocolDebug("info", "handshake", "response-submit-complete", {
      profileId: current.id,
      chatId,
      requestId: request.requestId,
      transactionId: result.transaction_id,
    });
    // The native acceptance can take long enough for mailbox events to arrive in
    // parallel. Merge only the acceptance fields into the *latest* profile so a
    // stale pre-await snapshot cannot erase a newly arrived request/message.
    commitProfiles(all => all.map(profile => {
      if (profile.id !== current.id || !profile.wallet) return profile;
      return {
        ...profile,
        wallet: { ...profile.wallet, public: result.public },
        chats: collapseDuplicateRequestThreads(profile.chats, request.requestId, chatId).map(thread => thread.id === chatId ? {
          ...thread,
          peerHydraHandle: request.peerHydraId,
          peerKaspaAddress: request.peerAddress,
          // Acceptance records consent and the responder identity, but the chat is
          // not cryptographically active until pq_finish is authenticated.
          bootstrapComplete: false,
          incomingRequest: {
            ...request,
            state: "accepted",
          },
        } : thread),
      };
    }));
    setChatFocusId(chatId);
    setTab("Chats");
  }

  function ignoreIncomingChat(chatId: string): void {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current) return;
    const chat = current.chats.find(thread => thread.id === chatId);
    if (!chat?.incomingRequest) return;
    updateProfile({
      ...current,
      chats: current.chats.map(thread => thread.id === chatId ? {
        ...thread,
        incomingRequest: { ...thread.incomingRequest!, state: "ignored" },
      } : thread),
    });
  }

  function deleteIncomingChatRequest(chatId: string): void {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current) return;
    const chat = current.chats.find(thread => thread.id === chatId);
    if (!chat?.incomingRequest || chat.incomingRequest.state !== "ignored") return;
    if (chat.messages.length > 0 || chat.contactId) {
      updateProfile({
        ...current,
        chats: current.chats.map(thread => thread.id === chatId
          ? { ...thread, incomingRequest: undefined }
          : thread),
      });
      return;
    }
    updateProfile({ ...current, chats: current.chats.filter(thread => thread.id !== chatId) });
  }

  async function sendChatMessage(chatId: string, body: string): Promise<void> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current) throw new Error("Select a Ghost Talk ID first.");
    if (!current.wallet) throw new Error("Create or import the Kaspa wallet first.");
    if (!sessionPassword) throw new Error("Unlock the wallet + HYDRA mailbox for this session first.");
    if (!current.hydraIdentityId) throw new Error("This profile has no HYDRA identity.");
    const chat = current.chats.find(thread => thread.id === chatId);
    if (!chat) throw new Error("The selected chat no longer exists.");
    if (chat.archived) throw new Error("This chat is archived. Unarchive it explicitly before sending, or start a new chat with this person.");
    if (chat.left) throw new Error("You left this chat. Start a new chat to establish a fresh secure session.");
    if (chat.peerLeft) throw new Error("The other participant ended this chat. Start a new chat to establish a fresh KKTP session.");
    const contact = chat.contactId
      ? current.contacts.find(candidate => candidate.id === chat.contactId)
      : undefined;
    const hydraHandle = contact?.hydraHandle ?? chat.peerHydraHandle;
    const kaspaAddress = contact?.kaspaAddress ?? chat.peerKaspaAddress;
    if (!kaspaAddress) {
      throw new Error("This chat has no Kaspa destination. Start it again using a valid Kaspa address or KNS name.");
    }
    if (chat.incomingRequest && chat.incomingRequest.state !== "accepted") {
      throw new Error(
        chat.incomingRequest.state === "pending"
          ? "Accept or ignore the incoming secure chat request before sending."
          : "Accept this ignored secure chat request before sending.",
      );
    }

    // Only the original GTCR requester initiates the first HYDRA handshake. The
    // acceptor may type immediately, but those messages stay locally queued until
    // the requester's FINISH establishes the session. This removes simultaneous
    // cross-init from the normal chat path entirely.
    const acceptedAwaitingHandshake = chat.incomingRequest?.state === "accepted";
    const handshakeInFlight = chat.messages.some(message =>
      message.direction === "out"
      && message.pending
      && (message.pendingStage === "handshake" || message.pendingStage === "finish")
    );
    const queueBehindHandshake = acceptedAwaitingHandshake || handshakeInFlight;
    const needsRequest = !queueBehindHandshake && (!hydraHandle || !chat.bootstrapComplete);
    if (needsRequest && chat.messages.some(message => message.direction === "out" && message.pendingStage === "request" && message.pending)) {
      throw new Error("A private Ghost Talk chat request is already awaiting this recipient's acceptance.");
    }
    const wireId = newWireId();
    const requestId = needsRequest ? newWireId() : undefined;
    const optimistic: Message = {
      id: wireId,
      wireId,
      sessionSid: requestId ?? chat.sessionSid,
      direction: "out",
      body,
      createdAt: Date.now(),
      pending: true,
      pendingStage: needsRequest ? "request" : queueBehindHandshake ? "handshake" : "delivery",
      contactRequestId: requestId,
      retryCount: 0,
      sendState: "sending",
    };
    // Local echo happens before any network/KDF/transaction work. The carrier
    // lifecycle updates this same logical message afterward.
    commitProfiles(all => all.map(profile => profile.id === current.id ? {
      ...profile,
      chats: profile.chats.map(thread => thread.id === chatId
        ? { ...thread, messages: [...thread.messages, optimistic] }
        : thread),
    } : profile));

    if (queueBehindHandshake) {
      // Never start a second HYDRA handshake while the first one is in flight. The
      // acceptor and any rapid follow-up sends stay local until native HYDRA reports
      // the ratchet established, then resendAwaitingForPeer flushes them in order.
      commitProfiles(all => all.map(profile => profile.id === current.id ? {
        ...profile,
        chats: profile.chats.map(thread => thread.id === chatId ? {
          ...thread,
          messages: thread.messages.map(message => message.id === wireId ? {
            ...message,
            pending: true,
            pendingStage: "handshake",
            retryAfter: undefined,
            retryCount: undefined,
            sendState: "sent",
            sendError: undefined,
          } : message),
        } : thread),
      } : profile));
      return;
    }

    try {
      if (needsRequest) {
        protocolDebug("info", "handshake", "discovery-submit-start", {
          profileId: current.id,
          chatId,
          requestId,
          destination: kaspaAddress,
          messageId: wireId,
        });
        const request = await sendMailboxContactRequest({
          profileId: current.id,
          password: sessionPassword,
          identityId: current.hydraIdentityId,
          senderDisplayName: current.label,
          sealed: current.wallet.sealed,
          public: current.wallet.public,
          destination: kaspaAddress,
          requestId: requestId!,
          wrpcEndpoint: current.wallet.wrpcEndpoint,
        });
        protocolDebug("info", "handshake", "discovery-submit-complete", {
          profileId: current.id,
          chatId,
          requestId,
          messageId: wireId,
          transactionId: request.transaction_id,
        });
        commitProfiles(all => all.map(profile => {
          if (profile.id !== current.id || !profile.wallet) return profile;
          return {
            ...profile,
            wallet: { ...profile.wallet, public: request.public },
            chats: profile.chats.map(thread => thread.id === chatId ? {
              ...thread,
              sessionSid: requestId,
              peerLeft: false,
              messages: thread.messages.map(message => message.id === wireId ? {
                ...message,
                sendState: "sent",
                pending: true,
                pendingStage: "request",
                contactRequestId: requestId,
                txid: request.transaction_id,
                // Kaspa is the durable mailbox. Once this transaction is
                // confirmed locally, do not manufacture another on-chain request;
                // the receiver's overlap scanner will consume this exact SID.
                retryAfter: undefined,
                retryCount: undefined,
              } : message),
            } : thread),
          };
        }));
        return;
      }

      const result = await sendMailboxMessage({
        profileId: current.id,
        password: sessionPassword,
        identityId: current.hydraIdentityId,
        senderDisplayName: current.label,
        contactId: hydraHandle!,
        destination: kaspaAddress,
        body,
        messageId: wireId,
        stegoProfile: decodeVoiceMessage(body) ? "Off" : current.settings.stego,
        sealed: current.wallet.sealed,
        public: current.wallet.public,
        feeSompi: "0",
        wrpcEndpoint: current.wallet.wrpcEndpoint,
      });
      commitProfiles(all => all.map(profile => {
        if (profile.id !== current.id || !profile.wallet) return profile;
        return {
          ...profile,
          wallet: { ...profile.wallet, public: result.public },
          chats: profile.chats.map(thread => thread.id === chatId ? {
            ...thread,
            messages: thread.messages.map(message => message.id === wireId ? {
              ...message,
              txid: result.pending_handshake ? undefined : result.transaction_id,
              pending: result.pending_handshake,
              pendingId: result.pending_id,
              pendingStage: result.pending_handshake ? "handshake" : undefined,
              // A successful pq_init broadcast is durable bootstrap progress, but
              // the logical message itself is not on Kaspa until FINISH/active send.
              // Once SubmitTransaction accepts that message carrier, it is Delivered.
              retryAfter: undefined,
              retryCount: undefined,
              sendState: result.pending_handshake ? "sent" : "delivered",
              sendError: undefined,
            } : message),
          } : thread),
        };
      }));
    } catch (error) {
      const detail = String(error);
      commitProfiles(all => all.map(profile => profile.id === current.id ? {
        ...profile,
        chats: profile.chats.map(thread => thread.id === chatId ? {
          ...thread,
          messages: thread.messages.map(message => message.id === wireId ? {
            ...message,
            pending: false,
            pendingId: undefined,
            sendState: "failed",
            sendError: detail,
          } : message),
        } : thread),
      } : profile));
      throw error;
    }
  }

  async function sendChatControl(chatId: string, body: string, reuseChange = false): Promise<void> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current?.wallet || !current.hydraIdentityId || !sessionPassword) {
      throw new Error("Unlock wallet + HYDRA mailbox before using realtime transport.");
    }
    const chat = current.chats.find(thread => thread.id === chatId);
    if (!chat || chat.left || chat.peerLeft || !chat.bootstrapComplete || !chat.sessionSid) {
      throw new Error("Finish establishing the secure chat before using realtime transport.");
    }
    const route = routeForChat(current, chat);
    if (!route?.hydraHandle || !route.kaspaAddress) {
      throw new Error("This chat has no authenticated HYDRA/Kaspa realtime route.");
    }
    const result = await sendMailboxMessage({
      profileId: current.id,
      password: sessionPassword,
      identityId: current.hydraIdentityId,
      senderDisplayName: current.label,
      contactId: route.hydraHandle,
      destination: route.kaspaAddress,
      body,
      messageId: newWireId(),
      stegoProfile: "Off",
      sealed: current.wallet.sealed,
      public: current.wallet.public,
      feeSompi: "0",
      wrpcEndpoint: current.wallet.wrpcEndpoint,
      reuseChange,
    });
    commitProfiles(all => all.map(profile => profile.id === current.id && profile.wallet
      ? { ...profile, wallet: { ...profile.wallet, public: result.public } }
      : profile));
  }

  async function sealDirectChatPacket(chatId: string, body: string): Promise<string> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current?.hydraIdentityId) throw new Error("Select and unlock a Ghost Talk ID first.");
    const chat = current.chats.find(thread => thread.id === chatId);
    if (!chat?.sessionSid || !chat.bootstrapComplete || chat.left || chat.peerLeft) {
      throw new Error("Direct transport requires the exact active Ghost Talk session.");
    }
    const route = routeForChat(current, chat);
    if (!route?.hydraHandle) throw new Error("Direct transport has no authenticated HYDRA peer.");
    return sealHydraDirect({
      profileId: current.id,
      contactId: route.hydraHandle,
      sessionSid: chat.sessionSid,
      body,
    });
  }

  async function openDirectChatPacket(chatId: string, envelopeB64: string): Promise<string | null> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current?.hydraIdentityId) throw new Error("Select and unlock a Ghost Talk ID first.");
    const chat = current.chats.find(thread => thread.id === chatId);
    if (!chat?.sessionSid || !chat.bootstrapComplete || chat.left || chat.peerLeft) {
      throw new Error("Direct transport requires the exact active Ghost Talk session.");
    }
    const route = routeForChat(current, chat);
    if (!route?.hydraHandle) throw new Error("Direct transport has no authenticated HYDRA peer.");
    return openHydraDirect({
      profileId: current.id,
      contactId: route.hydraHandle,
      sessionSid: chat.sessionSid,
      envelopeB64,
    });
  }

  function addDirectChatPeerToContacts(chatId: string): void {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    const chat = current?.chats.find(candidate => candidate.id === chatId);
    if (!chat?.peerKaspaAddress || chat.contactId) return;
    setContactPrefillTarget(chat.peerKaspaAddress);
    setTab("Contacts");
  }

  function archiveChat(chatId: string, archived: boolean): void {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current) return;
    updateProfile({
      ...current,
      chats: current.chats.map(chat => chat.id === chatId ? { ...chat, archived } : chat),
    });
  }

  async function leaveChat(chatId: string): Promise<void> {
    const current = profilesRef.current.find(profile => profile.id === activeId);
    if (!current) throw new Error("Select a Ghost Talk ID first.");
    const chat = current.chats.find(candidate => candidate.id === chatId);
    if (!chat) return;
    const contact = chat.contactId ? current.contacts.find(item => item.id === chat.contactId) : undefined;
    const handle = contact?.hydraHandle ?? chat.peerHydraHandle;
    const destination = contact?.kaspaAddress ?? chat.peerKaspaAddress;

    // Leave is locally immediate, but an established KKTP session also emits a
    // signed on-chain session_end before native ratchet teardown. The remote
    // peer can then close the exact SID and refuse further traffic from this
    // historical thread. Archive/history state remains purely local.
    updateProfile({
      ...current,
      chats: current.chats.map(candidate => candidate.id === chatId
        ? { ...candidate, left: true, archived: true, bootstrapComplete: false, incomingRequest: undefined }
        : candidate),
    });
    setIncomingNotice(notice => notice?.chatId === chatId ? null : notice);

    if (handle && sessionPassword) {
      try {
        if (
          !chat.peerLeft
          && destination
          && current.wallet
          && current.hydraIdentityId
        ) {
          const ended = await sendMailboxSessionEnd({
            profileId: current.id,
            password: sessionPassword,
            identityId: current.hydraIdentityId,
            contactId: handle,
            destination,
            sealed: current.wallet.sealed,
            public: current.wallet.public,
            wrpcEndpoint: current.wallet.wrpcEndpoint,
          });
          commitProfiles(all => all.map(profile => profile.id === current.id && profile.wallet
            ? { ...profile, wallet: { ...profile.wallet, public: ended.public } }
            : profile));
        }
      } catch (error) {
        // Local Leave is authoritative even if the network is unavailable. Keep
        // the failure visible in developer diagnostics; a remote peer cannot be
        // assumed closed unless its authenticated session_end reaches Kaspa.
        console.error("Ghost Talk session_end broadcast failed", error);
      } finally {
        try {
          await leaveHydraPeer(current.id, handle);
        } catch (error) {
          console.error("Ghost Talk local HYDRA leave cleanup failed", error);
        }
      }
    }
  }



  const content = (() => {
    switch (tab) {
      case "Chats":
        return (
          <ChatView
            chats={active.chats}
            mask={mask}
            onMask={setMask}
            route={active.settings.route}
            stego={active.settings.stego}
            onRoute={route =>
              updateProfile({ ...active, settings: { ...active.settings, route } })
            }
            onStego={stego =>
              updateProfile({ ...active, settings: { ...active.settings, stego } })
            }
            onSend={(chatId, body) => sendChatMessage(chatId, body)}
            onSendControl={sendChatControl}
            onSealDirect={sealDirectChatPacket}
            onOpenDirect={openDirectChatPacket}
            localHydraId={active.hydraIdentityId}
            realtimeControls={realtimeControls}
            onRealtimeControlHandled={id => setRealtimeControls(current => current.filter(item => item.id !== id))}
            onStartChat={startDirectChat}
            onAcceptRequest={acceptIncomingChat}
            onIgnoreRequest={ignoreIncomingChat}
            onDeleteRequest={deleteIncomingChatRequest}
            onArchive={archiveChat}
            onAddContact={addDirectChatPeerToContacts}
            onLeave={leaveChat}
            focusChatId={chatFocusId}
            onFocusHandled={() => setChatFocusId("")}
          />
        );
      case "Contacts":
        return (
          <ContactsView
            profile={active}
            unlocked={Boolean(sessionPassword)}
            onAddTarget={addContactTarget}
            prefillTarget={contactPrefillTarget}
            onPrefillConsumed={() => setContactPrefillTarget("")}
          />
        );
      case "Discover":
        return (
          <DiscoverView
            profile={active}
            unlocked={Boolean(sessionPassword)}
            onUpdate={updateProfile}
            onPublishDescriptor={publishCurrentDescriptor}
            onLookup={lookupPublicProfile}
            publicUsers={(active.publicDirectory ?? []).filter(user => user.kaspa_address !== active.wallet?.public.receive_addresses[0])}
            onStartChat={startDirectChatFromDiscover}
          />
        );
      case "Rooms":
        return <RoomsView />;
      case "Games":
        return <GamesView />;
      case "Kaspa":
        return (
          <KaspaWallet
            profile={active}
            snapshot={snapshot}
            sessionUnlocked={Boolean(sessionPassword)}
            sessionPassword={sessionPassword}
            requireSendPassword={active.settings.requireSendPassword}
            initialUnlockStatus={sessionUnlockError}
            onUnlock={password => unlockProfile(password, !active.settings.requireUnlockPassword)}
            onWalletReady={installWallet}
            onRecoveryBackupConfirmed={() => updateProfile({ ...active, recoveryBackupConfirmed: true })}
            onWallet={wallet => updateProfile({ ...active, wallet })}
            onKasSignerWallet={kasSignerWallet => updateProfile({ ...active, kasSignerWallet })}
            onSnapshot={setSnapshot}
          />
        );
      case "Settings":
        return (
          <SettingsView
            profile={active}
            profileCount={profiles.length}
            onUnlock={password => unlockProfile(password)}
            onUpdate={next =>
              updateProfile(profiles.length === 1 ? next : { ...next, autoLogin: false })
            }
            onPersistUpdate={next =>
              persistProfileUpdate(profiles.length === 1 ? next : { ...next, autoLogin: false })
            }
            onOpenDebug={() => setDebugOpen(true)}
          />
        );
    }
  })();

  return (
    <main className="app">
      <Sidebar
        profile={active}
        tab={tab}
        networkStatus={networkStatus}
        reconnectAttempts={reconnectAttempts}
        incomingRequests={active.chats.filter(chat => chat.incomingRequest?.state === "pending").length}
        onTab={setTab}
        onSwitchUser={() => {
          if (activeId) {
            void lockHydraProfile(activeId).catch(() => undefined);
            void lockWallet(activeId).catch(() => undefined);
          }
          setSessionPassword("");
          setSessionUnlockError("");
          setAutoUnlockSettledId("");
          pendingUnlockRef.current = null;
          setActiveId(null);
          setTab("Chats");
        }}
      />
      <section className="workspace">
        {incomingNotice && (
          <div className="incoming-notice" role="status">
            <span>{incomingNotice.text}</span>
            <div className="button-row">
              <button onClick={() => {
                if (incomingNotice.chatId) setChatFocusId(incomingNotice.chatId);
                setTab("Chats");
                setIncomingNotice(null);
              }}>Open chat</button>
              <button onClick={() => setIncomingNotice(null)} aria-label="Dismiss notification">×</button>
            </div>
          </div>
        )}
        {content}
      </section>
      <DebugLogWindow
        profileId={active.id}
        open={debugOpen && Boolean(active.settings.debugLogging)}
        onClose={() => setDebugOpen(false)}
      />
    </main>
  );
}

function findNewPendingRequestChatId(before: Profile, after: Profile): string | undefined {
  return after.chats.find(chat => {
    const request = chat.incomingRequest;
    if (request?.state !== "pending") return false;
    const previous = before.chats.find(candidate => candidate.id === chat.id)?.incomingRequest;
    return previous?.requestId !== request.requestId || previous.state !== "pending";
  })?.id;
}

function findNewIncomingMessageChatId(before: Profile, after: Profile): string | undefined {
  return after.chats.find(chat => {
    const previous = before.chats.find(candidate => candidate.id === chat.id);
    const beforeCount = previous?.messages.filter(message => message.direction === "in").length ?? 0;
    const afterCount = chat.messages.filter(message => message.direction === "in").length;
    return afterCount > beforeCount;
  })?.id;
}

function mergeAppliedMailboxProfile(latest: Profile, applied: Profile): Profile {
  const latestWallet = latest.wallet;
  const appliedWallet = applied.wallet;
  let wallet = appliedWallet;
  if (latestWallet && appliedWallet) {
    wallet = {
      ...appliedWallet,
      // Discover owns this cursor independently from mailbox delivery.
      directoryCheckpoint: latestWallet.directoryCheckpoint ?? appliedWallet.directoryCheckpoint,
      restEndpoint: latestWallet.restEndpoint,
      wrpcEndpoint: latestWallet.wrpcEndpoint,
      profileBackupHash: latestWallet.profileBackupHash ?? appliedWallet.profileBackupHash,
      public: {
        ...appliedWallet.public,
        // Never move an HD index backwards if an outbound operation advanced it
        // while asynchronous mailbox processing was in flight.
        next_receive_index: Math.max(
          latestWallet.public.next_receive_index,
          appliedWallet.public.next_receive_index,
        ),
        next_change_index: Math.max(
          latestWallet.public.next_change_index,
          appliedWallet.public.next_change_index,
        ),
      },
    };
  }
  return {
    ...latest,
    contacts: mergeAppliedMailboxContacts(latest.contacts, applied.contacts),
    chats: mergeAppliedMailboxChats(latest.chats, applied.chats),
    wallet,
  };
}

function mergeAppliedMailboxContacts(latest: Profile["contacts"], applied: Profile["contacts"]): Profile["contacts"] {
  const appliedById = new Map(applied.map(contact => [contact.id, contact]));
  const merged = latest.map(contact => ({ ...contact, ...(appliedById.get(contact.id) ?? {}) }));
  const known = new Set(latest.map(contact => contact.id));
  for (const contact of applied) {
    if (!known.has(contact.id)) merged.push(contact);
  }
  return merged;
}

function mergeAppliedMailboxChats(latest: Profile["chats"], applied: Profile["chats"]): Profile["chats"] {
  const appliedById = new Map(applied.map(chat => [chat.id, chat]));
  const merged = latest.map(chat => {
    const mailbox = appliedById.get(chat.id);
    if (!mailbox) return chat;
    const mailboxMessages = new Map(mailbox.messages.map(message => [message.id, message]));
    const messages = chat.messages.map(message => {
      const update = mailboxMessages.get(message.id);
      if (!update) return message;
      const mergedMessage = { ...message, ...update };
      const stateRank = { sending: 0, sent: 1, failed: 2, delivered: 3 } as const;
      if (
        message.sendState
        && update.sendState
        && stateRank[message.sendState] > stateRank[update.sendState]
      ) {
        mergedMessage.sendState = message.sendState;
        mergedMessage.sendError = message.sendError;
        if (message.sendState === "failed" || message.sendState === "delivered") mergedMessage.pending = false;
      }
      return mergedMessage;
    });
    const knownMessages = new Set(chat.messages.map(message => message.id));
    for (const message of mailbox.messages) {
      if (!knownMessages.has(message.id)) messages.push(message);
    }

    let incomingRequest = mailbox.incomingRequest;
    if (
      chat.incomingRequest
      && mailbox.incomingRequest?.requestId === chat.incomingRequest.requestId
      && chat.incomingRequest.state !== "pending"
    ) {
      // Accept/Ignore is a newer explicit local decision than an in-flight mailbox
      // snapshot that still saw the same request as pending.
      incomingRequest = chat.incomingRequest;
    } else if (
      !chat.incomingRequest
      && mailbox.incomingRequest
      && (chat.archived || chat.left)
    ) {
      // A stale mailbox pass may not resurrect a request into a thread the user
      // explicitly archived/left. A genuinely fresh request has its own active ID.
      incomingRequest = undefined;
    }

    return {
      ...chat,
      ...mailbox,
      contactId: mailbox.contactId ?? chat.contactId,
      peerKaspaAddress: mailbox.peerKaspaAddress ?? chat.peerKaspaAddress,
      peerKnsName: mailbox.peerKnsName ?? chat.peerKnsName,
      peerHydraHandle: mailbox.peerHydraHandle ?? chat.peerHydraHandle,
      // Archive/Leave are explicit local history/session boundaries. Mailbox work
      // is never allowed to undo them from an older async snapshot.
      archived: chat.archived,
      left: chat.left,
      peerLeft: Boolean(chat.peerLeft || mailbox.peerLeft),
      incomingRequest,
      messages,
    };
  });
  const known = new Set(latest.map(chat => chat.id));
  for (const chat of applied) {
    if (known.has(chat.id)) continue;
    const requestId = chat.incomingRequest?.requestId;
    if (requestId && merged.some(existing => existing.incomingRequest?.requestId === requestId)) {
      // A concurrent/stale mailbox snapshot must never materialize a second UI
      // thread for the same signed Discovery SID.
      continue;
    }
    merged.push(chat);
  }
  return collapseDuplicateRequestThreads(merged);
}

function pendingRequestCount(profile: Profile): number {
  return profile.chats.filter(chat => chat.incomingRequest?.state === "pending").length;
}

function incomingMessageCount(profile: Profile): number {
  return profile.chats.reduce(
    (total, chat) => total + chat.messages.filter(message => message.direction === "in").length,
    0,
  );
}

async function reconcileRememberedUnlockPolicies(
  profiles: Profile[],
): Promise<{ profiles: Profile[]; changed: boolean }> {
  let changed = false;
  const reconciled: Profile[] = [];
  for (const profile of profiles) {
    if (!profile.wallet || !profile.settings.requireUnlockPassword) {
      reconciled.push(profile);
      continue;
    }
    try {
      const remembered = await loadRememberedUnlock(profile.id);
      if (remembered) {
        changed = true;
        reconciled.push({
          ...profile,
          securityPolicyRevision: (profile.securityPolicyRevision ?? 0) + 1,
          settings: { ...profile.settings, requireUnlockPassword: false },
        });
        continue;
      }
    } catch {
      // A damaged/missing device credential must never silently weaken a profile
      // that is configured to require a password. Keep the stricter policy.
    }
    reconciled.push(profile);
  }
  return { profiles: reconciled, changed };
}

function autoLoginProfileId(profiles: Profile[]): string | null {
  return profiles.length === 1
    && profiles[0].autoLogin
    && profiles[0].recoveryBackupConfirmed
    ? profiles[0].id
    : null;
}

function mergePublicDirectory(
  profile: Profile,
  updates: PublicGhostProfile[],
  directoryCheckpoint?: string,
): Profile {
  const byAddress = new Map((profile.publicDirectory ?? []).map(item => [item.kaspa_address, item]));
  for (const update of updates) {
    if (!update.verified) continue;
    const existing = byAddress.get(update.kaspa_address);
    if (existing && compareDecimal(existing.descriptor_blue_score, update.descriptor_blue_score) > 0) continue;
    if (update.discoverable === false) byAddress.delete(update.kaspa_address);
    else byAddress.set(update.kaspa_address, update);
  }
  const publicDirectory = [...byAddress.values()]
    .sort((a, b) => compareDecimal(b.descriptor_blue_score, a.descriptor_blue_score))
    .slice(0, 100);
  const wallet = profile.wallet && directoryCheckpoint !== undefined
    ? { ...profile.wallet, directoryCheckpoint }
    : profile.wallet;
  return { ...profile, publicDirectory, wallet };
}

function compareDecimal(left: string, right: string): number {
  try {
    const a = BigInt(left);
    const b = BigInt(right);
    return a < b ? -1 : a > b ? 1 : 0;
  } catch {
    return left.length === right.length ? left.localeCompare(right) : left.length - right.length;
  }
}

function rotateReceiveAddress(profile: Profile, recommendedReceiveIndex: number): Profile {
  const wallet = profile.wallet;
  if (!wallet) return profile;
  const bounded = Math.max(
    wallet.public.next_receive_index,
    Math.min(recommendedReceiveIndex, wallet.public.receive_addresses.length - 1),
  );
  if (bounded === wallet.public.next_receive_index) return profile;
  return {
    ...profile,
    wallet: {
      ...wallet,
      public: { ...wallet.public, next_receive_index: bounded },
    },
  };
}

function addContact(profile: Profile, contact: Contact, update: (profile: Profile) => void) {
  if (profile.contacts.some(existing => existing.kaspaAddress === contact.kaspaAddress)) return;
  const matchingDirectChat = profile.chats.some(chat =>
    !chat.contactId && chat.peerKaspaAddress === contact.kaspaAddress
  );
  const chats = matchingDirectChat
    ? profile.chats.map(chat =>
        !chat.contactId && chat.peerKaspaAddress === contact.kaspaAddress
          ? {
              ...chat,
              contactId: contact.id,
              label: contact.label,
              peerKnsName: contact.knsName ?? chat.peerKnsName,
              peerHydraHandle: contact.hydraHandle ?? chat.peerHydraHandle,
              verifiedPublic: contact.verifiedPublic,
            }
          : chat
      )
    : [
        ...profile.chats,
        {
          id: crypto.randomUUID(),
          contactId: contact.id,
          label: contact.label,
          messages: [],
          peerKaspaAddress: contact.kaspaAddress,
          peerKnsName: contact.knsName,
          peerHydraHandle: contact.hydraHandle,
          verifiedPublic: contact.verifiedPublic,
          bootstrapComplete: false,
        },
      ];
  update({ ...profile, contacts: [...profile.contacts, contact], chats });
}

async function applyMailbox(
  profile: Profile,
  checkpoint: string,
  events: MailboxEvent[],
  recommendedReceiveIndex: number,
  password: string,
  onRealtimeControl?: (fromHydraId: string, sessionSid: string, body: string) => void,
): Promise<Profile> {
  const currentWallet = profile.wallet;
  if (!currentWallet) return profile;
  const nextReceiveIndex = Math.max(
    currentWallet.public.next_receive_index,
    Math.min(recommendedReceiveIndex, currentWallet.public.receive_addresses.length - 1),
  );
  let wallet = absorbMailboxEvents(
    {
      ...currentWallet,
      mailboxCheckpoint: checkpoint,
      mailboxScannerVersion: 1,
      public: { ...currentWallet.public, next_receive_index: nextReceiveIndex },
    },
    events,
  );
  let chats = profile.chats;
  let contacts = profile.contacts;
  const localKaspaAddresses = [...wallet.public.receive_addresses, ...wallet.public.change_addresses];

  if (!password || !profile.hydraIdentityId) {
    // GTCR first-contact requests are public, signed bootstrap objects. Verify and
    // surface them while the HYDRA vault is locked so a healthy Kaspa connection
    // cannot silently look idle. Acceptance still requires unlock and re-verifies
    // the exact retained signed request against the authenticated HYDRA card.
    for (const envelope of readyEnvelopes(wallet)) {
      try {
        const incoming = await previewHydraContactRequest({
          envelopeHex: envelope.envelopeHex,
          localKaspaAddresses,
        });
        if (!incoming) continue;
        const saved = contacts.find(contact =>
          contact.hydraHandle === incoming.peer_hydra_id
          || contact.kaspaAddress === incoming.peer_address
        );
        if (saved && saved.hydraHandle !== incoming.peer_hydra_id) {
          contacts = contacts.map(contact => contact.id === saved.id
            ? { ...contact, hydraHandle: incoming.peer_hydra_id }
            : contact);
        }
        chats = upsertIncomingRequest(chats, saved, incoming, profile.settings.autoIgnoreUnknownChats);
        wallet = removeEnvelope(wallet, envelope.packetId);
      } catch {
        // Keep malformed/non-previewable packets queued for the fully unlocked
        // HYDRA receive path, which remains the authority for encrypted traffic.
      }
    }
    return { ...profile, contacts, chats, wallet };
  }

  for (const envelope of readyEnvelopes(wallet)) {
    try {
      protocolDebug("debug", "mailbox", "envelope-dispatch", {
        profileId: profile.id,
        packetId: envelope.packetId,
        transactionId: envelope.transactionId,
      });
      const result = await receiveHydraMailbox({
        profileId: profile.id,
        password,
        identityId: profile.hydraIdentityId,
        envelopeHex: envelope.envelopeHex,
        localKaspaAddresses,
        activeSessionSids: activeConversationSids(chats),
      });
      let justCompletedMessageId: string | undefined;
      let retainEnvelopeForPairing = false;
      protocolDebug("info", "mailbox", "envelope-result", {
        profileId: profile.id,
        packetId: envelope.packetId,
        transactionId: envelope.transactionId,
        incomingRequest: Boolean(result.incoming_request),
        contactAccepted: Boolean(result.contact_accepted),
        control: Boolean(result.control),
        received: Boolean(result.received),
        messageId: result.message_id ?? null,
        sessionEstablishedPeer: result.session_established_peer ?? null,
        sessionEnded: Boolean(result.session_ended),
        deliveryAck: result.delivery_ack ?? null,
      });

      if (result.incoming_request) {
        const incoming = result.incoming_request;
        const saved = contacts.find(contact =>
          contact.hydraHandle === incoming.peer_hydra_id
          || contact.kaspaAddress === incoming.peer_address
        );
        if (saved && saved.hydraHandle !== incoming.peer_hydra_id) {
          contacts = contacts.map(contact => contact.id === saved.id
            ? { ...contact, hydraHandle: incoming.peer_hydra_id }
            : contact);
        }
        chats = upsertIncomingRequest(chats, saved, incoming, profile.settings.autoIgnoreUnknownChats);
      }

      if (result.contact_accepted) {
        const accepted = result.contact_accepted;
        protocolDebug("info", "handshake", "response-received-in-ui", {
          profileId: profile.id,
          requestId: accepted.request_id,
          peerHydraId: accepted.peer_hydra_id,
          peerAddress: accepted.peer_address,
          transactionId: envelope.transactionId,
        });
        const acceptedContact = contacts.find(contact =>
          contact.hydraHandle === accepted.peer_hydra_id
          || contact.kaspaAddress === accepted.acceptor_address
          || contact.kaspaAddress === accepted.peer_address
        );
        contacts = contacts.map(contact => acceptedContact && contact.id === acceptedContact.id
          ? {
              ...contact,
              kaspaAddress: accepted.peer_address,
              hydraHandle: accepted.peer_hydra_id,
            }
          : contact);
        const queued = findQueuedContactRequest(chats, accepted.request_id);
        if (!queued) {
          // Match the working KKTP behavior: a valid Response that arrives before
          // its local pending Discovery/message context is available is an orphan
          // response, not disposable traffic. Keep this already-bounded mailbox
          // envelope so a restart/state reconciliation can pair it on the next
          // drain instead of permanently losing consent/session establishment.
          retainEnvelopeForPairing = true;
          protocolDebug("warn", "handshake", "response-buffered-until-request-pairs", {
            profileId: profile.id,
            requestId: accepted.request_id,
            peerHydraId: accepted.peer_hydra_id,
            chats: chats.map(chat => ({
              id: chat.id,
              sessionSid: chat.sessionSid ?? null,
              bootstrapComplete: chat.bootstrapComplete,
              outgoing: chat.messages
                .filter(message => message.direction === "out")
                .map(message => ({
                  id: message.id,
                  wireId: message.wireId ?? null,
                  contactRequestId: message.contactRequestId ?? null,
                  pending: message.pending,
                  pendingStage: message.pendingStage ?? null,
                  sendState: message.sendState ?? null,
                })),
            })),
          });
        }
        if (queued) {
          protocolDebug("info", "handshake", "response-matched-first-message", {
            profileId: profile.id,
            requestId: accepted.request_id,
            chatId: queued.chat.id,
            messageId: queued.message.wireId,
          });
          // Commit authenticated acceptance first, then initiate exactly one HYDRA
          // handshake from the original GTCR requester. The acceptor never cross-
          // initiates during this bootstrap; its replies remain locally queued until
          // FINISH establishes the responder session.
          chats = chats.map(chat => chat.id === queued.chat.id ? {
            ...chat,
            contactId: acceptedContact?.id ?? chat.contactId,
            label: acceptedContact?.label || chat.label || accepted.peer_label || accepted.peer_address.slice(0, 18),
            peerKaspaAddress: accepted.peer_address,
            peerHydraHandle: accepted.peer_hydra_id,
            // The Response anchor fixes the peer/SID but does not establish the
            // HYDRA ratchet. Only pq_finish may flip bootstrapComplete to true.
            bootstrapComplete: false,
            sessionSid: accepted.request_id,
            peerLeft: false,
            messages: chat.messages.map(message => message.id === queued.message.id ? {
              ...message,
              contactRequestId: undefined,
              pendingId: undefined,
              pendingStage: "handshake",
              retryAfter: undefined,
              retryCount: undefined,
              sendState: "sent",
              sendError: undefined,
            } : message),
          } : chat);

          try {
            protocolDebug("info", "handshake", "pq-init-submit-start", {
              profileId: profile.id,
              requestId: accepted.request_id,
              chatId: queued.chat.id,
              messageId: queued.message.wireId,
              destination: accepted.peer_address,
              peerHydraId: accepted.peer_hydra_id,
            });
            const send = await sendMailboxMessage({
              profileId: profile.id,
              password,
              identityId: profile.hydraIdentityId,
              senderDisplayName: profile.label,
              contactId: accepted.peer_hydra_id,
              destination: accepted.peer_address,
              body: queued.message.body,
              messageId: queued.message.wireId!,
              stegoProfile: decodeVoiceMessage(queued.message.body) ? "Off" : profile.settings.stego,
              sealed: wallet.sealed,
              public: wallet.public,
              feeSompi: "0",
              wrpcEndpoint: wallet.wrpcEndpoint,
            });
            protocolDebug("info", "handshake", "pq-init-submit-complete", {
              profileId: profile.id,
              requestId: accepted.request_id,
              chatId: queued.chat.id,
              pendingHandshake: send.pending_handshake,
              pendingId: send.pending_id ?? null,
              transactionId: send.transaction_id,
            });
            wallet = { ...wallet, public: send.public };
            chats = updateMessage(chats, queued.chat.id, queued.message.id, message => ({
              ...message,
              pending: send.pending_handshake,
              pendingId: send.pending_id,
              pendingStage: send.pending_handshake ? "handshake" : undefined,
              txid: send.pending_handshake ? undefined : send.transaction_id,
              retryAfter: undefined,
              retryCount: undefined,
              sendState: "sent",
              sendError: undefined,
            }));
          } catch (error) {
            protocolDebug("error", "handshake", "pq-init-submit-failed", {
              profileId: profile.id,
              requestId: accepted.request_id,
              chatId: queued.chat.id,
              messageId: queued.message.wireId,
              error: String(error),
            });
            // Acceptance remains committed. Retry only because the actual carrier
            // send failed; never rebroadcast an already-successful offer on a timer.
            chats = updateMessage(chats, queued.chat.id, queued.message.id, message => ({
              ...message,
              pending: true,
              pendingId: undefined,
              pendingStage: "handshake",
              retryAfter: Date.now() + 1_000,
              retryCount: 0,
              sendState: "sent",
              sendError: String(error),
            }));
          }
        }
      }

      if (result.recovery) {
        const recoveryProjection = result.recovery;
        if (!validWireId(recoveryProjection.sid) || !recoveryProjection.peer_hydra_id) {
          throw new Error("KKTP recovery projection is missing its authenticated session identity.");
        }
        const recovery = await sendMailboxRecoveryOffer({
          profileId: profile.id,
          password,
          identityId: profile.hydraIdentityId,
          senderDisplayName: profile.label,
          sealed: wallet.sealed,
          public: wallet.public,
          destination: recoveryProjection.destination,
          offerHex: recoveryProjection.offer_hex,
          wrpcEndpoint: wallet.wrpcEndpoint,
        });
        wallet = { ...wallet, public: recovery.public };
        // The replacement SID becomes durable only after its pq_init carrier is
        // accepted by Kaspa. Persist it on the exact live peer thread so the
        // strict replay filter admits the replacement ratchet after restart.
        chats = chats.map(chat =>
          !chat.archived
            && !chat.left
            && !chat.peerLeft
            && chat.peerHydraHandle === recoveryProjection.peer_hydra_id
            ? {
                ...chat,
                sessionSid: recoveryProjection.sid,
                bootstrapComplete: false,
              }
            : chat
        );
        protocolDebug("info", "handshake", "recovery-sid-committed", {
          profileId: profile.id,
          peerHydraId: recoveryProjection.peer_hydra_id,
          sessionSid: recoveryProjection.sid,
          transactionId: recovery.transaction_id,
        });
        // Keep the stale ciphertext queued until recovery actually establishes
        // a replacement ratchet. If the app restarts before the answer arrives,
        // this durable packet is what safely triggers a fresh handshake again.
        continue;
      }

      if (result.control) {
        protocolDebug("info", "handshake", "control-submit-start", {
          profileId: profile.id,
          control: "handshake",
          destination: result.control.destination,
          completesPendingId: result.control.completes_pending_id ?? null,
        });
        // pq_resp means the initiator has already advanced HYDRA and prepared an
        // exact FINISH carrier. Stop the pq_init timer before awaiting FINISH
        // broadcast; if that broadcast fails the retained pq_resp envelope will
        // replay the same prepared FINISH instead of regenerating another INIT.
        if (result.control.completes_pending_id) {
          chats = chats.map(chat => ({
            ...chat,
            messages: chat.messages.map(message =>
              message.pendingId === result.control!.completes_pending_id
                ? { ...message, retryAfter: undefined, retryCount: undefined }
                : message,
            ),
          }));
        }
        const control = await sendMailboxControl({
          profileId: profile.id,
          password,
          sealed: wallet.sealed,
          public: wallet.public,
          destination: result.control.destination,
          payloadsHex: result.control.payloads_hex,
          completesPendingId: result.control.completes_pending_id,
          wrpcEndpoint: wallet.wrpcEndpoint,
        });
        protocolDebug("info", "handshake", "control-submit-complete", {
          profileId: profile.id,
          control: "handshake",
          transactionId: control.transaction_id,
          completesPendingId: result.control.completes_pending_id ?? null,
        });
        wallet = { ...wallet, public: control.public };
        if (result.control.completes_pending_id) {
          justCompletedMessageId = findMessageByPendingId(chats, result.control.completes_pending_id)?.id;
          chats = completePendingMessage(
            chats,
            result.control.completes_pending_id,
            control.transaction_id,
          );
        }
      }

      if (result.delivery_ack && result.delivery_ack_peer) {
        chats = markMessageDelivered(profile, chats, result.delivery_ack, result.delivery_ack_peer);
      }

      if (result.session_ended) {
        const ended = result.session_ended;
        chats = chats.map(chat => {
          if (chat.peerHydraHandle !== ended.peer_hydra_id) return chat;
          if (chat.sessionSid && chat.sessionSid !== ended.sid) return chat;
          if (!chat.sessionSid && (chat.left || !chat.bootstrapComplete || chat.messages.length === 0)) return chat;
          return {
            ...chat,
            sessionSid: chat.sessionSid ?? ended.sid,
            peerKaspaAddress: chat.peerKaspaAddress ?? ended.peer_address,
            peerLeft: true,
            bootstrapComplete: false,
            incomingRequest: undefined,
            messages: chat.messages.map(message =>
              message.direction === "out" && message.pending
                ? {
                    ...message,
                    pending: false,
                    pendingId: undefined,
                    pendingStage: undefined,
                    retryAfter: undefined,
                    retryCount: undefined,
                    sendState: "failed",
                    sendError: "The other participant ended this KKTP chat session.",
                  }
                : message,
            ),
          };
        });
      }

      if (result.received) {
        const wireId = result.message_id;
        if (!validWireId(wireId)) {
          throw new Error("Authenticated HYDRA message is missing its Ghost Talk delivery id.");
        }
        const body = decodeMessageBody(result.received.plaintext);
        const realtimeControl = Boolean(decodeLiveVoicePacket(body) || decodeDirectRouteSignal(body));
        if (realtimeControl) {
          const sessionSid = result.received.session_sid;
          if (!sessionSid || !validWireId(sessionSid)) {
            protocolDebug("warn", "voice", "realtime-control-without-session-discarded", {
              profileId: profile.id,
              peerHydraId: result.received.from,
              messageId: wireId,
              transactionId: envelope.transactionId,
            });
          } else {
            onRealtimeControl?.(result.received.from, sessionSid, body);
          }
        } else {
          const sessionSid = result.received.session_sid;
          if (!validWireId(sessionSid)) {
            protocolDebug("warn", "mailbox", "legacy-message-without-session-discarded", {
              profileId: profile.id,
              peerHydraId: result.received.from,
              messageId: wireId,
              transactionId: envelope.transactionId,
            });
          } else {
            chats = appendIncomingMessage(
              profile,
              chats,
              result.received.from,
              sessionSid,
              wireId,
              body,
              envelope.transactionId,
              envelope.blockTime,
              result.peer_address,
              result.peer_label,
            );
          }
        }
        // Base KKTP is one durable Kaspa carrier per active-session message.
        // Do not create a second on-chain ACK transaction for ordinary text or
        // recorded voice messages. The only signed ACK retained by Ghost Talk is
        // the pq_finish bootstrap proof handled below.
      }

      // pq_finish is special: its signed ACK proves the responder established the
      // ratchet and observed the embedded first message. Replays return the same
      // authenticated message id, so this ACK can be re-sent without creating a
      // new FINISH or a second logical message.
      if (result.session_established_peer && validWireId(result.message_id)) {
        const destination = result.peer_address
          ?? peerAddressForHydra(profile, chats, result.session_established_peer);
        if (destination) {
          wallet = queueDeliveryAck(
            wallet,
            result.message_id!,
            destination,
            result.session_established_peer,
          );
        }
      }

      if (result.session_established_peer) {
        protocolDebug("info", "handshake", "session-active-in-ui", {
          profileId: profile.id,
          peerHydraId: result.session_established_peer,
          messageId: result.message_id ?? null,
        });
        chats = chats.map(chat =>
          !chat.archived && !chat.left && !chat.peerLeft && chat.peerHydraHandle === result.session_established_peer
            ? {
                ...chat,
                bootstrapComplete: true,
                incomingRequest: chat.incomingRequest?.state === "accepted"
                  ? undefined
                  : chat.incomingRequest,
              }
            : chat
        );
        const resent = await resendAwaitingForPeer(
          profile,
          chats,
          wallet,
          password,
          result.session_established_peer,
          justCompletedMessageId,
        );
        chats = resent.chats;
        wallet = resent.wallet;
      }

      if (result.discard && !retainEnvelopeForPairing) {
        wallet = removeEnvelope(wallet, envelope.packetId);
      }
    } catch (error) {
      // Keep an unconsumed authenticated carrier packet queued. In particular,
      // a stale ciphertext remains durable while a restart-safe replacement
      // handshake is in flight. Successfully decrypted packets are marked
      // discard by native HYDRA and are never intentionally replayed. Do not hide
      // protocol failures: the Windows dev console now identifies the packet that
      // blocked progress instead of leaving the UI stuck with no diagnostic.
      protocolDebug("error", "mailbox", "envelope-processing-failed", {
        profileId: profile.id,
        packetId: envelope.packetId,
        transactionId: envelope.transactionId,
        error: String(error),
      });
      console.error("Ghost Talk mailbox packet processing failed", envelope.packetId, error);
    }
  }

  const retried = await retryScheduledMessages(profile, chats, wallet, password);
  chats = retried.chats;
  wallet = retried.wallet;
  wallet = await flushDeliveryAcks(profile, wallet, password);
  return { ...profile, contacts, chats, wallet };
}

function findMessageByPendingId(chats: Profile["chats"], pendingId: string): Message | undefined {
  for (const chat of chats) {
    const message = chat.messages.find(candidate => candidate.pendingId === pendingId);
    if (message) return message;
  }
  return undefined;
}

function completePendingMessage(
  chats: Profile["chats"],
  pendingId: string,
  transactionId: string,
): Profile["chats"] {
  return chats.map(chat => ({
    ...chat,
    messages: chat.messages.map(message =>
      message.pendingId === pendingId
        ? {
            ...message,
            pending: false,
            pendingId: undefined,
            // pq_finish contains the first durable message. Once Kaspa accepts
            // that carrier, delivery is complete. Native may retain/replay the
            // exact FINISH until its bootstrap ACK without changing message state.
            pendingStage: undefined,
            txid: transactionId,
            retryCount: undefined,
            retryAfter: undefined,
            sendState: "delivered",
            sendError: undefined,
          }
        : message,
    ),
  }));
}

function markMessageDelivered(
  profile: Profile,
  chats: Profile["chats"],
  wireId: string,
  acknowledgementPeer: string,
): Profile["chats"] {
  if (!validWireId(wireId) || !/^[0-9a-f]{64}$/i.test(acknowledgementPeer)) return chats;
  return chats.map(chat => {
    const route = routeForChat(profile, chat);
    if (!route || route.hydraHandle !== acknowledgementPeer) return chat;
    return {
      ...chat,
      messages: chat.messages.map(message =>
        message.direction === "out" && message.wireId === wireId
          ? { ...message, pending: false, pendingId: undefined, pendingStage: undefined, retryAfter: undefined, retryCount: undefined, sendState: "delivered", sendError: undefined }
          : message,
      ),
    };
  });
}

function appendIncomingMessage(
  profile: Profile,
  chats: Profile["chats"],
  from: string,
  sessionSid: string,
  wireId: string,
  body: string,
  txid: string,
  blockTime?: number,
  peerAddress?: string,
  peerLabel?: string,
): Profile["chats"] {
  const contact = profile.contacts.find(candidate => candidate.hydraHandle === from);
  const thread = chats.find(chat => {
    if (chat.archived || chat.left || chat.peerLeft || chat.sessionSid !== sessionSid) return false;
    if (contact && chat.contactId === contact.id) return true;
    return chat.peerHydraHandle === from
      || Boolean(peerAddress && chat.peerKaspaAddress === peerAddress);
  });
  if (!thread) {
    // A valid ciphertext for another SID is historical/replayed traffic. Never
    // create or reuse a UI thread based only on peer identity/address.
    protocolDebug("warn", "mailbox", "message-without-active-session-discarded", {
      peerHydraId: from,
      sessionSid,
      messageId: wireId,
      transactionId: txid,
      peerLabel: peerLabel ?? null,
    });
    return chats;
  }
  if (thread.messages.some(message => message.wireId === wireId || message.txid === txid)) return chats;
  const message: Message = {
    id: `in:${wireId}`,
    wireId,
    sessionSid,
    direction: "in",
    body,
    createdAt: blockTime ?? Date.now(),
    txid,
  };
  return chats.map(chat =>
    chat.id === thread.id
      ? { ...chat, bootstrapComplete: true, incomingRequest: undefined, messages: [...chat.messages, message] }
      : chat,
  );
}

function collapseDuplicateRequestThreads(
  chats: Profile["chats"],
  requestId?: string,
  preferredId?: string,
): Profile["chats"] {
  const matches = chats.filter(chat => {
    const id = chat.incomingRequest?.requestId;
    return Boolean(id && (!requestId || id === requestId));
  });
  if (matches.length < 2) return chats;

  const grouped = new Map<string, Profile["chats"]>();
  for (const chat of matches) {
    const id = chat.incomingRequest!.requestId;
    const list = grouped.get(id) ?? [];
    list.push(chat);
    grouped.set(id, list);
  }
  let output = [...chats];
  for (const [id, duplicates] of grouped) {
    if (duplicates.length < 2) continue;
    const preferred = duplicates.find(chat => chat.id === preferredId) ?? duplicates[0];
    const rank = { pending: 0, ignored: 1, accepted: 2 } as const;
    const strongest = duplicates
      .map(chat => chat.incomingRequest!)
      .sort((left, right) => rank[right.state] - rank[left.state])[0];
    const messages = [...preferred.messages];
    const seenMessages = new Set(messages.map(message => message.id));
    for (const duplicate of duplicates) {
      for (const message of duplicate.messages) {
        if (!seenMessages.has(message.id)) {
          messages.push(message);
          seenMessages.add(message.id);
        }
      }
    }
    const merged: Profile["chats"][number] = {
      ...preferred,
      contactId: preferred.contactId ?? duplicates.find(chat => chat.contactId)?.contactId,
      label: preferred.label || duplicates.find(chat => chat.label)?.label || strongest.peerAddress,
      peerKaspaAddress: preferred.peerKaspaAddress ?? duplicates.find(chat => chat.peerKaspaAddress)?.peerKaspaAddress,
      peerHydraHandle: preferred.peerHydraHandle ?? duplicates.find(chat => chat.peerHydraHandle)?.peerHydraHandle,
      verifiedPublic: duplicates.some(chat => Boolean(chat.verifiedPublic)),
      bootstrapComplete: duplicates.some(chat => Boolean(chat.bootstrapComplete)),
      archived: duplicates.some(chat => Boolean(chat.archived)),
      left: duplicates.some(chat => Boolean(chat.left)),
      peerLeft: duplicates.some(chat => Boolean(chat.peerLeft)),
      sessionSid: preferred.sessionSid ?? id,
      incomingRequest: { ...strongest },
      messages,
    };
    const duplicateIds = new Set(duplicates.map(chat => chat.id));
    output = output
      .filter(chat => !duplicateIds.has(chat.id) || chat.id === preferred.id)
      .map(chat => chat.id === preferred.id ? merged : chat);
  }
  return output;
}

function upsertIncomingRequest(
  chats: Profile["chats"],
  contact: Contact | undefined,
  incoming: { request_id: string; peer_address: string; local_address: string; peer_label: string; peer_hydra_id: string; signed_request_hex: string },
  autoIgnoreUnknown: boolean,
): Profile["chats"] {
  chats = collapseDuplicateRequestThreads(chats, incoming.request_id);

  // The SID is the conversation identity. A historical Discovery replay for an
  // already-known SID may refresh an *existing pending request* only; it may not
  // resurrect an archived/left/active session or manufacture a second request.
  const sameSession = chats.find(chat =>
    chat.sessionSid === incoming.request_id
    || chat.incomingRequest?.requestId === incoming.request_id
  );
  if (sameSession) {
    if (
      sameSession.archived
      || sameSession.left
      || sameSession.peerLeft
      || sameSession.incomingRequest?.requestId !== incoming.request_id
    ) {
      return chats;
    }
    return chats.map(chat => chat.id === sameSession.id ? {
      ...chat,
      label: contact?.label || incoming.peer_label || chat.label || incoming.peer_address.slice(0, 18),
      contactId: contact?.id ?? chat.contactId,
      peerKaspaAddress: incoming.peer_address,
      peerHydraHandle: incoming.peer_hydra_id,
      sessionSid: incoming.request_id,
      incomingRequest: {
        requestId: incoming.request_id,
        peerHydraId: incoming.peer_hydra_id,
        peerAddress: incoming.peer_address,
        localAddress: incoming.local_address,
        signedRequestHex: incoming.signed_request_hex,
        state: chat.incomingRequest!.state,
      },
    } : chat);
  }

  // A different SID from the same peer is never merged into a live conversation.
  // This is the critical history-replay boundary: old on-chain GTCR records can
  // be observed forever, but they cannot replace the active thread's SID.
  const activePeerSession = chats.find(chat =>
    !chat.archived
    && !chat.left
    && !chat.peerLeft
    && (
      chat.peerHydraHandle === incoming.peer_hydra_id
      || chat.peerKaspaAddress === incoming.peer_address
      || (contact && chat.contactId === contact.id)
    )
  );
  if (activePeerSession) return chats;

  return [...chats, {
    id: crypto.randomUUID(),
    contactId: contact?.id,
    label: contact?.label || incoming.peer_label || `Unknown ${incoming.peer_address.slice(0, 12)}`,
    messages: [],
    peerKaspaAddress: incoming.peer_address,
    peerHydraHandle: incoming.peer_hydra_id,
    verifiedPublic: contact?.verifiedPublic ?? false,
    bootstrapComplete: false,
    sessionSid: incoming.request_id,
    peerLeft: false,
    incomingRequest: {
      requestId: incoming.request_id,
      peerHydraId: incoming.peer_hydra_id,
      peerAddress: incoming.peer_address,
      localAddress: incoming.local_address,
      signedRequestHex: incoming.signed_request_hex,
      state: autoIgnoreUnknown && !contact ? "ignored" : "pending",
    },
  }];
}

function findQueuedContactRequest(chats: Profile["chats"], requestId: string) {
  // The signed GTCR request id is the authoritative join key between the
  // Response anchor and the sender's first queued message. Do not require
  // transient UI flags such as pendingStage to survive a restart/rebuild.
  // r48 could receive and authenticate a valid Response and then silently
  // discard it when those local presentation flags were stale, leaving the
  // sender forever on “Waiting for recipient to accept secure chat…”.
  for (const chat of chats) {
    const message = chat.messages.find(item =>
      item.direction === "out"
      && item.contactRequestId === requestId
      && validWireId(item.wireId)
      && item.sendState !== "delivered"
    );
    if (message) return { chat, message };
  }

  // Recovery fallback: sessionSid is durably committed with the outgoing GTCR.
  // If an older snapshot lost contactRequestId but retained the session SID,
  // recover the earliest non-delivered outgoing first message instead of
  // throwing away a cryptographically valid Response anchor.
  for (const chat of chats) {
    if (chat.sessionSid !== requestId || chat.bootstrapComplete || chat.archived || chat.left) continue;
    const message = chat.messages.find(item =>
      item.direction === "out"
      && validWireId(item.wireId)
      && item.sendState !== "delivered"
    );
    if (message) return { chat, message };
  }
  return undefined;
}

function queueDeliveryAck(
  wallet: WalletRecord,
  messageId: string,
  destination: string,
  destinationHydraId: string,
): WalletRecord {
  return {
    ...wallet,
    deliveryAcksPending: {
      ...(wallet.deliveryAcksPending ?? {}),
      [messageId]: { purpose: "handshake", destination, destinationHydraId, createdAt: Date.now() },
    },
  };
}

async function flushDeliveryAcks(
  profile: Profile,
  initialWallet: WalletRecord,
  password: string,
): Promise<WalletRecord> {
  if (!profile.hydraIdentityId) return initialWallet;
  let wallet = initialWallet;
  const pending = Object.entries(wallet.deliveryAcksPending ?? {})
    .sort(([, a], [, b]) => a.createdAt - b.createdAt)
    .slice(0, 4);
  for (const [messageId, ack] of pending) {
    try {
      const result = await sendMailboxDeliveryAck({
        profileId: profile.id,
        password,
        identityId: profile.hydraIdentityId,
        sealed: wallet.sealed,
        public: wallet.public,
        destination: ack.destination,
        destinationHydraId: ack.destinationHydraId,
        messageId,
        wrpcEndpoint: wallet.wrpcEndpoint,
      });
      const remaining = { ...(wallet.deliveryAcksPending ?? {}) };
      delete remaining[messageId];
      wallet = {
        ...wallet,
        public: result.public,
        deliveryAcksPending: Object.keys(remaining).length ? remaining : undefined,
      };
    } catch {
      // The pq_finish handshake acknowledgement is a durable local obligation.
      // A network outage or app restart leaves it queued for the next mailbox pass.
      break;
    }
  }
  return wallet;
}

function pendingRetryDelayMs(message: Message): number {
  const attempt = Math.max(0, message.retryCount ?? 0);
  // This timer is only for an actual outbound command failure. Successfully
  // submitted Kaspa bootstrap carriers are never timer-rebroadcast.
  const base = message.pendingStage === "request"
    ? 2_000
    : message.pendingStage === "handshake" || message.pendingStage === "finish"
      ? 1_000
      : 1_500;
  return Math.min(base * (2 ** Math.min(attempt, 3)), 8_000);
}

async function resendAwaitingForPeer(
  profile: Profile,
  initialChats: Profile["chats"],
  initialWallet: WalletRecord,
  password: string,
  peerHydraId: string,
  skipMessageId?: string,
): Promise<{ chats: Profile["chats"]; wallet: WalletRecord }> {
  let chats = initialChats;
  let wallet = initialWallet;
  for (const chat of chats) {
    const route = routeForChat(profile, chat);
    if (!route || route.hydraHandle !== peerHydraId) continue;
    for (const message of chat.messages) {
      if (
        message.direction !== "out"
        || !message.pending
        || message.pendingStage === "request"
        || message.id === skipMessageId
        || !validWireId(message.wireId)
      ) continue;
      try {
        const result = await sendMailboxMessage({
          profileId: profile.id,
          password,
          identityId: profile.hydraIdentityId!,
          senderDisplayName: profile.label,
          contactId: route.hydraHandle,
          destination: route.kaspaAddress,
          body: message.body,
          messageId: message.wireId,
          stegoProfile: decodeVoiceMessage(message.body) ? "Off" : profile.settings.stego,
          sealed: wallet.sealed,
          public: wallet.public,
          wrpcEndpoint: wallet.wrpcEndpoint,
        });
        wallet = { ...wallet, public: result.public };
        chats = updateMessage(chats, chat.id, message.id, current => ({
          ...current,
          txid: result.pending_handshake ? undefined : result.transaction_id,
          pending: result.pending_handshake,
          pendingId: result.pending_id,
          pendingStage: result.pending_handshake ? "handshake" : undefined,
          retryCount: undefined,
          retryAfter: undefined,
          sendState: result.pending_handshake ? "sent" : "delivered",
          sendError: undefined,
        }));
      } catch (error) {
        chats = updateMessage(chats, chat.id, message.id, current => ({
          ...current,
          retryCount: (current.retryCount ?? 0) + 1,
          retryAfter: Date.now() + pendingRetryDelayMs({ ...current, retryCount: (current.retryCount ?? 0) + 1 }),
          sendError: String(error),
        }));
      }
    }
  }
  return { chats, wallet };
}

async function retryScheduledMessages(
  profile: Profile,
  initialChats: Profile["chats"],
  initialWallet: WalletRecord,
  password: string,
): Promise<{ chats: Profile["chats"]; wallet: WalletRecord }> {
  if (!profile.hydraIdentityId) return { chats: initialChats, wallet: initialWallet };
  let chats = initialChats;
  let wallet = initialWallet;
  const now = Date.now();
  for (const chat of chats) {
    const message = chat.messages.find(item =>
      item.direction === "out"
      && item.pending
      && validWireId(item.wireId)
      && typeof item.retryAfter === "number"
      && item.retryAfter <= now
    );
    if (!message) continue;

    // A Discovery/GTCR is a consent/bootstrap carrier, not a HYDRA message.
    // This branch is only for a recorded local send failure/restart ambiguity;
    // a successfully submitted Discovery is never timer-rebroadcast.
    if (message.pendingStage === "request") {
      const contact = chat.contactId
        ? profile.contacts.find(candidate => candidate.id === chat.contactId)
        : undefined;
      const destination = contact?.kaspaAddress ?? chat.peerKaspaAddress;
      if (!destination || !validWireId(message.contactRequestId)) continue;
      try {
        const result = await sendMailboxContactRequest({
          profileId: profile.id,
          password,
          identityId: profile.hydraIdentityId,
          senderDisplayName: profile.label,
          sealed: wallet.sealed,
          public: wallet.public,
          destination,
          requestId: message.contactRequestId,
          wrpcEndpoint: wallet.wrpcEndpoint,
        });
        wallet = { ...wallet, public: result.public };
        chats = updateMessage(chats, chat.id, message.id, current => {
          return {
            ...current,
            txid: result.transaction_id,
            retryCount: undefined,
            retryAfter: undefined,
            sendError: undefined,
            sendState: "sent",
          };
        });
      } catch (error) {
        chats = updateMessage(chats, chat.id, message.id, current => {
          const retryCount = (current.retryCount ?? 0) + 1;
          return {
            ...current,
            retryCount,
            retryAfter: Date.now() + pendingRetryDelayMs({ ...current, retryCount }),
            sendError: String(error),
          };
        });
      }
      break;
    }

    const route = routeForChat(profile, chat);
    if (!route) continue;

    if (message.pendingStage === "finish") {
      try {
        const result = await retryMailboxHandshakeFinish({
          profileId: profile.id,
          password,
          identityId: profile.hydraIdentityId,
          contactId: route.hydraHandle,
          messageId: message.wireId!,
          sealed: wallet.sealed,
          public: wallet.public,
          wrpcEndpoint: wallet.wrpcEndpoint,
        });
        wallet = { ...wallet, public: result.public };
        chats = updateMessage(chats, chat.id, message.id, current => ({
          ...current,
          txid: result.transaction_id,
          pending: false,
          pendingId: undefined,
          pendingStage: undefined,
          retryCount: undefined,
          retryAfter: undefined,
          sendError: undefined,
          sendState: "delivered",
        }));
      } catch (error) {
        chats = updateMessage(chats, chat.id, message.id, current => {
          const retryCount = (current.retryCount ?? 0) + 1;
          return {
            ...current,
            retryCount,
            retryAfter: Date.now() + pendingRetryDelayMs({ ...current, retryCount }),
            sendError: String(error),
          };
        });
      }
      break;
    }

    try {
      const result = await sendMailboxMessage({
        profileId: profile.id,
        password,
        identityId: profile.hydraIdentityId,
        senderDisplayName: profile.label,
        contactId: route.hydraHandle,
        destination: route.kaspaAddress,
        body: message.body,
        messageId: message.wireId!,
        stegoProfile: decodeVoiceMessage(message.body) ? "Off" : profile.settings.stego,
        sealed: wallet.sealed,
        public: wallet.public,
        wrpcEndpoint: wallet.wrpcEndpoint,
      });
      wallet = { ...wallet, public: result.public };
      chats = updateMessage(chats, chat.id, message.id, current => ({
        ...current,
        txid: result.pending_handshake ? undefined : result.transaction_id,
        pending: result.pending_handshake,
        pendingId: result.pending_id,
        pendingStage: result.pending_handshake ? "handshake" : undefined,
        retryCount: undefined,
        retryAfter: undefined,
        sendError: undefined,
        sendState: result.pending_handshake ? "sent" : "delivered",
      }));
    } catch (error) {
      chats = updateMessage(chats, chat.id, message.id, current => {
        const retryCount = (current.retryCount ?? 0) + 1;
        return {
          ...current,
          retryCount,
          retryAfter: Date.now() + pendingRetryDelayMs({ ...current, retryCount }),
          sendError: String(error),
        };
      });
    }
    break;
  }
  return { chats, wallet };
}

function updateMessage(
  chats: Profile["chats"],
  chatId: string,
  messageId: string,
  update: (message: Message) => Message,
): Profile["chats"] {
  return chats.map(chat => chat.id === chatId
    ? { ...chat, messages: chat.messages.map(message => message.id === messageId ? update(message) : message) }
    : chat);
}

function routeForChat(profile: Profile, chat: Profile["chats"][number]) {
  const contact = chat.contactId
    ? profile.contacts.find(candidate => candidate.id === chat.contactId)
    : undefined;
  const hydraHandle = contact?.hydraHandle ?? chat.peerHydraHandle;
  const kaspaAddress = contact?.kaspaAddress ?? chat.peerKaspaAddress;
  if (!hydraHandle || !kaspaAddress) return undefined;
  return { hydraHandle, kaspaAddress, displayName: contact?.label ?? chat.label };
}

function peerAddressForHydra(
  profile: Profile,
  chats: Profile["chats"],
  hydraHandle: string,
): string | undefined {
  const contact = profile.contacts.find(candidate => candidate.hydraHandle === hydraHandle);
  if (contact) return contact.kaspaAddress;
  return chats.find(chat => chat.peerHydraHandle === hydraHandle)?.peerKaspaAddress;
}

function peerRouteRegistrations(profile: Profile) {
  const routes = new Map<string, { contactId: string; kaspaAddress: string; displayName: string; sessionSid?: string }>();
  for (const contact of profile.contacts) {
    if (contact.hydraHandle && contact.kaspaAddress) {
      routes.set(contact.hydraHandle, {
        contactId: contact.hydraHandle,
        kaspaAddress: contact.kaspaAddress,
        displayName: contact.label,
      });
    }
  }
  // Only a live/pending thread may supply the persisted SID authority. Archived,
  // left and peer-ended sessions remain history and cannot re-arm native replay.
  for (const chat of profile.chats) {
    const route = routeForChat(profile, chat);
    if (!route) continue;
    const sessionSid = !chat.archived && !chat.left && !chat.peerLeft && validWireId(chat.sessionSid)
      ? chat.sessionSid
      : undefined;
    const existing = routes.get(route.hydraHandle);
    if (existing?.sessionSid && !sessionSid) continue;
    routes.set(route.hydraHandle, {
      contactId: route.hydraHandle,
      kaspaAddress: route.kaspaAddress,
      displayName: route.displayName,
      sessionSid,
    });
  }
  return [...routes.values()];
}

function activeConversationSids(chats: Profile["chats"]): string[] {
  return chats
    .filter(chat => !chat.archived && !chat.left && !chat.peerLeft && validWireId(chat.sessionSid))
    .map(chat => chat.sessionSid!);
}

function newWireId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
}

function validWireId(value: string | undefined): value is string {
  return typeof value === "string" && /^[0-9a-f]{32}$/i.test(value);
}

function decodeMessageBody(plaintext: string): string {
  try {
    const event = JSON.parse(plaintext) as { kind?: { Text?: { body?: unknown } } };
    const body = event.kind?.Text?.body;
    return typeof body === "string" ? body : plaintext;
  } catch {
    return plaintext;
  }
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
