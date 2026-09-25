#!/usr/bin/env python3
"""Validate that a Ghost Talk source archive contains source inputs only."""
from __future__ import annotations

import argparse
import sys
import zipfile
from pathlib import PurePosixPath

FORBIDDEN_DIRS = {"target", "dist", ".git", ".cargo", "node_modules", "__pycache__"}
FORBIDDEN_SUFFIXES = {
    ".exe", ".dll", ".so", ".dylib", ".wasm", ".apk", ".aab", ".ipa", ".msi",
    ".deb", ".appimage", ".pyc", ".pyo", ".log", ".db", ".sqlite",
}


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def validate_member(name: str) -> None:
    path = PurePosixPath(name)
    parts = tuple(part.lower() for part in path.parts)
    source_parts = parts[1:] if parts and parts[0].startswith("ghost-talk-") else parts
    if any(part in FORBIDDEN_DIRS for part in source_parts) or (source_parts and source_parts[0] == "release"):
        fail(f"source archive contains generated/build directory: {name}")
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        fail(f"source archive contains generated/binary artifact: {name}")


def validate_required_source_contracts(archive: zipfile.ZipFile, names: list[str]) -> None:
    handshake_suffix = "examples/ghost-talk-app/crates/ghost-kasia/src/handshake.rs"
    matches = [name for name in names if name.endswith(handshake_suffix)]
    if len(matches) != 1:
        fail("source archive must contain exactly one ghost-kasia handshake source")
    text = archive.read(matches[0]).decode("utf-8")
    required = "#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]"
    if required not in text:
        fail("packaged KasiaHandshake is missing serde Serialize/Deserialize derives")

    config_suffix = "examples/ghost-talk-app/crates/ghost-talk-native/tauri.conf.json"
    configs = [name for name in names if name.endswith(config_suffix)]
    if len(configs) != 1:
        fail("source archive must contain exactly one native Tauri config")
    import json
    config = json.loads(archive.read(configs[0]).decode("utf-8"))
    frontend = config.get("build", {}).get("frontendDist")
    if not isinstance(frontend, str) or not frontend.strip():
        fail("packaged Tauri config is missing frontendDist")
    expected_frontend = "../../../../target/build/frontend"
    if frontend != expected_frontend:
        fail(f"packaged Tauri frontendDist must be {expected_frontend!r}, found {frontend!r}")

    wasm_index_suffix = "examples/ghost-talk-app/crates/ghost-wasm/index.html"
    wasm_trunk_suffix = "examples/ghost-talk-app/crates/ghost-wasm/Trunk.toml"
    indexes = [name for name in names if name.endswith(wasm_index_suffix)]
    trunks = [name for name in names if name.endswith(wasm_trunk_suffix)]
    if len(indexes) != 1 or len(trunks) != 1:
        fail("source archive must contain the Web frontend source index.html and Trunk.toml")
    trunk = archive.read(trunks[0]).decode("utf-8")
    if 'dist = "../../../../target/build/frontend"' not in trunk:
        fail("packaged Trunk config must emit generated frontend files under top-level target/build/frontend")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive")
    args = parser.parse_args()
    try:
        with zipfile.ZipFile(args.archive) as archive:
            names = [info.filename for info in archive.infolist() if not info.is_dir()]
            validate_required_source_contracts(archive, names)
    except (OSError, zipfile.BadZipFile) as error:
        fail(f"cannot inspect source archive: {error}")
    if not names:
        fail("source archive contains no files")
    for name in names:
        validate_member(name)
    print(f"PASS: source package contains {len(names)} source files and no generated/build artifacts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
