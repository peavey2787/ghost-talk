import { useEffect, useState } from "react";
import { KasSignerSigningModal } from "./KasSignerSigningModal";
import type { DerivationPreset, KasSignerWalletRecord, Profile, WalletRecord, WalletSnapshot } from "../model";
import {
  consolidateKaspa,
  createWallet,
  importWallet,
  isTauri,
  nextReceive,
  prepareKasSignerConsolidation,
  prepareKasSignerSend,
  presets,
  qrSvg,
  refreshWallet,
  sendKaspa,
  type KasSignerSigningPrompt,
} from "../native";

interface Props {
  profile: Profile;
  snapshot?: WalletSnapshot;
  sessionUnlocked: boolean;
  sessionPassword: string;
  requireSendPassword: boolean;
  initialUnlockStatus?: string;
  onUnlock: (password: string) => Promise<void>;
  onWalletReady: (wallet: WalletRecord, password: string, backupConfirmed: boolean) => Promise<void>;
  onRecoveryBackupConfirmed: () => void;
  onWallet: (wallet: WalletRecord) => void;
  onKasSignerWallet: (wallet: KasSignerWalletRecord) => void;
  onSnapshot: (snapshot: WalletSnapshot) => void;
}

type WalletMode = "create" | "import";
type SignerMode = "software" | "kassigner";
type KasSignerAction = "send" | "consolidate";

