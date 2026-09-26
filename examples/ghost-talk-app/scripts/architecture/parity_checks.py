"""Cross-host command and feature parity checks."""
from __future__ import annotations

import re

from .common import APP_CRATES, fail, text


def _native_commands() -> set[str]:
    source = text(APP_CRATES / "ghost-talk-native" / "src" / "lib.rs")
    match = re.search(r"tauri::generate_handler!\[(.*?)\]", source, re.DOTALL)
    if not match:
        fail("native Tauri command handler list could not be parsed")
        return set()
    return {
        token.strip()
        for token in match.group(1).split(",")
        if re.fullmatch(r"[a-z][a-z0-9_]*", token.strip())
    }


def _browser_commands() -> set[str]:
    root = APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host"
    command = re.compile(r'"([a-z][a-z0-9_]+)"')
    found: set[str] = set()
    for path in root.rglob("*.rs"):
        if path.name == "tests.rs":
            continue
        found.update(command.findall(text(path)))
    return found


def check_cross_platform_command_parity() -> None:
    native = _native_commands()
    browser = _browser_commands()
    if not native:
        return
    missing = sorted(native - browser)
    if missing:
        fail("standalone Web is missing native command parity: " + ", ".join(missing))
    if len(native) < 60:
        fail("native command surface unexpectedly shrank; review cross-platform parity before release")

    browser_root = APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host"
    for path in browser_root.rglob("*.rs"):
        if "not yet available in the standalone Web release" in text(path):
            fail(f"standalone Web contains a feature-gap fallback instead of a concrete parity implementation: {path.relative_to(APP_CRATES.parent)}")

    native_root = APP_CRATES / "ghost-talk-native" / "src"
    mobile_entry = text(native_root / "lib.rs")
    if "#[cfg_attr(mobile, tauri::mobile_entry_point)]" not in mobile_entry:
        fail("Android/iOS must share the same native Tauri command host as desktop")


def check_broadcast_transport_parity() -> None:
    api = text(APP_CRATES / "ghost-api" / "src" / "media.rs")
    shared = text(APP_CRATES / "ghost-broadcast" / "src" / "relay.rs")
    controller = text(APP_CRATES / "ghost-wasm" / "src" / "controllers" / "broadcast.rs")
    relay = text(APP_CRATES / "ghost-wasm" / "src" / "controllers" / "broadcast" / "relay.rs")
    studio = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "studio" / "live.rs")
    native = text(APP_CRATES / "ghost-talk-native" / "src" / "broadcast_commands.rs")
    browser = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "broadcast.rs")
    wasm_manifest = text(APP_CRATES / "ghost-wasm" / "Cargo.toml")
    relay_spec = text(APP_CRATES.parent / "docs" / "spec" / "broadcast-relay.md")

    required = (
        (api, "pub relay_url: Option<String>"),
        (shared, "pub enum RelayControl"),
        (shared, "pub fn encode_relay_frame"),
        (shared, "pub fn validate_relay_url"),
        (controller, "relay::start"),
        (controller, "relay::push"),
        (controller, "relay::stop"),
        (relay, "WebSocket::new"),
        (relay, "MAX_PENDING_BYTES"),
        (studio, "Broadcast relay"),
        (studio, "Web/mobile live RTMP uses the configured relay"),
        (native, "Android/iOS live RTMP requires a Ghost Talk broadcast relay"),
        (browser, "Standalone Web live RTMP requires a Ghost Talk broadcast relay"),
        (wasm_manifest, '"WebSocket"'),
        (relay_spec, "ASCII GTRF"),
        (relay_spec, "completed local recording can be explicitly archived to Kaspa"),
    )
    for source, token in required:
        if token not in source:
            fail(f"cross-platform broadcast parity is missing `{token}`")


