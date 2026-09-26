"""Architecture contracts for identity, media, room access, and protocol isolation."""
from __future__ import annotations

from pathlib import Path

from .common import APP_CRATES, fail, function_bodies, rel, structural_complexity, text


def _manifest_dependencies(crate: str) -> str:
    manifest = APP_CRATES / crate / "Cargo.toml"
    return text(manifest) if manifest.is_file() else ""


def _source_text(crate: str) -> str:
    root = APP_CRATES / crate / "src"
    if not root.is_dir():
        return ""
    return "\n".join(text(path) for path in root.rglob("*.rs"))


def check_identity_media_room_contracts() -> None:
    names = _source_text("ghost-names")
    api = _source_text("ghost-api")
    media = _source_text("ghost-media")
    rooms = _source_text("ghost-rooms")
    native_names = text(
        APP_CRATES / "ghost-talk-native" / "src" / "peer_commands" / "name_resolution.rs"
    )
    required_name_tokens = (
        "pub struct GhostNameResolver",
        "GhostNameNamespace::Kns",
        "GhostNameNamespace::DotK",
        "resolve_name",
        "verify_binding",
        "clear_cache",
    )
    if any(token not in names for token in required_name_tokens):
        fail("GhostNameResolver must own KNS/.k normalization, verification, and cache lifecycle")
    if "GhostNameResolver::normalize" not in native_names or "NameResolverState" not in native_names:
        fail("native peer resolution must use the unified GhostNameResolver facade")

    profile_tokens = (
        "pub primary_name: Option<ProfileName>",
        "pub verified_names: Vec<VerifiedName>",
        "pub avatar: Option<MediaReference>",
        "pub capabilities: Vec<String>",
        "pub signature: String",
    )
    if any(token not in api for token in profile_tokens):
        fail("PublicGhostProfile is missing canonical verified-name/media/capability fields")
    if "ghost-media" not in _manifest_dependencies("ghost-api"):
        fail("profile avatars must use ghost-media MediaReference instead of profile-local image types")

    media_tokens = ("pub struct MediaReference", "pub struct GhostMediaManifest", "verify_content")
    if any(token not in media for token in media_tokens):
        fail("ghost-media must own content identity, manifests, and integrity verification")

    room_tokens = ("pub enum RoomAccess", "InviteOnly", "Unlisted", "Public", "pub fn access(&self)")
    if any(token not in rooms for token in room_tokens):
        fail("ghost-rooms must own explicit Private/InviteOnly/Unlisted/Public access policy")


def check_ghost_kasia_indexer_boundary() -> None:
    kasia_dir = APP_CRATES / "ghost-kasia"
    ghost_indexer_dir = APP_CRATES / "ghost-indexer"
    native_protocol = _source_text("ghost-protocol")

    forbidden_native_tokens = ("kchat:1:", "ciph_msg:1:", "ghost_kasia")
    if any(token in native_protocol for token in forbidden_native_tokens):
        fail("Kasia wire/indexer details leaked into ghost-protocol")

    if kasia_dir.is_dir():
        kasia_manifest = _manifest_dependencies("ghost-kasia")
        if "ghost-indexer" in kasia_manifest:
            fail("ghost-kasia must use the Kasia indexer directly, never ghost-indexer")
        for owner in ("ghost-protocol", "ghost-hydra", "ghost-chat"):
            if "ghost-kasia" in _manifest_dependencies(owner):
                fail(f"{owner} must not depend on ghost-kasia; interoperability stays isolated")

    if ghost_indexer_dir.is_dir():
        source = _source_text("ghost-indexer")
        manifest = _manifest_dependencies("ghost-indexer")
        native_manifest = _manifest_dependencies("ghost-talk-native")
        native_peers = text(APP_CRATES / "ghost-talk-native" / "src" / "peer_commands.rs")
        if "ghost-kasia" in manifest or "kchat:1:" in source or "ciph_msg:1:" in source:
            fail("ghost-indexer must index Ghost-native state only; Kasia belongs to the Kasia indexer")
        if "ghost-indexer" not in native_manifest or "ghost_indexer::GhostProfileIndex" not in native_peers:
            fail("ghost-indexer must own the native Ghost public-directory projection")
        required = ("pub struct GhostProfileIndex", "ingest_payload", "pub fn latest", "verify_gtcd")
        if any(token not in source for token in required):
            fail("ghost-indexer must own verified, rebuildable Ghost L1 current-state indexing")

