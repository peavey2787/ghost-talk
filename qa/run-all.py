#!/usr/bin/env python3
from pathlib import Path
import argparse
import hashlib
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
passed = failed = skipped = 0


def gate(name, ok, detail=''):
    global passed, failed
    if ok:
        passed += 1
        print(f"PASS {name}")
    else:
        failed += 1
        print(f"FAIL {name}: {detail}")


def skip(name, why):
    global skipped
    skipped += 1
    print(f"SKIP {name}: {why}")


def text(path):
    return (ROOT / path).read_text(encoding='utf-8', errors='replace')


def main() -> int:
    parser = argparse.ArgumentParser(description='Ghost Talk fast/static QA gate')
    parser.add_argument('--static-only', action='store_true', help='do not invoke Cargo or frontend builds')
    args = parser.parse_args()

    allowed_dirs = {'crates', 'docs', 'qa', 'examples', 'external', 'scripts', '.git'}
    for p in ROOT.iterdir():
        if p.is_dir():
            gate(f"root dir {p.name}", p.name in allowed_dirs, f"unexpected top-level directory {p.name}")
    for req in ['Cargo.toml', '.gitignore', 'LICENSE', 'LICENSES.md', 'README.md', 'docs/roadmap.md',
                'qa/run-all.py', 'qa/run-all-tests.py', 'scripts/run-all-tests.cmd', 'scripts/run-all-tests.sh', 'REVISION']:
        gate(f"required {req}", (ROOT / req).exists())

    alltxt = '\n'.join(
        p.read_text(encoding='utf-8', errors='ignore') for p in ROOT.rglob('*')
        if p.is_file() and '.git' not in p.parts and p.stat().st_size < 2_000_000
    )
    gate('Ghost Talk name', 'Ghost Talk' in text('README.md'))
    gate('permanent semantic version', 'version = "0.1.0"' in text('Cargo.toml') and '"version":"0.1.0"' in text('crates/ghost-app/package.json') and '"version":"0.1.0"' in text('crates/ghost-app/src-tauri/tauri.conf.json'))
    revision = text('REVISION').strip() if (ROOT / 'REVISION').is_file() else ''
    gate('revision counter', len(revision) > 1 and revision[0] == 'r' and revision[1:].isdigit())
    gate('documented revision matches', revision in text('README.md') and f'v0.1.0 {revision}' in text('docs/project/implementation-status.md'))
    gate('no Ubuntu build script', not any((ROOT / 'scripts').rglob('*ubuntu*')))

    platform_scripts = {
        'Windows build': 'scripts/windows/build.cmd',
        'Windows run': 'scripts/windows/run.cmd',
        'Linux build': 'scripts/linux/build.sh',
        'Linux run': 'scripts/linux/run.sh',
        'Android cmd build': 'scripts/android/build.cmd',
        'Android cmd run': 'scripts/android/run.cmd',
        'Android shell build': 'scripts/android/build.sh',
        'Android shell run': 'scripts/android/run.sh',
        'iOS build': 'scripts/ios/build.sh',
        'iOS run': 'scripts/ios/run.sh',
        'iOS double-click build': 'scripts/ios/build.command',
        'iOS double-click run': 'scripts/ios/run.command',
        'WASM cmd build': 'scripts/wasm/build.cmd',
        'WASM cmd run': 'scripts/wasm/run.cmd',
        'WASM shell build': 'scripts/wasm/build.sh',
        'WASM shell run': 'scripts/wasm/run.sh',
    }
    for label, path in platform_scripts.items():
        gate(label, (ROOT / path).is_file())
    gate('platforms split into script folders', all((ROOT / 'scripts' / x).is_dir() for x in ['windows','linux','android','ios','wasm']))
    gate('no first-party PowerShell scripts', not any((ROOT / 'scripts').rglob('*.ps1')))
    windows_env = text('scripts/windows/_msvc-env.cmd') if (ROOT / 'scripts/windows/_msvc-env.cmd').is_file() else ''
    gate('Windows native env helper', (ROOT / 'scripts/windows/_msvc-env.cmd').is_file())
    gate('Windows RC auto-discovery', 'vswhere.exe' in windows_env and 'VsDevCmd.bat' in windows_env and 'Windows Kits\\10\\bin' in windows_env and 'rc.exe' in windows_env.lower())
    gate('Windows launchers initialize native env', all('_msvc-env.cmd' in text(path) for path in ['scripts/windows/build.cmd','scripts/windows/run.cmd','scripts/windows/bootstrap.cmd','scripts/run-all-tests.cmd']))
    gate('Windows RC variable reserved for resource compiler', all('set "RC=' not in text(path) for path in ['scripts/windows/build.cmd','scripts/windows/run.cmd','scripts/windows/bootstrap.cmd','scripts/run-all-tests.cmd']))
    gate('Windows frontend install includes dev dependencies', all('--include=dev' in text(path) for path in ['scripts/windows/build.cmd','scripts/windows/run.cmd','scripts/windows/bootstrap.cmd']))
    gate('Windows Tauri launch uses verified local CLI', all('node_modules\\.bin\\tauri.cmd' in text(path) for path in ['scripts/windows/build.cmd','scripts/windows/run.cmd']) and 'npm run tauri -- build' not in text('scripts/windows/build.cmd') and 'npm run tauri -- dev' not in text('scripts/windows/run.cmd'))
    gate('Devuan bootstrap policy', 'Devuan' in text('scripts/linux/bootstrap.sh'))
    gate('iOS 15 target', '"minimumSystemVersion":"15.0"' in text('crates/ghost-app/src-tauri/tauri.conf.json'))
    gate('Tauri frontend hook cwd', '"beforeDevCommand":{"script":"npm run dev","cwd":"..","wait":false}' in text('crates/ghost-app/src-tauri/tauri.conf.json') and '"beforeBuildCommand":{"script":"npm run build","cwd":"..","wait":true}' in text('crates/ghost-app/src-tauri/tauri.conf.json'))
    icon_dir = ROOT / 'crates/ghost-app/src-tauri/icons'
    required_icons = ['32x32.png', '128x128.png', '128x128@2x.png', 'icon.png', 'icon.ico', 'icon.icns']
    gate('Tauri icon set', all((icon_dir / name).is_file() and (icon_dir / name).stat().st_size > 0 for name in required_icons))
    gate('Tauri bundle icon config', '"icon":["icons/32x32.png","icons/128x128.png","icons/128x128@2x.png","icons/icon.icns","icons/icon.ico"]' in text('crates/ghost-app/src-tauri/tauri.conf.json'))
    gate('Xcode 16.2 policy', '16.2' in text('scripts/ios/build.sh') and '16.2' in text('scripts/ios/run.sh'))
    gate('complete test wrappers', 'qa\\run-all-tests.py' in text('scripts/run-all-tests.cmd') and 'qa/run-all-tests.py' in text('scripts/run-all-tests.sh'))
    gate('test wrapper argument forwarding', 'qa\\run-all-tests.py %*' in text('scripts/run-all-tests.cmd') and 'qa/run-all-tests.py "$@"' in text('scripts/run-all-tests.sh'))
    gate('CRLF-aware Git whitespace hygiene', 'cr-at-eol' in text('qa/run-all-tests.py') and 'blank-at-eol' in text('qa/run-all-tests.py'))
    gate('main-section resume support', all(name in text('qa/run-all-tests.py') for name in ['--from', 'static', 'rust', 'examples', 'frontend', 'hydra', 'hygiene', 'fuzz', '--list-sections']) and 'portal' not in text('qa/run-all-tests.py').split('SECTIONS =', 1)[1].split('SECTION_ALIASES', 1)[0])
    bare_main_npm = ''.join(['run([', '"npm"'])
    bare_static_npm = ''.join(['subprocess.run([', "'npm'"])
    gate('Windows npm command resolution', bare_main_npm not in text('qa/run-all-tests.py') and bare_static_npm not in text('qa/run-all.py'))
    webrtc_source = text('crates/ghost-app/ui/src/webrtc.ts')
    gate('WebRTC ArrayBuffer send boundary', 'this.channel.send(packet)' not in webrtc_source and 'packet.buffer instanceof ArrayBuffer' in webrtc_source and 'this.channel.send(payload)' in webrtc_source)
    route_model = text('crates/ghost-app/ui/src/model.ts')
    route_storage = text('crates/ghost-app/ui/src/storage.ts')
    gate('transport UX exposes only Auto and Kaspa only', 'export type Route = "Auto" | "Kaspa only";' in route_model and '["Auto", "Kaspa only"]' in text('crates/ghost-app/ui/src/components/ChatView.tsx') and 'return value === "Kaspa only" ? "Kaspa only" : "Auto";' in route_storage and 'Direct Preferred' not in route_model)
    gate('Auto automatically negotiates a post-handshake WebRTC route', 'ensureDirectUpgrade(selected)' in text('crates/ghost-app/ui/src/components/ChatView.tsx') and 'selected?.bootstrapComplete' in text('crates/ghost-app/ui/src/components/ChatView.tsx') and 'encodeDirectRouteSignal' in text('crates/ghost-app/ui/src/components/ChatView.tsx') and 'waitForIceGathering' in webrtc_source and 'Kaspa only** — never establish a direct peer connection' in text('docs/spec/voice.md'))
    gate('direct WebRTC bytes remain HYDRA protected', 'hydra_seal_direct' in text('crates/ghost-app/src-tauri/src/hydra_commands.rs') and 'hydra_open_direct' in text('crates/ghost-app/src-tauri/src/hydra_commands.rs') and 'await props.onSealDirect(chatId, body)' in text('crates/ghost-app/ui/src/components/ChatView.tsx') and 'carrier.send(base64ToBytes(encrypted))' in text('crates/ghost-app/ui/src/components/ChatView.tsx') and 'await props.onOpenDirect(chatId, bytesToBase64(packet))' in text('crates/ghost-app/ui/src/components/ChatView.tsx'))
    ui_main = text('crates/ghost-app/ui/src/main.tsx')
    identity_gate = text('crates/ghost-app/ui/src/components/IdentityGate.tsx')
    chat_view = text('crates/ghost-app/ui/src/components/ChatView.tsx')
    wallet_view = text('crates/ghost-app/ui/src/components/KaspaWallet.tsx')
    sidebar = text('crates/ghost-app/ui/src/components/Sidebar.tsx')
    mailbox_native = text('crates/ghost-app/src-tauri/src/mailbox_commands.rs')
    hydra_native = text('crates/ghost-app/src-tauri/src/hydra_commands.rs')
    mailbox_ui = text('crates/ghost-app/ui/src/mailbox.ts')
    settings_ui = text('crates/ghost-app/ui/src/components/SimpleViews.tsx')
    debug_window_ui = text('crates/ghost-app/ui/src/components/DebugLogWindow.tsx')
    debug_native = text('crates/ghost-app/src-tauri/src/debug_log.rs')
    voice_ui = text('crates/ghost-app/ui/src/voice.ts')
    wallet_native = text('crates/ghost-app/src-tauri/src/wallet_commands.rs')
    gate('startup ID gate and one-ID auto-login', 'Select Ghost Talk ID' in identity_gate and 'profiles.length === 1' in identity_gate and 'enforceAutoLogin' in ui_main)
    gate('wallet create/import and custom derivation UI', 'Create new' in wallet_view and 'Import' in wallet_view and 'Custom account path' in wallet_view and 'derivation_presets' in wallet_native)
    gate('Kaspa wallet navigation and core controls', '"Kaspa"' in sidebar and 'Send KAS' in wallet_view and 'Copy address' in wallet_view and 'Transaction history' in wallet_view and 'Consolidate' in wallet_view)
    brand_asset = ROOT / 'crates/ghost-app/ui/public/ghost-talk-icon.png'
    tauri_brand_asset = ROOT / 'crates/ghost-app/src-tauri/icons/icon.png'
    gate(
        'Ghost Talk brand mark uses packaged logo instead of purple G',
        brand_asset.is_file()
        and tauri_brand_asset.is_file()
        and brand_asset.read_bytes() == tauri_brand_asset.read_bytes()
        and (sidebar + identity_gate + ui_main).count('./ghost-talk-icon.png') == 5
        and 'brand-mark small">G<' not in sidebar
        and 'className="brand-mark">G<' not in identity_gate,
    )
    wallet_engine = text('crates/ghost-kaspa/src/wallet.rs')
    gate(
        'Kaspa signed mass and fee analysis delegates to Portal 1.0.1 node-aware policy',
        wallet_engine.count('.analyze(&signed)') == 3
        and wallet_engine.count('.analyze(signed_pskb)') == 2
        and 'analyze_with_fee_rate' not in wallet_engine
        and 'LOCAL_STANDARD_FEE_RATE_SOMPI_PER_GRAM' not in wallet_engine
        and 'first_signed' not in wallet_engine
        and 'retry_signed' not in wallet_engine,
    )
    gate('Stego selector is interactive', 'setStegoOpen(value => !value)' in chat_view and 'props.onStego(option.profile)' in chat_view)
    gate('advanced controls hidden by default', 'useState(false)' in chat_view and 'advanced &&' in chat_view)
    gate('main navigation renders distinct views', all(token in ui_main for token in ['case "Contacts"', 'case "Rooms"', 'case "Games"', 'case "Kaspa"', 'case "Settings"']))
    gate('privacy mask covers composer and rendered messages', 'type={props.mask ? "password" : "text"}' in chat_view and '"*".repeat' in chat_view and 'lastMessage(chat, props.mask)' in chat_view)
    gate('empty chats require contact or authenticated mailbox delivery', 'No chats yet' in chat_view and 'Encrypted Ghost Talk payload received via Kaspa' not in ui_main and 'receiveHydraMailbox' in ui_main)
    history_source = text('crates/ghost-history/src/lib.rs')
    gate('mailbox delivery is live-node authoritative with archival replay overlap', 'live.blocks.recv()' in mailbox_native and 'historical_mailbox_before_ms' in mailbox_native and 'historical_rest_cutoff_ms()' in mailbox_native and 'MAILBOX_HISTORY_OVERLAP' in mailbox_native and 'address_transactions_after' in history_source and 'mailboxSeenTxids' in mailbox_ui and 'MAX_SEEN_TXIDS = 2048' in mailbox_ui)
    gate('new mailbox scanner performs one-time full reconciliation before overlap resume', 'mailboxScannerVersion' in text('crates/ghost-app/ui/src/model.ts') and 'wallet.mailboxScannerVersion === 1 ? (wallet.mailboxCheckpoint || "0") : "0"' in ui_main and 'mailboxScannerVersion: 1' in ui_main)
    gate('history pagination uses REST timestamp cursor header', 'x-next-page-before' in history_source and 'before = Some(next_before)' in history_source and 'cursor = last' not in history_source)
    gate('address history accepts numeric or decimal-string score fields', 'fn optional_u64' in history_source and 'serde_json::Value::String(value)' in history_source and 'let blue_score = optional_u64(' in history_source)
    gate('mailbox GHST fragments persist until HYDRA succeeds', 'mailboxPending' in mailbox_ui and 'readyEnvelopes' in ui_main and 'removeEnvelope' in ui_main and 'hydra_receive_mailbox' in hydra_native)
    gate('mailbox pending storage is globally bounded', 'MAX_PENDING_BYTES = 2 * 1024 * 1024' in mailbox_ui and 'bucketBytes(bucket.parts) > MAX_ENVELOPE_BYTES' in mailbox_ui and 'trimPending(pending)' in mailbox_ui)
    storage_ui = text('crates/ghost-app/ui/src/storage.ts')
    gate('persisted mailbox state is normalized before use', 'normalizeWallet(profile.wallet)' in storage_ui and 'normalizePending(wallet.mailboxPending)' in storage_ui and 'MAX_PENDING_PACKETS = 128' in storage_ui)
    gate('restart reconciliation preserves newer sender-side chat history', 'reconcilePersistedHistory' in storage_ui and 'mergePersistedChats' in storage_ui and 'mergePersistedMessages' in storage_ui and 'cachedRemaining.values()' in storage_ui and 'reconcilePersistedHistory(nativeProfiles, cached)' in ui_main and 'localStorage is synchronous' in ui_main and 'saveProfiles(updated)' in ui_main and 'queueNativeProfileSave(updated)' in ui_main)
    remembered_unlock_native = text('crates/ghost-app/src-tauri/src/remembered_unlock_commands.rs')
    gate('wallet secrets stay native and optional remembered unlock stays out of profile JSON', 'sealed: Vec<u8>' in wallet_native and 'sessionPassword' in ui_main and 'remembered-unlock-v1' in remembered_unlock_native and 'rememberedUnlock' not in storage_ui and 'localStorage' not in hydra_native)
    gate('wallet secret zeroizes on drop', 'ZeroizeOnDrop' in text('crates/ghost-kaspa/src/wallet.rs') and 'seed.zeroize()' in text('crates/ghost-kaspa/src/wallet.rs'))
    gate('wallet signing entropy zeroizes', 'entropy.zeroize()' in text('crates/ghost-kaspa/src/wallet.rs') and 'let signed_result =' in text('crates/ghost-kaspa/src/wallet.rs'))
    backup_native = text('crates/ghost-app/src-tauri/src/backup_commands.rs')
    backup_ui = text('crates/ghost-app/ui/src/profileBackup.ts')
    settings_ui = text('crates/ghost-app/ui/src/components/SimpleViews.tsx')
    kaspa_source = text('crates/ghost-kaspa/src/lib.rs')
    gate('new accounts use one confirmed 24-word recovery root', 'Confirm backup and open Ghost Talk' in identity_gate and 'one cryptographic backup' in identity_gate and 'There is no separate HYDRA mnemonic' in identity_gate and 'recoveryBackupConfirmed' in identity_gate)
    gate('backup confirmation is click-only without indexed-word challenge', all(token not in (identity_gate + wallet_view) for token in ['challengeFor', 'backup-challenge', 'Word #', 'setAnswers', 'backupAnswers']) and 'Confirm backup and continue' in wallet_view)
    gate('single-root setup has no second recovery state', all(token not in identity_gate for token in ['BackupKind', 'hydraMnemonic', 'beginHydraBackup', 'HYDRA ID recovery words', 'Restore both backups']))
    gate('blocking wallet and HYDRA crypto stay off Tauri main thread', 'spawn_blocking' in text('crates/ghost-app/src-tauri/src/lib.rs') and all(token in hydra_native for token in ['pub async fn hydra_initialize_from_wallet', 'pub async fn hydra_ensure', 'pub async fn hydra_receive_mailbox']) and all(token in wallet_native for token in ['pub async fn wallet_create', 'pub async fn wallet_import', 'pub async fn wallet_unlock', 'pub async fn wallet_reveal_recovery']))
    gate('wallet-derived HYDRA setup is retry-safe after partial frontend transition', '[existing] =>' in hydra_native and 'Retry-safe setup' in hydra_native and 'not derived from its Ghost Talk recovery root' in hydra_native)
    gate('auto-login requires the unified recovery backup', 'recoveryBackupConfirmed' in storage_ui and 'profiles.length !== 1' in storage_ui and 'walletBackupConfirmed' not in storage_ui and 'hydraBackupConfirmed' not in storage_ui)
    gate('old dual-backup profile storage is not migrated', 'ghost-talk/profiles/v2' in storage_ui and 'ghost-talk/profiles/v1' not in storage_ui)
    gate('one recovery mnemonic can be revealed later', 'Reveal Ghost Talk recovery' in settings_ui and 'revealWalletRecovery' in settings_ui and 'revealHydraMnemonic' not in settings_ui)
    gate('recovery reveal consumes the entered password', 'setWalletRecovery(recovery)' in settings_ui and 'setPassword("")' in settings_ui and 'recoveryGrant' not in settings_ui)
    gate('Ghost Talk app accepts only 24-word recovery roots', '12 words' not in identity_gate and '12 words' not in wallet_view and r'mnemonic.trim().split(/\s+/).length !== 24' in identity_gate and r'mnemonic.trim().split(/\s+/).length !== 24' in wallet_view)
    wallet_source = text('crates/ghost-kaspa/src/wallet.rs')
    gate('HYDRA identity is domain-separated from the BIP39 seed', 'blake3::derive_key("GhostTalk/HYDRA-ID/v1", &seed)' in wallet_source and 'pub fn hydra_identity_seed' in wallet_source and 'hydra_identity_seed(&secret)' in hydra_native)
    gate('legacy independent HYDRA recovery commands are removed', all(token not in hydra_native for token in ['hydra_create_identity', 'hydra_import_identity', 'hydra_reveal_mnemonic', 'mnemonic24_from_entropy', 'entropy_from_mnemonic24']))
    hydra_identity_api = text('external/hydra-msg/crates/hydra-msg/src/api/identity.rs')
    gate('HYDRA identity unlock avoids redundant current-KDF verification', 'Some(decrypt_seed(&previous, password)?)' in hydra_identity_api and 'verify_password(&previous, password)?' not in hydra_identity_api and 'identity_seed() already authenticates' in hydra_identity_api)
    cargo_root = text('Cargo.toml')
    gate('dev crypto dependencies are optimized without weaker KDF parameters', '[profile.dev.package."*"]' in cargo_root and 'opt-level = 3' in cargo_root and 'Ok((17, 8, 1))' in text('external/hydra-msg/crates/hydra-msg/src/codec/kdf.rs'))
    gate('Kaspa profile backups are wallet-derived authenticated encryption', 'Aes256Gcm' in backup_native and 'BACKUP_AAD' in backup_native and 'profile_backup_key' in backup_native and 'GTBK' in backup_native and 'plaintext.zeroize()' in backup_native)
    gate('contact backup defaults on and non-Kaspa message backup is optional', 'contactsBackupKaspa: true' in storage_ui and 'backupMessagesKaspa: false' in storage_ui and 'if (message.txid || message.direction === "system") continue' in backup_ui and 'Also archive messages that were not sent through Kaspa' in settings_ui)
    gate('encrypted Kaspa contact/message restore is wired into recovery UI', 'restoreProfileBackup' in identity_gate and 'mergeRestoredArchive' in identity_gate and 'Restore from Kaspa' in settings_ui and 'profile_backup_restore' in backup_native)
    gateway_native = text('crates/ghost-app/src-tauri/src/kaspa_gateway.rs')
    app_native_tree = '\n'.join(p.read_text(encoding='utf-8', errors='replace') for p in (ROOT / 'crates/ghost-app/src-tauri/src').glob('*.rs'))
    gate('Kaspa Portal 1.0.1 is a registry dependency with no copied source tree', cargo_root.count('kaspa-portal = "1.0.1"') == 1 and not (ROOT / 'external/kaspa-portal').exists() and 'path = "external/kaspa-portal"' not in cargo_root)
    gate('Ghost Talk does not override Kaspa Portal connection retry/timeout policy', '.timeout_ms(' not in kaspa_source and '.max_retries(' not in kaspa_source and 'tokio::time::timeout(Duration::from_secs(3), portal.connect())' not in gateway_native)
    gate('wallet and mailbox planning delegates fee and mass policy to Portal 1.0.1', wallet_source.count('.plan_send_with_payload(') >= 3 and wallet_source.count('.analyze(') >= 5 and 'first_signed' not in wallet_source and 'retry_signed' not in wallet_source and 'fee_replan_ms' not in wallet_source and 'conservative_preflight' not in kaspa_source)
    gate('KKTP anchors are admitted by direct BlockAdded and archival recovery filters', 'payload.starts_with(b"KKTP:")' in history_source and 'payload.starts_with(b"KKTP:")' in kaspa_source and '.filter(|observation| observation.payload.starts_with(&ghost_protocol::GHST_MAGIC))' not in mailbox_native and 'live.blocks.recv()' in mailbox_native and 'historical_mailbox_before_ms' in mailbox_native)
    gate('Ghost Kaspa current-chain queries use the public Kaspa Portal ChainApi', '.chain()' in kaspa_source and '.utxos_many(addresses)' in kaspa_source and '.virtual_daa_score()' in kaspa_source and 'kaspa_portal::network::queries::' not in kaspa_source)
    gate('live BlockAdded observation stays separate from durable REST history', 'pub struct LiveTransactionObservation' in kaspa_source and '.map(to_live_mailbox_event)' in mailbox_native and 'fn to_live_mailbox_event(observation: LiveTransactionObservation) -> MailboxEvent' in mailbox_native and 'fn to_mailbox_event(observation: DagObservation) -> MailboxEvent' in mailbox_native and 'ingest_live_indexer' not in mailbox_native and 'ingest_indexer' not in mailbox_native)
    gate('Discover current state rides the same live BlockAdded stream without gating private mailbox delivery', 'ghost://directory-live' in mailbox_native and 'sync_public_directory' not in mailbox_native and 'mailbox_directory.ingest_payload' in mailbox_native and 'Public profile announcements are current-state Ghost' in mailbox_native and 'onDirectoryLive' in ui_main)
    gate('mailbox UI processing is serialized against overlapping native and local drains', 'mailboxApplyChain' in ui_main and 'older async event can never' in ui_main and 'Every mailbox application path shares mailboxApplyChain' in ui_main and 'window.setInterval(drainObserved, 500)' in ui_main and 'mergeAppliedMailboxProfile' in ui_main)
    gate('async mailbox commits merge instead of erasing newer local chat state', 'mergeAppliedMailboxChats' in ui_main and 'mergeAppliedMailboxContacts' in ui_main and 'Archive/Leave are explicit local history/session boundaries. Mailbox work' in ui_main and 'same signed Discovery SID' in ui_main and 'collapseDuplicateRequestThreads(merged)' in ui_main and 'if (!knownMessages.has(message.id)) messages.push(message);' in ui_main)
    gate('incoming chat requests and messages produce visible app notification', 'New encrypted chat request received.' in ui_main and 'New encrypted Ghost Talk message received.' in ui_main and 'incoming-notice' in text('crates/ghost-app/ui/src/style.css'))
    gate('signed first-contact requests surface while HYDRA is locked', 'hydra_preview_contact_request' in hydra_native and 'previewHydraContactRequest' in ui_main and 'GTCR first-contact requests are public, signed bootstrap objects' in ui_main and 'Acceptance still requires unlock' in ui_main)
    gate('first-contact REST history is archival-only and never the healthy live path', 'MAILBOX_PRIORITY_HISTORY_INTERVAL: Duration = Duration::from_secs(60)' in mailbox_native and 'sync_priority_private_mailbox' in mailbox_native and 'public.receive_addresses.first()' in mailbox_native and 'historical_mailbox_before_ms' in mailbox_native and '(recovery_needed || live_stream.is_none())' in mailbox_native and 'recovery_needed = false;' in mailbox_native and 'live BlockAdded stream is authoritative for new carriers' in mailbox_native)
    hydra_facade = text('crates/ghost-hydra/src/lib.rs')
    hydra_vendor_channel = text('external/hydra-msg/crates/hydra-msg/src/handshake/channel.rs')
    kktp_doc = text('docs/spec/kktp.md')
    peer_native = text('crates/ghost-app/src-tauri/src/peer_commands.rs')
    profile_state_native = text('crates/ghost-app/src-tauri/src/profile_state_commands.rs')
    main_ui = text('crates/ghost-app/ui/src/main.tsx')
    chat_ui = text('crates/ghost-app/ui/src/components/ChatView.tsx')
    contacts_ui = text('crates/ghost-app/ui/src/components/SimpleViews.tsx')
    wallet_ui = text('crates/ghost-app/ui/src/components/KaspaWallet.tsx')
    protocol_source = text('crates/ghost-protocol/src/lib.rs')
    ghost_kaspa_source = text('crates/ghost-kaspa/src/lib.rs')
    storage_source = text('crates/ghost-app/ui/src/storage.ts')
    gate('live HYDRA mailbox state is memory-resident and retry-aware', 'HydraRuntimeState' in hydra_native and 'pending_outbound' in hydra_native and 'prepared_completion' in hydra_native and 'offer_payloads_hex' in mailbox_native and 'export_transaction_checkpoint' not in hydra_facade)
    gate('KKTP v2 PQ bootstrap is authenticated and automatic', 'KktpHandshakeControl' in hydra_native and all(stage in hydra_native for stage in ['"pq_init"', '"pq_resp"', '"pq_finish"']) and 'sign_application_context' in hydra_native and 'verify_contact_application_context' in hydra_native and 'reply_handshake' in hydra_native and 'finish_handshake' in hydra_native and 'accept_finish' in hydra_native and 'completes_pending_id' in main_ui)
    gate('KKTP bootstrap uses durable carriers and serialized local consumption', 'Every mailbox application path shares mailboxApplyChain' in main_ui and 'window.setInterval(drainObserved, 500)' in main_ui and 'submitted Kaspa bootstrap carriers are never timer-rebroadcast' in main_ui and 'nextBootstrapReplay' not in main_ui and 'nextResponseReplay' not in main_ui and 'responseRetryAfter' not in main_ui and 'mailbox_retry_handshake_finish' in mailbox_native and 'retained KKTP FINISH' in mailbox_native)
    gate('accepted consent is not reported ACTIVE before peer-confirmed pq_finish', 'Acceptance records consent and the responder identity' in main_ui and 'bootstrapComplete: false' in main_ui and 'The first signed ACK is also the initiator' in hydra_native and 'session_established_peer: establishes_session.then_some' in hydra_native)
    gate('wallet-authored KKTP loopback traffic is discarded instead of wedging mailbox queue', 'expected_sender == &local' in hydra_native and 'request.sender.hydra_identity_id == runtime.identity_id' in hydra_native and 'accepted.responder.hydra_identity_id == runtime.identity_id' in hydra_native and 'wire.sender_hydra_id == runtime.identity_id' in hydra_native and 'ack.signer_hydra_id == identity_id' in hydra_native)
    gate('pq_finish remains ACK-retained natively while message delivery commits on Kaspa broadcast', 'prepared.message_id == ack.message_id' in hydra_native and 'runtime.pending_outbound = None' in hydra_native and 'runtime.prepared_completion = None' in hydra_native and "secure-session FINISH is still awaiting the peer's signed acknowledgement" in mailbox_native and 'pq_finish is special' in main_ui and 'sendState: "delivered"' in main_ui and 'Once Kaspa accepts' in main_ui)
    gate('pq-init diagnostics preserve SID ownership after pending-state storage', 'sid: sid.clone()' in mailbox_native and 'pq-init-prepared' in mailbox_native and 'profile={} sid={} peer={} pending={} message={}' in mailbox_native)
    gate('KKTP handshake wrapper is ML-DSA context signed before state changes', 'pq_sig_b64' in protocol_source and 'control.signing_bytes()' in hydra_native and 'verify_contact_application_context' in hydra_native and r'HYDRA-MSG/GhostTalk/KKTP/v2/context\0' in hydra_vendor_channel and 'sign_application_context' in hydra_facade and 'verify_contact_application_context' in hydra_facade and 'pq_sig_b64' in kktp_doc)
    gate('pending HYDRA handshake has explicit per-contact abort', 'pub fn abort_handshake' in hydra_vendor_channel and 'pending_offers' in hydra_vendor_channel and 'accepted_inits' in hydra_vendor_channel and 'pub fn abort_handshake' in hydra_facade and 'runtime.hydra.abort_handshake' in hydra_native)
    gate('address and KNS allow private bootstrap while verified live descriptors remain optional', 'resolve_ghost_peer' in peer_native and 'KnsResolver' in peer_native and 'hydra_handle: None' in peer_native and 'verified_public: false' in peer_native and 'directory.latest(&target.address)' in peer_native and 'verify_gtcd(&descriptor)' in peer_native and 'preview_contact(&card)' in peer_native and 'add_contact(&card)' not in peer_native and 'public Ghost Talk profile is not required' in contacts_ui)
    gate('direct chats do not require saved contacts', 'onStartChat' in chat_ui and 'startDirectChat' in main_ui and 'peerKaspaAddress' in main_ui and 'not saved as contact' in chat_ui)
    gate('private first-contact requests are signed and destination-bound', 'GhostContactRequest' in protocol_source and 'GhostContactAccept' in protocol_source and 'verify_contact_request(&request)' in hydra_native and 'verify_contact_accept(&accepted)' in hydra_native and 'recipient_kaspa_address' in hydra_native and 'acceptor_kaspa_address' in hydra_native and 'contact_request_signature_binds_recipient_destination' in ghost_kaspa_source and 'contact_accept_signature_binds_exact_acceptor_and_return_destination' in ghost_kaspa_source)
    gate('private bootstrap protocol has KKTP v2 canonical round-trip coverage', 'kktp_v2_discovery_and_response_are_canonical_and_share_sid' in protocol_source and 'GHOST_KKTP_VERSION' in protocol_source and 'KKTP_ANCHOR_PREFIX' in protocol_source and 'private_contact_request_roundtrips_and_binds_destination_field' in protocol_source and 'private_contact_accept_roundtrips_with_exact_acceptor_address' in protocol_source)
    gate('KKTP session role mailbox and sequence state are native-bound', 'KktpSessionBinding' in hydra_native and 'kktp_mailbox_id' in hydra_native and 'send_seq' in hydra_native and 'recv_next_seq' in hydra_native and 'open_kktp_envelope' in hydra_native and 'KKTP encrypted inner metadata does not match the authenticated outer record' in hydra_native)
    gate('KKTP canonical outer and inner records enforce replay ordering', 'kktp_mailbox_message_rejects_noncanonical_wire_json' in protocol_source and 'canonical_json' in protocol_source and 'wire.seq < binding.recv_next_seq' in hydra_native and 'wire.seq > binding.recv_next_seq' in hydra_native and 'seq < binding.recv_next_seq' in hydra_native and 'seq > binding.recv_next_seq' in hydra_native)
    gate('KKTP active-message retry cache commits on successful submit without a second ACK transaction', 'PreparedKktpDelivery' in hydra_native and 'MAX_PREPARED_KKTP_DELIVERIES: usize = 4096' in hydra_native and 'prepared_kktp_deliveries.get(&message_id)' in mailbox_native and 'Retry the exact same SID/seq/HYDRA ciphertext' in mailbox_native and '.prepared_kktp_deliveries' in mailbox_native and '.remove(&message_id);' in mailbox_native and 'peer-signed acknowledgements are reserved for pq_finish' in mailbox_native and 'Do not create a second on-chain ACK transaction for ordinary text' in main_ui)
    gate('unsent post-encryption KKTP failures retire poisoned sequence state', 'reset_kktp_after_unsent_advance' in hydra_native and 'HYDRA has already advanced its sending ratchet' in hydra_native and 'the secure session was reset before any KKTP sequence gap could be created' in hydra_native and 'HYDRA produced multiple logical envelopes for one KKTP direct message' in hydra_native)
    gate('live voice cannot create strict KKTP sequence gaps', 'if use_kktp && reuse_change' in mailbox_native and 'prepare_realtime_mailbox' in mailbox_native and 'Realtime SID-bound transport' in kktp_doc and 'cannot consume `KktpSessionBinding.send_seq` / `recv_next_seq`' in kktp_doc)
    gate('retired KKTP SID replay cache is bounded and same-SID consent is idempotent', 'MAX_RETIRED_KKTP_SIDS: usize = 4096' in hydra_native and 'retired_kktp_sids.clear()' in hydra_native and 'same_kktp_session' in hydra_native and 'never tear down a ratchet merely because the same signed' in hydra_native and 'retired session' in hydra_native)
    gate('new direct sends use KKTP while legacy direct wire stays decode compatible', 'prepare_kktp_mailbox' in mailbox_native and 'kktp_handshake_payload' in mailbox_native and 'KktpMailboxMessage::decode' in hydra_native and 'MAILBOX_HANDSHAKE_MAGIC' in hydra_native and 'MAILBOX_ENVELOPE_MAGIC' in hydra_native and 'encode_offer_control' not in mailbox_native and 'encode_offer_control' not in hydra_native)
    gate('KKTP HYDRA-PQ deployment profile is documented', '# Ghost Talk KKTP v2 — HYDRA-PQ profile' in kktp_doc and 'ML-KEM-768' in kktp_doc and 'ML-DSA-65' in kktp_doc and 'pq_sig_b64' in kktp_doc and 'session_end' in kktp_doc and 'Authentication is deliberately **dual-layered**' in kktp_doc)
    gate('KKTP session_end is dual-authenticated and sent before local teardown', 'KktpSessionEnd' in protocol_source and 'pq_signing_bytes' in protocol_source and 'kaspa_signing_bytes' in protocol_source and 'sign_kktp_session_end' in ghost_kaspa_source and 'verify_kktp_session_end' in ghost_kaspa_source and 'mailbox_send_session_end' in mailbox_native and 'prepare_kktp_session_end' in hydra_native and 'sendMailboxSessionEnd' in main_ui and main_ui.find('sendMailboxSessionEnd') < main_ui.find('await leaveHydraPeer', main_ui.find('async function leaveChat')))
    gate('verified remote session_end closes only the matching SID and blocks old-chat sends', 'handle_kktp_session_end' in hydra_native and 'binding.sid != end.sid' in hydra_native and 'let matching_live_binding' in hydra_native and 'if matching_live_binding' in hydra_native and 'blocked_peers.insert(end.sender_hydra_id.clone())' in hydra_native and 'do not globally' in hydra_native and 'session_ended: Some' in hydra_native and 'runtime.blocked_peers.contains(&contact_id)' in mailbox_native and 'sessionSid?: string' in storage_ui + text('crates/ghost-app/ui/src/model.ts') and 'peerLeft?: boolean' in text('crates/ghost-app/ui/src/model.ts') and 'result.session_ended' in main_ui and 'chat.sessionSid !== ended.sid' in main_ui and 'chat.peerLeft' in chat_ui)
    gate('public discoverability never bypasses first-chat consent', 'bootstrapComplete: false' in main_ui and 'const needsRequest = !queueBehindHandshake && (!hydraHandle || !chat.bootstrapComplete);' in main_ui and 'sendMailboxContactRequest' in main_ui and 'A public Ghost Talk profile is optional' in chat_ui)
    gate('unknown chat auto-ignore is opt-in retained and reviewable', 'autoIgnoreUnknownChats: false' in storage_ui and 'Automatically ignore new chats from people not in Contacts' in settings_ui and 'Ignored requests' in chat_ui and 'state: autoIgnoreUnknown && !contact ? "ignored" : "pending"' in main_ui and 'incomingRequests={active.chats.filter(chat => chat.incomingRequest?.state === "pending").length}' in main_ui)
    gate('unsolicited chat requests do not consume persistent HYDRA contact slots before acceptance', 'preview_contact(&card)?' in hydra_native and 'signed_request_hex: hex::encode(&envelope)' in hydra_native and 'accept_signed_contact_request' in hydra_native and 'runtime.hydra.add_contact(&card)?' in hydra_native and 'signedRequestHex: request.signedRequestHex' in main_ui and 'signed_request_hex: String' in mailbox_native)
    gate('accepted private bootstrap updates saved contact to authenticated return address', 'contact.kaspaAddress === accepted.acceptor_address' in main_ui and 'kaspaAddress: accepted.peer_address' in main_ui and 'contactId: acceptedContact?.id ?? chat.contactId' in main_ui)
    gate('Discover is opt-in and current live owner-signed record wins', 'DiscoverView' in contacts_ui and 'Make private / unlist' in contacts_ui and 'PublicDirectoryState' in peer_native and 'verify_gtcd(&descriptor)' in peer_native and 'existing.blue_score > blue_score' in peer_native and 'if !descriptor.discoverable' in peer_native and 'Public Ghost Talk profiles are optional' in contacts_ui)
    gate('Discover shows up to 100 verified public users with direct chat action', 'Top public users' in contacts_ui and 'publicUsers.slice(0, 100)' in contacts_ui and 'Start chat' in contacts_ui and 'publicDirectory' in storage_ui and 'MAX_PUBLIC_DIRECTORY = 100' in storage_ui and 'startDirectChatFromDiscover' in ui_main)
    sidebar_ui = text('crates/ghost-app/ui/src/components/Sidebar.tsx')
    style_ui = text('crates/ghost-app/ui/src/style.css')
    gate('sidebar shows Kaspa connection lifecycle beside Ghost Talk brand', 'network-dot' in sidebar_ui and 'reconnectAttempts' in sidebar_ui and all(f'.network-dot.{state}' in style_ui for state in ['connected', 'disconnected', 'connecting', 'reconnecting']))
    gate('Kaspa status listeners attach before monitor/RPC and live refresh confirms connected', 'const listeners = await Promise.allSettled([' in main_ui and main_ui.find('const listeners = await Promise.allSettled([') < main_ui.find('await startWalletMonitor({') < main_ui.find('const value = await refreshWallet(') and 'setNetworkStatus("connected")' in main_ui and 'setReconnectAttempts(0)' in main_ui and 'permanently blinking yellow' in main_ui)
    gate('native profile mirror persists IDs in ordered writes', 'profiles-v2.json' in profile_state_native and 'nativeSaveChain' in main_ui and '.then(() => saveNativeProfileState(serialized))' in main_ui and 'loadNativeProfileState()' in main_ui)
    gate('wallet login refresh does not present stale zero balance', 'setSnapshot(undefined)' in main_ui and 'refreshWallet(' in main_ui and 'Loading…' in wallet_ui)
    gate('used receive addresses rotate automatically', 'recommended_receive_index' in text('crates/ghost-app/src-tauri/src/wallet_commands.rs') and 'rotateReceiveAddress' in main_ui and 'Used receive addresses rotate automatically' in wallet_ui)
    gate('copy address gives visible success feedback', '✓ Copied!' in wallet_ui and 'copiedAddress' in wallet_ui)
    gate('send transaction status stays inside send card', 'sendStatus' in wallet_ui and wallet_ui.find('{sendStatus && <div className="status send-status">{sendStatus}</div>}') > wallet_ui.find('Send KAS'))
    gate('new-ID initialization uses a dedicated loading gate', 'if (busy)' in identity_gate and identity_gate.find('if (busy)') < identity_gate.find('if (backup)') and 'Creating Ghost Talk ID' in identity_gate and 'recovery screen will appear when initialization is complete' in identity_gate)
    gate('locked Kaspa view is a real gate', 'if (!sessionUnlocked)' in wallet_ui and wallet_ui.find('if (!sessionUnlocked)') < wallet_ui.find('className="wallet-grid"') and 'unlockStatus && <div className="status">{unlockStatus}</div>' in wallet_ui)
    gate('HYDRA unlock reuses an already-open native profile', 'runtime_if_present' in hydra_native and 'reuse that handle instead of attempting a second' in hydra_native and 'native profile is already open' not in hydra_native)
    native_store = text('external/hydra-msg/crates/hydra-msg/src/persistence/native_store.rs')
    native_lock_tests = text('external/hydra-msg/crates/hydra-msg/src/tests/native_concurrency.rs')
    native_lock_region = native_store[native_store.find('impl NativeProfileLock'):native_store.find('/// Native opaque-byte store')]
    gate('HYDRA native profile lock is OS-backed and stale-marker safe', 'file.try_lock()' in native_lock_region and 'TryLockError::WouldBlock' in native_lock_region and '.create_new(true)' not in native_lock_region and 'stale_native_profile_marker_does_not_block_reopen' in native_lock_tests)
    gate('Kaspa and encrypted peer resolution share one unlocked session', 'setSessionPassword(password)' in main_ui and 'Unlock wallet + mailbox in Kaspa before resolving encrypted peers.' in main_ui and 'sessionUnlocked={Boolean(sessionPassword)}' in main_ui)
    gate('security prompt policies require password authorization', 'Wallet security prompts' in settings_ui and 'changeSecuritySetting' in settings_ui and 'await unlockWallet({' in settings_ui and 'requireUnlockPassword' in settings_ui and 'requireSendPassword' in settings_ui)
    gate('optional automatic unlock uses isolated authenticated device credential', 'Aes256Gcm' in remembered_unlock_native and 'Payload {' in remembered_unlock_native and 'remembered_unlock_set' in remembered_unlock_native and 'remembered_unlock_load' in remembered_unlock_native and 'loadRememberedUnlock' in main_ui and 'setRememberedUnlock' in settings_ui)
    gate('unlocked session can authorize sends when per-send password is disabled', 'requireSendPassword ? sendPassword : sessionPassword' in wallet_ui and 'Authorized by the unlocked wallet + mailbox session.' in wallet_ui and 'requireSendPassword: false' in storage_ui)
    gate('software KAS sends reuse the unlocked secret only when per-send password policy permits', 'reuse_unlocked: bool' in wallet_native and 'let secret = if reuse_unlocked' in wallet_native and 'state.secret_or_open(&profile_id, &password, &sealed, &public)?' in wallet_native and 'let secret = open_secret(&password, &sealed)?;' in wallet_native and 'reuseUnlocked: !requireSendPassword' in wallet_ui and 'reuseUnlocked: args.reuseUnlocked' in text('crates/ghost-app/ui/src/native.ts'))
    gate('unlocked HYDRA mailbox avoids repeated password KDFs', 'identity_id: String' in hydra_native and 'selected HYDRA identity does not match the unlocked Ghost Talk ID' in hydra_native and 'set_active_identity(&identity_id, &password)' not in mailbox_native and 'set_active_identity(&identity_id, &password)' not in peer_native)
    gate('native profile mirror survives interrupted Windows replacement', 'profiles-v2.json.bak' in profile_state_native and 'file.sync_all()' in profile_state_native and 'fs::rename(&path, &backup)' in profile_state_native and 'read_profile_state(&backup_path(&path)?)' in profile_state_native)
    gate('native profile loader recovers fsynced chat snapshot staged at process exit', 'recover_staged_profile_state(&path)?' in profile_state_native and 'read_profile_state(&temp)' in profile_state_native and 'newest visible state after a process interruption' in profile_state_native and 'let _ = fs::remove_file(&temp);' in profile_state_native)
    protocol_source = text('crates/ghost-protocol/src/lib.rs')
    ghost_kaspa_source = text('crates/ghost-kaspa/src/lib.rs')
    storage_source = text('crates/ghost-app/ui/src/storage.ts')
    gate('signed durable acknowledgements are bootstrap-only and stale per-message ACK queues are dropped', 'GTACK_MAGIC' in protocol_source and 'GhostDeliveryAck' in protocol_source and 'verify_delivery_ack(&ack)' in hydra_native and 'delivery_ack_peer: Some(acknowledgement_peer.clone())' in hydra_native and 'sign_delivery_ack(&mut ack' in mailbox_native and 'purpose: "handshake"' in text('crates/ghost-app/ui/src/model.ts') and 'ack.purpose !== "handshake"' in storage_source and 'if (result.session_established_peer && validWireId(result.message_id))' in main_ui and 'Do not create a second on-chain ACK transaction for ordinary text' in main_ui)
    gate('stale ratchet ciphertext triggers authenticated re-handshake without persisting sessions', 'MAILBOX_ENVELOPE_MAGIC' in hydra_native and 'pending_recovery' in hydra_native and 'discard: false' in hydra_native and 'mailbox_send_recovery_offer' in mailbox_native and 'HANDSHAKE_FINISH_ONLY' in hydra_native and 'export_transaction_checkpoint' not in hydra_facade)
    gate('logical message ids drive dedupe exact retry and post-recovery resend', 'newWireId()' in main_ui and 'message.wireId === wireId' in main_ui and 'exact failed-submit retry' in text('crates/ghost-app/ui/src/model.ts') and 'session_established_peer' in main_ui and 'resendAwaitingForPeer' in main_ui and 'messageId: message.wireId' in main_ui)
    gate('Kaspa address or KNS is sufficient user input for contacts', 'Kaspa address or KNS' in contacts_ui and 'Name (optional)' in contacts_ui and '!label.trim() || !target.trim()' not in contacts_ui and 'first message uses a private Kaspa request/accept bootstrap' in contacts_ui)
    gate('HYDRA mailbox preserves text message kind', 'HydraMessage::text' in text('crates/ghost-hydra/src/lib.rs') and 'HydraMessage::bytes(data.to_vec())' not in text('crates/ghost-hydra/src/lib.rs') and 'm.text()' in text('crates/ghost-hydra/src/lib.rs'))
    kassigner_native = text('crates/ghost-app/src-tauri/src/kassigner_commands.rs')
    kassigner_modal = text('crates/ghost-app/ui/src/components/KasSignerSigningModal.tsx')
    kassigner_camera = text('crates/ghost-app/ui/src/kassignerCamera.ts')
    hydra_upstream_pin = text('external/hydra-msg/GHOST_TALK_UPSTREAM.md')
    gate('chat rows open threads directly and notifications focus the exact chat', 'aria-label={`Open chat with ${compactKaspaLabel(threadLabelSource(chat))}`}' in chat_ui and 'onClick={() => setSelectedId(chat.id)}' in chat_ui and 'focusChatId' in chat_ui and 'onFocusHandled' in chat_ui and 'findNewIncomingMessageChatId' in main_ui and '>Open chat</button>' in main_ui)
    gate('outgoing chat messages mark successful Kaspa broadcast as Delivered', 'sendState: "sending"' in main_ui and 'sendState: result.pending_handshake ? "sent" : "delivered"' in main_ui and 'sendState: "delivered"' in main_ui and 'Broadcasting to Kaspa…' in chat_ui and 'Awaiting delivery…' not in chat_ui and '✓✓ Delivered' in chat_ui and 'normalized.pendingStage === "finish" || normalized.pendingStage === "delivery"' in storage_ui)
    gate('chat archive and leave are explicit local/session actions', 'onArchive' in chat_ui and 'onLeave' in chat_ui and 'Archive' in chat_ui and 'Leave' in chat_ui and 'leaveHydraPeer' in main_ui and 'left: true, archived: true' in main_ui)
    gate('archived chats remain reachable when no active thread exists', 'Archived chats ({archivedChats.length})' in chat_ui and 'setShowArchived(true)' in chat_ui and 'No active chats' in chat_ui and 'Unarchive' in chat_ui)
    gate('archive is sticky and historical request SIDs cannot overwrite an active thread', 'Archive is a sticky local history boundary.' in main_ui and '!chat.archived' in main_ui and 'const id = crypto.randomUUID();' in main_ui and 'fresh 128-bit' in main_ui and 'setChatFocusId(existingActive.id)' in main_ui and 'setChatFocusId(id)' in main_ui and 'This chat is archived. Unarchive it explicitly before sending' in main_ui and 'const sameSession = chats.find' in main_ui and 'const activePeerSession = chats.find' in main_ui and 'if (activePeerSession) return chats;' in main_ui and 'Boolean(chat.archived)' in chat_ui)
    gate('mailbox delivery is bound to the exact active KKTP SID across restart/history replay', 'active_session_sids: Vec<String>' in hydra_native and 'allowed_kktp_sids' in hydra_native and 'kktp_sid_is_current' in hydra_native and all(token in hydra_native for token in ['stale-response-discarded', 'stale-handshake-discarded', 'stale-message-discarded', 'stale-session-end-discarded']) and 'session_sid: Some(sid.to_owned())' in hydra_native and 'activeSessionSids: activeConversationSids(chats)' in main_ui and 'chat.sessionSid !== sessionSid' in main_ui and 'message-without-active-session-discarded' in main_ui)
    gate('restart recovery persists its replacement SID only after successful Kaspa submit', 'pub sid: String' in hydra_native and 'pub peer_hydra_id: String' in hydra_native and 'recovery-sid-committed' in main_ui and 'sessionSid: recoveryProjection.sid' in main_ui and main_ui.index('const recovery = await sendMailboxRecoveryOffer') < main_ui.index('recovery-sid-committed'))
    gate('selected chat auto-scrolls to the newest appended message', 'const messagesRef = useRef<HTMLDivElement | null>(null)' in chat_ui and 'node.scrollTop = node.scrollHeight' in chat_ui and '[selected?.id, selected?.messages.length]' in chat_ui and 'ref={messagesRef}' in chat_ui)
    gate('direct chat can prefill Contacts add form with peer Kaspa address', 'onAddContact' in chat_ui and '＋ Contact' in chat_ui and 'addDirectChatPeerToContacts' in main_ui and 'setTab("Contacts")' in main_ui and 'prefillTarget={contactPrefillTarget}' in main_ui and 'Kaspa address copied from the direct chat' in contacts_ui and 'matchingDirectChat' in main_ui and '!chat.contactId && chat.peerKaspaAddress === updated.kaspaAddress' in main_ui)
    gate('unlock password policy is durably versioned and reconciled across rebuild origins', 'securityPolicyRevision' in storage_source and 'reconcileSecurityPolicies' in storage_source and 'persistProfileUpdate' in main_ui and 'await onPersistUpdate' in settings_ui and 'reconcileRememberedUnlockPolicies' in main_ui and 'saveProfiles(updated);' in main_ui and 'await queueNativeProfileSave(updated);' in main_ui)
    gate('accepted first-contact commits then requester initiates exactly one HYDRA bootstrap', 'Commit authenticated acceptance first, then initiate exactly one HYDRA' in main_ui and 'const send = await sendMailboxMessage({' in main_ui and 'pendingStage: send.pending_handshake ? "handshake" : undefined' in main_ui and 'Acceptance remains committed. Retry only because the actual carrier' in main_ui)
    gate('acceptor and rapid follow-up messages queue behind the one initiator handshake', 'const queueBehindHandshake = acceptedAwaitingHandshake || handshakeInFlight;' in main_ui and 'if (queueBehindHandshake)' in main_ui and 'message.id === skipMessageId' in main_ui and 'competing local initiator handshake takes precedence' not in hydra_native and 'session_established_peer: Some(sender_id)' in hydra_native and 'session_established_peer: Some(pending.contact_id.clone())' in hydra_native)
    gate('HD cursor rotation does not restart the latency-sensitive mailbox monitor', 'HD receive/change cursors are wallet bookkeeping, not monitor identity.' in main_ui and 'active.wallet.public.account_path,\n        // HD receive/change cursors are wallet bookkeeping' in main_ui and 'down the cached Portal in the middle of GTCR -> GTCA -> HYDRA bootstrap.' in main_ui)
    gate('long-lived mailbox monitor receives live HD cursor updates without reconnect', 'wallet_monitor_update_public' in mailbox_native and 'Latest public HD cursor state for each running profile' in mailbox_native and 'let current_public = mailbox_publics' in mailbox_native and 'updateWalletMonitorPublic(active.id, active.wallet.public)' in main_ui and 'fresh GTCR could target a newly advertised address' in main_ui)
    gate('one signed Discovery SID maps to one request thread and authenticated Response pairing', 'collapseDuplicateRequestThreads(chats, incoming.request_id);' in main_ui and 'collapseDuplicateRequestThreads(profile.chats, request.requestId, chatId)' in main_ui and 'return Boolean(id && (!requestId || id === requestId));' in main_ui and 'normalizeChats(Array.isArray(profile.chats)' in storage_source and 'const requestIndex = new Map<string, number>();' in storage_source and 'legacySuccessReplay' in storage_source and 'const queued = findQueuedContactRequest(chats, accepted.request_id);' in main_ui and 'if (queued) {' in main_ui and 'queued.chat.peerKaspaAddress === accepted.acceptor_address' not in main_ui)

    gate('authenticated Response pairing survives restart flags and buffers orphans', 'item.contactRequestId === requestId' in ui_main and 'chat.sessionSid !== requestId' in ui_main and 'response-buffered-until-request-pairs' in ui_main and 'result.discard && !retainEnvelopeForPairing' in ui_main and 'pendingStage === "request"' not in ui_main[ui_main.find('function findQueuedContactRequest'):ui_main.find('function queueDeliveryAck')])
    gate('protocol debug logging is opt-in redacted and bounded', 'debugLogging: false' in text('crates/ghost-app/ui/src/storage.ts') and 'MAX_ENTRIES: usize = 4_000' in debug_native and 'if !is_enabled()' in debug_native and 'Message plaintext, passwords, private keys, seed words and ciphertext are not logged' in debug_window_ui and 'Enable protocol/network debug logging' in settings_ui)
    gate('protocol debug window exposes handshake boundaries and redacted session state', 'Protocol & Kaspa Debug' in debug_window_ui and 'getHydraDebugState' in debug_window_ui and 'hydra_debug_state' in hydra_native and 'response-received-verified' in hydra_native and 'pq-control-received' in hydra_native and 'pq-finish-accepted-session-active' in hydra_native and 'envelope-processing-failed' in ui_main)
    gate('leave and fresh acceptance clear stale per-peer handshake wrapper state', 'clear_peer_handshake_state' in hydra_native and 'prepared_completion' in hydra_native and 'prepared_recovery_finish' in hydra_native and 'clear_peer_handshake_state(&mut runtime, &contact_id);' in hydra_native and 'clear_peer_handshake_state(runtime, &contact.handle);' in hydra_native)
    gate('REST active-address indexing cannot suppress live mailbox delivery', 'active_addresses(' not in mailbox_native and 'live.blocks.recv()' in mailbox_native and 'historical_mailbox_before_ms' in mailbox_native)
    gate('incoming chat request UI is compact full-address accept-or-ignore', 'chat-request-banner compact' in chat_ui and '<code className="request-address" title={request.peerAddress}>{request.peerAddress}</code>' in chat_ui and 'This sender used a direct Kaspa request' not in chat_ui and 'Incoming encrypted chat request' not in chat_ui and '.chat-request-banner.compact' in style_ui)
    gate('chat Kaspa labels omit network prefix and retain both address ends', 'threadLabelSource' in chat_ui and 'payload.slice(0, 6)' in chat_ui and 'payload.slice(-4)' in chat_ui and 'return `${payload.slice(0, 6)}...${payload.slice(-4)}`' in chat_ui and 'thread-row-text' in style_ui)
    gate('voice messages and 1:1 Kaspa live calls are separate working controls', '☎ Call' in chat_ui and 'toggleVoiceMessage' in chat_ui and 'Voice message' in chat_ui and 'LIVE_VOICE_WINDOW_MS = 350' in voice_ui and 'LIVE_VOICE_JITTER_MS = 400' in voice_ui and 'playbackRef.current.push(packet.sequence!' in chat_ui and 'LiveVoiceWindowRecorder' in chat_ui and 'encodeLiveVoicePacket' in chat_ui and 'decodeVoiceMessage' in chat_ui and '.composer button:nth-last-child(2){display:none}' not in style_ui)
    gate('live voice capture continues while the previous carrier is sent', 'flushAndRestart' in voice_ui and 'restart capture before the caller awaits encryption' in voice_ui and 'windowStartedAt = performance.now()' in chat_ui and 'const { mime, blob } = await recorder.flushAndRestart();' in chat_ui and 'recordIndependentOpusWindow' not in chat_ui)
    gate('live voice playback is sequence ordered and gaplessly scheduled on one AudioContext timeline', 'context.decodeAudioData' in voice_ui and 'source.start(startAt)' in voice_ui and 'this.nextStartTime = startAt + decoded.duration' in voice_ui and 'LIVE_VOICE_GAP_WAIT_MS = 1_200' in voice_ui and 'new Audio(' not in voice_ui)
    gate('live voice decode uses concrete ArrayBuffer compatible with TypeScript DOM BlobPart generics', 'const encoded = new ArrayBuffer(bytes.byteLength);' in voice_ui and 'new Uint8Array(encoded).set(bytes);' in voice_ui and 'context.decodeAudioData(encoded)' in voice_ui and 'new Blob([bytes]' not in voice_ui)
    gate('live voice sequence has no sender-created holes and starts playout from sequence zero', 'const sequence = liveSequenceRef.current;' in chat_ui and 'liveSequenceRef.current = sequence + 1;' in chat_ui and 'const sequence = liveSequenceRef.current++;' not in chat_ui and 'this.expectedSequence = 0;' in voice_ui)
    gate('Auto direct media uses an ordered reliable WebRTC data channel', 'createDataChannel("ghost-media", { ordered: true })' in webrtc_source and 'ordered: false' not in webrtc_source and 'maxRetransmits' not in webrtc_source)
    gate('voice request and accept route by exact authenticated SID', 'REALTIME_MAILBOX_MAGIC: &[u8; 4] = b"GTR1"' in hydra_native and 'session_sid: Some(sid)' in hydra_native and 'candidate.sessionSid?.toLowerCase() === sessionSid.toLowerCase()' in main_ui and 'realtime-control-wrong-session-discarded' in main_ui)
    gate('voice call ref commits synchronously before peer accept can arrive', 'callRef.current = next;' in chat_ui and 'callRef.current = incoming;' in chat_ui and 'callRef.current = connected;' in chat_ui)
    gate('voice modal contains long peer addresses', 'voice-call-peer' in chat_ui and '.voice-call-peer{min-width:0;max-width:100%;white-space:normal;overflow-wrap:anywhere;word-break:break-all}' in style_ui and '.voice-call-modal{min-width:0;overflow:hidden}' in style_ui)
    gate('live voice avoids per-window delivery ACKs and HD change exhaustion', 'Kaspa fallback media windows do not create chat bubbles or per-window delivery-ack transactions' in chat_ui and 'reuseChange' in main_ui and 'send_payload_reuse_change' in wallet_engine and 'prepare_realtime_mailbox' in mailbox_native)
    gate('realtime and durable mailbox sends serialize wallet UTXO handoff', 'outbound_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>' in wallet_native and 'wallet_state.outbound_lock(&profile_id)?' in mailbox_native and 'let _outbound_guard = outbound_lock.lock().await;' in mailbox_native and 'wait_for_wallet_utxo_change(portal, public, visible_total).await?;' in mailbox_native and 'Auto signaling, voice control/media, and durable text' in mailbox_native)
    gate('mailbox latency path consumes BlockAdded directly and keeps archival REST independent', 'MAILBOX_SCAN_INTERVAL: Duration = Duration::from_millis(100)' in mailbox_native and 'MAILBOX_PRIORITY_HISTORY_INTERVAL: Duration = Duration::from_secs(60)' in mailbox_native and 'MAILBOX_BROAD_HISTORY_INTERVAL: Duration = Duration::from_secs(10 * 60)' in mailbox_native and 'WALLET_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(3)' in mailbox_native and 'priority_history_sync' in mailbox_native and 'broad_history_sync' in mailbox_native and 'snapshot_sync' in mailbox_native and 'live.blocks.recv()' in mailbox_native and 'historical_mailbox_before_ms' in mailbox_native and 'outbound_portal' in mailbox_native)
    gate('ordinary Kaspa send uses the cached Portal 1.0.1 connection and payload-aware planner', '.timeout_ms(' not in ghost_kaspa_source and '.max_retries(' not in ghost_kaspa_source and 'tokio::time::timeout(Duration::from_secs(3), portal.connect())' not in gateway_native and 'outbound_portal(' in wallet_native and 'kaspa-send-timing' in wallet_native and 'utxo_plan_ms' in wallet_engine and 'submit_ms' in wallet_engine and '.plan_send_with_payload(' in wallet_engine)
    gate('successful Kaspa send updates balance immediately and reconciles only against node UTXOs', 'optimisticSnapshotAfterSend' in wallet_view and 'pendingSoftwareBalance' in wallet_view and 'balance syncing' in wallet_view and 'transactionVisible' not in wallet_view and 'fresh.balance_sompi === args.optimistic.balance_sompi' in wallet_view and 'REST is archival (>48h)' in wallet_view)
    gate('REST history has bounded interactive network timeouts', 'connect_timeout(std::time::Duration::from_secs(2))' in history_source and '.timeout(std::time::Duration::from_secs(5))' in history_source)
    gate('automatic endpoint selection is metadata-only and hands a concrete TLS wRPC URL to Kaspa Portal', '{resolver}/v2/kaspa/{network}/tls/wrpc/borsh' in wallet_native and all(host in wallet_native for host in ['eric.kaspa.stream', 'troy.kaspa.stream', 'alex.kaspa.red', 'liam.kaspa.green', 'luke.kaspa.blue']) and 'TLS resolver returned non-WSS endpoint' in wallet_native and 'PortalFacade::connect(network, &endpoint).await' in gateway_native and 'connect_async(' not in gateway_native and 'connect_async(' not in wallet_native)
    gate('Kaspa Portal 1.0.1 is imported unchanged and connected through its published builder API', cargo_root.count('kaspa-portal = "1.0.1"') == 1 and not (ROOT / 'external/kaspa-portal').exists() and '.network(network)' in ghost_kaspa_source and '.endpoint(endpoint)' in ghost_kaspa_source and '.connect()' in ghost_kaspa_source and '.indexer(indexer)' not in ghost_kaspa_source and 'reqwest.workspace = true' in text('crates/ghost-app/src-tauri/Cargo.toml') and 'reqwest::header::ACCEPT' in wallet_native and 'reqwest::header::USER_AGENT' in wallet_native)
    gate('Portal 1.0.1 native Send futures eliminate the Ghost executor shim', 'portal: kaspa_portal::KaspaPortal' in ghost_kaspa_source and 'PortalCommand' not in ghost_kaspa_source and 'PORTAL_COMMAND_CAPACITY' not in ghost_kaspa_source and 'std::thread::Builder::new()' not in ghost_kaspa_source and 'new_current_thread()' not in ghost_kaspa_source and 'portal_worker_loop' not in ghost_kaspa_source and 'require_send_sync::<PortalFacade>()' in ghost_kaspa_source and 'require_send(PortalFacade::connect(' in ghost_kaspa_source)
    gate('BlockAdded uses Portal 1.0.1 first-class API without a duplicate websocket or wire decoder', '.subscribe_block_added()' in ghost_kaspa_source and '.next_block_added()' in ghost_kaspa_source and 'tokio_tungstenite' not in ghost_kaspa_source and 'connect_async' not in ghost_kaspa_source and 'decode_wrpc_envelope' not in ghost_kaspa_source and 'decode_block_added' not in ghost_kaspa_source and 'subscription::block_added' not in ghost_kaspa_source and 'tokio-tungstenite' not in cargo_root and 'futures-util' not in cargo_root)
    gate('current wallet truth comes only from the shared Kaspa Portal', 'portal.current_utxos(&addresses)' in wallet_native and 'portal.current_virtual_daa_score()' in wallet_native and 'history.balances(' not in wallet_native and 'historical.active_addresses(' not in wallet_native and 'historical.utxo_count(' not in wallet_native and 'historical.virtual_blue_score(' not in wallet_native)
    gate('settings label REST as archival-only and public wRPC as current authority', 'Historical REST endpoint (&gt;48h only)' in settings_ui and 'resolve a real public Kaspa node automatically' in settings_ui and 'it never supplies balance, UTXOs, DAA, connection status, sends, or live chat' in settings_ui)
    gate('native app uses REST only for proven older-than-48-hour archival history', 'wallet_history_before_ms' in wallet_native and 'historical_mailbox_before_ms' in mailbox_native and 'wallet_history_before_ms' in backup_native and 'RestHistory' not in peer_native and 'const HISTORICAL_REST_MIN_AGE: Duration = Duration::from_secs(48 * 60 * 60)' in wallet_native and 'FORTY_EIGHT_HOURS_MS: u64 = 48 * 60 * 60 * 1_000' in mailbox_native and 'saturating_sub(48 * 60 * 60 * 1_000)' in backup_native)
    gate('REST history crate exposes archival transaction retrieval only', 'pub async fn wallet_history_before_ms' in history_source and 'pub async fn historical_mailbox_before_ms' in history_source and 'pub async fn balances' not in history_source and 'pub async fn active_addresses' not in history_source and 'pub async fn utxo_count' not in history_source and 'pub async fn virtual_blue_score' not in history_source and 'pub async fn virtual_parent_hash' not in history_source and 'pub async fn live_blocks_since' not in history_source and 'pub async fn dag_walk' not in history_source and '/addresses/balances' not in history_source and '/addresses/active' not in history_source and '/utxos/count' not in history_source and '/info/blockdag' not in history_source and '/blocks-from-bluescore' not in history_source)
    gate('mailbox debug distinguishes direct live carriers from recovery history', 'live-block-carriers-observed' in mailbox_native and 'priority-history-carriers-observed' in mailbox_native and 'broad-history-carriers-observed' in mailbox_native and 'without_output_addresses' not in mailbox_native)
    gate('mailbox address-history delivery never waits for public-node reconnect', 'let mut live_reconnect: Option<tokio::task::JoinHandle<Result<LiveBlockStream, String>>>' in mailbox_native and 'live_reconnect = Some(tokio::spawn(async move {' in mailbox_native and 'Never await endpoint discovery/connection inside the authoritative' in mailbox_native and 'task.abort();' in mailbox_native)
    gate('reconnect attempts use bounded backoff rather than 500ms scan churn', 'let mut reconnect_attempts = 0u32;' in mailbox_native and 'reconnect_attempts = reconnect_attempts.saturating_add(1);' in mailbox_native and 'live_retry_delay(reconnect_attempts)' in mailbox_native and '.min(30)' in mailbox_native and 'failure_streak' not in mailbox_native)
    gate('active reconnect state is not overwritten by disconnected in the same monitor loop', 'live-block-stream-connect-failed' in mailbox_native and 'last_network_status = "reconnecting";' in mailbox_native and 'else if last_network_status != "disconnected"' not in mailbox_native)
    gate('ambiguous mailbox submits are never replayed and gateway owns transport lifecycle', 'send_payload_via_gateway' in mailbox_native and 'Never replay an ambiguous SubmitTransaction automatically' in mailbox_native and 'gateway.note_operation_error(error).await' in mailbox_native and 'self.invalidate_generation(generation, &endpoint, error).await' in gateway_native and 'state.portals.write().await.remove(profile_id);' not in mailbox_native)
    gate('KasSigner SDK owns hardware signing while Ghost Talk retains wallet and broadcast policy', (ROOT / 'external/kassigner-sdk').is_dir() and (ROOT / 'external/kassigner-protocol').is_dir() and (ROOT / 'external/shared-signer').is_dir() and 'kassigner_sdk::prepare' in kassigner_native and 'kassigner_sdk::complete' in kassigner_native and 'kassigner_sdk::finalize' in kassigner_native and 'plan_send_unsigned' in kassigner_native and 'plan_consolidation_unsigned' in kassigner_native and 'broadcast_external_signed_pskb' in kassigner_native)
    gate('KasSigner hardware flow has QR request camera response and software fallback selector', 'KasSignerSigningModal' in wallet_ui and 'Software' in wallet_ui and 'KasSigner' in wallet_ui and 'openKasSignerCamera' in kassigner_modal and 'getUserMedia' in kassigner_camera and 'scanKasSignerResponseFrame' in kassigner_modal and 'Paste signed response hex instead' in kassigner_modal and 'Verify and broadcast' in kassigner_modal)
    gate('KasSigner animated request QR supports back pause resume and forward controls', 'const [qrPaused, setQrPaused]' in kassigner_modal and 'frames.length <= 1 || qrPaused' in kassigner_modal and '← Back' in kassigner_modal and '{qrPaused ? "Resume" : "Pause"}' in kassigner_modal and 'Forward →' in kassigner_modal and 'kassigner-frame-controls' in style_ui)
    gate('KasSigner response scanner exposes and renders per-frame completion dots', 'bits: Vec<bool>' in kassigner_native and 'bits: progress.bits' in kassigner_native and 'bits: boolean[]' in text('crates/ghost-app/ui/src/native.ts') and 'setScanBits(progress.bits.slice(0, progress.total))' in kassigner_modal and 'scanBits.some(Boolean)' in kassigner_modal and 'kassigner-frame-dot scanned' in kassigner_modal and '.kassigner-frame-dot.scanned' in style_ui)
    gate('Ghost Talk translates Portal output derivations before every KasSigner prepare', 'kaspaPortalDerivation' in kassigner_native and 'kassigner_sdk::attach_output_derivation' in kassigner_native and 'prepare_kassigner_request' in kassigner_native and kassigner_native.count('prepare_kassigner_request(&pskb') == 3 and 'portal_derivation_translation_preserves_change_hint' in kassigner_native and 'portal_derivation_translation_rejects_malformed_hint' in kassigner_native)
    gate('KasSigner hardware signing derives authority from the correct wallet source before QR', 'canonical_ghost_public_from_secret' in kassigner_native and 'validate_public_projection(secret, public)' in kassigner_native and 'ghost_kaspa::wallet::derive_public(secret)' in kassigner_native and 'canonical_kassigner_public' in kassigner_native and 'kassigner_sdk::pair_normal(kpub, network)' in kassigner_native and 'validate_portal_output_derivation' in kassigner_native and 'kassigner_protocol::address_to_script_pubkey(address)' in kassigner_native and 'does not match the authoritative wallet derivation; signing request refused' in kassigner_native and 'profileId: kasSignerWallet ? undefined : profile.id' in wallet_ui and 'sealed: kasSignerWallet ? undefined : wallet?.sealed' in wallet_ui and 'accountFingerprint: kasSignerWallet?.accountFingerprint' in wallet_ui)
    native_ui = text('crates/ghost-app/ui/src/native.ts')
    send_wrapper = native_ui[native_ui.index('export async function sendKaspa'):native_ui.index('export async function consolidateKaspa')]
    gate('software send wrapper never carries KasSigner account fingerprint metadata', 'accountFingerprint' not in send_wrapper and 'accountFingerprint: args.accountFingerprint || null' in native_ui)
    kassigner_kpub_scanner = text('crates/ghost-app/ui/src/components/KasSignerKpubScanner.tsx')
    kassigner_proof_modal = text('crates/ghost-app/ui/src/components/KasSignerIdentityProofModal.tsx')
    gate('KasSigner camera scanning preserves high-resolution QR detail on Android', 'MAX_SCAN_WIDTH = 1280' in kassigner_camera and 'MAX_SCAN_HEIGHT = 960' in kassigner_camera and 'focusMode' in kassigner_camera and 'luminanceBase64' in kassigner_camera and 'decode_camera_luminance' in kassigner_native and 'BASE64' in kassigner_native and 'captureKasSignerLuminance' in kassigner_modal and 'captureKasSignerLuminance' in kassigner_kpub_scanner and 'captureKasSignerLuminance' in kassigner_proof_modal and '640 / sourceWidth' not in kassigner_modal and '640 / sourceWidth' not in kassigner_kpub_scanner and '640 / sourceWidth' not in kassigner_proof_modal)
    gate('KasSigner camera flows invalidate pending getUserMedia and scan work', 'cameraGenerationRef' in kassigner_modal and 'generation !== cameraGenerationRef.current' in kassigner_modal and 'cameraGenerationRef' in kassigner_kpub_scanner and 'generation !== cameraGenerationRef.current' in kassigner_kpub_scanner and 'generationRef' in kassigner_proof_modal and 'generation !== generationRef.current' in kassigner_proof_modal)
    gate('KasSigner-backed ID imports only public account material', 'kassigner_import_kpub' in kassigner_native and 'kassigner_sdk::pair_normal' in kassigner_native and 'watch_only: true' in kassigner_native and 'kpub: Some(canonical_kpub)' in kassigner_native and 'KasSigner account kpub' in identity_gate and 'Scan KasSigner kpub QR' in identity_gate and 'scanKasSignerAccountQr' in kassigner_kpub_scanner)
    gate('KasSigner kpub import requires hardware proof-of-possession before ID completion', 'kassigner_begin_identity_proof' in kassigner_native and 'kassigner_complete_identity_proof' in kassigner_native and 'KasSigner Signed Message v1\\0' in text('crates/ghost-kaspa/src/lib.rs') and 'account_xonly_from_kpub' in kassigner_native and 'message_hash != expected_hash' in kassigner_native and 'verify_kassigner_message_signature' in kassigner_native and 'ownershipProof: proof' in identity_gate and 'Sign Message' in kassigner_proof_modal and 'copied/stolen kpub alone' in kassigner_proof_modal)
    gate('KasSigner-backed financial wallet cannot silently fall back to software signing', 'const kasSignerWallet = profile.kasSignerWallet' in wallet_ui and 'setSignerMode(profile.kasSignerWallet ? "kassigner" : "software")' in wallet_ui and 'KasSigner · watch-only' in wallet_ui and 'every spend must be approved on the matching KasSigner hardware wallet' in wallet_ui and 'onKasSignerWallet' in wallet_ui)
    gate('KasSigner-backed chat keeps a separate low-value software mailbox root', 'separate low-value mailbox root' in identity_gate and 'Messaging still needs a private HYDRA identity and a low-value Kaspa mailbox fee key' in identity_gate and 'Ghost Talk mailbox fee wallet' in wallet_ui and 'messages never require a hardware scan' in wallet_ui and 'kasSignerWallet' in storage_ui)
    gate('KasSigner watch-only kpub survives Portal transaction planning', 'pub kpub: Option<String>' in text('crates/ghost-kaspa/src/wallet.rs') and 'pub watch_only: bool' in text('crates/ghost-kaspa/src/wallet.rs') and 'kpub: self.kpub.clone().unwrap_or_default()' in text('crates/ghost-kaspa/src/wallet.rs'))
    gate('current HYDRA stego commit is pinned and model-free deterministic cover is executable', 'a8b4b317cef85edce9d0f0528bb53125ebf6333b' in hydra_upstream_pin and 'hydra_stego::Stego::new()' in hydra_facade and 'send_compact' in hydra_facade and 'receive_compact' in hydra_facade and 'current_hydra_deterministic_stego_roundtrips_compact_carrier_bytes' in hydra_facade and 'Deterministic", available: true' in chat_ui)
    gate('HYDRA model-backed stego profiles are capability-gated instead of silently falling back', 'Fast Unicode", available: false' in chat_ui and 'Fast Hybrid", available: false' in chat_ui and 'Arithmetic", available: false' in chat_ui and 'Requires a configured local HYDRA language-model backend' in chat_ui)
    gate('KasSigner vendored SDK license attribution is present', 'kassigner-sdk' in text('LICENSES.md') and 'MIT OR Apache-2.0' in text('LICENSES.md') and 'rqrr-nostd' in text('LICENSES.md'))
    sdk_pkg = ROOT / 'external/kassigner-sdk/pkg'
    sdk_hashes = {
        'kassigner_sdk_bg.wasm': '3eb4cc606adc3c7042a5d84507927c5dd32a6a4f3a16cc79f17beace9bb27ca9',
        'kassigner_sdk.js': '4bd0f2e145347a4baa61602923a8961097e0ce653ecdb2e98c97daa5109cf017',
        'kassigner_sdk.d.ts': '18c2a3fd76481ab09bf9e2efb4eda02e176adb16e90ca587a8cac8b9d957d7d1',
        'kassigner_sdk_bg.wasm.d.ts': '03062fb3c8e8e73f5327503a4b2626c1d0629d2354211bfe91b1a53aeb30050b',
        'package.json': '6d6ea66bcb1ef37f22832272f03b95d044a3e7c4acb195c2a93a0594eb543b5e',
    }
    gate(
        'current supplied KasSigner SDK artifact is pinned exactly',
        all((sdk_pkg / name).is_file() and hashlib.sha256((sdk_pkg / name).read_bytes()).hexdigest() == expected for name, expected in sdk_hashes.items())
        and '3eb4cc606adc3c7042a5d84507927c5dd32a6a4f3a16cc79f17beace9bb27ca9' in text('external/KASSIGNER_UPSTREAM.md'),
    )
    kassigner_caps = text('external/kassigner-protocol/src/capabilities/mod.rs')
    gate(
        'native KasSigner SDK capability boundary matches supplied 2.0.0 package',
        'pub const SDK_VERSION: &str = "2.0.0";' in text('external/kassigner-sdk/src/lib.rs')
        and 'max_inputs: 32' in kassigner_caps
        and 'max_outputs: 8' in kassigner_caps
        and 'qr_max_frames: shared_signer::qr_frame::MAX_FRAMES as u8' in kassigner_caps
        and 'QR_MULTI_FRAME_FRAGMENT_BYTES: usize = 91' in kassigner_caps
        and 'QR_SINGLE_FRAME_PAYLOAD_BYTES: usize = 134' in kassigner_caps
        and 'qr_session_binding: true' in kassigner_caps
        and 'pub const GENERATION_CURRENT: u8 = 0x04' in text('external/kassigner-protocol/src/wire/kspt/model.rs')
        and 'pub const MAX_FRAMES: usize = 64' in text('external/shared-signer/src/qr_frame.rs'),
    )
    node = shutil.which('node')
    sdk_probe = ROOT / 'qa/checks/kassigner-sdk-probe.mjs'
    if node and sdk_probe.is_file():
        result = subprocess.run([node, str(sdk_probe)], cwd=ROOT, capture_output=True, text=True)
        gate('current supplied KasSigner SDK WASM runtime probe', result.returncode == 0, (result.stderr or result.stdout).strip())
    else:
        skip('current supplied KasSigner SDK WASM runtime probe', 'Node.js unavailable')
    room_examples = '\n'.join(p.read_text(encoding='utf-8', errors='replace') for p in (ROOT / 'examples').glob('*/src/*.rs'))
    gate('room examples authorize by ContactId', 'can_send_text(Role::' not in room_examples and 'can_send_audio(Role::' not in room_examples)
    gate('room examples pass owned labels', 'presenter(Id128([3; 16]), "Town Hall".to_owned())' in room_examples and 'radio(Id128([4; 16]), "Kaspa Radio".to_owned())' in room_examples)
    suite = text('qa/run-all-tests.py')
    fuzz_start = suite.find('banner("7/7 COVERAGE-GUIDED FUZZING')
    deep_fuzz = suite.find('"--deep-fuzz"', fuzz_start)
    deep_fuzz_end = suite.find('cwd=hydra)', deep_fuzz)
    success_banner = suite.find('banner("ALL GHOST TALK + UPSTREAM INTEGRATION TESTS PASSED")', deep_fuzz_end)
    post_deep = suite[deep_fuzz_end + len('cwd=hydra)'):success_banner] if deep_fuzz_end >= 0 and success_banner >= 0 else 'run('
    gate('fuzz last policy', 0 <= fuzz_start < deep_fuzz < deep_fuzz_end < success_banner and 'run(' not in post_deep)
    gate('Hydra deep fuzz included', '--deep-fuzz' in text('qa/run-all-tests.py'))
    hydra_source = text('crates/ghost-hydra/src/lib.rs')
    group_region_start = hydra_source.find('pub struct GroupGuard')
    group_region_end = hydra_source.find('fn parse_role', group_region_start)
    group_region = hydra_source[group_region_start:group_region_end] if group_region_start >= 0 and group_region_end >= 0 else ''
    gate(
        'HYDRA GroupError mapping',
        'fn group_error(error: hydra_group::GroupError) -> String' in hydra_source
        and '.map_err(group_error)?;' in group_region
        and '.map_err(group_error)' in group_region
        and 'map_err(|e| e.to_string())' not in group_region,
    )

    ghost_core_source = text('crates/ghost-core/src/lib.rs')
    ghost_protocol_source = text('crates/ghost-protocol/src/lib.rs')
    mailbox_source = text('crates/ghost-app/src-tauri/src/mailbox_commands.rs')
    native_ui_source = text('crates/ghost-app/ui/src/native.ts')
    gate('80 KiB app ceiling', 'MAX_GHOST_TX_PAYLOAD: usize = 80 * 1024' in ghost_core_source)
    gate('Portal 1.0.1 KSPT-v1 physical payload boundary is 65535 bytes', 'MAX_KSPT_V1_PAYLOAD_BYTES: usize = u16::MAX as usize' in ghost_core_source and 'KSPT_MAX_PAYLOAD_BYTES' not in ghost_core_source)
    gate('GHST physical frames fit the Portal KSPT-v1 boundary', 'GHST_DATA_MAX: usize = MAX_KSPT_V1_PAYLOAD_BYTES - GHST_HEADER' in ghost_protocol_source and 'eighty_kib_logical_carrier_only_fragments_at_real_kspt_v1_boundary' in ghost_protocol_source)
    gate('mailbox sender fragments only above the real KSPT-v1 physical boundary', 'decode_mailbox_payloads' in mailbox_source and 'payload.len() > ghost_core::MAX_KSPT_V1_PAYLOAD_BYTES' in mailbox_source and 'send_mailbox_payloads_via_gateway' in mailbox_source)
    gate('normal Kaspa HYDRA traffic uses authenticated compact envelopes', '.send_compact(id, msg)' in hydra_source and '.receive_compact(data)' in hydra_source and 'LITE_ENVELOPE_SIZE' in hydra_source)
    gate('fragmented HYDRA controls cross the Tauri boundary as vectors', 'payloads_hex: Vec<String>' in hydra_native and 'payloads_hex: Vec<String>' in mailbox_source and 'payloads_hex: string[]' in native_ui_source and 'payloadsHex: string[]' in native_ui_source and 'frame_single_control' not in hydra_native)
    gate('mailbox receiver enforces the same Portal KSPT-v1 bounds', 'KSPT_V1_MAX_PAYLOAD_BYTES = 65_535' in text('crates/ghost-app/ui/src/mailbox.ts') and 'GHST_DATA_MAX_BYTES = KSPT_V1_MAX_PAYLOAD_BYTES - GHST_HEADER_BYTES' in text('crates/ghost-app/ui/src/mailbox.ts') and 'declaredLength > GHST_DATA_MAX_BYTES' in text('crates/ghost-app/ui/src/mailbox.ts'))
    gate('Ghost Talk has no duplicate approximate Kaspa mass model', 'conservative_preflight' not in ghost_kaspa_source and 'MassEstimate' not in ghost_kaspa_source and 'TxShape' not in ghost_kaspa_source and 'payload_preflight' not in text('crates/ghost-wasm/src/lib.rs') and 'preflight,' not in text('crates/ghost-app/src-tauri/src/lib.rs'))
    gate('Portal-owned fee planning removes stale replan timing schema', 'signed_analysis_ms' in wallet_source and 'fee_replan_ms' not in wallet_source and 'replanned' not in wallet_source and 'signed_analysis_ms: number' in native_ui_source and 'fee_replan_ms' not in native_ui_source and 'replanned: boolean' not in native_ui_source)
    gate('GHST bounds', 'GHST_DATA_MAX' in ghost_protocol_source)
    gate('Kaspa Portal source is never patched or copied into Ghost Talk', not (ROOT / 'external/kaspa-portal').exists() and 'GHOST_TALK_PATCH.md' not in '\n'.join(str(path) for path in ROOT.rglob('*')))
    gate('80 KiB remains logical policy while Portal 1.0.1 owns transaction validation', 'MAX_GHOST_TX_PAYLOAD: usize = 80 * 1024' in ghost_core_source and 'MAX_KSPT_V1_PAYLOAD_BYTES: usize = u16::MAX as usize' in ghost_core_source and 'MAX_PAYLOAD_SIZE' not in ghost_kaspa_source and not (ROOT / 'external/kaspa-portal').exists())
    gate('dead Portal wallet/indexer compatibility wrappers are removed', 'PortalWalletProjection' not in ghost_kaspa_source and 'pub async fn import_kpub' not in ghost_kaspa_source and '.indexer()' not in ghost_kaspa_source and 'serde_json.workspace = true' not in text('crates/ghost-kaspa/Cargo.toml'))
    kaspa_source = text('crates/ghost-kaspa/src/lib.rs')
    field_reassign_hits = []
    import re
    field_reassign_pattern = re.compile(r"let\s+mut\s+(\w+)\s*=\s*[^;]+::default\(\);\s*\n\s*\1\.\w+\s*=", re.MULTILINE)
    for rust_file in (ROOT / 'crates').glob('ghost-*/src/**/*.rs'):
        if field_reassign_pattern.search(rust_file.read_text(encoding='utf-8', errors='replace')):
            field_reassign_hits.append(str(rust_file.relative_to(ROOT)))
    gate('no first-party field-reassign-with-default patterns', not field_reassign_hits, ','.join(field_reassign_hits))

    for p in (ROOT / 'crates').glob('*/Cargo.toml'):
        name = p.parent.name
        source = p.read_text()
        if name != 'ghost-hydra':
            gate(f'{name} no hydra import', 'hydra-msg' not in source and 'hydra-stego' not in source and 'hydra-group' not in source)
        if name != 'ghost-kaspa':
            gate(f'{name} no portal import', 'kaspa-portal' not in source)
    gate('Kinesis not Cargo dependency', 'kaspa-kinesis' not in '\n'.join(p.read_text() for p in (ROOT / 'crates').glob('*/Cargo.toml')))
    wasm_manifest = text('crates/ghost-wasm/Cargo.toml')
    gate('WASM getrandom js feature', "[target.'cfg(target_arch = \"wasm32\")'.dependencies]" in wasm_manifest and 'getrandom = { version = \"0.2\", features = [\"js\"] }' in wasm_manifest)

    bad = []
    for p in ROOT.rglob('*'):
        if p.is_file() and '.git' not in p.parts and any(x in p.parts for x in ['target', 'node_modules', '__pycache__']):
            bad.append(str(p.relative_to(ROOT)))
    gate('no generated build/cache files', not bad, ','.join(bad[:5]))
    gate('GPL root license', 'GNU GENERAL PUBLIC LICENSE' in text('LICENSE') and 'Version 2' in text('LICENSE'))

    try:
        compile(text('qa/run-all.py'), str(ROOT / 'qa/run-all.py'), 'exec')
        compile(text('qa/run-all-tests.py'), str(ROOT / 'qa/run-all-tests.py'), 'exec')
        gate('QA runner syntax', True)
    except Exception as exc:
        gate('QA runner syntax', False, str(exc))

    if not args.static_only:
        if shutil.which('cargo'):
            for cmd, label in [
                (['cargo','fmt','--all','--','--check'], 'cargo fmt'),
                (['cargo','check','--workspace'], 'cargo check'),
                (['cargo','test','--workspace','--lib'], 'cargo test'),
            ]:
                result = subprocess.run(cmd, cwd=ROOT)
                gate(label, result.returncode == 0, f'exit {result.returncode}')
            for crate in ['ghost-hydra', 'ghost-kaspa']:
                result = subprocess.run(['cargo','check','-p',crate,'--features','upstream'], cwd=ROOT)
                gate(f'{crate} upstream check', result.returncode == 0, f'exit {result.returncode}')
        else:
            skip('Rust validation', 'cargo not installed')
        app = ROOT / 'crates/ghost-app'
        npm = shutil.which('npm')
        if npm and (app / 'node_modules').exists():
            result = subprocess.run([npm, 'run', 'build'], cwd=app)
            gate('frontend build', result.returncode == 0, f'exit {result.returncode}')
        else:
            skip('frontend build', 'npm dependencies not installed')

    print(f"\nGhost Talk QA: {passed} PASS / {failed} FAIL / {skipped} SKIP")
    return 1 if failed else 0


if __name__ == '__main__':
    raise SystemExit(main())
