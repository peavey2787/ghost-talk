#!/usr/bin/env python3
"""Generate a deterministic CycloneDX JSON SBOM from one or more cargo-metadata files."""
from __future__ import annotations

import argparse
import json
from pathlib import Path


def component(package: dict) -> dict:
    item = {
        "type": "library",
        "bom-ref": package["id"],
        "name": package["name"],
        "version": package["version"],
    }
    license_name = package.get("license")
    if license_name:
        item["licenses"] = [{"expression": license_name}]
    source = package.get("source")
    if source:
        item["properties"] = [{"name": "cargo:source", "value": source}]
    return item


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("metadata", nargs="+")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    packages: dict[str, dict] = {}
    dependencies: dict[str, set[str]] = {}
    for metadata_path in args.metadata:
        data = json.loads(Path(metadata_path).read_text(encoding="utf-8"))
        for package in data.get("packages", []):
            packages[package["id"]] = package
        resolve = data.get("resolve") or {}
        for node in resolve.get("nodes", []):
            dependencies.setdefault(node["id"], set()).update(dep["pkg"] for dep in node.get("deps", []))
    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": "urn:uuid:00000000-0000-0000-0000-000000000000",
        "version": 1,
        "metadata": {"component": {"type": "application", "name": "ghost-talk-release"}},
        "components": [component(packages[key]) for key in sorted(packages)],
        "dependencies": [
            {"ref": key, "dependsOn": sorted(dependencies.get(key, set()))}
            for key in sorted(packages)
        ],
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(bom, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