def check_media_broadcast_interop_completion() -> None:
    broadcast_dir = APP_CRATES / "ghost-broadcast"
    kasia_dir = APP_CRATES / "ghost-kasia"
    indexer_dir = APP_CRATES / "ghost-indexer"
    if not broadcast_dir.is_dir() or not kasia_dir.is_dir() or not indexer_dir.is_dir():
        fail("communications/media expansion requires ghost-broadcast, ghost-kasia, and ghost-indexer owners")
        return

    broadcast = _source_text("ghost-broadcast")
    broadcast_manifest = _manifest_dependencies("ghost-broadcast")
    required_broadcast = (
        "pub struct BroadcastPipeline",
        "pub struct FileRecordingSink",
        "pub struct FfmpegRtmpSink",
        "pub struct RtmpSecret",
        "ZeroizeOnDrop",
        "fn fan_out",
    )
    if any(token not in broadcast for token in required_broadcast):
        fail("ghost-broadcast must own shared fan-out, recording, RTMP, and redacted secret handling")
    if "ghost-media" not in broadcast_manifest:
        fail("ghost-broadcast metadata must reuse ghost-media references/manifests")

    rooms = _source_text("ghost-rooms")
    if "pub struct ChannelRoom" in rooms:
        fail("stage/radio must be Room policy presets, not a parallel ChannelRoom implementation")
    for token in ("RoomMode::Stage", "RoomMode::Radio", "ChannelPolicy::PresentersOnly"):
        if token not in rooms:
            fail("ghost-rooms must express stage/radio behavior through authoritative Room policies")
            break

    room_voice = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "call" / "room_voice.rs")
    required_room_voice = ("BrowserVoiceSender", "room.can_speak", "push_broadcast", "encode_room_voice")
    if any(token not in room_voice for token in required_room_voice):
        fail("Room voice must reuse the existing Ghost voice sender and authoritative Room speaker policy")
    if "get_user_media" in room_voice or "MediaRecorder" in room_voice:
        fail("Room voice must not open a second browser capture/recording stack")

    native_media = "\n".join(
        text(path) for path in (APP_CRATES / "ghost-talk-native" / "src" / "media_commands").rglob("*.rs")
    )
    archive_tokens = (
        "KaspaArchiveLocator",
        "archive_cache::store_chunk",
        "ArchiveProgress",
        "mark_in_flight",
        "max_cost_sompi",
    )
    if any(token not in native_media for token in archive_tokens):
        fail("Kaspa media archival must provide verified caching, resumable progress, and a confirmed cost bound")

    studio_root = APP_CRATES / "ghost-wasm" / "src" / "components" / "studio"
    archive_ui = text(studio_root / "archive.rs")
    catalog_ui = text(studio_root / "catalog.rs")
    live_ui = text(studio_root / "live.rs")
    studio_controller = text(APP_CRATES / "ghost-wasm" / "src" / "controllers" / "studio.rs")
    if "struct ArchivePublishTask" not in archive_ui or "pub(crate) struct EpisodePublishRequest" not in studio_controller:
        fail("Studio archive/episode publication must group async inputs instead of exceeding Clippy argument limits")
    if "ui: &CatalogUi" not in catalog_ui or "let rtmp = if ui.rtmp_server.trim().is_empty()" not in live_ui:
        fail("Studio catalog/live controls must retain Clippy-clean state grouping and explicit conditional rendering")

    capabilities = text(APP_CRATES / "ghost-api" / "src" / "capabilities.rs")
    capability_tokens = (
        'pub const KASIA_V1: &str = "kasia-v1"',
        'pub const GHOST_AVATAR_V1: &str = "ghost-avatar-v1"',
        'pub const GHOST_ROOM_VOICE_V1: &str = "ghost-room-voice-v1"',
        'pub const GHOST_MEDIA_V1: &str = "ghost-media-v1"',
        'pub const GHOST_BROADCAST_V1: &str = "ghost-broadcast-v1"',
        "established_ghost_never_falls_back_to_kasia",
    )
    if any(token not in capabilities for token in capability_tokens):
        fail("capability discovery and executable no-silent-downgrade regression must cover the expansion")

    kasia = _source_text("ghost-kasia")
    kasia_tokens = (
        'const CURRENT_ROOT: &str = "kchat:1:"',
        'const COMPAT_ROOT: &str = "ciph_msg:1:"',
        "ephemeral compressed secp256k1 key || nonce",
        "pub struct KasiaIndexerClient",
        "pub struct KasiaContactMapping",
    )
    if any(token not in kasia for token in kasia_tokens):
        fail("ghost-kasia must own current KaChat writes, compatibility reads, ECIES framing, indexer access, and durable contact mapping")



