"""Windows quality-runner primitives: locking, subprocess execution, and WASM setup."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass

from windows.rustup_bootstrap import ensure_pinned_toolchain

WASM_TARGET = "wasm32-unknown-unknown"
CARGO_LLVM_COV_VERSION = "0.8.7"
WASM_PACK_VERSION = "0.14.0"


@dataclass(frozen=True)
class Gate:
    display: str
    argv: tuple[str, ...]
    cwd: Path


class CheckoutLock:
    """Hold a per-checkout, cross-process single-flight lock for the entire QA run."""

    def __init__(self, checkout: Path) -> None:
        digest = hashlib.sha256(str(checkout.resolve()).lower().encode()).hexdigest()[:20]
        self.path = Path(tempfile.gettempdir()) / f"ghost-talk-quality-gates-{digest}.lock"
        self.handle = None

    def __enter__(self) -> "CheckoutLock":
        self.handle = self.path.open("a+b")
        self.handle.seek(0)
        if self.path.stat().st_size == 0:
            self.handle.write(b"0")
            self.handle.flush()
        try:
            if os.name == "nt":
                import msvcrt
                self.handle.seek(0)
                msvcrt.locking(self.handle.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as exc:
            self.handle.close()
            self.handle = None
            raise RuntimeError(
                "another Ghost Talk quality-gate run is already active for this checkout"
            ) from exc
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        if self.handle is None:
            return
        try:
            if os.name == "nt":
                import msvcrt
                self.handle.seek(0)
                msvcrt.locking(self.handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                fcntl.flock(self.handle.fileno(), fcntl.LOCK_UN)
        finally:
            self.handle.close()
            try:
                self.path.unlink()
            except OSError:
                pass


def pinned_toolchain(repo_root: Path) -> str:
    source = (repo_root / "rust-toolchain.toml").read_text(encoding="utf-8")
    match = re.search(r'^\s*channel\s*=\s*"([^"]+)"', source, re.MULTILINE)
    if not match:
        raise RuntimeError("rust-toolchain.toml does not declare a toolchain channel")
    return match.group(1)


def require_tool(name: str, env: dict[str, str] | None = None) -> bool:
    path = None if env is None else env.get("PATH")
    if shutil.which(name, path=path):
        return True
    print(f"ERROR: required executable is not on PATH: {name}", file=sys.stderr, flush=True)
    return False


def _runnable(path: str | None, *args: str, env: dict[str, str] | None = None) -> bool:
    if not path:
        return False
    try:
        result = subprocess.run(
            (path, *args),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            shell=False,
            check=False,
            env=env,
        )
    except (FileNotFoundError, OSError):
        return False
    return result.returncode == 0


def prepare_rust_tools(toolchain: str, env: dict[str, str]) -> bool:
    """Prepare the repository-pinned rustup toolchain without modifying system Rust."""
    try:
        rustup, cargo, rustc = ensure_pinned_toolchain(toolchain, env)
    except (RuntimeError, OSError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr, flush=True)
        return False
    print(f"Rust toolchain: {toolchain}", flush=True)
    print(f"Rust cargo: {cargo}", flush=True)
    print(f"Rust rustc: {rustc}", flush=True)
    print(f"Rust rustup: {rustup}", flush=True)
    return True



def _cargo_llvm_cov_version(cargo: str, env: dict[str, str]) -> str | None:
    try:
        result = subprocess.run(
            (cargo, "llvm-cov", "--version"),
            text=True,
            capture_output=True,
            env=env,
            shell=False,
            check=False,
        )
    except (FileNotFoundError, OSError):
        return None
    if result.returncode != 0:
        return None
    return result.stdout.strip()


def ensure_cargo_llvm_cov(app_root: Path, env: dict[str, str]) -> int:
    """Provide the pinned coverage subcommand without modifying the user's Cargo home."""
    local = env.get("LOCALAPPDATA") or env.get("USERPROFILE")
    if not local:
        print("ERROR: LOCALAPPDATA/USERPROFILE is unavailable for QA tool setup.", file=sys.stderr)
        return 127
    tools_root = Path(local) / "GhostTalk" / "qa-tools"
    tools_bin = tools_root / "bin"
    env["PATH"] = str(tools_bin) + os.pathsep + env.get("PATH", "")
    cargo = env.get("GHOST_TALK_CARGO_EXE") or shutil.which("cargo", path=env.get("PATH"))
    if not cargo:
        print("ERROR: cargo is unavailable for cargo-llvm-cov installation.", file=sys.stderr)
        return 127
    version = _cargo_llvm_cov_version(cargo, env)
    if version and CARGO_LLVM_COV_VERSION in version:
        return 0
    print(f"Installing cargo-llvm-cov {CARGO_LLVM_COV_VERSION} for Ghost Talk QA...", flush=True)
    gate = Gate(
        f"cargo install cargo-llvm-cov {CARGO_LLVM_COV_VERSION}",
        (cargo, "install", "cargo-llvm-cov", "--version", CARGO_LLVM_COV_VERSION, "--locked", "--root", str(tools_root)),
        app_root,
    )
    rc = run_gate(gate, env)
    if rc:
        return rc
    version = _cargo_llvm_cov_version(cargo, env)
    if not version or CARGO_LLVM_COV_VERSION not in version:
        print(
            "ERROR: cargo-llvm-cov installation completed but Cargo cannot invoke the pinned subcommand.",
            file=sys.stderr,
        )
        return 127
    return 0


