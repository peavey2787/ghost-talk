"""Runtime owner, call, mailbox, transport, storage, and dead-code checks."""
from __future__ import annotations

import re
from pathlib import Path

from .common import (
    APP_ROOT, REPO_ROOT, APP_CRATES, SDK_CRATES, WASM, NATIVE, HYDRA, DOMAIN,
    MAX_LINES, NORMAL_FILE_LINES, MAX_FUNCTION_LINES, MAX_COMPLEXITY,
    NORMAL_FUNCTION_LINES, NORMAL_COMPLEXITY, MAX_RS_FILES_PER_DIRECTORY,
    fail, rel, text, sanitize_rust, function_bodies, structural_complexity,
)


def check_call_ownership(files: list[Path]) -> None:
    domain_call = text(DOMAIN / "call.rs")
    call_store = text(DOMAIN / "call" / "store.rs")
    call_manager = text(DOMAIN / "call" / "manager.rs")
    model = text(WASM / "model" / "chat_room.rs")
    call_root = text(WASM / "components" / "call.rs")
    manager = text(WASM / "components" / "call" / "manager.rs")
    termination = text(WASM / "components" / "call" / "termination.rs")

    if "struct CallStore" not in call_store or "pub struct CallManager" not in call_manager:
        fail("ghost-domain must hide CallStore behind the public CallManager authority")
    if re.search(r"pub\s+struct\s+CallStore", call_store):
        fail("CallStore must remain private to ghost-domain CallManager")
    if re.search(r"pub\s+calls\s*:", model):
        fail("durable Profile still persists live call state")
    for name in (
        "call_bootstrap_request_id",
        "call_bootstrap_call_id",
        "pending_incoming_call_id",
        "pending_incoming_call_signal_based",
    ):
        if name in model:
            fail(f"Chat still owns obsolete call lifecycle field {name}")

    for path in files:
        clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        if re.search(r"\.phase\s*=(?!=)", clean) and path != DOMAIN / "call" / "transition.rs":
            fail(f"direct CallRecord phase mutation outside ghost-domain transition authority: {rel(path)}")

    wasm_source = "\n".join(path.read_text(encoding="utf-8", errors="replace") for path in WASM.rglob("*.rs"))
    manager_instances = len(re.findall(r"<LiveCallManager(?:\s|>)", wasm_source))
    if manager_instances != 1:
        fail(f"expected exactly one application-scoped <LiveCallManager>, found {manager_instances}")
    if "CallFacade" in wasm_source or "LiveCallController" in wasm_source:
        fail("superseded call lifecycle implementation remains")
    if "call_manager: Rc<RefCell<CallManager>>" not in call_root:
        fail("LiveCallManager runtime does not own one process-local CallManager")
    if "current.clone(), CallEvent::Hangup" not in manager:
        fail("Hang up is not bound to the exact rendered CallRecord")
    if not any(pattern in manager for pattern in ("toggle_mute_callback(runtime, current.clone())", "toggle_mute_callback(runtime.clone(), current.clone())")):
        fail("Mute is not bound to the exact rendered CallRecord")
    if termination.find("end_runtime_call(&runtime, &call.call_id, event)") < 0:
        fail("Hang up does not commit the local call transition before signaling")


def check_identity_ownership() -> None:
    identity = text(DOMAIN / "identity.rs")
    contacts = text(APP_CRATES / "ghost-contacts" / "src" / "service.rs")
    request_ingress = text(WASM / "app" / "mailbox" / "request_ingress.rs")
    signaling = text(WASM / "components" / "call" / "signaling.rs")
    call_identity = text(WASM / "components" / "call" / "identity.rs")
    if "pub struct PeerBinding" not in identity or "optional_binding_matches" not in identity:
        fail("canonical PeerBinding identity domain is missing")
    if "ContactService::resolve_peer" not in contacts:
        fail("ContactService does not own authenticated PeerBinding contact resolution")
    if "ContactService::resolve_peer" not in request_ingress or "PeerBinding::new" not in request_ingress:
        fail("incoming chat request contact resolution bypasses ContactService/PeerBinding")
    if "PeerBinding::new" not in call_identity or "call_peer_matches_contact" not in call_identity:
        fail("call contact/chat resolution bypasses canonical PeerBinding")
    if "call_peer_matches_contact" not in signaling or "call_peer_matches_chat" not in signaling:
        fail("call signaling bypasses canonical call peer resolution helpers")
    if re.search(r"address_matches\s*\|\|\s*hydra_matches", request_ingress):
        fail("incoming request still permits one-sided contact identity matching")
    if "unwrap_or_else(|| incoming.peer_address.clone())" not in request_ingress:
        fail("unknown incoming chat does not default its primary label to the authenticated Kaspa address")