def check_crap_safe_zero_coverage_boundaries() -> None:
    targets = (
        (APP_CRATES / "ghost-rooms" / "src" / "model.rs", "allowed_by_policy"),
        (APP_CRATES / "ghost-talk-native" / "src" / "broadcast_commands.rs", "validate_config"),
        (APP_CRATES / "ghost-talk-native" / "src" / "hydra_commands" / "session" / "projection.rs", "project_kktp_inner"),
        (APP_CRATES / "ghost-talk-native" / "src" / "mailbox_commands" / "handshake" / "handshake_send.rs", "validate_send_content"),
        (APP_CRATES / "ghost-talk-native" / "src" / "media_commands" / "mod.rs", "validate_locator"),
    )
    for path, function in targets:
        bodies = {name: body for name, _start, _end, body in function_bodies(text(path))}
        body = bodies.get(function)
        if body is None or structural_complexity(body) > 4:
            fail(f"zero-coverage CRAP boundary must remain CC<=4: {rel(path)}::{function}")


def check_message_reaction_contracts() -> None:
    domain = _source_text("ghost-domain")
    protocol = _source_text("ghost-protocol")
    chat = _source_text("ghost-chat")
    rooms = _source_text("ghost-rooms")
    wasm = _source_text("ghost-wasm")
    reaction_kinds = (
        "Like", "Love", "Haha", "Sad", "Surprised",
        "Dislike", "Disgust", "Angry", "Fear",
    )
    if "pub enum ReactionKind" not in domain or any(kind not in domain for kind in reaction_kinds):
        fail("ghost-domain must own the canonical nine-value Ghost reaction vocabulary")
    if "pub fn set_actor_reaction" not in domain or "pub fn merge_reactions" not in domain:
        fail("ghost-domain must own one-reaction-per-actor mutation and concurrent reaction merging")
    if "pub struct GhostReactionEvent" not in protocol or '"reaction"' not in protocol:
        fail("ghost-protocol must carry reactions as authenticated KKTP inner events")
    if "pub reactions: Vec<MessageReaction>" not in chat or "set_message_reaction" not in chat:
        fail("ghost-chat must persist reactions on the target message through ChatService")
    if "pub reactions: Vec<MessageReaction>" not in rooms or "RoomWire::Reaction" not in wasm:
        fail("Room reactions must reuse Room messages and the authoritative Room relay path")
    if "actor_hydra_id != from" not in wasm or "ghost-reaction-v1" not in wasm:
        fail("reaction ingress must bind the reacting actor to authenticated Ghost transport identity")