def _direct_tool_version(executable: str | None, env: dict[str, str]) -> str | None:
    if not executable:
        return None
    try:
        result = subprocess.run(
            (executable, "--version"),
            text=True,
            capture_output=True,
            env=env,
            shell=False,
            check=False,
        )
    except (FileNotFoundError, OSError):
        return None
    return result.stdout.strip() if result.returncode == 0 else None


def ensure_wasm_pack(app_root: Path, env: dict[str, str]) -> int:
    """Provide the pinned wasm-pack binary inside Ghost Talk's isolated QA tools."""
    local = env.get("LOCALAPPDATA") or env.get("USERPROFILE")
    if not local:
        print("ERROR: LOCALAPPDATA/USERPROFILE is unavailable for QA tool setup.", file=sys.stderr)
        return 127
    tools_root = Path(local) / "GhostTalk" / "qa-tools"
    tools_bin = tools_root / "bin"
    env["PATH"] = str(tools_bin) + os.pathsep + env.get("PATH", "")
    executable = shutil.which("wasm-pack", path=env.get("PATH"))
    version = _direct_tool_version(executable, env)
    if version and WASM_PACK_VERSION in version:
        env["GHOST_TALK_WASM_PACK_EXE"] = executable
        return 0
    cargo = env.get("GHOST_TALK_CARGO_EXE") or shutil.which("cargo", path=env.get("PATH"))
    if not cargo:
        print("ERROR: cargo is unavailable for wasm-pack installation.", file=sys.stderr)
        return 127
    print(f"Installing wasm-pack {WASM_PACK_VERSION} for Ghost Talk QA...", flush=True)
    gate = Gate(
        f"cargo install wasm-pack {WASM_PACK_VERSION}",
        (cargo, "install", "wasm-pack", "--version", WASM_PACK_VERSION, "--locked", "--root", str(tools_root)),
        app_root,
    )
    rc = run_gate(gate, env)
    if rc:
        return rc
    executable = str(tools_bin / ("wasm-pack.exe" if os.name == "nt" else "wasm-pack"))
    version = _direct_tool_version(executable, env)
    if not version or WASM_PACK_VERSION not in version:
        print("ERROR: wasm-pack installation completed but the pinned executable is unavailable.", file=sys.stderr)
        return 127
    env["GHOST_TALK_WASM_PACK_EXE"] = executable
    return 0

def run_gate(gate: Gate, env: dict[str, str]) -> int:
    print(f"\n==> {gate.display}", flush=True)
    try:
        result = subprocess.run(gate.argv, cwd=gate.cwd, env=env, shell=False, check=False)
    except FileNotFoundError:
        print(f"ERROR: executable not found: {gate.argv[0]}", file=sys.stderr, flush=True)
        return 127
    if result.returncode:
        print(
            f"ERROR: gate failed with exit code {result.returncode}: {gate.display}",
            file=sys.stderr,
            flush=True,
        )
    return result.returncode


def _probe_wasm_target(app_root: Path, toolchain: str, env: dict[str, str]) -> bool:
    rustup = env.get("GHOST_TALK_RUSTUP_EXE")
    rustc = env.get("GHOST_TALK_RUSTC_EXE") or shutil.which("rustc", path=env.get("PATH"))
    if rustup:
        prefix = (rustup, "run", toolchain, "rustc")
    elif rustc:
        prefix = (rustc,)
    else:
        return False
    with tempfile.TemporaryDirectory(prefix="ghost-talk-wasm-target-") as tmp:
        source = Path(tmp) / "probe.rs"
        output = Path(tmp) / "probe.rmeta"
        source.write_text("#![no_std]\npub fn ghost_talk_wasm_target_probe() {}\n", encoding="utf-8")
        try:
            result = subprocess.run(
                (*prefix, "--target", WASM_TARGET, "--crate-name", "ghost_talk_wasm_target_probe",
                 "--crate-type", "lib", "--emit", "metadata", str(source), "-o", str(output)),
                cwd=app_root, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                shell=False, check=False,
            )
        except (FileNotFoundError, OSError):
            return False
        return result.returncode == 0 and output.is_file()


def ensure_wasm_target(app_root: Path, toolchain: str, env: dict[str, str]) -> int:
    if _probe_wasm_target(app_root, toolchain, env):
        print(f"WASM target ready: {WASM_TARGET} ({toolchain})", flush=True)
        return 0
    rustup = env.get("GHOST_TALK_RUSTUP_EXE")
    if not rustup:
        print(
            f"ERROR: {WASM_TARGET} is not usable and rustup was not found. "
            f"Install the target with your Rust toolchain manager, then rerun QA.",
            file=sys.stderr, flush=True,
        )
        return 127
    print(f"Installing {WASM_TARGET} for Rust {toolchain}...", flush=True)
    gate = Gate(
        f"rustup target add --toolchain {toolchain} {WASM_TARGET}",
        (rustup, "target", "add", "--toolchain", toolchain, WASM_TARGET),
        app_root,
    )
    rc = run_gate(gate, env)
    if rc:
        return rc
    if _probe_wasm_target(app_root, toolchain, env):
        return 0
    print(
        f"ERROR: Rust {toolchain} still cannot compile for {WASM_TARGET} after target installation.",
        file=sys.stderr, flush=True,
    )
    return 1

