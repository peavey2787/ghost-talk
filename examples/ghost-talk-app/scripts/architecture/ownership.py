"""Application/domain ownership, privacy, schema, and controller boundary checks."""
from __future__ import annotations

import re
from pathlib import Path

from .common import (
    APP_ROOT, REPO_ROOT, APP_CRATES, SDK_CRATES, WASM, NATIVE, HYDRA, DOMAIN,
    MAX_LINES, NORMAL_FILE_LINES, MAX_FUNCTION_LINES, MAX_COMPLEXITY,
    NORMAL_FUNCTION_LINES, NORMAL_COMPLEXITY, MAX_RS_FILES_PER_DIRECTORY,
    fail, rel, text, sanitize_rust, function_bodies, structural_complexity,
)


def check_profile_domain_boundaries(files: list[Path]) -> None:
    allowed_collection_writer = WASM / "app" / "profile_updates.rs"
    direct_collection_mutation = re.compile(r"\.(?:chats|contacts|rooms)\.(?:push|insert|retain|remove|clear|iter_mut)\b")
    direct_collection_assignment = re.compile(r"\.(?:chats|contacts|rooms)\s*=(?!=)")
    nested_mutable_collection = re.compile(r"\.(?:messages|members)\.iter_mut\(")
    for path in files:
        if not str(path).startswith(str(WASM)):
            continue
        clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        if direct_collection_mutation.search(clean):
            fail(f"direct durable collection mutation bypasses domain service: {rel(path)}")
        if path != allowed_collection_writer and direct_collection_assignment.search(clean):
            fail(f"direct durable collection replacement bypasses profile reconciliation: {rel(path)}")
        if nested_mutable_collection.search(clean):
            fail(f"nested durable collection mutation bypasses canonical record methods: {rel(path)}")

    owner_services = (
        APP_CRATES / "ghost-chat" / "src" / "service.rs",
        APP_CRATES / "ghost-contacts" / "src" / "service.rs",
        APP_CRATES / "ghost-rooms" / "src" / "service.rs",
    )
    escaping_mut_ref = re.compile(r"pub\s+fn\s+(?:by_id_mut|by_index_mut|find_mut|iter_mut|message_mut|message_by_index_mut|resolve_exact_mut)\b")
    for service in owner_services:
        if escaping_mut_ref.search(sanitize_rust(text(service))):
            fail(f"domain service exposes a mutable reference outside its ownership boundary: {rel(service)}")

    patch = text(WASM / "model" / "profile_patch.rs")
    if re.search(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:before|after)\s*:\s*Profile\b", patch):
        fail("ProfilePatch still captures whole Profile snapshots")


