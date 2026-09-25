#!/usr/bin/env python3
"""Fail closed on reproducibility locks and external commercial-release evidence."""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOCKS = (
    ROOT / "examples/ghost-talk-app/Cargo.lock",
    ROOT / "examples/ghost-talk-app/crates/ghost-wasm/Cargo.lock",
)
REQUIRED = {
    "funded_multi_instance_e2e",
    "production_keystore_biometrics",
    "group_cryptographic_lifecycle",
    "device_audio_hil",
    "platform_hardening",
    "interoperability",
    "background_notifications",
    "ui_platform_parity",
    "release_manifest_signature",
}


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def check_locks() -> None:
    missing = [str(path.relative_to(ROOT)) for path in LOCKS if not path.is_file()]
    if missing:
        fail("commercial release requires committed Cargo lockfile(s): " + ", ".join(missing))


def check_evidence() -> None:
    evidence_path = os.environ.get("GHOST_TALK_RELEASE_EVIDENCE", "").strip()
    if not evidence_path:
        fail("GHOST_TALK_RELEASE_EVIDENCE must point to qualification evidence JSON")
    path = Path(evidence_path).expanduser().resolve()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read release evidence {path}: {error}")
    revision = (ROOT / "REVISION").read_text(encoding="utf-8").strip()
    if data.get("revision") != revision:
        fail(f"release evidence revision {data.get('revision')!r} does not match {revision!r}")
    requirements = data.get("requirements")
    if not isinstance(requirements, dict):
        fail("release evidence must contain a requirements object")
    missing = sorted(REQUIRED - set(requirements))
    if missing:
        fail("release evidence is missing requirement(s): " + ", ".join(missing))
    for name in sorted(REQUIRED):
        record = requirements[name]
        if not isinstance(record, dict) or record.get("status") != "pass":
            fail(f"release evidence `{name}` is not verified pass")
        for field in ("tested_at", "environment", "evidence"):
            if not str(record.get(field, "")).strip():
                fail(f"release evidence `{name}` is missing {field}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--locks-only", action="store_true")
    args = parser.parse_args()
    check_locks()
    if not args.locks_only:
        check_evidence()
    print("PASS: release lockfiles" + ("" if args.locks_only else " and external qualification evidence"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