def check_mailbox_ownership() -> None:
    pipeline = text(WASM / "app" / "mailbox" / "mailbox_pipeline.rs")
    domain_mailbox = text(DOMAIN / "mailbox.rs")
    runtime_mailbox = text(APP_CRATES / "ghost-runtime" / "src" / "mailbox.rs")
    if "pub enum PacketDisposition" not in domain_mailbox:
        fail("mailbox envelope disposition domain is missing")
    if "pub struct MailboxService" not in runtime_mailbox:
        fail("durable mailbox frame/envelope state is not owned by MailboxService")
    if "MailboxService::ready" not in pipeline:
        fail("mailbox drain bypasses the canonical MailboxService")
    if "PacketDisposition::Retryable" not in pipeline or ".removes_envelope()" not in pipeline:
        fail("mailbox drain does not isolate retryable envelope failures through PacketDisposition")
    if "for envelope in envelopes" not in pipeline:
        fail("mailbox drain pipeline is missing")


def check_transport_ownership(files: list[Path]) -> None:
    session = text(APP_CRATES / "ghost-chat" / "src" / "session.rs")
    node = text(APP_CRATES / "ghost-kaspa" / "src" / "node_session.rs")
    gateway = text(NATIVE / "kaspa_gateway.rs")
    if "pub struct HydraSessionManager" not in session:
        fail("durable HYDRA chat-session projection lacks HydraSessionManager")
    if "pub struct NodeSession" not in node:
        fail("Kaspa node selection lacks canonical NodeSession")
    if "session: NodeSession" not in gateway:
        fail("native Kaspa gateway does not own its exact network/endpoint through NodeSession")
    # Ghost owns only the transport-neutral p2p-net adapter. Browser/native
    # WebRTC, SDP, ICE, STUN, relay, and libp2p transport mechanics belong to
    # p2p-net and must never be reintroduced into application crates.
    forbidden_transport = re.compile(
        r"(?:RTCPeerConnection|RTCSessionDescription|RTCDataChannel|DirectCarrier|"
        r"DirectSignal|DirectSdp|hydra_(?:seal|open)_direct|negotiation[_-]?id|"
        r"iceServers|stun:|turn:|use\s+libp2p(?:::|\s))",
        flags=re.I,
    )
    for path in APP_CRATES.rglob("*.rs"):
        clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        if forbidden_transport.search(clean):
            fail(f"application-owned transport mechanics reappeared: {rel(path)}")

    p2p_adapter = text(APP_CRATES / "ghost-p2p" / "src" / "wasm.rs")
    if "p2p_net::wasm::WasmNode" not in p2p_adapter:
        fail("Ghost browser p2p adapter does not delegate to p2p-net WasmNode")
    realtime_sender = text(WASM / "controllers" / "call" / "realtime.rs")
    if "seal_realtime(" not in realtime_sender or "send_realtime_carrier(" not in realtime_sender:
        fail("realtime sender does not preserve the seal-once GTR1 fallback boundary")
    forbidden = re.compile(r"\.(?:begin_session|establish_session|mark_transport_restore|set_transport_restore_message_id|merge_resumed_transport|clear_transport_restore|complete_bootstrap|reset_transport|rejoin_with_session|rejoin_without_session|set_session_role)\(")
    for path in files:
        if str(path).startswith(str(APP_CRATES / "ghost-chat")):
            continue
        if forbidden.search(sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))):
            fail(f"HYDRA session projection mutation bypasses HydraSessionManager: {rel(path)}")


def check_wallet_progress(files: list[Path]) -> None:
    owner = APP_CRATES / "ghost-runtime" / "src" / "wallet.rs"
    reconciler = WASM / "app" / "profile_updates.rs"
    for path in files:
        if not str(path).startswith(str(WASM)):
            continue
        clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        if re.search(r"\bwallet\.public\s*=(?!=)", clean):
            fail(f"direct wallet.public replacement can roll progress backward: {rel(path)}")
        if path not in {owner, reconciler} and re.search(r"(?:\.wallet\.as_mut\s*\(|\.wallet\s*=(?!=))", clean):
            fail(f"direct wallet mutation bypasses WalletStateService: {rel(path)}")
    if "struct WalletStateService" not in text(owner):
        fail("WalletStateService ownership boundary is missing")


def check_obvious_dead_crate_api(files: list[Path]) -> None:
    """Reject crate-private functions whose identifier appears only at definition.

    This is intentionally conservative: common method names are left to Clippy,
    while an identifier unique to its crate and never referenced is certainly
    dead first-party API and should be deleted rather than suppressed.
    """
    crate_files: dict[Path, list[Path]] = {}
    for path in files:
        src_index = next((index for index, part in enumerate(path.parts) if part == "src"), None)
        if src_index is None or src_index == 0:
            continue
        crate_root = Path(*path.parts[:src_index])
        crate_files.setdefault(crate_root, []).append(path)

    declaration_re = re.compile(r"\bpub\(crate\)\s+(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\b")
    for crate_root, members in crate_files.items():
        combined = "\n".join(
            sanitize_rust(path.read_text(encoding="utf-8", errors="replace")) for path in members
        )
        declarations: list[tuple[Path, str, int]] = []
        for path in members:
            raw = path.read_text(encoding="utf-8", errors="replace")
            clean = sanitize_rust(raw)
            for match in declaration_re.finditer(clean):
                declarations.append((path, match.group(1), raw.count("\n", 0, match.start()) + 1))
        for path, name, line in declarations:
            if len(re.findall(rf"\b{re.escape(name)}\b", combined)) == 1:
                fail(f"{rel(path)}:{line} crate-private function `{name}` has no first-party caller")


