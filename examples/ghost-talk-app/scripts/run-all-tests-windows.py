"""Sequential, single-flight Windows quality-gate runner for the Ghost Talk app."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys

from windows.quality_gate_support import (
    CheckoutLock,
    Gate,
    WASM_TARGET,
    ensure_cargo_llvm_cov,
    ensure_wasm_pack,
    ensure_wasm_target,
    pinned_toolchain,
    prepare_rust_tools,
    run_gate,
)

APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]


def gate(display: str, *argv: str, cwd: Path = APP_ROOT) -> Gate:
    return Gate(display, tuple(argv), cwd)


def run_many(gates: tuple[Gate, ...], env: dict[str, str]) -> int:
    for item in gates:
        rc = run_gate(item, env)
        if rc:
            return rc
    return 0


def run() -> int:
    toolchain = pinned_toolchain(REPO_ROOT)
    env = os.environ.copy()
    env["RUSTUP_TOOLCHAIN"] = toolchain
    env["CARGO_INCREMENTAL"] = "0"
    browser = env.get("GHOST_TALK_WASM_BROWSER", "firefox").lower()
    if browser not in {"firefox", "chrome"}:
        print("ERROR: GHOST_TALK_WASM_BROWSER must be firefox or chrome.", file=sys.stderr)
        return 2
    # Cargo is required now; rustup is optional until a missing WASM target must be installed.
    if not prepare_rust_tools(toolchain, env):
        return 127

    native = (
        gate("python scripts/check-test-matrix.py", sys.executable, "scripts/check-test-matrix.py"),
        gate("python scripts/check-architecture.py", sys.executable, "scripts/check-architecture.py"),
        gate("cargo fmt --all -- --check", "cargo", "fmt", "--all", "--", "--check"),
        gate("cargo fmt --manifest-path crates/ghost-wasm/Cargo.toml -- --check", "cargo", "fmt", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--", "--check"),
        gate("cargo check --workspace --all-targets", "cargo", "check", "--workspace", "--all-targets"),
        gate("cargo check --workspace --all-targets --all-features", "cargo", "check", "--workspace", "--all-targets", "--all-features"),
        gate("cargo check --manifest-path crates/ghost-wasm/Cargo.toml --all-targets", "cargo", "check", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets"),
        gate("cargo check --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features", "cargo", "check", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets", "--all-features"),
    )
    rc = run_many(native, env)
    if rc:
        return rc

    # ensure-wasm-target: the Windows runner performs provisioning in-process so
    # there is no nested batch control flow and no opportunity for overlapping gates.
    rc = ensure_wasm_target(APP_ROOT, toolchain, env)
    if rc:
        return rc
    rc = ensure_wasm_clang(env)
    if rc:
        return rc
    rc = run_gate(
        gate("cargo check --manifest-path crates/ghost-wasm/Cargo.toml --target wasm32-unknown-unknown --lib", "cargo", "check", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--target", WASM_TARGET, "--lib"),
        env,
    )
    if rc:
        return rc
    rc = ensure_cargo_llvm_cov(APP_ROOT, env)
    if rc:
        return rc
    rc = run_gate(gate("cargo llvm-cov --version", "cargo", "llvm-cov", "--version"), env)
    if rc:
        return rc
    rc = ensure_wasm_pack(APP_ROOT, env)
    if rc:
        return rc
    wasm_pack = env.get("GHOST_TALK_WASM_PACK_EXE")
    if not wasm_pack:
        print("ERROR: pinned wasm-pack executable was not resolved.", file=sys.stderr)
        return 127

    tests = (
        gate("cargo clippy --workspace --all-targets -- -D warnings", "cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"),
        gate("cargo clippy --workspace --all-targets --all-features -- -D warnings", "cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"),
        gate("cargo test --workspace --all-targets --no-fail-fast", "cargo", "test", "--workspace", "--all-targets", "--no-fail-fast"),
        gate("cargo test --workspace --all-targets --all-features --no-fail-fast", "cargo", "test", "--workspace", "--all-targets", "--all-features", "--no-fail-fast"),
        gate("cargo test --workspace --doc --no-fail-fast", "cargo", "test", "--workspace", "--doc", "--no-fail-fast"),
        gate("cargo test --workspace --all-features --doc --no-fail-fast", "cargo", "test", "--workspace", "--all-features", "--doc", "--no-fail-fast"),
        gate("cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --all-targets -- -D warnings", "cargo", "clippy", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets", "--", "-D", "warnings"),
        gate("cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features -- -D warnings", "cargo", "clippy", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets", "--all-features", "--", "-D", "warnings"),
        gate("cargo test --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --no-fail-fast", "cargo", "test", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets", "--no-fail-fast"),
        gate("cargo test --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features --no-fail-fast", "cargo", "test", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets", "--all-features", "--no-fail-fast"),
        gate("cargo test --manifest-path crates/ghost-wasm/Cargo.toml --doc --no-fail-fast", "cargo", "test", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--doc", "--no-fail-fast"),
        gate("cargo test --manifest-path crates/ghost-wasm/Cargo.toml --all-features --doc --no-fail-fast", "cargo", "test", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-features", "--doc", "--no-fail-fast"),
        gate("cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings", "cargo", "clippy", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--target", WASM_TARGET, "--all-targets", "--", "-D", "warnings"),
        gate("cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets --all-features -- -D warnings", "cargo", "clippy", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--target", WASM_TARGET, "--all-targets", "--all-features", "--", "-D", "warnings"),
        gate("cargo clippy --manifest-path e2e/harness/Cargo.toml --all-targets -- -D warnings", "cargo", "clippy", "--manifest-path", "e2e/harness/Cargo.toml", "--all-targets", "--", "-D", "warnings"),
        gate("cargo test --manifest-path e2e/harness/Cargo.toml --all-targets --no-fail-fast", "cargo", "test", "--manifest-path", "e2e/harness/Cargo.toml", "--all-targets", "--no-fail-fast"),
    )
    rc = run_many(tests, env)
    if rc:
        return rc
    rc = run_gate(gate(f"wasm-pack test --headless --{browser} crates/ghost-wasm", wasm_pack, "test", "--headless", f"--{browser}", "crates/ghost-wasm"), env)
    if rc:
        return rc
    return run_coverage(env)


def ensure_wasm_clang(env: dict[str, str]) -> int:
    """secp256k1-sys/ring compile C for wasm32; bind the shared clang bootstrap."""
    if env.get("CC_wasm32_unknown_unknown"):
        return 0
    script = REPO_ROOT / "scripts" / "tooling" / "ensure-wasm-clang.ps1"
    result = subprocess.run(
        ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(script)],
        capture_output=True, text=True, check=False,
    )
    lines = [line for line in result.stdout.splitlines() if "|" in line]
    if result.returncode or not lines:
        print(f"ERROR: WebAssembly clang bootstrap failed: {result.stderr.strip()}", file=sys.stderr)
        return result.returncode or 1
    env["CC_wasm32_unknown_unknown"], env["AR_wasm32_unknown_unknown"] = lines[-1].split("|", 1)
    return 0


def run_coverage(env: dict[str, str]) -> int:
    app_cov = APP_ROOT / "target" / "coverage"
    root_cov = REPO_ROOT / "target" / "coverage"
    app_cov.mkdir(parents=True, exist_ok=True)
    root_cov.mkdir(parents=True, exist_ok=True)
    root_default = Path(env.get("GHOST_TALK_ROOT_LCOV_DEFAULT", root_cov / "root-default.lcov"))
    root_all = Path(env.get("GHOST_TALK_ROOT_LCOV_ALL_FEATURES", root_cov / "root-all-features.lcov"))
    if "GHOST_TALK_ROOT_LCOV_DEFAULT" not in env:
        rc = run_gate(gate("cargo llvm-cov --workspace --all-targets --no-fail-fast --lcov --output-path root-default.lcov", "cargo", "llvm-cov", "--manifest-path", str(REPO_ROOT / "Cargo.toml"), "--workspace", "--all-targets", "--no-fail-fast", "--lcov", "--output-path", str(root_default), cwd=REPO_ROOT), env)
        if rc:
            return rc
    if "GHOST_TALK_ROOT_LCOV_ALL_FEATURES" not in env:
        rc = run_gate(gate("cargo llvm-cov --workspace --all-targets --all-features --no-fail-fast --lcov --output-path root-all-features.lcov", "cargo", "llvm-cov", "--manifest-path", str(REPO_ROOT / "Cargo.toml"), "--workspace", "--all-targets", "--all-features", "--no-fail-fast", "--lcov", "--output-path", str(root_all), cwd=REPO_ROOT), env)
        if rc:
            return rc
    app_default = app_cov / "workspace-default.lcov"
    app_all = app_cov / "workspace-all-features.lcov"
    wasm_default = app_cov / "ghost-wasm-default.lcov"
    wasm_all = app_cov / "ghost-wasm-all-features.lcov"
    coverage = (
        gate("cargo llvm-cov --workspace --all-targets --no-fail-fast --lcov --output-path workspace-default.lcov", "cargo", "llvm-cov", "--workspace", "--all-targets", "--no-fail-fast", "--lcov", "--output-path", str(app_default)),
        gate("cargo llvm-cov --workspace --all-targets --all-features --no-fail-fast --lcov --output-path workspace-all-features.lcov", "cargo", "llvm-cov", "--workspace", "--all-targets", "--all-features", "--no-fail-fast", "--lcov", "--output-path", str(app_all)),
        gate("cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --no-fail-fast --lcov --output-path ghost-wasm-default.lcov", "cargo", "llvm-cov", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets", "--no-fail-fast", "--lcov", "--output-path", str(wasm_default)),
        gate("cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml --all-targets --all-features --no-fail-fast --lcov --output-path ghost-wasm-all-features.lcov", "cargo", "llvm-cov", "--manifest-path", "crates/ghost-wasm/Cargo.toml", "--all-targets", "--all-features", "--no-fail-fast", "--lcov", "--output-path", str(wasm_all)),
    )
    rc = run_many(coverage, env)
    if rc:
        return rc
    reports = (root_default, root_all, app_default, app_all, wasm_default, wasm_all)
    for report in reports:
        if not report.is_file() or report.stat().st_size == 0:
            print(f"ERROR: required LCOV report is missing or empty: {report}", file=sys.stderr)
            return 1
    return run_gate(gate("python scripts/check-crap.py --max-crap 25 --unmeasured-report", sys.executable, "scripts/check-crap.py", *(str(path) for path in reports), "--max-crap", "25", "--report", str(app_cov / "crap-report.tsv"), "--unmeasured-report", str(app_cov / "crap-unmeasured.tsv")), env)


def main() -> int:
    print("Ghost Talk reference app - COMPLETE quality gates", flush=True)
    try:
        with CheckoutLock(REPO_ROOT):
            return run()
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr, flush=True)
        return 75


if __name__ == "__main__":
    raise SystemExit(main())