def check_kaskold_compatibility_and_ui_contracts() -> None:
    root = APP_CRATES.parents[2]
    compat_root = APP_CRATES / "ghost-talk-native" / "src" / "kaskold_compat.rs"
    native = text(compat_root)
    shared_root = APP_CRATES / "ghost-kaskold"
    shared = "\n".join(text(path) for path in (shared_root / "src").rglob("*.rs"))
    native_manifest = _manifest_dependencies("ghost-talk-native")
    composer = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "chat" / "composer.rs")
    reactions = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "chat" / "reaction_ui.rs")
    details = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "chat" / "view" / "details.rs")
    styles = text(APP_CRATES / "ghost-wasm" / "style.css")
    wallet = text(APP_CRATES / "ghost-runtime" / "src" / "wallet.rs")
    sdk = root / "external" / "kaskold" / "kaskold-sdk" / "src" / "lib.rs"
    vault = root / "external" / "kaskold" / "vault-runtime" / "src" / "wallet_tools.rs"
    if not sdk.is_file() or not vault.is_file():
        fail("KasKold 2.0 SDK/Vault compatibility source must remain vendored with Ghost Talk")
        return
    required_shared = (
        "add_restored_wallet", "add_recovery_material", "add_account_xprv",
        "add_raw_private_key", "add_portable_backup", "add_stego_backup",
        "backup_recovery_phrase", "backup_seedqr", "backup_compact_seedqr",
        "backup_account_xprv", "portable_backup", "portable_xprv_backup", "stego_backup",
        "kaskold_sdk::prepare", "kaskold_sdk::complete", "kaskold_sdk::finalize",
    )
    if any(token not in shared for token in required_shared):
        fail("shared KasKold compatibility must cover every Vault import/backup and SDK PSKT signing path")
    if "ghost-kaskold" not in native_manifest or "ghost_kaskold::" not in native:
        fail("native KasKold commands must delegate to the shared ghost-kaskold facade")
    shared_manifest = text(shared_root / "Cargo.toml")
    for dependency, rel_path in (
        ("vault-runtime", "../../../../external/kaskold/vault-runtime"),
        ("kaskold-sdk", "../../../../external/kaskold/kaskold-sdk"),
    ):
        if dependency not in shared_manifest or rel_path not in shared_manifest:
            fail(f"ghost-kaskold must own the official {dependency} path dependency")
        if not (shared_root / rel_path).resolve().is_dir():
            fail(f"ghost-kaskold dependency path must resolve inside the Ghost Talk source tree: {rel_path}")
    sdk_manifest = text(root / "external" / "kaskold" / "kaskold-sdk" / "Cargo.toml")
    protocol_manifest = text(root / "external" / "kaskold" / "kaskold-protocol" / "Cargo.toml")
    for manifest in (sdk_manifest, protocol_manifest):
        if '=0.2.108' not in manifest.replace(" ", "") or '=0.3.85' not in manifest.replace(" ", ""):
            fail("vendored KasKold WASM bindings must stay aligned with Ghost Talk's pinned wasm-bindgen/js-sys family")
    root_manifest = text(root / "Cargo.toml")
    for crate in ("hot-wallet", "kaskold-protocol", "kaskold-sdk", "offline-signer", "shared-signer", "vault-runtime"):
        token = f'"external/kaskold/{crate}"'
        if token not in root_manifest:
            fail(f"vendored KasKold crate must be explicitly excluded from the Ghost Talk root workspace: {crate}")
    if "ghost_storage::seal" not in shared or "ghost_storage::open" not in shared:
        fail("shared KasKold secret inventory must remain encrypted by Ghost profile custody")
    if "local.kaskold_inventory != baseline.kaskold_inventory" not in wallet:
        fail("wallet reconciliation must preserve local KasKold encrypted inventory changes")
    if 'let masked_draft = "*".repeat(state.draft.chars().count());' not in composer or \
            '{masked_draft}</span>' not in composer:
        fail("Mask mode must precompute and render one literal asterisk per composer character")
    wasm_model = text(APP_CRATES / "ghost-wasm" / "src" / "model.rs")
    if any(token in wasm_model for token in ("KasKoldInventoryResult", "KasKoldReviewResult", "KasKoldSignResult", "KasKoldWalletSummary")):
        fail("ghost-wasm must not retain obsolete KasKold inventory/review/sign DTO re-exports")
    wasm_manifest = text(APP_CRATES / "ghost-wasm" / "Cargo.toml")
    if "vault-runtime" not in wasm_manifest or "kaskold-sdk" not in wasm_manifest:
        fail("Web KasKold restore/backup/signer flows must consume the official vendored facades")
    restore_ui = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "account" / "kaskold_restore.rs")
    backup_ui = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "account" / "kaskold.rs")
    wallet_ui = APP_CRATES / "ghost-wasm" / "src" / "components" / "account" / "wallet"
    send_ui = text(wallet_ui / "send.rs") + text(wallet_ui / "send" / "signer.rs")
    if "Import from KasKold" not in restore_ui or "Import from KasKold" in backup_ui:
        fail("KasKold import must appear only in the identity restore flow")
    if "KasKold-compatible backup" not in backup_ui or "Use Signer" not in send_ui:
        fail("Kaspa must expose only compact KasKold backup plus Send KAS Use Signer")
    for token in ("kaskold_sdk::prepare", "kaskold_sdk::complete", "broadcast_signer_send_and_patch"):
        if token not in send_ui:
            fail(f"Send KAS KasKold signer flow is missing protocol step: {token}")
    if 'class="reaction-trigger"' not in reactions or '{"👍"}' not in reactions:
        fail("message reactions must open from an attached thumbs-up trigger")
    if 'class="message-bubble-wrap"' not in details or 'render_reaction_picker' not in details:
        fail("reaction controls must stay attached to their target message")
    css_tokens = ("flex-wrap:nowrap", ".reaction-menu{position:absolute", ".rooms-layout{grid-template-columns")
    if any(token not in styles for token in css_tokens):
        fail("chat actions/reactions and Rooms layout must retain the expanded desktop UI contract")

