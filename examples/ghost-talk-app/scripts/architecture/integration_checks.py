"""External integration pins and package-version policy checks."""
from __future__ import annotations

import hashlib
import re

from .common import (
    APP_ROOT, REPO_ROOT, APP_CRATES, SDK_CRATES, WASM, NATIVE, HYDRA, DOMAIN,
    MAX_LINES, NORMAL_FILE_LINES, MAX_FUNCTION_LINES, MAX_COMPLEXITY,
    NORMAL_FUNCTION_LINES, NORMAL_COMPLEXITY, MAX_RS_FILES_PER_DIRECTORY,
    fail, rel, text, sanitize_rust, function_bodies, structural_complexity,
)


def check_dotk_integration() -> None:
    names = APP_CRATES / "ghost-names" / "src"
    manifest = names / "dotk_mainnet.json"
    expected_sha256 = "0696babcb5c47a965088597afe849147fa0ab2681a3972af8c9451a12e48b382"
    try:
        digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
    except FileNotFoundError:
        fail(f"missing pinned DotK deployment manifest: {rel(manifest)}")
        digest = ""
    if digest and digest != expected_sha256:
        fail("pinned DotK mainnet deployment manifest changed without an explicit protocol review")

    dotk = text(names / "dotk.rs")
    chain = text(names / "dotk_chain.rs")
    deed = text(names / "dotk_deed.rs")
    peer = text(NATIVE / "peer_commands.rs")
    peer_names = text(NATIVE / "peer_commands" / "name_resolution.rs")
    name_resolver = text(names / "resolver.rs")
    recipient = text(WASM / "components" / "recipient_input.rs")
    workspace_manifest = text(APP_ROOT / "Cargo.toml")
    registry = "ee2128c03dfac7f6d74734bb3c879bd999434c47a55945b8a6daae2a1e4a21de"
    if registry not in deed or "DOTK_BOND_SOMPI: u64 = 100_000_000" not in deed:
        fail("DotK resolver must pin the reviewed registry covenant and 1-KAS deed bond")
    if "derive_deed" not in dotk or "verify_live_deed_wrpc" not in dotk:
        fail("DotK resolution must derive the deed locally before chain verification")
    directory_authority_tokens = ('text(&row, "address")', 'text(&row, "deedAddress")', 'text(&row, "registryCovenantId")')
    if any(token in dotk for token in directory_authority_tokens):
        fail("DotK directory/index may provide owner discovery only; address/deed/registry authority must remain local/on-chain")
    required_chain_proof = (
        "KaspaPortal::builder",
        ".utxos(",
        "covenant_id",
        "DOTK_REGISTRY",
        "live_outpoint_exists",
        "UtxoEntry",
    )
    if any(token not in chain for token in required_chain_proof):
        fail("DotK resolution is missing direct Toccata wRPC covenant proof or live-outpoint freshness checks")
    forbidden_bridge_tokens = ("kaspire.kaslab.space", "GHOST_DOTK_PROOF_NODE_URL")
    if any(token in dotk or token in chain for token in forbidden_bridge_tokens):
        fail("DotK proof must use the active Kaspa wRPC node, not an external proof bridge")
    for manifest in REPO_ROOT.glob("**/Cargo.toml"):
        if any(part in {"target", "vendor", "external"} for part in manifest.parts):
            continue
        if re.search(r'(?m)^\s*kaspa-(?:wrpc-client|addresses|txscript|hashes|consensus-core|rpc-core)\s*[=.]', text(manifest)):
            fail(f"direct Rusty-Kaspa dependency remains (Kaspa access goes through Kaspa Portal): {rel(manifest)}")

    wallet_dir = APP_CRATES / "ghost-kaspa" / "src" / "wallet"
    kaspa_wallet = "".join(text(wallet_dir / name) for name in ("planning.rs", "consolidation.rs"))
    if not re.search(r'kaspa-portal = \{ git = "https://github.com/peavey2787/kaspa-portal.git", rev = "[0-9a-f]{40}" \}', workspace_manifest):
        fail("Ghost Talk must pin Kaspa Portal (>= 1.1.0 notification API) to one reviewed revision")
    fee_tokens = ("analysis.fee_sufficient", ".minimum_fee_sompi", ".recommended_fee_sompi")
    if any(token not in kaspa_wallet for token in fee_tokens):
        fail("wallet/consolidation flow must enforce Portal's Toccata-aware fee analysis and replan floor")
    if re.search(r'(?i)(?:fee[_ ]rate|sompi[_ ]per[_ ]gram).*\b1\b', kaspa_wallet):
        fail("wallet code appears to contain a pre-Toccata 1-sompi/gram fixed-fee assumption")
    dotk_route = (
        'ends_with(".k")' in peer_names
        and "GhostNameResolver::normalize(GhostNameNamespace::DotK" in peer_names
        and "names.resolve(&claim, Some(&endpoint))" in peer_names
        and "DotkResolver" in name_resolver
        and re.search(r"\.dotk\s*\.resolve_owner\(&claim\.name,\s*endpoint\)", name_resolver)
    )
    if not dotk_route:
        fail("native peer resolution does not route .k through the unified verified DotK resolver")
    if "gateway.portal_for_network" not in peer_names or "portal.endpoint()" not in peer_names:
        fail("DotK chain proof must derive its endpoint from the shared NodeSession/Kaspa gateway")
    if 'ends_with(".k")' not in recipient or "fresh deed proof" not in recipient:
        fail("recipient input may satisfy an explicit .k name from cached application state")
    if "dotk_name" not in text(APP_CRATES / "ghost-api" / "src" / "peer.rs"):
        fail("resolved/public peer DTOs do not preserve the DotK alias")