def check_obsolete_code(files: list[Path]) -> None:
    for path in files:
        clean = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        if "GTH1" in clean:
            fail(f"obsolete GTH1 handshake remains in {rel(path)}")
        if re.search(r"(?:legacy|deprecated)", clean, flags=re.I):
            fail(f"legacy/deprecated implementation marker remains in production Rust: {rel(path)}")
        if re.search(r"#\[allow\((?:[^)]*\bdead_code\b[^)]*)\)\]", clean):
            fail(f"production dead-code suppression is forbidden: {rel(path)}")


def check_rust_surface_sanity(files: list[Path]) -> None:
    hook_re = re.compile(
        r"#\[hook\]\s*(?:#\[[^\]]+\]\s*)*(?:pub(?:\([^)]*\))?\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)"
    )
    dangling_attr_re = re.compile(r"(?m)^\s*#\[[^\n]+\]\s*\Z")
    fn_header_re = re.compile(r"(?s)\bfn\s+[A-Za-z_][A-Za-z0-9_]*[^\{;]*[\{;]")
    invalid_param_visibility = re.compile(
        r"\bpub(?:\([^)]*\))?\s+[A-Za-z_][A-Za-z0-9_]*\s*:"
    )
    for path in files:
        source = path.read_text(encoding="utf-8", errors="replace")
        if dangling_attr_re.search(source):
            fail(f"dangling Rust attribute at end of file: {rel(path)}")
        for header in fn_header_re.finditer(sanitize_rust(source)):
            value = header.group(0)
            if "(" in value and ")" in value:
                params = value[value.find("(") + 1:value.rfind(")")]
                if invalid_param_visibility.search(params):
                    line = source.count("\n", 0, header.start()) + 1
                    fail(f"{rel(path)}:{line} function parameter contains an invalid visibility qualifier")
        for match in hook_re.finditer(source):
            name = match.group(1)
            if not name.startswith("use_"):
                line = source.count("\n", 0, match.start()) + 1
                fail(f"{rel(path)}:{line} Yew hook {name} must start with use_")


def check_storage_root_ownership() -> None:
    storage = NATIVE / "persistence" / "storage_root.rs"
    source = text(storage)
    if ".path()" not in source or ".app_data_dir()" not in source:
        fail("Ghost Talk data root must use Tauri's cross-platform application-data API")
    if "cfg!(debug_assertions)" not in source or 'base.join("development")' not in source:
        fail("debug/development builds must use an app-data namespace isolated from release state")
    if "build_mode_data_root(base, cfg!(debug_assertions))" not in source:
        fail("storage_root must choose the development namespace from the Rust build mode")
    for path in NATIVE.rglob("*.rs"):
        if path == storage:
            continue
        if "app_data_dir()" in sanitize_rust(path.read_text(encoding="utf-8", errors="replace")):
            fail(f"runtime storage bypasses storage_root utility: {rel(path)}")


def check_native_instance_lock_ownership() -> None:
    hydra_source = "\n".join(
        path.read_text(encoding="utf-8", errors="replace") for path in HYDRA.rglob("*.rs")
    )
    required = (
        "state.hydra.lock",
        "state.ghost-talk.profile.lease",
        "acquire_native_profile_lease",
        "FileExt::try_lock_exclusive",
        "struct NativeProfileLease",
        "_profile_lease: NativeProfileLease",
        "recover_stale_native_profile_lock",
        "hydra_msg::HydraMsgError",
        "_lease: &NativeProfileLease",
        "lock_modified_unix_seconds",
        "process_instance_can_own_lock",
        "ProcessesToUpdate::Some",
        "process.start_time()",
        "sysinfo::IS_SUPPORTED_SYSTEM",
    )
    for detail in required:
        if detail not in hydra_source:
            fail(f"HYDRA profile lease is missing required ownership detail: {detail}")
    if "std::fs::remove_file(" not in hydra_source:
        fail("HYDRA stale upstream sentinel recovery is missing")
    if "hydra_msg::HydraError" in hydra_source:
        fail("HYDRA facade must use upstream hydra_msg::HydraMsgError, not nonexistent HydraError")
    if "state.ghost-talk.open.guard" in hydra_source:
        fail("short-lived HYDRA open guards must not replace the lifetime profile lease")
    if "process_alive::state" in hydra_source:
        fail("HYDRA ownership must not trust bare PID liveness without process-start identity")
    facade_open = HYDRA / "facade" / "hydra_facade.rs"
    direct_open_sites = []
    for path in HYDRA.rglob("*.rs"):
        if "hydra_msg::Hydra::open" in sanitize_rust(path.read_text(encoding="utf-8", errors="replace")):
            direct_open_sites.append(path)
    if direct_open_sites != [facade_open]:
        rendered = ", ".join(rel(path) for path in direct_open_sites) or "none"
        fail(f"all native HYDRA opens must pass through HydraFacade profile leasing; found: {rendered}")