def check_strict_ownership_surface(files: list[Path]) -> None:
    """Enforce the ownership-refactor acceptance surface."""
    generic_mutator = re.compile(r"\b(?:with_[A-Za-z0-9_]*_mut|for_each_mut)\b")
    direct_native = re.compile(r"\b(?:crate::)?native::")
    component_domain_mutation = re.compile(
        r"\b(?:ghost_chat::(?:ChatService|HydraSessionManager)|"
        r"ghost_contacts::ContactService|ghost_rooms::(?:RoomService|RoomTombstoneService))::"
        r"(?:push|insert_front|leave|set_role|begin|begin_at_index|establish|clear_restore|rejoin|"
        r"rejoin_without_session|record_message|record_message_at_index|apply_message_send_result|"
        r"mark_message_[A-Za-z0-9_]+|accept_incoming_request|set_incoming_request_state|toggle_archive|"
        r"set_contact_id|bind_peer_at_index|remove_messages_by_id|reopen_room_transport_at|sort_messages|"
        r"retire_room_transports_for_peer|new_contact|apply_public_profile_by_id|restore_missing_identity_by_id|"
        r"bind_peer_by_id|add_member|add_message|accept|remove_by_id|remove_member_by_address|ban_member|unban|"
        r"record|clear)\b"
    )
    direct_call_manager_mutation = re.compile(
        r"\.(?:event|fail|begin_outgoing|receive_signal|attach_chat|set_bootstrap_request)\s*\("
    )
    mut_profile = re.compile(r"&mut\s+Profile\b")
    cross_store = re.compile(r"&mut\s+(ChatStore|ContactStore|RoomStore)\b")

    profile_allow = {
        WASM / "app" / "profile_updates.rs",
        WASM / "app" / "shell" / "persistence.rs",
        WASM / "native" / "invoke.rs",
        WASM / "native" / "restore.rs",
        WASM / "native" / "transport_restore.rs",
    }
    for path in files:
        clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        if generic_mutator.search(clean):
            fail(f"generic mutable ownership escape hatch remains: {rel(path)}")
        if str(path).startswith(str(WASM / "components")) and direct_native.search(clean):
            fail(f"Yew/component module calls native adapter directly: {rel(path)}")
        if (str(path).startswith(str(WASM / "components"))
                and path.name != "browser_tests.rs"
                and component_domain_mutation.search(clean)):
            fail(f"component bypasses controller and invokes a domain mutation authority directly: {rel(path)}")
        if str(path).startswith(str(WASM / "components")) and "WalletStateService::" in clean:
            fail(f"component bypasses AccountController and mutates wallet state directly: {rel(path)}")
        if str(path).startswith(str(WASM / "components")):
            production = clean.split("#[cfg(test)]", 1)[0]
            if "ProfilePatch::new" in production:
                fail(f"component constructs persistence deltas instead of dispatching an application command: {rel(path)}")
        if str(path).startswith(str(WASM / "components")) and direct_call_manager_mutation.search(clean):
            fail(f"component bypasses CallController and mutates CallManager directly: {rel(path)}")
        if str(path).startswith(str(WASM)) and mut_profile.search(clean) and path not in profile_allow:
            fail(f"arbitrary &mut Profile remains outside persistence/migration/runtime composition: {rel(path)}")

    # A domain service may mutate only its own store.
    domains = {
        APP_CRATES / "ghost-chat": "ChatStore",
        APP_CRATES / "ghost-contacts": "ContactStore",
        APP_CRATES / "ghost-rooms": "RoomStore",
    }
    for root, own in domains.items():
        for path in root.glob("src/**/*.rs"):
            clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
            for store in cross_store.findall(clean):
                if store != own:
                    fail(f"cross-domain mutable store parameter `{store}` in {rel(path)}")


def check_lifecycle_mutation_guards(files: list[Path]) -> None:
    """Reject application-layer lifecycle-field assignment outside domain owners.

    Chat/room records remain serde-friendly read models, but the application may
    not mutate their lifecycle fields directly. Store privacy plus this gate
    makes semantic owner commands the only accepted write surface.
    """
    chat_assignment = re.compile(
        r"\.(?:archived|left|peer_left|bootstrap_complete|incoming_request|room_transport_only|"
        r"unread_count|transport_restore_pending|transport_restore_message_id)\s*=(?!=)"
    )
    room_assignment = re.compile(r"\.(?:pending_acceptance|revision|members|bans|messages)\s*=(?!=)")
    for path in WASM.rglob("*.rs"):
        clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        if chat_assignment.search(clean):
            fail(f"direct chat lifecycle-field assignment bypasses ChatService/HydraSessionManager: {rel(path)}")
        if room_assignment.search(clean):
            fail(f"direct room lifecycle-field assignment bypasses RoomService: {rel(path)}")


def check_controller_event_and_browser_regressions() -> None:
    router = text(WASM / "app" / "application_router.rs")
    for event in (
        "ContactVerified", "IncomingMessage", "ChatSessionBound", "ChatSessionEnded",
        "RoomMemberRemoved", "CallEnded", "WalletUpdated", "MailboxEnvelopeConsumed",
    ):
        if event not in router:
            fail(f"typed application event `{event}` is missing from ApplicationRouter")

    call_browser = text(WASM / "components" / "call" / "browser_tests.rs")
    chat_browser = text(WASM / "components" / "chat" / "browser_tests.rs")
    required_call_tests = ("rendered_mute_unmute_and_hangup", "rendered_accept_and_decline")
    required_chat_tests = (
        "rendered_incoming_chat_notification",
        "rendered_start_chat",
        "rendered_composer_sends_exact_message_body",
        "rendered_leave_commits_local_chat_transition",
    )
    for name in required_call_tests:
        if name not in call_browser:
            fail(f"browser call-control regression `{name}` is missing")
    for name in required_chat_tests:
        if name not in chat_browser:
            fail(f"browser chat regression `{name}` is missing")

    two_client = text(APP_CRATES / "ghost-runtime" / "tests" / "two_instance_ownership_regression.rs")
    for marker in (
        "contact-b", "kaspatest:c", "call-1", "CallEvent::SetMuted(true)",
        "call-2", "ChatService::leave", "serde_json::to_string", "post-restart-c",
    ):
        if marker not in two_client:
            fail(f"deterministic two-instance ownership regression is missing `{marker}`")


