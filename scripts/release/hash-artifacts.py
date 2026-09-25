#!/usr/bin/env python3
"""Compare reproducible build pairs and emit a SHA-256 release manifest."""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pair", action="append", nargs=3, metavar=("NAME", "A", "B"), required=True)
    parser.add_argument("--manifest", required=True)
    args = parser.parse_args()
    rows: list[tuple[str, str, Path]] = []
    for name, first_name, second_name in args.pair:
        first, second = Path(first_name), Path(second_name)
        if not first.is_file() or not second.is_file():
            raise SystemExit(f"ERROR: reproducibility artifact missing for {name}")
        first_hash, second_hash = digest(first), digest(second)
        if first_hash != second_hash:
            raise SystemExit(f"ERROR: reproducibility mismatch for {name}: {first_hash} != {second_hash}")
        rows.append((name, first_hash, first))
    manifest = Path(args.manifest)
    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text("".join(f"{sha}  {name}\n" for name, sha, _ in rows), encoding="utf-8")
    print(f"PASS: reproducible artifacts match; manifest written to {manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
