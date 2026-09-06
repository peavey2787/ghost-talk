import { useEffect, useState } from "react";
import type { Profile, PublicGhostProfile, WalletRecovery } from "../model";
import {
  publishProfileBackup,
  restoreProfileBackup,
  revealWalletRecovery,
  setRememberedUnlock,
  unlockWallet,
} from "../native";
import {
  backupContacts,
  backupMessages,
  mergeRestoredArchive,
  profileBackupFingerprint,
} from "../profileBackup";

export function ContactsView({
  profile,
  unlocked,
  onAddTarget,
  prefillTarget,
  onPrefillConsumed,
}: {
  profile: Profile;
  unlocked: boolean;
  onAddTarget: (label: string, target: string) => Promise<void>;
  prefillTarget?: string;
  onPrefillConsumed?: () => void;
}) {
  const [label, setLabel] = useState("");
  const [target, setTarget] = useState("");
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  useEffect(() => {
    if (!prefillTarget) return;
    setTarget(prefillTarget);
    setStatus("Kaspa address copied from the direct chat. Add a name if you want, then save the contact.");
    onPrefillConsumed?.();
  }, [prefillTarget, onPrefillConsumed]);

  async function add(): Promise<void> {
    if (!target.trim() || busy) return;
    setBusy(true);
    setStatus("Resolving Kaspa destination…");
    try {
      await onAddTarget(label.trim(), target.trim());
      setLabel("");
      setTarget("");
      setStatus("Contact added. If this person is unlisted, their HYDRA public identity will be exchanged privately when you first message them.");
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="page">
      <div className="page-head">
        <div><h2>Contacts</h2><p>Save people by Kaspa address or KNS. Publishing a public Ghost Talk profile is optional.</p></div>
      </div>
      <form
        className="card form-grid contact-add-card"
        onSubmit={event => { event.preventDefault(); void add(); }}
      >
        <h3>Add contact</h3>
        <label>Name (optional)<input value={label} onChange={event => setLabel(event.target.value)} placeholder="Uses verified public name when available" /></label>
        <label>Kaspa address or KNS<input value={target} onChange={event => setTarget(event.target.value)} placeholder="kaspa:… or alice.kas" /></label>
        <small>
          A public Ghost Talk profile is not required. If one exists and verifies, Ghost Talk imports its HYDRA public identity immediately and shows a verified mark. Otherwise the first message uses a private Kaspa request/accept bootstrap.
        </small>
        <button className="primary" disabled={busy || !unlocked || !target.trim()}>
          {busy ? "Resolving address…" : "Add contact"}
        </button>
        {!unlocked && <small>Unlock wallet + mailbox first.</small>}
      </form>
      {status && <div className="status contact-status">{status}</div>}
      <div className="card-list">
        {profile.contacts.length === 0 ? (
          <div className="card muted">No contacts yet. You can still start a one-off chat from Chats without saving the person here.</div>
        ) : profile.contacts.map(contact => (
          <div className="card row-card" key={contact.id}>
            <span className="avatar">{contact.label[0]?.toUpperCase()}</span>
            <div>
              <b>{contact.label}{contact.verifiedPublic && <span className="verified-mark" title="Latest public Ghost Talk profile verified against its Kaspa owner">✓</span>}</b>
              <small>{contact.knsName ?? contact.kaspaAddress}</small>
              {contact.knsName && <small>{contact.kaspaAddress}</small>}
              {!contact.verifiedPublic && <small>Private / unlisted peer</small>}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

export function DiscoverView({
  profile,
  unlocked,
  onUpdate,
  onPublishDescriptor,
  onLookup,
  publicUsers,
  onStartChat,
}: {
  profile: Profile;
  unlocked: boolean;
  onUpdate: (profile: Profile) => void;
  onPublishDescriptor: (discoverable: boolean) => Promise<string>;
  onLookup: (target: string) => Promise<PublicGhostProfile | null>;
  publicUsers: PublicGhostProfile[];
  onStartChat: (target: string) => Promise<string>;
}) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<PublicGhostProfile | null>();
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const ghostAddress = profile.wallet?.public.receive_addresses[0] ?? "";

  async function publish(discoverable: boolean): Promise<void> {
    if (busy) return;
    setBusy(true);
    setStatus(discoverable ? "Publishing your latest public Ghost Talk profile…" : "Publishing an unlisted profile update…");
    try {
      setStatus(await onPublishDescriptor(discoverable));
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function lookup(): Promise<void> {
    const target = query.trim();
    if (!target || busy) return;
    setBusy(true);
    setStatus("Checking the latest owner-signed Ghost Talk profile…");
    try {
      const found = await onLookup(target);
      setResult(found);
      setStatus(found ? "Latest public profile verified." : "No current public Ghost Talk profile was found. You can still start a private chat to this address/KNS from Chats.");
    } catch (error) {
      setResult(null);
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="page">
      <div className="page-head">
        <div>
          <h2>Discover</h2>
          <p>Public Ghost Talk profiles are optional. Private/unlisted users can still send and receive chats over Kaspa.</p>
        </div>
      </div>

      <div className="card form-grid discover-profile-card">
        <h3>Your public profile</h3>
        {ghostAddress && <><small>Stable Ghost Talk address</small><code className="address-code">{ghostAddress}</code></>}
        <label>Public username<input value={profile.settings.publicUsername} maxLength={48} onChange={event => onUpdate({ ...profile, settings: { ...profile.settings, publicUsername: event.target.value } })} placeholder="Optional username" /></label>
        <label>Description<textarea rows={3} value={profile.settings.publicDescription} maxLength={320} onChange={event => onUpdate({ ...profile, settings: { ...profile.settings, publicDescription: event.target.value } })} placeholder="Optional short description" /></label>
        <label>Interests<input value={profile.settings.publicInterests} onChange={event => onUpdate({ ...profile, settings: { ...profile.settings, publicInterests: event.target.value } })} placeholder="music, kaspa, games" /></label>
        <small>
          Publishing writes an owner-signed self-transaction to this stable address and pays a normal Kaspa network fee. The newest valid profile transaction controls your current listing. Old blockchain records are immutable, so “Make private / unlist” publishes a newer opt-out record rather than deleting history.
        </small>
        <div className="button-row">
          <button className="primary" disabled={!unlocked || busy || !ghostAddress} onClick={() => void publish(true)}>Publish / update public profile</button>
          <button disabled={!unlocked || busy || !ghostAddress} onClick={() => void publish(false)}>Make private / unlist</button>
        </div>
        {!unlocked && <small>Unlock wallet + mailbox before publishing profile changes.</small>}
      </div>

      <div className="card form-grid discover-search-card">
        <h3>Find a public user</h3>
        <label>Kaspa address or KNS<input value={query} onChange={event => setQuery(event.target.value)} placeholder="kaspa:… or alice.kas" onKeyDown={event => { if (event.key === "Enter") { event.preventDefault(); void lookup(); } }} /></label>
        <button className="primary" disabled={busy || !query.trim()} onClick={() => void lookup()}>{busy ? "Searching…" : "Find latest profile"}</button>
      </div>

      {status && <div className="status discover-status">{status}</div>}
      {result && (
        <div className="card public-profile-result">
          <div className="public-profile-title">
            <div>
              <h3>{result.username || result.display_name || result.kns_name || "Ghost Talk user"}<span className="verified-mark" title="Latest profile is signed by this Kaspa address">✓</span></h3>
              <small>Verified public Ghost Talk profile</small>
            </div>
            <button className="primary" disabled={!unlocked} onClick={() => void onStartChat(result.kns_name || result.kaspa_address)}>Start chat</button>
          </div>
          <code className="address-code">{result.kaspa_address}</code>
          {result.kns_name && <p><b>KNS:</b> {result.kns_name}</p>}
          {result.description && <p>{result.description}</p>}
          {result.interests.length > 0 && <div className="interest-list">{result.interests.map(value => <span key={value}>{value}</span>)}</div>}
          <small>Descriptor accepting blue score: {result.descriptor_blue_score}</small>
        </div>
      )}

      <div className="card discover-public-list">
        <div className="public-profile-title">
          <div>
            <h3>Top public users</h3>
            <small>Up to 100 of the newest owner-verified public profiles this client has indexed. Newer profile updates replace older ones automatically.</small>
          </div>
          <span className="directory-count">{Math.min(publicUsers.length, 100)}</span>
        </div>
        {publicUsers.length === 0 ? (
          <p className="muted">No public profiles have been observed by this client yet. The directory fills from accepted Kaspa GTCD transactions while Ghost Talk is connected, and from profiles you look up directly.</p>
        ) : (
          <div className="public-user-list">
            {publicUsers.slice(0, 100).map(user => (
              <div className="public-user-row" key={`${user.kaspa_address}:${user.descriptor_blue_score}`}>
                <div>
                  <b>{user.username || user.display_name || user.kns_name || `${user.kaspa_address.slice(0, 18)}…`}<span className="verified-mark" title="Latest observed profile is signed by this Kaspa address">✓</span></b>
                  <small>{user.kns_name || user.kaspa_address}</small>
                  {user.description && <small>{user.description}</small>}
                  {user.interests.length > 0 && <div className="interest-list compact">{user.interests.slice(0, 6).map(value => <span key={value}>{value}</span>)}</div>}
                </div>
                <button className="primary" disabled={!unlocked} onClick={() => void onStartChat(user.kns_name || user.kaspa_address)}>Start chat</button>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

export function RoomsView() {
  const [rooms, setRooms] = useState<string[]>([]);
  const [name, setName] = useState("");
  return (
    <section className="page">
      <div className="page-head"><div><h2>Rooms</h2><p>Create local room shells; membership and policy are applied when invites are exchanged.</p></div></div>
      <form
        className="card inline-form"
        onSubmit={event => {
          event.preventDefault();
          if (name.trim()) {
            setRooms(value => [...value, name.trim()]);
            setName("");
          }
        }}
      >
        <input value={name} onChange={event => setName(event.target.value)} placeholder="Room name" />
        <button className="primary">Create room</button>
      </form>
      {rooms.length ? (
        <div className="card-list">
          {rooms.map(room => <div className="card" key={room}><b>{room}</b><small>Local · no members yet</small></div>)}
        </div>
      ) : <div className="card muted">No rooms yet.</div>}
    </section>
  );
}

export function GamesView() {
  const [running, setRunning] = useState(false);
  return (
    <section className="page">
      <div className="page-head"><div><h2>Games</h2><p>Deterministic Ghost Talk game sessions use the authenticated session channel.</p></div></div>
      <div className="card game-card">
        <div><b>Pong</b><small>{running ? "Practice session running" : "Ready"}</small></div>
        <button className={running ? "on" : "primary"} onClick={() => setRunning(value => !value)}>{running ? "Stop" : "Start practice"}</button>
      </div>
    </section>
  );
}

interface SettingsProps {
  profile: Profile;
  profileCount: number;
  onUpdate: (profile: Profile) => void;
  onPersistUpdate: (profile: Profile) => Promise<void>;
  onUnlock: (password: string) => Promise<void>;
  onOpenDebug: () => void;
}

export function SettingsView({ profile, profileCount, onUpdate, onPersistUpdate, onUnlock, onOpenDebug }: SettingsProps) {
  const wallet = profile.wallet;
  const [password, setPassword] = useState("");
  const [walletRecovery, setWalletRecovery] = useState<WalletRecovery>();
  const [status, setStatus] = useState("");
  const [securityPassword, setSecurityPassword] = useState("");
  const [securityStatus, setSecurityStatus] = useState("");
  const [securityBusy, setSecurityBusy] = useState(false);

  useEffect(() => {
    setPassword("");
    setWalletRecovery(undefined);
    setStatus("");
    setSecurityPassword("");
    setSecurityStatus("");
    setSecurityBusy(false);
  }, [profile.id]);

  async function showRecovery(): Promise<void> {
    if (!wallet || walletRecovery || password.length < 8) return;
    const unlock = password;
    setStatus("Unlocking Ghost Talk recovery data…");
    try {
      const recovery = await revealWalletRecovery({
        password: unlock,
        sealed: wallet.sealed,
        public: wallet.public,
      });
      setWalletRecovery(recovery);
      setPassword("");
      setStatus("");
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function backupProfileNow(): Promise<void> {
    if (!wallet || password.length < 8) return;
    if (!profile.settings.contactsBackupKaspa && !profile.settings.backupMessagesKaspa) {
      setStatus("Enable contact backup or optional message backup first.");
      return;
    }
    const unlock = password;
    setStatus("Encrypting and publishing profile recovery snapshot to Kaspa…");
    try {
      const contacts = profile.settings.contactsBackupKaspa ? backupContacts(profile) : [];
      const messages = backupMessages(profile);
      if (contacts.length === 0 && messages.length === 0) {
        setStatus("There is nothing to back up yet.");
        return;
      }
      const fingerprint = await profileBackupFingerprint(profile);
      const result = await publishProfileBackup({
        password: unlock,
        sealed: wallet.sealed,
        public: wallet.public,
        contacts,
        messages,
        wrpcEndpoint: wallet.wrpcEndpoint,
      });
      onUpdate({
        ...profile,
        wallet: { ...wallet, public: result.public, profileBackupHash: fingerprint },
      });
      setStatus(`Encrypted recovery snapshot published in ${result.transaction_ids.length} Kaspa transaction(s).`);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setPassword("");
    }
  }

  async function restoreProfileNow(): Promise<void> {
    if (!wallet || password.length < 8) return;
    const unlock = password;
    setStatus("Searching Kaspa for the latest encrypted Ghost Talk recovery snapshot…");
    try {
      const restored = await restoreProfileBackup({
        password: unlock,
        sealed: wallet.sealed,
        public: wallet.public,
        restEndpoint: wallet.restEndpoint,
      });
      if (!restored) {
        setStatus("No complete authenticated Ghost Talk recovery snapshot was found for this wallet.");
        return;
      }
      const merged = mergeRestoredArchive(profile, restored.archive);
      const fingerprint = await profileBackupFingerprint(merged);
      onUpdate({
        ...merged,
        wallet: merged.wallet
          ? { ...merged.wallet, profileBackupHash: fingerprint }
          : merged.wallet,
      });
      setStatus(`Restored ${restored.archive.contacts.length} contact(s) and ${restored.archive.messages.length} optional archived message(s).`);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setPassword("");
    }
  }

  async function changeSecuritySetting(
    setting: "requireUnlockPassword" | "requireSendPassword",
    value: boolean,
  ): Promise<void> {
    if (!wallet) return;
    if (securityPassword.length < 8) {
      setSecurityStatus("Enter the current ID / wallet password to change security settings.");
      return;
    }
    setSecurityBusy(true);
    setSecurityStatus("Authorizing security setting change…");
    try {
      await unlockWallet({
        profileId: profile.id,
        password: securityPassword,
        sealed: wallet.sealed,
        public: wallet.public,
      });
      if (setting === "requireUnlockPassword") {
        await setRememberedUnlock({
          profileId: profile.id,
          enabled: !value,
          password: securityPassword,
          sealed: wallet.sealed,
          public: wallet.public,
        });
        if (!value) {
          await onUnlock(securityPassword);
        }
      }
      await onPersistUpdate({
        ...profile,
        securityPolicyRevision: (profile.securityPolicyRevision ?? 0) + 1,
        settings: { ...profile.settings, [setting]: value },
      });
      setSecurityPassword("");
      setSecurityStatus(
        setting === "requireUnlockPassword"
          ? value
            ? "Password will be required to unlock this ID on the next app session."
            : "Automatic device unlock enabled for this ID."
          : value
            ? "Password will be required for every KAS send and consolidation."
            : "The unlocked session may authorize KAS sends without another password prompt.",
      );
    } catch (error) {
      setSecurityStatus(String(error));
    } finally {
      setSecurityBusy(false);
    }
  }

  return (
    <section className="page">
      <div className="page-head"><div><h2>Settings</h2><p>Identity, recovery, network and local privacy settings.</p></div></div>
      <div className="card form-grid">
        <h3>Identity</h3>
        <label>Display name<input value={profile.label} onChange={event => onUpdate({ ...profile, label: event.target.value })} /></label>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={profile.autoLogin}
            disabled={profileCount !== 1 || !profile.recoveryBackupConfirmed}
            onChange={event => onUpdate({ ...profile, autoLogin: event.target.checked })}
          />
          <span>
            <b>Auto login</b>
            <small>
              {profileCount !== 1
                ? "Disabled whenever more than one local ID exists."
                : profile.recoveryBackupConfirmed
                  ? "Skip the ID picker for this one local ID."
                  : "Disabled until the 24-word Ghost Talk recovery backup is confirmed."}
            </small>
          </span>
        </label>
      </div>

      <div className="card form-grid chat-privacy-card">
        <h3>Chat privacy</h3>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={profile.settings.autoIgnoreUnknownChats}
            onChange={event => onUpdate({
              ...profile,
              settings: { ...profile.settings, autoIgnoreUnknownChats: event.target.checked },
            })}
          />
          <span>
            <b>Automatically ignore new chats from people not in Contacts</b>
            <small>
              Unknown first-contact requests are retained under Chats → Ignored requests without delivering message plaintext.
              They do not create a normal incoming-request badge or interrupt you. You can review, accept, or delete them later.
              Saved contacts are never auto-ignored. The bootstrap carries only public identity material on the public Kaspa chain.
            </small>
          </span>
        </label>
      </div>

      <div className="card form-grid security-card">
        <h3>Wallet security prompts</h3>
        <p className="muted">
          Enter the current ID / wallet password below before changing either policy. These controls
          change prompting behavior; they do not change your 24-word recovery root.
        </p>
        <label>
          Current ID / wallet password
          <input
            type="password"
            value={securityPassword}
            onChange={event => setSecurityPassword(event.target.value)}
            placeholder="Required to change these options"
          />
        </label>
        <div className="security-actions">
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={profile.settings.requireUnlockPassword}
              disabled={!wallet || securityBusy}
              onChange={event => void changeSecuritySetting("requireUnlockPassword", event.target.checked)}
            />
            <span>
              <b>Require password to unlock wallet + mailbox</b>
              <small>
                When enabled, each new app session must be unlocked manually. When disabled, Ghost
                Talk stores a separate device-local unlock credential so this OS account can open the ID automatically.
              </small>
            </span>
          </label>
          {!profile.settings.requireUnlockPassword && (
            <p className="warning">
              Automatic unlock trades local-device protection for convenience. Anyone who can access
              this OS account's Ghost Talk app data can potentially open the wallet and messaging identity.
            </p>
          )}
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={profile.settings.requireSendPassword}
              disabled={!wallet || securityBusy}
              onChange={event => void changeSecuritySetting("requireSendPassword", event.target.checked)}
            />
            <span>
              <b>Require password for each KAS send</b>
              <small>
                When disabled, an already-unlocked wallet session authorizes sends and UTXO consolidation
                without asking for the password again.
              </small>
            </span>
          </label>
        </div>
        {securityStatus && <div className="status">{securityStatus}</div>}
      </div>

      <div className="card form-grid recovery-card">
        <h3>Recovery</h3>
        {profile.kasSignerWallet ? (
          <p className="warning">
            This KasSigner-backed ID has one 24-word <b>Ghost Talk messaging recovery</b> root. It restores
            HYDRA plus the low-value mailbox fee wallet, but it does <b>not</b> restore the watch-only
            KasSigner financial account. Keep the KasSigner device's own wallet recovery backup separately.
          </p>
        ) : (
          <p className="warning">
            Ghost Talk has one 24-word BIP39 recovery root. The same words and optional BIP39
            passphrase restore both the Kaspa wallet and the deterministic HYDRA messaging identity.
            Encrypted contact snapshots can then be restored from Kaspa. Live ratcheted session state
            is intentionally not backed up; restored contacts establish fresh secure sessions.
          </p>
        )}
        <label>
          Re-enter ID / wallet password to reveal recovery data
          <input type="password" value={password} onChange={event => setPassword(event.target.value)} />
        </label>
        <div className="button-row">
          <button
            disabled={!wallet || Boolean(walletRecovery) || password.length < 8}
            onClick={() => void showRecovery()}
          >
            Reveal Ghost Talk recovery
          </button>
          {walletRecovery && (
            <button onClick={() => setWalletRecovery(undefined)}>Hide recovery data</button>
          )}
        </div>
        {walletRecovery && (
          <div className="recovery-output">
            <b>{profile.kasSignerWallet ? "Ghost Talk messaging recovery · 24 words" : "Ghost Talk account · 24 words"}</b>
            <p className="recovery">{walletRecovery.mnemonic}</p>
            <small>Kaspa network: {walletRecovery.network}</small>
            <small>Kaspa derivation: {walletRecovery.account_path}</small>
            {walletRecovery.passphrase ? (
              <p className="warning">BIP39 passphrase: <code>{walletRecovery.passphrase}</code> — this is required with the mnemonic to restore {profile.kasSignerWallet ? "the Ghost Talk messaging identity/mailbox" : "both Kaspa and HYDRA"}.</p>
            ) : <small>No BIP39 passphrase was configured.</small>}
            {profile.kasSignerWallet && <small>The imported KasSigner financial wallet is restored from KasSigner's own recovery, not these words.</small>}
            <button
              onClick={() => navigator.clipboard.writeText(
                `${walletRecovery.mnemonic}\nBIP39 passphrase: ${walletRecovery.passphrase || "(none)"}\nDerivation: ${walletRecovery.account_path}\nNetwork: ${walletRecovery.network}`
              )}
            >
              Copy Ghost Talk recovery details
            </button>
          </div>
        )}
        {status && <div className="status">{status}</div>}
      </div>

      <div className="card form-grid recovery-card">
        <h3>Encrypted Kaspa recovery backup</h3>
        <p className="muted">
          Contact snapshots are encrypted with a key derived from the restored Kaspa wallet before
          they are written on-chain. Public observers see transaction metadata and opaque payloads,
          not contact names/addresses. Each backup transaction still pays a network fee.
        </p>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={profile.settings.contactsBackupKaspa}
            onChange={event => onUpdate({
              ...profile,
              settings: { ...profile.settings, contactsBackupKaspa: event.target.checked },
            })}
          />
          <span><b>Back up contacts on Kaspa</b><small>Enabled by default for recoverable contacts.</small></span>
        </label>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={profile.settings.backupMessagesKaspa}
            onChange={event => onUpdate({
              ...profile,
              settings: { ...profile.settings, backupMessagesKaspa: event.target.checked },
            })}
          />
          <span>
            <b>Also archive messages that were not sent through Kaspa</b>
            <small>Optional. Kaspa-mailbox messages are excluded because their encrypted carrier is already on-chain.</small>
          </span>
        </label>
        <small>Last local backup fingerprint: {wallet?.profileBackupHash?.slice(0, 16) ?? "none"}</small>
        <div className="button-row">
          <button disabled={!wallet || password.length < 8} onClick={() => void backupProfileNow()}>Back up now</button>
          <button disabled={!wallet || password.length < 8} onClick={() => void restoreProfileNow()}>Restore from Kaspa</button>
        </div>
        <small>Use the same password field in Recovery above to authorize the local wallet vault.</small>
      </div>

      <div className="card form-grid debug-settings-card">
        <h3>Protocol & Kaspa debugging</h3>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={profile.settings.debugLogging}
            onChange={event => onUpdate({
              ...profile,
              settings: { ...profile.settings, debugLogging: event.target.checked },
            })}
          />
          <span>
            <b>Enable protocol/network debug logging</b>
            <small>
              Disabled by default. Records handshake stages, Kaspa carrier IDs, session IDs, sequence counters,
              mailbox monitor activity, and per-stage KAS send timings (wallet ready, Portal, UTXO plan, signed Portal analysis,
              submit). It never records message plaintext, passwords, recovery words,
              private keys or encrypted message payloads.
            </small>
          </span>
        </label>
        <div className="button-row">
          <button disabled={!profile.settings.debugLogging} onClick={onOpenDebug}>Open debug window</button>
        </div>
      </div>

      <div className="card form-grid">
        <h3>Kaspa endpoints</h3>
        <label>Historical REST endpoint (&gt;48h only)<input disabled={!wallet} value={wallet?.restEndpoint ?? ""} placeholder="Network default" onChange={event => wallet && onUpdate({ ...profile, wallet: { ...wallet, restEndpoint: event.target.value } })} /></label>
        <label>wRPC endpoint<input disabled={!wallet} value={wallet?.wrpcEndpoint ?? ""} placeholder="Auto resolver" onChange={event => wallet && onUpdate({ ...profile, wallet: { ...wallet, wrpcEndpoint: event.target.value } })} /></label>
        <small>Leave wRPC blank to resolve a real public Kaspa node automatically. REST is archival transaction history only for records proven older than 48 hours; it never supplies balance, UTXOs, DAA, connection status, sends, or live chat.</small>
      </div>
    </section>
  );
}