def check_shared_model_usage() -> None:
    model_dir = WASM / "model"
    source = "\n".join(path.read_text(encoding="utf-8", errors="replace") for path in model_dir.glob("*.rs"))
    forbidden = ("Contact", "Chat", "Room", "Settings", "WalletProjection")
    for name in forbidden:
        if re.search(rf"\b(?:pub(?:\([^)]*\))?\s+)?struct\s+{name}\b", sanitize_rust(source)):
            fail(f"ghost-wasm redeclares canonical shared model `{name}`")


def struct_schema(path: Path) -> list[tuple[str, tuple[tuple[str, str], ...]]]:
    raw = path.read_text(encoding="utf-8", errors="replace")
    clean = sanitize_rust(raw)
    found: list[tuple[str, tuple[tuple[str, str], ...]]] = []
    for match in re.finditer(r"\b(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{", clean):
        depth = 1
        end = match.end()
        while end < len(clean) and depth:
            if clean[end] == "{": depth += 1
            elif clean[end] == "}": depth -= 1
            end += 1
        if depth:
            continue
        body = raw[match.end():end - 1]
        body = re.sub(r"#\[[^\]]*\]", "", body)
        body = re.sub(r"//[^\n]*", "", body)
        fields = []
        for field in re.finditer(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([^,\n]+)", body):
            fields.append((field.group(1), re.sub(r"\s+", "", field.group(2))))
        if len(fields) >= 2:
            found.append((match.group(1), tuple(fields)))
    return found


def check_duplicate_struct_schemas(files: list[Path]) -> None:
    by_schema: dict[tuple[tuple[str, str], ...], list[tuple[str, Path]]] = {}
    for path in files:
        for name, schema in struct_schema(path):
            by_schema.setdefault(schema, []).append((name, path))
    exemptions = {
        frozenset({"WalletSecret", "WalletRecovery"}),
        frozenset({"WalletPublic", "WalletProjection"}),
    }
    exemption_doc = text(APP_ROOT / "docs" / "spec" / "schema-exemptions.md")
    for entries in by_schema.values():
        if len(entries) < 2:
            continue
        names = frozenset(name for name, _ in entries)
        if names in exemptions:
            if not all(f"`{name}`" in exemption_doc for name in names):
                fail(f"schema exemption lacks documentation for: {', '.join(sorted(names))}")
            continue
        rendered = ", ".join(f"{name} ({rel(path)})" for name, path in entries)
        fail(f"materially identical first-party struct schemas require consolidation/exemption: {rendered}")


def check_contact_binding_privacy() -> None:
    contact_model = text(APP_CRATES / "ghost-contacts" / "src" / "model.rs")
    for field in ("kaspa_address", "hydra_handle"):
        if re.search(rf"(?m)^\s*pub\s+{field}\s*:", contact_model):
            fail(f"Contact identity field `{field}` must not be publicly mutable; use PeerBinding/ContactService")
    if "pub fn kaspa_address(&self)" not in contact_model or "pub fn hydra_handle(&self)" not in contact_model:
        fail("Contact must expose read-only identity accessors while ContactService owns binding writes")

    chat_model = text(APP_CRATES / "ghost-chat" / "src" / "chat.rs")
    chat_binding_fields = ("contact_id", "peer_kaspa_address", "peer_hydra_handle", "peer_kns_name", "peer_dotk_name")
    for field in chat_binding_fields:
        if re.search(rf"(?m)^\s*pub\s+{field}\s*:", chat_model):
            fail(f"Chat peer binding field `{field}` must not be publicly mutable; use ChatService/HydraSessionManager")
    for accessor in ("contact_id", "peer_kaspa_address", "peer_hydra_handle", "peer_kns_name", "peer_dotk_name"):
        if f"pub fn {accessor}(&self)" not in chat_model:
            fail(f"Chat must expose read-only `{accessor}` access while domain owners retain binding writes")

    room_model = text(APP_CRATES / "ghost-rooms" / "src" / "model.rs")
    if re.search(r"(?m)^\s*pub\s+fn\s+bind_peer\s*\(", room_model):
        fail("RoomMember::bind_peer must remain crate-private so RoomService owns peer binding")