def check_native_frontend_contract() -> None:
    """Native Tauri frontend build output must stay under the repository target tree."""
    import json
    config_path = APP_CRATES / "ghost-talk-native" / "tauri.conf.json"
    try:
        config = json.loads(text(config_path))
    except Exception as error:
        fail(f"native Tauri config is unreadable: {error}")
        return
    frontend = config.get("build", {}).get("frontendDist")
    expected = "../../../../target/build/frontend"
    if frontend != expected:
        fail(f"native Tauri frontendDist must be {expected!r}, found {frontend!r}")
    trunk = text(APP_CRATES / "ghost-wasm" / "Trunk.toml")
    if 'dist = "../../../../target/build/frontend"' not in trunk:
        fail("Trunk output must remain under top-level target/build/frontend")
    windows_stage = text(APP_ROOT / "scripts" / "windows" / "_stage-frontend.cmd")
    linux_common = text(APP_ROOT / "scripts" / "linux" / "_common.sh")
    if "target\\build\\frontend" not in windows_stage:
        fail("Windows builds must consume WASM output from top-level target/build/frontend")
    if "target/build/frontend" not in linux_common:
        fail("Unix builds must consume WASM output from top-level target/build/frontend")
    if (APP_CRATES / "ghost-wasm" / "dist").exists():
        fail("generated WASM dist directory must not live in the source tree")
    if (APP_CRATES / "ghost-talk-native" / "frontend").exists():
        fail("generated native frontend directory must not live in the source tree")
    windows_build = text(APP_ROOT / "scripts" / "windows" / "build.cmd")
    if "CARGO_TARGET_DIR=%REPO_ROOT%\\target" not in windows_build:
        fail("Windows desktop Cargo output must use the repository-level target directory")
    if "target\\dist\\windows\\%BUILD_PROFILE%" not in windows_build:
        fail("Windows desktop builds must stage final artifacts under target/dist/windows/<profile>")
    if not (APP_ROOT / "scripts" / "windows" / "build-debug.cmd").is_file():
        fail("Windows desktop debug build wrapper is missing")
    web_build = text(APP_ROOT / "scripts" / "wasm" / "build.cmd")
    for token in (
        "CARGO_TARGET_DIR=%REPO_ROOT%\\target",
        "target\\build\\frontend",
        "target\\dist\\web\\release",
        "--platform web --profile release",
    ):
        if token not in web_build:
            fail(f"Web release build is missing required target-tree contract: {token}")
    if not (APP_ROOT / "build-web-release.cmd").is_file():
        fail("app-level build-web-release.cmd wrapper is missing")
    artifacts = text(APP_ROOT / "scripts" / "artifacts" / "stage.py")
    if '"web"' not in artifacts or '_copy_web' not in artifacts:
        fail("artifact staging must support recursive Web release bundles")
    wasm_manifest = text(APP_CRATES / "ghost-wasm" / "Cargo.toml")
    for token in (
        'getrandom = { version = "0.2.17", features = ["js"] }',
        'getrandom_v03 = { package = "getrandom", version = "0.3", features = ["wasm_js"] }',
        'getrandom_v04 = { package = "getrandom", version = "0.4", features = ["wasm_js"] }',
        'ring = { version = "0.17", features = ["wasm32_unknown_unknown_js"] }',
    ):
        if token not in wasm_manifest:
            fail("standalone Web crypto must enable browser entropy for getrandom 0.2/0.3/0.4 and ring")
    browser_invoke = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "invoke.rs")
    if browser_invoke.count("super::browser_host::invoke(command, args).await") < 2:
        fail("standalone Web commands must bypass Tauri invoke for value and unit calls")
    browser_host = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "mod.rs")
    for token in ("profile_state_", "wallet_", "hydra_", "unknown standalone Web command"):
        if token not in browser_host:
            fail(f"standalone Web browser host is missing runtime contract: {token}")
    command_pattern = re.compile(
        r'"((?:wallet|hydra|mailbox|media|kasia|broadcast|profile_backup|debug_log|kaspa_archive|remembered_unlock)_[a-z0-9_]+'
        r'|resolve_ghost_peer|lookup_ghost_profile|publish_ghost_descriptor|profile_state_[a-z0-9_]+)"'
    )
    native_command_root = APP_CRATES / "ghost-wasm" / "src" / "native"
    frontend_commands: set[str] = set()
    for source_path in native_command_root.rglob("*.rs"):
        if "browser_host" in source_path.parts:
            continue
        frontend_commands.update(command_pattern.findall(text(source_path)))
    browser_commands: set[str] = set()
    for source_path in (native_command_root / "browser_host").rglob("*.rs"):
        browser_commands.update(command_pattern.findall(text(source_path)))
    missing_browser_commands = sorted(frontend_commands - browser_commands)
    if missing_browser_commands:
        fail(
            "standalone Web must implement every frontend native command; missing: "
            + ", ".join(missing_browser_commands)
        )
    shared_live = text(APP_CRATES / "ghost-kaspa" / "src" / "live.rs")
    for signature in ("pub fn fallback_live_event_id", "pub fn is_live_ghost_carrier"):
        if signature not in shared_live:
            fail(f"shared native/Web live-carrier helper is missing: {signature}")
    portal_client = text(APP_CRATES / "ghost-kaspa" / "src" / "portal" / "client.rs")
    browser_live = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "runtime" / "live.rs")
    for token in ("fallback_live_event_id", "is_live_ghost_carrier"):
        if token not in portal_client or token not in browser_live:
            fail(f"native and Web block adapters must share `{token}`")
    discover_editor = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "discover" / "view" / "editor.rs")
    if "ghost_address.is_empty()" in discover_editor or "Publish / update" not in discover_editor:
        fail("Discover publish/update must not depend on a previously registered Ghost address")
    browser_discover = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "discover.rs")
    browser_kaspa = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "kaspa.rs")
    for token in ("publish_ghost_descriptor", "build_signed_descriptor", "send_payload"):
        if token not in browser_discover:
            fail(f"standalone Web Discover publish path is missing `{token}`")
    if "PUBLIC_WRPC_RESOLVERS" not in browser_kaspa:
        fail("standalone Web Discover/Signer must resolve a public Kaspa wRPC endpoint on demand")
    if "active_addresses: addresses" in browser_kaspa or "funded_addresses" not in browser_kaspa:
        fail("standalone Web wallet snapshot must report funded addresses from live UTXOs, not every derived address")
    browser_live = text(
        APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "runtime" / "live.rs"
    )
    for token in ("subscribe_block_added", '"ghost://wallet-live"', '"ghost://directory-live"'):
        if token not in browser_live and token != '"ghost://directory-live"':
            fail(f"standalone Web live Kaspa pipeline is missing `{token}`")
    browser_peer = text(
        APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "runtime" / "peer.rs"
    )
    browser_mailbox = text(
        APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "runtime" / "mailbox.rs"
    )
    if '"resolve_ghost_peer"' not in browser_peer:
        fail("standalone Web must resolve Ghost peers from the live Kaspa directory")
    for token in ('"mailbox_send_contact_request"', '"hydra_receive_mailbox"'):
        if token not in browser_mailbox:
            fail(f"standalone Web contact-request parity is missing `{token}`")
    for token in (
        '"wallet_monitor_start"',
        '"wallet_monitor_snapshot"',
        "PORTALS",
        "profile_portal",
        "current_virtual_daa_score",
    ):
        if token not in browser_kaspa:
            fail(f"standalone Web persistent Kaspa monitor is missing `{token}`")
    browser_wallet = text(
        APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "wallet.rs"
    )
    if 'command.starts_with("wallet_monitor_")' in browser_wallet:
        fail("standalone Web wallet monitor must not be a no-op in the wallet host")
    browser_network = text(
        APP_CRATES / "ghost-wasm" / "src" / "app" / "profile_runtime" / "network.rs"
    )
    for token in ("browser_wallet_monitor_loop", '"connected"', "browser_wallet_snapshot"):
        if token not in browser_network:
            fail(f"standalone Web network bootstrap is missing `{token}`")
    avatar_editor = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "discover" / "view" / "avatar_editor.rs")
    avatar_crop = text(APP_CRATES / "ghost-wasm" / "src" / "components" / "discover" / "view" / "avatar_editor" / "crop.rs")
    if "Apply crop" not in avatar_editor or "drag_callbacks" not in avatar_editor or "to_data_url_with_type" not in avatar_crop:
        fail("Discover avatar editor must preview, drag/reposition, and render the applied crop")
    for token in ("value_as_number", "oninput", "onchange"):
        if token not in avatar_editor:
            fail(f"Discover avatar range controls are missing live slider binding `{token}`")
    browser_media = text(APP_CRATES / "ghost-wasm" / "src" / "native" / "browser_host" / "media.rs")
    for token in ("media_import_local", "media_fetch_verified", "ghost-talk.media.v1"):
        if token not in browser_media:
            fail(f"standalone Web avatar media cache is missing `{token}`")

    for signature in (
        'pub async fn subscribe_block_added(&self) -> Result<(), String>',
        'pub async fn next_block_added(&self) -> Result<LiveBlockEvent, String>',
    ):
        marker = '#[cfg(not(target_arch = "wasm32"))]\n    ' + signature
        if marker not in portal_client:
            fail(f"native-only Kaspa Portal BlockAdded adapter is not WASM-gated: {signature}")