export function KaspaWallet({
  profile,
  snapshot,
  sessionUnlocked,
  sessionPassword,
  requireSendPassword,
  initialUnlockStatus,
  onUnlock,
  onWalletReady,
  onRecoveryBackupConfirmed,
  onWallet,
  onKasSignerWallet,
  onSnapshot,
}: Props) {
  const wallet = profile.wallet;
  const kasSignerWallet = profile.kasSignerWallet;
  const financialPublic = kasSignerWallet?.public ?? wallet?.public;
  const financialRestEndpoint = kasSignerWallet?.restEndpoint ?? wallet?.restEndpoint;
  const financialWrpcEndpoint = kasSignerWallet?.wrpcEndpoint ?? wallet?.wrpcEndpoint;
  const [mode, setMode] = useState<WalletMode>("create");
  const [showSetup, setShowSetup] = useState(!wallet);
  const [presetList, setPresetList] = useState<DerivationPreset[]>([]);
  const [presetIndex, setPresetIndex] = useState(0);
  const [customPath, setCustomPath] = useState("m/44'/111111'/0'");
  const [network, setNetwork] = useState("mainnet");
  const [mnemonic, setMnemonic] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [password, setPassword] = useState("");
  const [backupMnemonic, setBackupMnemonic] = useState("");
  const [backupPassphrase, setBackupPassphrase] = useState("");
  const [status, setStatus] = useState("");
  const [sendStatus, setSendStatus] = useState("");
  const [pendingSoftwareBalance, setPendingSoftwareBalance] = useState<{ balance: string; transactionId: string }>();
  const [copiedAddress, setCopiedAddress] = useState(false);
  const [qr, setQr] = useState("");
  const [destination, setDestination] = useState("");
  const [amountKas, setAmountKas] = useState("");
  const [feeSompi, setFeeSompi] = useState("0");
  const [sendPassword, setSendPassword] = useState("");
  const [unlockPassword, setUnlockPassword] = useState("");
  const [unlockStatus, setUnlockStatus] = useState(initialUnlockStatus ?? "");
  const [signerMode, setSignerMode] = useState<SignerMode>(kasSignerWallet ? "kassigner" : "software");
  const [hardwareSnapshot, setHardwareSnapshot] = useState<WalletSnapshot>();
  const [kasSignerPrompt, setKasSignerPrompt] = useState<KasSignerSigningPrompt | null>(null);
  const [kasSignerAction, setKasSignerAction] = useState<KasSignerAction>("send");

  useEffect(() => {
    presets().then(setPresetList).catch(error => setStatus(String(error)));
  }, []);

  useEffect(() => {
    if (!sessionUnlocked) setUnlockStatus(initialUnlockStatus ?? "");
  }, [initialUnlockStatus, sessionUnlocked]);

  useEffect(() => {
    setSignerMode(profile.kasSignerWallet ? "kassigner" : "software");
    setHardwareSnapshot(undefined);
    setSendPassword("");
    setUnlockPassword("");
    setSendStatus("");
    setPendingSoftwareBalance(undefined);
  }, [profile.id]);

  const selectedPreset = presetList[presetIndex];
  const accountPath = selectedPreset?.path ?? customPath;
  const receiveAddress = financialPublic?.receive_addresses[financialPublic.next_receive_index] ?? "";
  const displaySnapshot = kasSignerWallet
    ? hardwareSnapshot
    : snapshot && pendingSoftwareBalance
      ? { ...snapshot, balance_sompi: pendingSoftwareBalance.balance }
      : snapshot;

  useEffect(() => {
    setCopiedAddress(false);
    if (!receiveAddress) {
      setQr("");
      return;
    }
    qrSvg(receiveAddress).then(setQr).catch(() => setQr(""));
  }, [receiveAddress]);

  useEffect(() => {
    if (!kasSignerWallet) return;
    void refresh();
  }, [profile.id, kasSignerWallet?.accountFingerprint]);

  async function createOrImport() {
    try {
      if (mode === "create") {
        setStatus("Creating wallet…");
        const result = await createWallet({
          password,
          passphrase,
          accountPath,
          network,
        });
        await onWalletReady(
          { sealed: result.sealed, public: result.public, mailboxCheckpoint: "0" },
          password,
          false,
        );
        setBackupMnemonic(result.mnemonic);
        setBackupPassphrase(passphrase);
        setStatus("Wallet created. Back up the recovery words before continuing.");
      } else {
        setStatus("Importing wallet…");
        const result = await importWallet({
          password,
          mnemonic,
          passphrase,
          accountPath,
          network,
        });
        await onWalletReady(
          { sealed: result.sealed, public: result.public, mailboxCheckpoint: "0" },
          password,
          true,
        );
        setShowSetup(false);
        setStatus("Wallet imported");
      }
      setPassword("");
      setPassphrase("");
      setMnemonic("");
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function refresh() {
    if (!financialPublic) return;
    try {
      setStatus("Refreshing…");
      const next = await refreshWallet(financialPublic, financialRestEndpoint, financialWrpcEndpoint);
      if (kasSignerWallet) setHardwareSnapshot(next);
      else onSnapshot(next);
      if (next.recommended_receive_index > financialPublic.next_receive_index) {
        const nextPublic = { ...financialPublic, next_receive_index: next.recommended_receive_index };
        if (kasSignerWallet) onKasSignerWallet({ ...kasSignerWallet, public: nextPublic });
        else if (wallet) onWallet({ ...wallet, public: nextPublic });
      }
      setStatus("Wallet refreshed");
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function beginKasSignerSend() {
    if (!financialPublic) return;
    try {
      setSendStatus("Preparing KasSigner signing request…");
      const prompt = await prepareKasSignerSend({
        public: financialPublic,
        profileId: kasSignerWallet ? undefined : profile.id,
        password: kasSignerWallet ? undefined : sessionPassword,
        sealed: kasSignerWallet ? undefined : wallet?.sealed,
        accountFingerprint: kasSignerWallet?.accountFingerprint,
        destination,
        amountSompi: kasToSompi(amountKas),
        feeSompi: feeSompi || "0",
        wrpcEndpoint: financialWrpcEndpoint,
      });
      setKasSignerAction("send");
      setKasSignerPrompt(prompt);
      setSendStatus("");
    } catch (error) {
      setSendStatus(String(error));
    }
  }

  async function beginKasSignerConsolidation() {
    if (!financialPublic) return;
    try {
      setStatus("Preparing KasSigner consolidation request…");
      const prompt = await prepareKasSignerConsolidation({
        public: financialPublic,
        profileId: kasSignerWallet ? undefined : profile.id,
        password: kasSignerWallet ? undefined : sessionPassword,
        sealed: kasSignerWallet ? undefined : wallet?.sealed,
        accountFingerprint: kasSignerWallet?.accountFingerprint,
        feeSompi: feeSompi || "0",
        wrpcEndpoint: financialWrpcEndpoint,
      });
      setKasSignerAction("consolidate");
      setKasSignerPrompt(prompt);
      setStatus("");
    } catch (error) {
      setStatus(String(error));
    }
  }

  if (!wallet || showSetup) {
    return (
      <section className="page">
        <div className="page-head">
          <div>
            <h2>Kaspa wallet</h2>
            <p>
              Ghost Talk uses one 24-word BIP39 root for both this Kaspa wallet and the derived HYDRA identity.
              Plaintext recovery words are never written to frontend storage.
            </p>
          </div>
          {wallet && !backupMnemonic && <button onClick={() => setShowSetup(false)}>Back to wallet</button>}
        </div>
        <div className="card wallet-setup">
          <div className="segmented">
            <button className={mode === "create" ? "active" : ""} onClick={() => setMode("create")}>
              Create new
            </button>
            <button className={mode === "import" ? "active" : ""} onClick={() => setMode("import")}>
              Import
            </button>
          </div>
          <label>
            Network
            <select value={network} onChange={event => setNetwork(event.target.value)}>
              <option value="mainnet">Mainnet</option>
              <option value="testnet-10">Testnet-10</option>
              <option value="testnet-11">Testnet-11</option>
            </select>
          </label>
          <label>
            Derivation path
            <select value={presetIndex} onChange={event => setPresetIndex(Number(event.target.value))}>
              {presetList.map((preset, index) => (
                <option key={preset.label} value={index} disabled={!preset.supported}>
                  {preset.label}
                </option>
              ))}
            </select>
          </label>
          {selectedPreset && <small>{selectedPreset.note}</small>}
          {selectedPreset?.path === null && (
            <label>
              Custom account path
              <input value={customPath} onChange={event => setCustomPath(event.target.value)} />
            </label>
          )}
          {mode === "create" ? (
            <p className="muted">New Ghost Talk accounts always create one 24-word recovery phrase.</p>
          ) : (
            <label>
              Ghost Talk recovery mnemonic
              <textarea
                rows={4}
                value={mnemonic}
                onChange={event => setMnemonic(event.target.value)}
                placeholder="24 recovery words"
              />
            </label>
          )}
          <label>
            Optional BIP39 passphrase
            <input type="password" value={passphrase} onChange={event => setPassphrase(event.target.value)} />
          </label>
          <label>
            Wallet encryption password
            <input
              type="password"
              value={password}
              onChange={event => setPassword(event.target.value)}
              placeholder="At least 8 characters"
            />
          </label>
          <button
            className="primary"
            disabled={
              !isTauri
              || password.length < 8
              || !accountPath
              || (mode === "import" && mnemonic.trim().split(/\s+/).length !== 24)
            }
            onClick={createOrImport}
          >
            {mode === "create" ? "Create wallet" : "Import wallet"}
          </button>
          {!isTauri && (
            <small className="warning">Wallet creation/import requires the native Ghost Talk app.</small>
          )}
          {status && <div className="status">{status}</div>}
        </div>
        {backupMnemonic && (
          <div className="modal-backdrop">
            <div className="modal backup-card">
              <h3>Back up your Ghost Talk recovery words now</h3>
              <p>
                These 24 words restore both the Kaspa wallet and the HYDRA identity derived from it. Save them securely, then confirm below.
              </p>
              <div className="recovery-grid">
                {backupMnemonic.split(/\s+/).map((word, index) => (
                  <span key={`${word}-${index}`}><b>{index + 1}</b> {word}</span>
                ))}
              </div>
              {backupPassphrase && (
                <p className="warning">
                  This account uses a BIP39 passphrase. The mnemonic alone will NOT restore the same
                  Kaspa wallet or HYDRA identity. Back up this passphrase too: <code>{backupPassphrase}</code>
                </p>
              )}
              <button
                onClick={() => navigator.clipboard.writeText(
                  backupPassphrase
                    ? `${backupMnemonic}\nBIP39 passphrase: ${backupPassphrase}`
                    : backupMnemonic
                )}
              >
                Copy recovery details
              </button>
              <button
                className="primary"
                onClick={() => {
                  onRecoveryBackupConfirmed();
                  setBackupMnemonic("");
                  setBackupPassphrase("");
                  setShowSetup(false);
                  setStatus("Ghost Talk recovery backup confirmed");
                }}
              >
                Confirm backup and continue
              </button>
            </div>
          </div>
        )}
      </section>
    );
  }

  if (!sessionUnlocked) {
    return (
      <section className="page unlock-only">
        <div className="page-head">
          <div>
            <h2>Kaspa</h2>
            <p>{financialPublic?.network} · {kasSignerWallet ? "KasSigner watch-only account" : financialPublic?.account_path}</p>
          </div>
        </div>
        <div className="card unlock-card">
          <h3>Unlock wallet + mailbox</h3>
          <p className="muted">
            Unlock once for this app session so Kaspa wallet actions and encrypted HYDRA mailbox
            operations share the same authenticated session. The password is kept only in memory.
          </p>
          <div className="inline-form">
            <input
              type="password"
              value={unlockPassword}
              onChange={event => setUnlockPassword(event.target.value)}
              placeholder="Wallet / ID password"
              onKeyDown={event => {
                if (event.key === "Enter" && unlockPassword.length >= 8) {
                  event.currentTarget.blur();
                  void (async () => {
                    try {
                      setUnlockStatus("Unlocking wallet and HYDRA mailbox…");
                      await onUnlock(unlockPassword);
                      setUnlockPassword("");
                      setUnlockStatus("");
                    } catch (error) {
                      setUnlockStatus(String(error));
                    }
                  })();
                }
              }}
            />
            <button
              className="primary"
              disabled={unlockPassword.length < 8}
              onClick={async () => {
                try {
                  setUnlockStatus("Unlocking wallet and HYDRA mailbox…");
                  await onUnlock(unlockPassword);
                  setUnlockPassword("");
                  setUnlockStatus("");
                } catch (error) {
                  setUnlockStatus(String(error));
                }
              }}
            >
              Unlock
            </button>
          </div>
          {unlockStatus && <div className="status">{unlockStatus}</div>}
        </div>
      </section>
    );
  }

  return (
    <section className="page">
      <div className="page-head">
        <div>
          <h2>Kaspa</h2>
          <p>{financialPublic?.network} · {kasSignerWallet ? "KasSigner watch-only account" : financialPublic?.account_path}</p>
        </div>
        <div className="button-row">
          <button onClick={refresh}>Refresh</button>
        </div>
      </div>
      <div className="wallet-grid">
        <div className="card balance-card">
          <small>Available balance</small>
          <strong>{displaySnapshot ? `${formatKas(displaySnapshot.balance_sompi)} KAS` : "Loading…"}</strong>
          <span>
            {displaySnapshot ? `${displaySnapshot.utxo_count} UTXOs · blue score ${displaySnapshot.blue_score}` : "Fetching current Kaspa balance and UTXOs…"}
          </span>
        </div>
        {kasSignerWallet && wallet && (
          <div className="card mailbox-fee-card">
            <small>Ghost Talk mailbox fee wallet</small>
            <strong>{snapshot ? `${formatKas(snapshot.balance_sompi)} KAS` : "Loading…"}</strong>
            <code>{wallet.public.receive_addresses[wallet.public.next_receive_index] ?? ""}</code>
            <span className="muted">Used only for encrypted chat carrier/control transactions so messages never require a hardware scan.</span>
            <button onClick={() => {
              setDestination(wallet.public.receive_addresses[wallet.public.next_receive_index] ?? "");
              setSignerMode("kassigner");
            }}>Fund mailbox from KasSigner wallet</button>
          </div>
        )}
        <div className="card receive-card">
          <h3>Receive</h3>
          <div className="qr-box">
            {qr ? <div dangerouslySetInnerHTML={{ __html: qr }} /> : <span>QR</span>}
          </div>
          <code>{receiveAddress}</code>
          <div className="button-row">
            <button
              className={copiedAddress ? "copied" : ""}
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(receiveAddress);
                  setCopiedAddress(true);
                  window.setTimeout(() => setCopiedAddress(false), 1600);
                } catch (error) {
                  setStatus(String(error));
                }
              }}
            >
              {copiedAddress ? "✓ Copied!" : "Copy address"}
            </button>
            <button
              onClick={async () => {
                try {
                  if (!financialPublic) return;
                  const publicWallet = await nextReceive(financialPublic);
                  if (kasSignerWallet) onKasSignerWallet({ ...kasSignerWallet, public: publicWallet });
                  else if (wallet) onWallet({ ...wallet, public: publicWallet });
                } catch (error) {
                  setStatus(String(error));
                }
              }}
            >
              Fresh address
            </button>
          </div>
          <small>Used receive addresses rotate automatically when wallet activity is observed.</small>
        </div>
        <div className="card send-card">
          <div className="card-title-row">
            <h3>Send</h3>
            {kasSignerWallet ? (
              <span className="watch-only-badge">KasSigner · watch-only</span>
            ) : (
              <div className="segmented compact" aria-label="Transaction signer">
                <button className={signerMode === "software" ? "active" : ""} onClick={() => setSignerMode("software")}>Software</button>
                <button className={signerMode === "kassigner" ? "active" : ""} onClick={() => setSignerMode("kassigner")}>KasSigner</button>
              </div>
            )}
          </div>
          <label>
            To
            <input value={destination} onChange={event => setDestination(event.target.value)} placeholder="kaspa:…" />
          </label>
          <label>
            Amount (KAS)
            <input inputMode="decimal" value={amountKas} onChange={event => setAmountKas(event.target.value)} placeholder="0.0" />
          </label>
          <label>
            Requested fee (sompi)
            <input
              inputMode="numeric"
              value={feeSompi}
              onChange={event => setFeeSompi(event.target.value.replace(/\D/g, ""))}
            />
          </label>
          {signerMode === "software" ? (
            requireSendPassword ? (
              <label>
                Wallet password
                <input type="password" value={sendPassword} onChange={event => setSendPassword(event.target.value)} />
              </label>
            ) : (
              <small>Authorized by the unlocked wallet + mailbox session.</small>
            )
          ) : (
            <div className="hardware-signer-note">
              <strong>KasSigner hardware approval</strong>
              <small>
                Ghost Talk plans the transaction and broadcasts it; KasSigner reviews and signs the exact PSKT by QR.
                {kasSignerWallet
                  ? "This financial account is watch-only in Ghost Talk. Its private Kaspa keys never enter the app; every spend must be approved on the matching KasSigner hardware wallet."
                  : "The hardware must contain the same Kaspa account. Chat mailbox transactions stay software-signed so every message does not require a hardware scan."}
              </small>
              {kasSignerWallet?.ownershipProof && (
                <small className="verified-text">✓ Account ownership was proven on KasSigner when this Ghost Talk ID was created.</small>
              )}
            </div>
          )}
          {signerMode === "software" ? (
            <button
              className="primary"
              disabled={!destination || !amountKas || !(requireSendPassword ? sendPassword : sessionPassword)}
              onClick={async () => {
                try {
                  setSendStatus("Planning, signing and broadcasting…");
                  const amountSompi = kasToSompi(amountKas);
                  const destinationAtSubmit = destination;
                  const result = await sendKaspa({
                    profileId: profile.id,
                    reuseUnlocked: !requireSendPassword,
                    password: requireSendPassword ? sendPassword : sessionPassword,
                    sealed: wallet.sealed,
                    public: wallet.public,
                    destination: destinationAtSubmit,
                    amountSompi,
                    feeSompi: feeSompi || "0",
                    restEndpoint: wallet.restEndpoint,
                    wrpcEndpoint: wallet.wrpcEndpoint,
                  });

                  // Kaspa accepted the transaction on the selected public node. Do
                  // not wait for archival REST history before reporting the broadcast,
                  // and never leave the pre-send balance on screen. The exact outgoing
                  // amount + actual signed fee is known locally; change remains ours.
                  const snapshotForDebit = snapshot && pendingSoftwareBalance
                    ? { ...snapshot, balance_sompi: pendingSoftwareBalance.balance }
                    : snapshot;
                  const optimistic = optimisticSnapshotAfterSend(
                    snapshotForDebit,
                    result.public,
                    destinationAtSubmit,
                    amountSompi,
                    result.fee_sompi,
                  );
                  if (optimistic) {
                    onSnapshot(optimistic);
                    setPendingSoftwareBalance({
                      balance: optimistic.balance_sompi,
                      transactionId: result.transaction_id,
                    });
                  }
                  onWallet({ ...wallet, public: result.public });
                  setDestination("");
                  setAmountKas("");
                  if (requireSendPassword) setSendPassword("");
                  setSendStatus(`Broadcast ${result.transaction_id} · balance syncing`);

                  // Reconcile only against the current UTXO set from the same
                  // Portal/public-node connection. REST is archival (>48h) and can
                  // never prove a newly broadcast transaction or current balance.
                  void reconcileSoftwareSendSnapshot({
                    transactionId: result.transaction_id,
                    publicWallet: result.public,
                    restEndpoint: wallet.restEndpoint,
                    wrpcEndpoint: wallet.wrpcEndpoint,
                    optimistic,
                    onSnapshot,
                    onWallet: nextPublic => onWallet({ ...wallet, public: nextPublic }),
                    onConfirmed: () => {
                      setPendingSoftwareBalance(current =>
                        current?.transactionId === result.transaction_id ? undefined : current,
                      );
                      setSendStatus(`Broadcast ${result.transaction_id} · balance confirmed`);
                    },
                  });
                } catch (error) {
                  setSendStatus(String(error));
                }
              }}
            >
              Send KAS
            </button>
          ) : (
            <button className="primary" disabled={!destination || !amountKas} onClick={() => void beginKasSignerSend()}>
              Review & sign with KasSigner
            </button>
          )}
          {sendStatus && <div className="status send-status">{sendStatus}</div>}
        </div>
        <div className="card consolidation-card">
          <h3>UTXO consolidation</h3>
          <p>Consolidate spendable UTXOs into the current fresh receive address. A normal transaction fee applies.</p>
          {signerMode === "kassigner" ? (
            <button onClick={() => void beginKasSignerConsolidation()}>Consolidate with KasSigner</button>
          ) : (
            <button
              onClick={async () => {
                const credential = requireSendPassword ? sendPassword : sessionPassword;
                if (!credential) {
                  setStatus(requireSendPassword
                    ? "Enter the wallet password in Send first."
                    : "Unlock the wallet + mailbox session first.");
                  return;
                }
                try {
                  setStatus("Consolidating UTXOs…");
                  const result = await consolidateKaspa({
                    profileId: profile.id,
                    reuseUnlocked: !requireSendPassword,
                    password: credential,
                    sealed: wallet.sealed,
                    public: wallet.public,
                    feeSompi: feeSompi || "0",
                    restEndpoint: wallet.restEndpoint,
                    wrpcEndpoint: wallet.wrpcEndpoint,
                  });
                  onWallet({ ...wallet, public: result.public });
                  setStatus(`Consolidation broadcast ${result.transaction_id}`);
                } catch (error) {
                  setStatus(String(error));
                }
              }}
            >
              Consolidate
            </button>
          )}
        </div>
        <div className="card history-card">
          <h3>Transaction history</h3>
          {displaySnapshot?.history.length ? (
            <div className="history-list">
              {displaySnapshot.history.map(entry => (
                <div key={entry.transaction_id}>
                  <code>{entry.transaction_id}</code>
                  <small>
                    Blue score {entry.blue_score}{entry.ghost_payload ? " · Ghost payload" : ""}
                  </small>
                </div>
              ))}
            </div>
          ) : (
            <p className="muted">No indexed transactions yet.</p>
          )}
        </div>
      </div>
      {kasSignerPrompt && (
        <KasSignerSigningModal
          prompt={kasSignerPrompt}
          onCancel={() => setKasSignerPrompt(null)}
          onBroadcast={(result, message) => {
            if (kasSignerWallet) onKasSignerWallet({ ...kasSignerWallet, public: result.public });
            else if (wallet) onWallet({ ...wallet, public: result.public });
            setKasSignerPrompt(null);
            if (kasSignerAction === "send") {
              setDestination("");
              setAmountKas("");
              setSendStatus(`${message} ${result.transaction_id}`);
            } else {
              setStatus(`${message} ${result.transaction_id}`);
            }
            void refresh();
          }}
        />
      )}
      {status && <div className="status floating-status">{status}</div>}
    </section>
  );
}

function optimisticSnapshotAfterSend(
  snapshot: WalletSnapshot | undefined,
  publicWallet: WalletRecord["public"],
  destination: string,
  amountSompi: string,
  feeSompi: string,
): WalletSnapshot | undefined {
  if (!snapshot) return undefined;
  try {
    const before = BigInt(snapshot.balance_sompi);
    const amount = BigInt(amountSompi);
    const fee = BigInt(feeSompi);
    const selfSend = [...publicWallet.receive_addresses, ...publicWallet.change_addresses]
      .some(address => address === destination);
    const debit = fee + (selfSend ? 0n : amount);
    if (debit > before) return snapshot;
    return { ...snapshot, balance_sompi: (before - debit).toString() };
  } catch {
    return snapshot;
  }
}

async function reconcileSoftwareSendSnapshot(args: {
  transactionId: string;
  publicWallet: WalletRecord["public"];
  restEndpoint?: string;
  wrpcEndpoint?: string;
  optimistic?: WalletSnapshot;
  onSnapshot: (snapshot: WalletSnapshot) => void;
  onWallet: (publicWallet: WalletRecord["public"]) => void;
  onConfirmed: () => void;
}): Promise<void> {
  const delays = [500, 1_000, 2_000, 4_000, 8_000];
  for (const delay of delays) {
    await new Promise<void>(resolve => window.setTimeout(resolve, delay));
    try {
      const fresh = await refreshWallet(args.publicWallet, args.restEndpoint, args.wrpcEndpoint);
      const balanceVisible = Boolean(
        args.optimistic && fresh.balance_sompi === args.optimistic.balance_sompi,
      );
      if (!balanceVisible) continue;
      args.onSnapshot(fresh);
      if (fresh.recommended_receive_index > args.publicWallet.next_receive_index) {
        args.onWallet({
          ...args.publicWallet,
          next_receive_index: fresh.recommended_receive_index,
        });
      }
      args.onConfirmed();
      return;
    } catch {
      // The long-lived native Portal monitor is an independent reconciliation
      // path. Keep the locally exact post-send balance rather than replacing it
      // with stale data or turning a successful broadcast into a UI error.
    }
  }
}

function kasToSompi(input: string): string {
  const value = input.trim();
  if (!/^\d+(\.\d{0,8})?$/.test(value)) {
    throw new Error("KAS amount must have at most 8 decimal places");
  }
  const [whole, fraction = ""] = value.split(".");
  return (BigInt(whole) * 100_000_000n + BigInt(fraction.padEnd(8, "0"))).toString();
}

function formatKas(sompi: string): string {
  const value = BigInt(/^\d+$/.test(sompi) ? sompi : "0");
  const whole = value / 100_000_000n;
  const fraction = (value % 100_000_000n)
    .toString()
    .padStart(8, "0")
    .replace(/0+$/, "");
  return fraction ? `${whole}.${fraction}` : whole.toString();
}
