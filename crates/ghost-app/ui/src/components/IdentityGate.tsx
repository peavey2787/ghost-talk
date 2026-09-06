import { useState } from "react";
import { KasSignerKpubScanner } from "./KasSignerKpubScanner";
import { KasSignerIdentityProofModal } from "./KasSignerIdentityProofModal";
import type { KasSignerIdentityOwnershipProof, Profile, WalletRecord } from "../model";
import {
  beginKasSignerIdentityProof,
  createWallet,
  importWallet,
  importKasSignerKpub,
  initializeHydraFromWallet,
  isTauri,
  restoreProfileBackup,
  revealWalletRecovery,
  type KasSignerIdentityProofPrompt,
} from "../native";
import { mergeRestoredArchive, profileBackupFingerprint } from "../profileBackup";
import { newProfile } from "../storage";

interface Props {
  profiles: Profile[];
  onSelect: (id: string, unlockPassword?: string) => void;
  onAddProfile: (profile: Profile) => void;
  onUpdateProfile: (profile: Profile) => void;
  onAutoLogin: (enabled: boolean) => void;
}

type AccountMode = "create" | "restore" | "kassigner";

interface BackupState {
  profileId: string;
  mnemonic: string;
  passphrase?: string;
}

interface PendingKasSignerProofState {
  profile: Profile;
  prompt: KasSignerIdentityProofPrompt;
  createdMnemonic: string;
  passphrase: string;
  restoringGhostIdentity: boolean;
}

const accountPaths = [
  { label: "Kaspa standard account", value: "m/44'/111111'/0'" },
  { label: "Kaspa account #1", value: "m/44'/111111'/1'" },
  { label: "Custom", value: "custom" },
];

