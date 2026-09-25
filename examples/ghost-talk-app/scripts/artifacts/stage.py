#!/usr/bin/env python3
"""Stage user-facing Ghost Talk application artifacts into stable target folders."""
from __future__ import annotations

import argparse
import shutil
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
APP_ROOT = REPO_ROOT / "examples" / "ghost-talk-app"
NATIVE_ROOT = APP_ROOT / "crates" / "ghost-talk-native"
TARGET_ROOT = REPO_ROOT / "target"


def _copy(files: list[Path], destination: Path) -> list[Path]:
    if not files:
        raise SystemExit("ERROR: no user-facing build artifacts were found to stage")
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)

    staged: list[Path] = []
    used_names: set[str] = set()
    for source in files:
        name = source.name
        if name in used_names:
            parent = source.parent.name.replace(" ", "-")
            name = f"{parent}-{name}"
        used_names.add(name)
        output = destination / name
        shutil.copy2(source, output)
        staged.append(output)
    return staged


def _android_search_roots() -> tuple[Path, ...]:
    """Known Tauri/Gradle package roots across CLI/Gradle layout revisions.

    Tauri owns the generated Gradle project, so its internal package location is
    not part of Ghost Talk's public build contract. Keep discovery flexible and
    always stage final Android packages into repository target/android, with a
    target/dist compatibility mirror.
    """
    return (
        NATIVE_ROOT / "gen" / "android",
        TARGET_ROOT / "aarch64-linux-android",
        TARGET_ROOT / "armv7-linux-androideabi",
    )


def _android(profile: str) -> list[Path]:
    extensions = {".apk", ".aab"}
    profile_token = profile.lower()
    candidates: set[Path] = set()
    for root in _android_search_roots():
        if not root.is_dir():
            continue
        for path in root.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in extensions:
                continue
            # The search roots contain only build-owned locations; staged
            # target/android and target/dist copies are deliberately excluded.
            relative = str(path.relative_to(root)).lower()
            if profile_token in relative or profile_token in path.name.lower():
                candidates.add(path.resolve())
    return sorted(candidates)


def _windows(profile: str) -> list[Path]:
    cargo_profile = TARGET_ROOT / profile
    files: list[Path] = []
    app = cargo_profile / "ghost-talk-native.exe"
    if app.is_file():
        files.append(app)
    bundle = cargo_profile / "bundle"
    if bundle.is_dir():
        allowed = {".exe", ".msi", ".msix"}
        files.extend(
            sorted(path for path in bundle.rglob("*") if path.is_file() and path.suffix.lower() in allowed)
        )
    return files


def _linux(profile: str) -> list[Path]:
    cargo_profile = TARGET_ROOT / profile
    files: list[Path] = []
    app = cargo_profile / "ghost-talk-native"
    if app.is_file():
        files.append(app)
    bundle = cargo_profile / "bundle"
    if bundle.is_dir():
        allowed = {".deb", ".rpm", ".appimage"}
        files.extend(
            sorted(path for path in bundle.rglob("*") if path.is_file() and path.suffix.lower() in allowed)
        )
    return files


def _ios(_profile: str) -> list[Path]:
    build = NATIVE_ROOT / "gen" / "apple" / "build"
    if not build.is_dir():
        return []
    return sorted(path for path in build.rglob("*.ipa") if path.is_file())


def _copy_web(destination: Path) -> list[Path]:
    source = TARGET_ROOT / "build" / "frontend"
    index = source / "index.html"
    if not index.is_file():
        raise SystemExit(
            f"ERROR: web build completed but no frontend entry point was found: {index}"
        )
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(source, destination)
    return sorted(path for path in destination.rglob("*") if path.is_file())


def _mirror(staged: list[Path], source_root: Path, destination: Path) -> list[Path]:
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)
    mirrored: list[Path] = []
    for source in staged:
        output = destination / source.relative_to(source_root)
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, output)
        mirrored.append(output)
    return mirrored


def _write_manifest(destination: Path, staged: list[Path]) -> Path:
    manifest = destination / "ARTIFACTS.txt"
    lines = [
        f"{path.relative_to(destination).as_posix()}\t{path.stat().st_size} bytes"
        for path in staged
        if path != manifest
    ]
    manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return manifest


def _android_failure(profile: str) -> SystemExit:
    roots = _android_search_roots()
    existing: list[Path] = []
    for root in roots:
        if not root.is_dir():
            continue
        existing.extend(
            path
            for path in root.rglob("*")
            if path.is_file() and path.suffix.lower() in {".apk", ".aab"}
        )
    searched = "\n".join(f"  searched: {root}" for root in roots)
    found = "\n".join(f"  found: {path}" for path in sorted(set(existing)))
    if not found:
        found = "  no APK/AAB files were found in any known Tauri/Gradle package root"
    return SystemExit(
        f"ERROR: Android {profile} build completed but no matching APK/AAB was found.\n"
        f"{searched}\n{found}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--platform", required=True, choices=("android", "windows", "linux", "ios", "web")
    )
    parser.add_argument("--profile", required=True, choices=("release", "debug"))
    args = parser.parse_args()

    destination = (
        TARGET_ROOT / "android" / args.profile
        if args.platform == "android"
        else TARGET_ROOT / "dist" / args.platform / args.profile
    )
    if args.platform == "web":
        if args.profile != "release":
            raise SystemExit("ERROR: Web artifact staging currently supports release builds only")
        staged = _copy_web(destination)
    else:
        discover = {
            "android": _android,
            "windows": _windows,
            "linux": _linux,
            "ios": _ios,
        }[args.platform]
        discovered = discover(args.profile)
        if args.platform == "android":
            suffixes = {path.suffix.lower() for path in discovered}
            if not {".apk", ".aab"}.issubset(suffixes):
                raise _android_failure(args.profile)
        elif not discovered:
            raise SystemExit(
                f"ERROR: no {args.platform} {args.profile} artifacts were found to stage"
            )
        staged = _copy(discovered, destination)

    manifest = _write_manifest(destination, staged)
    mirror_manifest = None
    mirror = None
    if args.platform == "android":
        mirror = TARGET_ROOT / "dist" / "android" / args.profile
        mirrored = _mirror(staged, destination, mirror)
        mirror_manifest = _write_manifest(mirror, mirrored)

    print("\n===== Ghost Talk distributable artifacts =====")
    if args.platform == "web":
        print(destination)
        print(f"Web entry point: {destination / 'index.html'}")
    else:
        for path in staged:
            print(path)
    print(f"Manifest: {manifest}")
    if mirror is not None and mirror_manifest is not None:
        print(f"Compatibility mirror: {mirror}")
        print(f"Mirror manifest: {mirror_manifest}")
    print("==============================================")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