def check_shared_parity_owners() -> None:
    native_kaskold = text(APP_CRATES / "ghost-talk-native" / "src" / "kaskold_compat.rs")
    web_kaskold = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "interop" / "kaskold.rs")
    for source, host in ((native_kaskold, "native"), (web_kaskold, "Web")):
        if "ghost_kaskold::" not in source:
            fail(f"{host} KasKold host must delegate to shared ghost-kaskold logic")

    shared_kaspa = text(APP_CRATES / "ghost-kaspa" / "src" / "lib.rs")
    native_history = text(APP_CRATES / "ghost-talk-native" / "src" / "wallet_commands" / "send.rs")
    web_history = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "runtime" / "history.rs")
    history_token = "project_wallet_history_entry"
    if history_token not in shared_kaspa or any(history_token not in source for source in (native_history, web_history)):
        fail("native/Web wallet history classification must remain shared in ghost-kaspa")

    shared_archive = text(APP_CRATES / "ghost-kaspa" / "src" / "archive.rs")
    native_archive = text(APP_CRATES / "ghost-talk-native" / "src" / "media_commands" / "archive" / "mod.rs")
    web_archive = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "runtime" / "archive" / "mod.rs")
    if any(token not in shared_archive for token in ("archive_plan", "archive_remaining_cost", "ArchiveProgress")):
        fail("native/Web Kaspa archive planning/progress must remain shared")
    if "ghost_kaspa::archive_plan" not in native_archive or "ghost_kaspa::archive_plan" not in web_archive:
        fail("native/Web Kaspa archive hosts must delegate planning to ghost-kaspa")

    native_address = text(APP_CRATES / "ghost-talk-native" / "src" / "wallet_commands" / "address.rs")
    native_publish = text(APP_CRATES / "ghost-talk-native" / "src" / "peer_commands" / "publish.rs")
    if "pub(crate) fn stable_address" not in native_address:
        fail("native wallet commands must own stable receive-address selection")
    if "crate::wallet_commands::stable_address" not in native_publish or "crate::wallet_commands::stable_address" not in native_archive:
        fail("native profile publishing and Kaspa archive planning must share stable receive-address selection")
    if "fn stable_address" in native_publish:
        fail("native peer publishing must not duplicate stable receive-address selection")

def check_browser_host_compile_contracts() -> None:
    api = text(APP_CRATES / "ghost-api" / "src" / "lib.rs")
    browser = APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host"
    kasia = text(browser / "interop" / "kasia.rs") + text(browser / "interop" / "kasia" / "indexer.rs")
    backup = text(browser / "backup.rs")
    credentials = text(browser / "support" / "credentials.rs")
    mailbox = text(browser / "runtime" / "mailbox.rs") + text(browser / "runtime" / "mailbox" / "request.rs")
    media = text(browser / "media.rs")
    native = text(APP_CRATES / "ghost-wasm" / "src" / "native.rs")
    relay = text(APP_CRATES / "ghost-wasm" / "src" / "controllers" / "broadcast" / "relay.rs")

    for token in ("BroadcastStartRequest", "derivation_presets"):
        if token not in api:
            fail(f"ghost-api root must re-export browser-host dependency `{token}`")
    for token in ("fn decrypt_context", "fn decrypt_handshake", "debug::record"):
        if token not in kasia:
            fail(f"standalone Web Kasia adapter is missing compile contract `{token}`")
    if "ghost_api::validate_profile_backup_inputs" not in backup:
        fail("standalone Web backup restore must use the shared profile-backup validator")
    if "fn credential_key" not in credentials:
        fail("standalone Web remembered unlock must derive a namespaced credential key")
    if "Result<MailboxSendResult, String>" not in mailbox or "to_value(send_contact_request(args).await?)" not in mailbox:
        fail("browser mailbox contact-request dispatch must stay typed internally and serialize at the command boundary")
    for token in ("fetch_remote(url.as_str())", "fetch(&reference, locator.as_str())"):
        if token not in media:
            fail(f"browser media dispatch must borrow owned location strings explicitly: `{token}`")
    if "pub(crate) use invoke::js_error;" not in native or "crate::native::js_error" not in relay:
        fail("broadcast relay must consume the narrow native JavaScript-error facade")

    restricted = "pub(in crate::native::browser_host)"
    required_helpers = {
        browser / "support" / "util.rs": ("required_str", "required", "to_value", "open_wallet_secret"),
        browser / "support" / "storage.rs": ("local_storage",),
        browser / "runtime" / "history.rs": ("gather", "backup_payloads"),
        browser / "runtime" / "live.rs": ("start", "stop"),
        browser / "runtime" / "session.rs": ("set_identity", "clear"),
        browser / "runtime" / "session" / "debug.rs": ("debug_value",),
    }
    for path, helpers in required_helpers.items():
        source = text(path)
        for helper in helpers:
            pattern = rf"{re.escape(restricted)}\s+(?:async\s+)?fn\s+{re.escape(helper)}\b"
            if not re.search(pattern, source):
                fail(f"browser-host cross-module helper `{helper}` must be restricted to the browser_host boundary")

