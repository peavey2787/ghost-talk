"""Isolated rustup bootstrap for Ghost Talk's pinned Windows QA toolchain."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import urllib.request

RUSTUP_DIST = "https://static.rust-lang.org/rustup/dist"
WASM_TARGET = "wasm32-unknown-unknown"


def _runnable(path: str | None, *args: str) -> bool:
    if not path:
        return False
    try:
        result = subprocess.run(
            (path, *args),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            shell=False,
            check=False,
        )
    except (FileNotFoundError, OSError):
        return False
    return result.returncode == 0


def _host_triple() -> str:
    machine = platform.machine().lower()
    if machine in {"amd64", "x86_64"}:
        return "x86_64-pc-windows-msvc"
    if machine in {"arm64", "aarch64"}:
        return "aarch64-pc-windows-msvc"
    raise RuntimeError(f"unsupported Windows architecture for rustup bootstrap: {machine}")


def _local_homes(env: dict[str, str]) -> tuple[Path, Path, Path]:
    root_text = env.get("LOCALAPPDATA") or env.get("USERPROFILE")
    if not root_text:
        raise RuntimeError("LOCALAPPDATA/USERPROFILE is unavailable for isolated Rust setup")
    root = Path(root_text) / "GhostTalk" / "rust-toolchain"
    return root, root / "cargo", root / "rustup"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _download_verified(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    checksum_url = f"{url}.sha256"
    try:
        with urllib.request.urlopen(checksum_url, timeout=60) as response:
            expected = response.read().decode("ascii").strip().split()[0].lower()
        temporary = destination.with_suffix(destination.suffix + ".download")
        with urllib.request.urlopen(url, timeout=120) as response, temporary.open("wb") as output:
            shutil.copyfileobj(response, output)
    except OSError as exc:
        raise RuntimeError(f"failed to download official rustup bootstrap: {exc}") from exc
    actual = _sha256(temporary)
    if actual != expected:
        temporary.unlink(missing_ok=True)
        raise RuntimeError("official rustup bootstrap SHA-256 verification failed")
    temporary.replace(destination)


def _existing_rustup(env: dict[str, str]) -> str | None:
    candidates = [env.get("GHOST_TALK_RUSTUP_EXE"), shutil.which("rustup", path=env.get("PATH"))]
    if env.get("USERPROFILE"):
        candidates.append(str(Path(env["USERPROFILE"]) / ".cargo" / "bin" / "rustup.exe"))
    for candidate in candidates:
        if _runnable(candidate, "--version"):
            return str(Path(candidate).resolve())
    return None


def ensure_rustup(env: dict[str, str]) -> str:
    existing = _existing_rustup(env)
    if existing:
        return existing
    root, cargo_home, rustup_home = _local_homes(env)
    env["CARGO_HOME"] = str(cargo_home)
    env["RUSTUP_HOME"] = str(rustup_home)
    cargo_bin = cargo_home / "bin"
    rustup_exe = cargo_bin / "rustup.exe"
    env["PATH"] = str(cargo_bin) + os.pathsep + env.get("PATH", "")
    if _runnable(str(rustup_exe), "--version"):
        return str(rustup_exe)
    host = _host_triple()
    installer = root / "bootstrap" / "rustup-init.exe"
    url = f"{RUSTUP_DIST}/{host}/rustup-init.exe"
    if not installer.is_file():
        print(f"Downloading official rustup bootstrap for {host}...", flush=True)
        _download_verified(url, installer)
    print("Initializing isolated Ghost Talk rustup environment...", flush=True)
    result = subprocess.run(
        (str(installer), "-y", "--no-modify-path", "--profile", "minimal", "--default-toolchain", "none"),
        env=env,
        shell=False,
        check=False,
    )
    if result.returncode or not _runnable(str(rustup_exe), "--version"):
        raise RuntimeError(f"rustup bootstrap failed with exit code {result.returncode}")
    return str(rustup_exe)


def _capture(argv: tuple[str, ...], env: dict[str, str]) -> str | None:
    try:
        result = subprocess.run(argv, env=env, text=True, capture_output=True, shell=False, check=False)
    except (FileNotFoundError, OSError):
        return None
    return result.stdout.strip() if result.returncode == 0 else None


def _run(argv: tuple[str, ...], env: dict[str, str]) -> bool:
    return subprocess.run(argv, env=env, shell=False, check=False).returncode == 0


def ensure_pinned_toolchain(toolchain: str, env: dict[str, str]) -> tuple[str, str, str]:
    rustup = ensure_rustup(env)
    env["GHOST_TALK_RUSTUP_EXE"] = rustup
    env["RUSTUP_TOOLCHAIN"] = toolchain
    if not _runnable(rustup, "run", toolchain, "rustc", "--version"):
        print(f"Installing pinned Rust {toolchain} (minimal profile)...", flush=True)
        if not _run((rustup, "toolchain", "install", toolchain, "--profile", "minimal"), env):
            raise RuntimeError(f"failed to install pinned Rust toolchain {toolchain}")
    installed = _capture((rustup, "component", "list", "--toolchain", toolchain, "--installed"), env) or ""
    missing = [
        name
        for name in ("rustfmt", "clippy", "llvm-tools-preview")
        if not any(line.startswith(f"{name}-") for line in installed.splitlines())
    ]
    if missing:
        print(f"Installing Rust {toolchain} components: {', '.join(missing)}...", flush=True)
        if not _run((rustup, "component", "add", "--toolchain", toolchain, *missing), env):
            raise RuntimeError(f"failed to install required Rust components: {', '.join(missing)}")
    targets = _capture((rustup, "target", "list", "--toolchain", toolchain, "--installed"), env) or ""
    if WASM_TARGET not in targets.splitlines():
        print(f"Installing {WASM_TARGET} for Rust {toolchain}...", flush=True)
        if not _run((rustup, "target", "add", "--toolchain", toolchain, WASM_TARGET), env):
            raise RuntimeError(f"failed to install {WASM_TARGET} for Rust {toolchain}")
    cargo = _capture((rustup, "which", "--toolchain", toolchain, "cargo"), env)
    rustc = _capture((rustup, "which", "--toolchain", toolchain, "rustc"), env)
    if not cargo or not rustc:
        raise RuntimeError(f"rustup could not resolve cargo/rustc for pinned Rust {toolchain}")
    proxy_bin = str(Path(rustup).parent)
    extra_bins = [proxy_bin]
    if env.get("USERPROFILE"):
        extra_bins.append(str(Path(env["USERPROFILE"]) / ".cargo" / "bin"))
    env["PATH"] = os.pathsep.join(extra_bins + [env.get("PATH", "")])
    env["GHOST_TALK_CARGO_BIN"] = proxy_bin
    env["GHOST_TALK_CARGO_EXE"] = cargo
    env["GHOST_TALK_RUSTC_EXE"] = rustc
    return rustup, cargo, rustc