export function IdentityGate({
  profiles,
  onSelect,
  onAddProfile,
  onUpdateProfile,
  onAutoLogin,
}: Props) {
  const [creating, setCreating] = useState(profiles.length === 0);
  const [pendingProfileId, setPendingProfileId] = useState("");
  const [mode, setMode] = useState<AccountMode>("create");
  const [label, setLabel] = useState("");
  const [password, setPassword] = useState("");
  const [network, setNetwork] = useState("mainnet");
  const [pathChoice, setPathChoice] = useState(accountPaths[0].value);
  const [customPath, setCustomPath] = useState("m/44'/111111'/0'");
  const [mnemonic, setMnemonic] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [kasSignerKpub, setKasSignerKpub] = useState("");
  const [scanKpub, setScanKpub] = useState(false);
  const [pendingKasSignerProof, setPendingKasSignerProof] = useState<PendingKasSignerProofState>();
  const [backup, setBackup] = useState<BackupState>();
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);

  const only = profiles.length === 1 ? profiles[0] : undefined;
  const pending = profiles.find(profile => profile.id === pendingProfileId);
  const backupWords = backup ? backup.mnemonic.trim().split(/\s+/).filter(Boolean) : [];
  const accountPath = pathChoice === "custom" ? customPath.trim() : pathChoice;

  async function initializeIdentity(profile: Profile, wallet: WalletRecord): Promise<Profile> {
    const hydra = await initializeHydraFromWallet({
      profileId: profile.id,
      password,
      sealed: wallet.sealed,
      public: wallet.public,
    });
    return { ...profile, wallet, hydraIdentityId: hydra.identity_id };
  }

  async function restoreArchive(profile: Profile): Promise<Profile> {
    if (!profile.wallet) return profile;
    const archive = await restoreProfileBackup({
      password,
      sealed: profile.wallet.sealed,
      public: profile.wallet.public,
    });
    if (!archive) return profile;
    const merged = mergeRestoredArchive(profile, archive.archive);
    const fingerprint = await profileBackupFingerprint(merged);
    return {
      ...merged,
      wallet: merged.wallet
        ? { ...merged.wallet, profileBackupHash: fingerprint }
        : merged.wallet,
    };
  }

  async function beginKasSignerOwnershipProof(
    profile: Profile,
    createdMnemonic: string,
    restoringGhostIdentity: boolean,
  ): Promise<void> {
    const hardware = profile.kasSignerWallet;
    const kpub = hardware?.public.kpub;
    if (!hardware || !kpub || !profile.hydraIdentityId) {
      throw new Error("KasSigner ownership proof requires the imported account kpub and initialized HYDRA identity.");
    }
    const prompt = await beginKasSignerIdentityProof({
      kpub,
      network: hardware.public.network,
      profileId: profile.id,
      hydraIdentityId: profile.hydraIdentityId,
    });
    setPendingKasSignerProof({
      profile,
      prompt,
      createdMnemonic,
      passphrase,
      restoringGhostIdentity,
    });
  }

  async function finishKasSignerOwnershipProof(proof: KasSignerIdentityOwnershipProof): Promise<void> {
    const pendingProof = pendingKasSignerProof;
    if (!pendingProof?.profile.kasSignerWallet) return;
    let next: Profile = {
      ...pendingProof.profile,
      kasSignerWallet: {
        ...pendingProof.profile.kasSignerWallet,
        ownershipProof: proof,
      },
    };
    onUpdateProfile(next);
    setPendingKasSignerProof(undefined);
    if (pendingProof.restoringGhostIdentity) {
      next = await restoreArchive(next);
      onUpdateProfile(next);
      const id = next.id;
      const unlock = password;
      resetSetup();
      onSelect(id, unlock);
      return;
    }
    if (pendingProof.createdMnemonic) {
      setBackup({
        profileId: next.id,
        mnemonic: pendingProof.createdMnemonic,
        passphrase: pendingProof.passphrase,
      });
    } else if (next.wallet) {
      const recovery = await revealWalletRecovery({
        password,
        sealed: next.wallet.sealed,
        public: next.wallet.public,
      });
      setBackup({
        profileId: next.id,
        mnemonic: recovery.mnemonic,
        passphrase: recovery.passphrase,
      });
    } else {
      throw new Error("Ghost Talk wallet state disappeared after KasSigner ownership verification.");
    }
    setStatus("");
  }

  async function createOrRestoreAccount(): Promise<void> {
    if (!label.trim() || password.length < 8 || busy || !accountPath || !isTauri) return;
    setBusy(true);
    setStatus(mode === "create" ? "Creating Ghost Talk account…" : mode === "restore" ? "Restoring Ghost Talk account…" : "Importing KasSigner watch-only account…");
    try {
      const profile = newProfile(label);
      if (mode === "kassigner") {
        if (!kasSignerKpub.trim()) throw new Error("Scan or paste the KasSigner account kpub first.");
        const hardware = await importKasSignerKpub({ kpub: kasSignerKpub, network });
        // A watch-only kpub cannot derive a private HYDRA identity. Ghost Talk therefore
        // creates/restores a separate low-value mailbox root used only for messaging/carrier
        // fees and derives HYDRA from that root. The imported KasSigner account remains
        // strictly watch-only and every financial spend requires hardware approval.
        const restoringGhostIdentity = Boolean(mnemonic.trim());
        if (restoringGhostIdentity && mnemonic.trim().split(/\s+/).length !== 24) {
          throw new Error("KasSigner-backed Ghost Talk restore requires exactly 24 Ghost Talk recovery words.");
        }
        let local: { sealed: number[]; public: WalletRecord["public"] };
        let createdMnemonic = "";
        if (restoringGhostIdentity) {
          local = await importWallet({ password, mnemonic, passphrase, accountPath: accountPaths[0].value, network });
        } else {
          const created = await createWallet({ password, passphrase, accountPath: accountPaths[0].value, network });
          local = created;
          createdMnemonic = created.mnemonic;
        }
        const wallet = walletRecord(local.sealed, local.public);
        let incomplete: Profile = {
          ...profile,
          wallet,
          kasSignerWallet: {
            accountFingerprint: hardware.account_fingerprint,
            public: hardware.public,
          },
          recoveryBackupConfirmed: restoringGhostIdentity,
        };
        onAddProfile(incomplete);
        setPendingProfileId(incomplete.id);
        incomplete = await initializeIdentity(incomplete, wallet);
        onUpdateProfile(incomplete);
        await beginKasSignerOwnershipProof(incomplete, createdMnemonic, restoringGhostIdentity);
        setStatus("");
        return;
      }
      if (mode === "create") {
        const created = await createWallet({ password, passphrase, accountPath, network });
        const wallet = walletRecord(created.sealed, created.public);
        const incomplete = { ...profile, wallet, recoveryBackupConfirmed: false };
        onAddProfile(incomplete);
        setPendingProfileId(incomplete.id);
        const initialized = await initializeIdentity(incomplete, wallet);
        onUpdateProfile(initialized);
        setBackup({
          profileId: initialized.id,
          mnemonic: created.mnemonic,
          passphrase,
        });
        setStatus("");
        return;
      }

      if (!mnemonic.trim()) {
        throw new Error("Restore requires the 24-word Ghost Talk recovery mnemonic.");
      }
      const imported = await importWallet({
        password,
        mnemonic,
        passphrase,
        accountPath,
        network,
      });
      const wallet = walletRecord(imported.sealed, imported.public);
      const incomplete = { ...profile, wallet, recoveryBackupConfirmed: false };
      onAddProfile(incomplete);
      setPendingProfileId(incomplete.id);
      let restored = await initializeIdentity(incomplete, wallet);
      restored = { ...restored, recoveryBackupConfirmed: true };
      restored = await restoreArchive(restored);
      onUpdateProfile(restored);
      const id = restored.id;
      resetSetup();
      onSelect(id, password);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function repairProfile(): Promise<void> {
    if (!pending || password.length < 8 || busy || !isTauri) return;
    setBusy(true);
    try {
      let current = pending;
      if (!current.wallet) {
        if (mode === "create") {
          setStatus("Creating Ghost Talk account…");
          const created = await createWallet({ password, passphrase, accountPath, network });
          const wallet = walletRecord(created.sealed, created.public);
          current = { ...current, wallet, recoveryBackupConfirmed: false, autoLogin: false };
          onUpdateProfile(current);
          current = await initializeIdentity(current, wallet);
          onUpdateProfile(current);
          setBackup({ profileId: current.id, mnemonic: created.mnemonic, passphrase });
          setStatus("");
          return;
        }

        if (!mnemonic.trim()) throw new Error("Enter the 24-word Ghost Talk recovery mnemonic.");
        const imported = await importWallet({ password, mnemonic, passphrase, accountPath, network });
        const wallet = walletRecord(imported.sealed, imported.public);
        current = { ...current, wallet, recoveryBackupConfirmed: false, autoLogin: false };
        onUpdateProfile(current);
        current = await initializeIdentity(current, wallet);
        current = { ...current, recoveryBackupConfirmed: true };
        current = await restoreArchive(current);
        onUpdateProfile(current);
        const id = current.id;
        const unlock = password;
        resetSetup();
        onSelect(id, unlock);
        return;
      }

      const currentWallet = current.wallet;
      if (!currentWallet) throw new Error("Ghost Talk wallet state disappeared during repair.");

      if (!current.hydraIdentityId) {
        setStatus("Deriving your HYDRA identity from the Ghost Talk recovery root…");
        current = await initializeIdentity(current, currentWallet);
        onUpdateProfile(current);
      }

      if (current.kasSignerWallet && !current.kasSignerWallet.ownershipProof) {
        setStatus("Verify control of the imported KasSigner account before this ID can open.");
        await beginKasSignerOwnershipProof(current, "", current.recoveryBackupConfirmed);
        setStatus("");
        return;
      }

      if (!current.recoveryBackupConfirmed) {
        setStatus("Unlocking Ghost Talk recovery words…");
        const recovery = await revealWalletRecovery({
          password,
          sealed: currentWallet.sealed,
          public: currentWallet.public,
        });
        setBackup({
          profileId: current.id,
          mnemonic: recovery.mnemonic,
          passphrase: recovery.passphrase,
        });
        setStatus("");
        return;
      }

      const id = current.id;
      const unlock = password;
      resetSetup();
      onSelect(id, unlock);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  function confirmBackup(): void {
    if (!backup || backupWords.length !== 24 || busy) return;
    const profile = profiles.find(candidate => candidate.id === backup.profileId);
    if (!profile?.wallet || !profile.hydraIdentityId) {
      setStatus("The Ghost Talk account is not fully initialized yet.");
      return;
    }
    if (profile.kasSignerWallet && !profile.kasSignerWallet.ownershipProof) {
      setStatus("Verify KasSigner account ownership before completing this ID.");
      return;
    }
    const next = { ...profile, autoLogin: false, recoveryBackupConfirmed: true };
    onUpdateProfile(next);
    const id = next.id;
    const unlock = password;
    resetSetup();
    onSelect(id, unlock);
  }

  function chooseProfile(profile: Profile): void {
    if (
      profile.wallet
      && profile.hydraIdentityId
      && profile.recoveryBackupConfirmed
      && (!profile.kasSignerWallet || Boolean(profile.kasSignerWallet.ownershipProof))
    ) {
      onSelect(profile.id);
      return;
    }
    setPendingProfileId(profile.id);
    setCreating(false);
    setPassword("");
    setMode("create");
    setStatus("");
  }

  function resetSetup(): void {
    setCreating(false);
    setPendingProfileId("");
    setMode("create");
    setLabel("");
    setPassword("");
    setMnemonic("");
    setPassphrase("");
    setKasSignerKpub("");
    setScanKpub(false);
    setPendingKasSignerProof(undefined);
    setBackup(undefined);
    setStatus("");
  }

  if (pendingKasSignerProof) {
    return (
      <KasSignerIdentityProofModal
        prompt={pendingKasSignerProof.prompt}
        onVerified={proof => { void finishKasSignerOwnershipProof(proof); }}
        onCancel={() => {
          setPendingKasSignerProof(undefined);
          setStatus("KasSigner ownership verification is required before this ID can be opened.");
        }}
      />
    );
  }

  if (busy) {
    return (
      <div className="gate-shell">
        <section className="gate-card loading-card" aria-live="polite" aria-busy="true">
          <img className="brand-mark" src="./ghost-talk-icon.png" alt="" />
          <div className="loading-spinner" aria-hidden="true" />
          <h1>{mode === "create" ? "Creating Ghost Talk ID" : mode === "restore" ? "Restoring Ghost Talk ID" : "Importing KasSigner ID"}</h1>
          <p className="muted">{status || "Initializing the Kaspa wallet and HYDRA identity…"}</p>
          <small>Keep Ghost Talk open. Your recovery screen will appear when initialization is complete.</small>
        </section>
      </div>
    );
  }

  if (backup) {
    return (
      <div className="gate-shell">
        <section className="gate-card backup-card">
          <img className="brand-mark" src="./ghost-talk-icon.png" alt="" />
          <h1>Back up your Ghost Talk account</h1>
          {profiles.find(candidate => candidate.id === backup.profileId)?.kasSignerWallet ? (
            <p>
              These <b>24 words restore this Ghost Talk messaging identity and its low-value Kaspa mailbox fee wallet.</b>
              They do <b>not</b> restore the imported KasSigner financial wallet. Keep the KasSigner device's own
              recovery backup separately; Ghost Talk stores only its account kpub and cannot spend it without hardware approval.
            </p>
          ) : (
            <p>
              These <b>24 recovery words are the one cryptographic backup for this Ghost Talk ID.</b>
              They restore both the Kaspa wallet/address chain and the deterministic HYDRA messaging
              identity. There is no separate HYDRA mnemonic.
            </p>
          )}
          <RecoveryWords words={backupWords} label="Ghost Talk recovery words" />
          {backup.passphrase && (
            <p className="warning">
              This account uses a BIP39 passphrase. The 24 words alone restore a different account.
              Back up this passphrase too: <code>{backup.passphrase}</code>
            </p>
          )}
          <button onClick={() => navigator.clipboard.writeText(
            backup.passphrase
              ? `${backup.mnemonic}\nBIP39 passphrase: ${backup.passphrase}`
              : backup.mnemonic
          )}>
            Copy recovery details
          </button>
          <p className="warning">
            Encrypted contact snapshots can be recovered from Kaspa after restoring this account.
            Live ratcheted session state is intentionally not backed up; contacts establish fresh
            secure sessions after device loss.
          </p>
          <button className="primary wide" disabled={busy} onClick={confirmBackup}>
            Confirm backup and open Ghost Talk
          </button>
          {status && <div className="status">{status}</div>}
        </section>
      </div>
    );
  }

  return (
    <div className="gate-shell">
      <section className="gate-card">
        <img className="brand-mark" src="./ghost-talk-icon.png" alt="" />
        <h1>Select Ghost Talk ID</h1>
        <p className="muted">Choose the local identity to open before Ghost Talk starts.</p>

        {profiles.length > 0 && !pending && !creating && (
          <div className="identity-list">
            {profiles.map(profile => (
              <button className="identity-row" key={profile.id} onClick={() => chooseProfile(profile)}>
                <span className="avatar">{profile.label.slice(0, 1).toUpperCase()}</span>
                <span>
                  <b>{profile.label}</b>
                  <small>{profile.id}</small>
                  {!profile.recoveryBackupConfirmed && <small className="warning">Recovery backup required</small>}
                </span>
                <span className="arrow">›</span>
              </button>
            ))}
          </div>
        )}

        {only && !pending && !creating && only.recoveryBackupConfirmed && (
          <label className="toggle-row">
            <input
              type="checkbox"
              checked={only.autoLogin}
              onChange={event => onAutoLogin(event.target.checked)}
            />
            <span>
              <b>Auto login this ID</b>
              <small>Available only when one local ID exists and its 24-word recovery backup is confirmed.</small>
            </span>
          </label>
        )}

        {pending ? (
          <div className="create-id">
            <h3>Finish setup for {pending.label}</h3>
            <p className="muted">
              {pending.kasSignerWallet
                ? "This ID uses a watch-only KasSigner financial wallet plus a separate Ghost Talk mailbox/HYDRA recovery root."
                : "A current Ghost Talk profile needs one 24-word Kaspa BIP39 root and the HYDRA identity deterministically derived from it."}
            </p>
            {!pending.wallet && <WalletFields
              mode={mode}
              network={network}
              onNetwork={setNetwork}
              pathChoice={pathChoice}
              onPathChoice={setPathChoice}
              customPath={customPath}
              onCustomPath={setCustomPath}
              mnemonic={mnemonic}
              onMnemonic={setMnemonic}
              passphrase={passphrase}
              onPassphrase={setPassphrase}
            />}
            <label>
              ID / wallet password
              <input type="password" value={password} onChange={event => setPassword(event.target.value)} />
            </label>
            <div className="button-row">
              <button onClick={() => { setPendingProfileId(""); setPassword(""); setStatus(""); }}>Back</button>
              <button className="primary" disabled={password.length < 8 || busy} onClick={() => void repairProfile()}>
                {busy ? "Working…" : "Continue"}
              </button>
            </div>
            {status && <div className="status">{status}</div>}
          </div>
        ) : creating ? (
          <div className="create-id">
            <h3>{mode === "create" ? "Create new account" : mode === "restore" ? "Restore account" : "Import KasSigner account"}</h3>
            <div className="segmented">
              <button className={mode === "create" ? "active" : ""} onClick={() => setMode("create")}>New account</button>
              <button className={mode === "restore" ? "active" : ""} onClick={() => setMode("restore")}>Restore account</button>
              <button className={mode === "kassigner" ? "active" : ""} onClick={() => setMode("kassigner")}>KasSigner</button>
            </div>
            <label>Display name<input value={label} onChange={event => setLabel(event.target.value)} /></label>
            {mode === "kassigner" ? (
              <KasSignerFields
                network={network}
                onNetwork={setNetwork}
                kpub={kasSignerKpub}
                onKpub={setKasSignerKpub}
                mnemonic={mnemonic}
                onMnemonic={setMnemonic}
                passphrase={passphrase}
                onPassphrase={setPassphrase}
                onScan={() => setScanKpub(true)}
              />
            ) : (
              <WalletFields
                mode={mode}
                network={network}
                onNetwork={setNetwork}
                pathChoice={pathChoice}
                onPathChoice={setPathChoice}
                customPath={customPath}
                onCustomPath={setCustomPath}
                mnemonic={mnemonic}
                onMnemonic={setMnemonic}
                passphrase={passphrase}
                onPassphrase={setPassphrase}
              />
            )}
            <label>
              Local encryption password
              <input type="password" value={password} onChange={event => setPassword(event.target.value)} placeholder="At least 8 characters" />
            </label>
            <p className="muted">
              {mode === "kassigner"
                ? "This local password protects Ghost Talk's HYDRA/mailbox state. The imported KasSigner account remains watch-only and never exposes a private Kaspa key to Ghost Talk."
                : "This local password encrypts the device vault. It is not part of recovery. The same 24-word BIP39 root deterministically restores both Kaspa and HYDRA."}
            </p>
            <div className="button-row">
              {profiles.length > 0 && <button onClick={() => setCreating(false)}>Cancel</button>}
              <button
                className="primary"
                disabled={
                  busy
                  || !isTauri
                  || !label.trim()
                  || password.length < 8
                  || (mode !== "kassigner" && !accountPath)
                  || (mode === "restore" && mnemonic.trim().split(/\s+/).length !== 24)
                  || (mode === "kassigner" && !kasSignerKpub.trim())
                }
                onClick={() => void createOrRestoreAccount()}
              >
                {busy ? "Working…" : mode === "create" ? "Create Ghost Talk account" : mode === "restore" ? "Restore Ghost Talk account" : "Create KasSigner-backed ID"}
              </button>
            </div>
            {!isTauri && <small className="warning">Account creation/restoration requires the native Ghost Talk app.</small>}
            {status && <div className="status">{status}</div>}
          </div>
        ) : (
          <button className="primary wide" onClick={() => setCreating(true)}>＋ Add / restore ID</button>
        )}
      </section>
      {scanKpub && (
        <KasSignerKpubScanner
          onScan={payload => { setKasSignerKpub(payload); setScanKpub(false); setStatus("KasSigner account QR captured."); }}
          onClose={() => setScanKpub(false)}
        />
      )}
    </div>
  );
}

function KasSignerFields(props: {
  network: string;
  onNetwork: (network: string) => void;
  kpub: string;
  onKpub: (kpub: string) => void;
  mnemonic: string;
  onMnemonic: (mnemonic: string) => void;
  passphrase: string;
  onPassphrase: (passphrase: string) => void;
  onScan: () => void;
}) {
  return (
    <>
      <label>
        Kaspa network
        <select value={props.network} onChange={event => props.onNetwork(event.target.value)}>
          <option value="mainnet">Mainnet</option>
          <option value="testnet-10">Testnet-10</option>
          <option value="testnet-11">Testnet-11</option>
        </select>
      </label>
      <label>
        KasSigner account kpub
        <textarea
          rows={4}
          value={props.kpub}
          onChange={event => props.onKpub(event.target.value.trim())}
          placeholder="kpub1… or scan the account QR"
        />
      </label>
      <button type="button" onClick={props.onScan}>Scan KasSigner kpub QR</button>
      <details>
        <summary>Restore an existing Ghost Talk identity with this KasSigner wallet</summary>
        <label>
          Ghost Talk recovery words (24)
          <textarea
            rows={4}
            value={props.mnemonic}
            onChange={event => props.onMnemonic(event.target.value)}
            placeholder="Leave blank to create a new Ghost Talk identity"
          />
        </label>
        <label>
          Optional Ghost Talk BIP39 passphrase
          <input type="password" value={props.passphrase} onChange={event => props.onPassphrase(event.target.value)} />
        </label>
      </details>
      <p className="muted">
        Ghost Talk stores only this public account descriptor for your financial wallet. During ID creation, KasSigner must also sign a one-time Ghost Talk ownership challenge, so a copied or stolen kpub alone cannot be claimed as yours. Receive/change addresses and balances are watch-only; every financial spend is reviewed and signed on KasSigner.
      </p>
      <p className="warning">
        Messaging still needs a private HYDRA identity and a low-value Kaspa mailbox fee key. Ghost Talk creates those locally and shows one separate 24-word Ghost Talk recovery phrase after import. Those words do not restore your KasSigner wallet.
      </p>
    </>
  );
}

function WalletFields(props: {
  mode: AccountMode;
  network: string;
  onNetwork: (network: string) => void;
  pathChoice: string;
  onPathChoice: (path: string) => void;
  customPath: string;
  onCustomPath: (path: string) => void;
  mnemonic: string;
  onMnemonic: (mnemonic: string) => void;
  passphrase: string;
  onPassphrase: (passphrase: string) => void;
}) {
  return (
    <>
      <label>
        Kaspa network
        <select value={props.network} onChange={event => props.onNetwork(event.target.value)}>
          <option value="mainnet">Mainnet</option>
          <option value="testnet-10">Testnet-10</option>
          <option value="testnet-11">Testnet-11</option>
        </select>
      </label>
      <label>
        Kaspa derivation
        <select value={props.pathChoice} onChange={event => props.onPathChoice(event.target.value)}>
          {accountPaths.map(path => <option key={path.value} value={path.value}>{path.label}</option>)}
        </select>
      </label>
      {props.pathChoice === "custom" && (
        <label>Custom account path<input value={props.customPath} onChange={event => props.onCustomPath(event.target.value)} /></label>
      )}
      {props.mode === "create" ? (
        <p className="muted">
          New accounts always use one 24-word BIP39 recovery phrase. Ghost Talk derives the HYDRA
          identity from the same BIP39 seed with a separate domain.
        </p>
      ) : (
        <label>
          Ghost Talk recovery mnemonic (24 words)
          <textarea
            rows={4}
            value={props.mnemonic}
            onChange={event => props.onMnemonic(event.target.value)}
            placeholder="24 recovery words"
          />
        </label>
      )}
      <label>
        Optional BIP39 passphrase
        <input type="password" value={props.passphrase} onChange={event => props.onPassphrase(event.target.value)} />
      </label>
    </>
  );
}

function RecoveryWords({ words, label }: { words: string[]; label: string }) {
  return (
    <div className="recovery-grid" aria-label={label}>
      {words.map((word, index) => <span key={`${word}-${index}`}><b>{index + 1}</b> {word}</span>)}
    </div>
  );
}

function walletRecord(sealed: number[], publicWallet: WalletRecord["public"]): WalletRecord {
  return { sealed, public: publicWallet, mailboxCheckpoint: "0" };
}
