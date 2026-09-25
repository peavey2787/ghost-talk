#!/usr/bin/env python3
"""Create a deterministic source-only Ghost Talk ZIP from the working tree."""
from __future__ import annotations

import argparse
import os
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SKIP_DIRS = {"target", "dist", ".git", ".cargo", "node_modules", "__pycache__"}
SKIP_SUFFIXES = {".pyc", ".pyo", ".log", ".db", ".sqlite"}
FIXED_TIME = (1980, 1, 1, 0, 0, 0)


def included(path: Path) -> bool:
    rel = path.relative_to(ROOT)
    parts = tuple(part.lower() for part in rel.parts)
    if parts and parts[0] == "release":
        return False
    return not any(part in SKIP_DIRS for part in parts) and path.suffix.lower() not in SKIP_SUFFIXES


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    output = Path(args.output).expanduser().resolve()
    revision = (ROOT / "REVISION").read_text(encoding="utf-8").strip()
    prefix = f"ghost-talk-{revision}/"
    files = sorted(path for path in ROOT.rglob("*") if path.is_file() and included(path) and path.resolve() != output)
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in files:
            rel = path.relative_to(ROOT).as_posix()
            info = zipfile.ZipInfo(prefix + rel, FIXED_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            mode = 0o755 if os.access(path, os.X_OK) else 0o644
            info.external_attr = mode << 16
            archive.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