def check_versions() -> None:
    version_re = re.compile(r'(?m)^version\s*=\s*"([^"]+)"')
    bad = []
    for manifest in sorted(REPO_ROOT.glob("**/Cargo.toml")):
        if any(part in {"target", "vendor", "external"} for part in manifest.parts):
            continue
        match = version_re.search(manifest.read_text(encoding="utf-8", errors="replace"))
        if match and match.group(1) != "0.1.0":
            bad.append(f"{rel(manifest)}={match.group(1)}")
    if bad:
        fail("Ghost Talk package semantic versions must remain 0.1.0: " + ", ".join(bad))



def check_release_engineering() -> None:
    required = (
        REPO_ROOT / "SECURITY.md",
        REPO_ROOT / "CONTRIBUTING.md",
        REPO_ROOT / "CHANGELOG.md",
        REPO_ROOT / ".github" / "workflows" / "quality.yml",
        REPO_ROOT / "docs" / "release" / "RELEASE.md",
        REPO_ROOT / "scripts" / "release" / "check-release-inputs.py",
        REPO_ROOT / "scripts" / "release" / "package-source.py",
        REPO_ROOT / "scripts" / "release" / "check-source-package.py",
        REPO_ROOT / "scripts" / "release" / "generate-sbom.py",
        REPO_ROOT / "scripts" / "release" / "hash-artifacts.py",
        REPO_ROOT / "scripts" / "release" / "run-release-gates.sh",
        REPO_ROOT / "scripts" / "release" / "run-release-gates.cmd",
    )
    for path in required:
        if not path.is_file():
            fail(f"missing downstream/release-engineering surface: {rel(path)}")
    shell = text(REPO_ROOT / "scripts" / "release" / "run-release-gates.sh")
    for token in ("cargo deny check", "cargo audit --file", "cargo metadata", "--locked",
                  "generate-sbom.py", "hash-artifacts.py", "check-release-inputs.py",
                  "package-source.py", "check-source-package.py"):
        if token not in shell:
            fail(f"release gate is missing required step `{token}`")
    roadmap = text(APP_ROOT / "docs" / "roadmap.md")
    if "automated testing is intentionally limited to Rust unit tests" in roadmap:
        fail("roadmap contains obsolete test-coverage claims")
