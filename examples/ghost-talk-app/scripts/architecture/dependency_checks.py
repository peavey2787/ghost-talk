"""First-party dependency ownership and crate reachability checks."""
from __future__ import annotations

import re
import tomllib
from collections import defaultdict
from pathlib import Path

from .common import APP_CRATES, REPO_ROOT, SDK_CRATES, fail, rel, sanitize_rust

LEAF_CRATES = {
    "ghost-talk",          # published SDK root
    "ghost-talk-wasm",     # published browser adapter
    "ghost-talk-native",   # native application executable
    "ghost-realtime",      # reusable Ghost/Kinesis realtime protocol/router surface
    "ghost-p2p",           # reusable Ghost/Kinesis p2p identity/binding surface
    "ghost-wasm",          # standalone browser application workspace
    "ghost-direct-chat",   # focused composition/demo binaries
    "ghost-kaspa-voice",
    "ghost-pong",
    "ghost-presenter-room",
    "ghost-radio-room",
}


def _manifest_data(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _dependency_tables(data: dict):
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        yield data.get(key, {})
    for target in data.get("target", {}).values():
        for key in ("dependencies", "dev-dependencies", "build-dependencies"):
            yield target.get(key, {})


def _manifests() -> dict[str, Path]:
    result: dict[str, Path] = {}
    roots = (SDK_CRATES, APP_CRATES)
    for root in roots:
        for manifest in root.glob("*/Cargo.toml"):
            data = _manifest_data(manifest)
            package = data.get("package", {}).get("name")
            if package:
                result[package] = manifest
    return result


def _crate_source(manifest: Path) -> str:
    source_root = manifest.parent / "src"
    if not source_root.is_dir():
        return ""
    return "\n".join(
        sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        for path in source_root.rglob("*.rs")
    )


def check_first_party_dependencies() -> None:
    manifests = _manifests()
    inbound: dict[str, set[str]] = defaultdict(set)

    for owner, manifest in manifests.items():
        data = _manifest_data(manifest)
        source = _crate_source(manifest)
        for table in _dependency_tables(data):
            for dependency, spec in table.items():
                if not isinstance(spec, dict) or "path" not in spec:
                    continue
                package = spec.get("package", dependency)
                if package in manifests:
                    inbound[package].add(owner)
                token = dependency.replace("-", "_")
                if not re.search(rf"\b{re.escape(token)}\b", source):
                    fail(
                        f"{rel(manifest)} declares unused first-party path dependency "
                        f"`{dependency}`"
                    )

    for package, manifest in sorted(manifests.items()):
        if "example" in package.lower() or "example" in manifest.parent.name.lower():
            fail(
                f"{rel(manifest)} uses `example` in a first-party crate/package name; "
                "use the crate's actual responsibility instead"
            )
        if package in LEAF_CRATES:
            continue
        if not inbound.get(package):
            fail(
                f"{rel(manifest)} is an unconsumed first-party library crate; "
                "remove it or document/connect it as a supported public surface"
            )


def check_declared_first_party_usage() -> None:
    """Every directly named first-party crate must be declared by the consuming manifest."""
    manifests = _manifests()
    for owner, manifest in manifests.items():
        source = _crate_source(manifest)
        declared = _declared_dependency_names(manifest)
        for package in manifests:
            if package == owner:
                continue
            token = package.replace("-", "_")
            if re.search(rf"\b{re.escape(token)}::", source) and package not in declared:
                fail(
                    f"{rel(manifest)} uses first-party crate `{package}` without declaring "
                    "a direct path dependency"
                )


def check_lockfile_policy() -> None:
    """Applications must commit a lockfile; reusable SDK root may stay unlocked."""
    app_lock = REPO_ROOT / "examples" / "ghost-talk-app" / "Cargo.lock"
    if not app_lock.is_file():
        fail("examples/ghost-talk-app/Cargo.lock is required for reproducible application builds")



def _lock_packages(path: Path) -> dict[str, set[str]]:
    data = _manifest_data(path)
    return {
        package["name"]: set(package.get("dependencies", []))
        for package in data.get("package", [])
    }


def _declared_dependency_names(manifest: Path) -> set[str]:
    data = _manifest_data(manifest)
    names: set[str] = set()
    for table in _dependency_tables(data):
        for dependency, spec in table.items():
            if isinstance(spec, dict):
                names.add(spec.get("package", dependency))
            else:
                names.add(dependency)
    return names


def _local_dependency_names(manifest: Path) -> set[str]:
    data = _manifest_data(manifest)
    names: set[str] = set()
    for table in _dependency_tables(data):
        for dependency, spec in table.items():
            if isinstance(spec, dict) and "path" in spec:
                names.add(spec.get("package", dependency))
    return names


def _locked_dependency_present(dependencies: set[str], name: str) -> bool:
    return any(value == name or value.startswith(f"{name} ") for value in dependencies)


def check_local_lock_consistency() -> None:
    """Fail when committed locks omit a first-party path dependency."""
    app_root = REPO_ROOT / "examples" / "ghost-talk-app"
    lock_sets = (
        (app_root / "Cargo.lock", app_root / "crates"),
        (app_root / "crates" / "ghost-wasm" / "Cargo.lock", app_root / "crates"),
    )
    for lock, crates in lock_sets:
        if not lock.is_file():
            fail(f"missing committed application lockfile: {rel(lock)}")
            continue
        locked = _lock_packages(lock)
        for manifest in crates.glob("*/Cargo.toml"):
            package = _manifest_data(manifest).get("package", {}).get("name")
            if package not in locked:
                continue
            for dependency in _local_dependency_names(manifest):
                if dependency in locked and not _locked_dependency_present(locked[package], dependency):
                    fail(f"{rel(lock)} is stale: `{package}` is missing path dependency `{dependency}`")



def check_yew_indexmap_unification() -> None:
    """Keep the browser UI and Kaspa graph on one compatible indexmap 2.x."""
    manifest = _manifest_data(APP_CRATES / "ghost-wasm" / "Cargo.toml")
    wasm = manifest.get("target", {}).get('cfg(target_arch = "wasm32")', {}).get("dependencies", {})
    implicit = wasm.get("implicit-clone", {})
    yew = wasm.get("yew", {})
    if not isinstance(implicit, dict) or implicit.get("version") != "=0.6.0" or "map" not in implicit.get("features", []):
        fail("ghost-wasm must pin implicit-clone 0.6.0 with its map feature for Yew 0.22")
    if wasm.get("indexmap") != "=2.14.2":
        fail("ghost-wasm must pin the shared Yew/Kaspa indexmap package to 2.14.2")
    if not isinstance(yew, dict) or yew.get("version") != "=0.22.0" or "csr" not in yew.get("features", []):
        fail("ghost-wasm must pin Yew 0.22.0 with its CSR feature")

    lock = _manifest_data(APP_CRATES / "ghost-wasm" / "Cargo.lock")
    packages = lock.get("package", [])
    expected = (("implicit-clone", "0.6.0"), ("yew", "0.22.0"))
    for name, version in expected:
        package = next((item for item in packages if item.get("name") == name and item.get("version") == version), None)
        if package is None or "indexmap 2.14.2" not in package.get("dependencies", []):
            fail(f"standalone WASM lock must bind {name} {version} to indexmap 2.14.2")

def check_kasia_wasm_portability() -> None:
    """Keep native Kasia crypto/indexer dependencies out of browser builds."""
    kasia = _manifest_data(APP_CRATES / "ghost-kasia" / "Cargo.toml")
    features = kasia.get("features", {})
    native = set(features.get("native", []))
    crypto = set(features.get("crypto", []))
    portable_crypto = {"dep:chacha20poly1305", "dep:hkdf", "dep:secp256k1", "dep:sha2", "dep:thiserror", "dep:zeroize"}
    if features.get("default") != ["native"] or "crypto" not in native or "dep:reqwest" not in native:
        fail("ghost-kasia native feature must compose portable crypto with its native indexer")
    if not portable_crypto.issubset(crypto):
        fail("ghost-kasia crypto feature must own the browser-safe cryptographic dependency set")

    for package in ("ghost-runtime", "ghost-wasm"):
        manifest = _manifest_data(APP_CRATES / package / "Cargo.toml")
        spec = manifest.get("dependencies", {}).get("ghost-kasia", {})
        if not isinstance(spec, dict) or spec.get("default-features") is not False:
            fail(f"{package} must consume ghost-kasia with default-features = false")
        if package == "ghost-wasm" and "crypto" not in spec.get("features", []):
            fail("ghost-wasm must explicitly enable ghost-kasia's portable crypto feature")

    native_manifest = _manifest_data(APP_CRATES / "ghost-talk-native" / "Cargo.toml")
    native_spec = native_manifest.get("dependencies", {}).get("ghost-kasia", {})
    if not isinstance(native_spec, dict) or "native" not in native_spec.get("features", []):
        fail("ghost-talk-native must explicitly enable ghost-kasia's native feature")


def check_mailbox_submission_ownership() -> None:
    """Keep generic wallet unlock/fee/submission wiring in one mailbox owner."""
    mailbox = APP_CRATES / "ghost-talk-native" / "src" / "mailbox_commands"
    owner = mailbox / "submission" / "outbound_send.rs"
    forbidden = ("MailboxSubmitContext::new", "secret_or_open(", "parse_u64_decimal")
    for path in mailbox.rglob("*.rs"):
        if path == owner:
            continue
        source = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
        for token in forbidden:
            if token in source:
                fail(
                    f"{rel(path)} duplicates outbound mailbox submission setup `{token}`; "
                    "route it through submission/outbound_send.rs"
                )
