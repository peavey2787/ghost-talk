"""Guard that run-all-tests covers every first-party test/coverage surface and preserves gate ordering."""
from __future__ import annotations

import re
import sys
from pathlib import Path
APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]
APP_RUN_SH = APP_ROOT / "scripts" / "run-all-tests.sh"
APP_RUN_CMD = APP_ROOT / "scripts" / "run-all-tests.cmd"
APP_RUN_WINDOWS = APP_ROOT / "scripts" / "run-all-tests-windows.py"
ROOT_RUN_SH = REPO_ROOT / "scripts" / "run-all-tests.sh"
ROOT_RUN_CMD = REPO_ROOT / "scripts" / "run-all-tests.cmd"
WASM_TARGET_SH = REPO_ROOT / "scripts" / "tooling" / "ensure-wasm-target.sh"
WASM_TARGET_CMD = REPO_ROOT / "scripts" / "tooling" / "ensure-wasm-target.cmd"
ROOT_WASM = REPO_ROOT / "crates" / "ghost-talk-wasm"
APP_WASM = APP_ROOT / "crates" / "ghost-wasm"

errors: list[str] = []


def fail(message: str) -> None:
    errors.append(message)

def text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        fail(f"missing quality runner: {path.relative_to(REPO_ROOT)}")
        return ""

def normalized(source: str) -> str:
    source = source.replace("\\", "/")
    source = re.sub(r"\^\s*\r?\n\s*", " ", source)
    source = re.sub(r"\\\s*\r?\n\s*", " ", source)
    return source


def require(source: str, token: str, label: str) -> None:
    if token not in source:
        fail(f"{label} is missing `{token}`")


def require_command(source: str, prefix: str, flags: tuple[str, ...], label: str) -> None:
    candidates = [line.strip() for line in source.splitlines() if prefix in line]
    if not any(all(flag in line for flag in flags) for line in candidates):
        fail(f"{label} is missing complete command `{prefix}` with flags {', '.join(flags)}")


def require_command_variant(
    source: str,
    prefix: str,
    required: tuple[str, ...],
    forbidden: tuple[str, ...],
    label: str,
) -> None:
    candidates = [line.strip() for line in source.splitlines() if prefix in line]
    if not any(
        all(flag in line for flag in required) and all(flag not in line for flag in forbidden)
        for line in candidates
    ):
        details = ", ".join(required) or "(no extra flags)"
        excluded = ", ".join(forbidden) or "(none)"
        fail(
            f"{label} is missing `{prefix}` variant requiring [{details}] "
            f"and forbidding [{excluded}]"
        )


def nearest_manifest(path: Path) -> Path | None:
    current = path.parent
    while current != current.parent and REPO_ROOT in (current, *current.parents):
        candidate = current / "Cargo.toml"
        if candidate.is_file():
            return candidate.resolve()
        if current == REPO_ROOT:
            break
        current = current.parent
    return None


def first_party_rust_files() -> list[Path]:
    files: list[Path] = []
    for path in REPO_ROOT.rglob("*.rs"):
        if any(part in {"target", ".git", "vendor", "external"} for part in path.parts):
            continue
        if nearest_manifest(path) is not None:
            files.append(path)
    return sorted(files)


def check_manifest_test_enablement(manifests: set[Path]) -> None:
    disabled_re = re.compile(
        r"(?m)^\s*(autotests|autobenches|doctest|test)\s*=\s*false\s*(?:#.*)?$"
    )
    for manifest in sorted(manifests):
        source = manifest.read_text(encoding="utf-8", errors="replace")
        for match in disabled_re.finditer(source):
            key = match.group(1)
            fail(
                f"{manifest.relative_to(REPO_ROOT)} disables `{key}`; the mandatory full-suite "
                "runner may not hide first-party tests/doctests/auto test targets"
            )


def main() -> int:
    root_sh = normalized(text(ROOT_RUN_SH))
    root_cmd = normalized(text(ROOT_RUN_CMD))
    app_sh = normalized(text(APP_RUN_SH))
    app_cmd = normalized(text(APP_RUN_CMD))
    app_cmd_matrix = app_cmd + "\n" + normalized(text(APP_RUN_WINDOWS))
    require(app_cmd, 'scripts/windows/_python-env.cmd', 'application Windows launcher'); require(app_cmd, 'scripts/windows/_rust-env.cmd', 'application Windows launcher')
    py_env = normalized(text(APP_ROOT / "scripts" / "windows" / "_python-env.cmd")); require(py_env, "C:/msys64/mingw64/bin/python3.exe", "Windows Python discovery helper"); require(py_env, "import sys; print(sys.executable)", "Windows Python direct-command probe")
    require(py_env, "%LOCALAPPDATA%/Python/pythoncore-*", "modern Windows Python discovery"); require(py_env, "reg query", "registered Windows Python discovery"); ("where %~1" in py_env.lower()) and fail("Windows Python discovery must execute candidates directly instead of requiring WHERE")
    rust_env = normalized(text(APP_ROOT / "scripts" / "windows" / "_rust-env.cmd")); require(rust_env, "%USERPROFILE%/.cargo/bin", "Windows Rust discovery helper"); require(rust_env, "cargo.exe", "Windows Rust discovery helper"); require(rust_env, "rustup.exe", "Windows optional rustup discovery"); require(app_cmd_matrix, "prepare_rust_tools", "Windows Python quality runner"); require(app_cmd_matrix, "rustup is optional until", "Windows rustup optionality")
    rustup_bootstrap = normalized(text(APP_ROOT / "scripts" / "windows" / "rustup_bootstrap.py")); require(rustup_bootstrap, "static.rust-lang.org/rustup/dist", "isolated Windows rustup bootstrap"); require(rustup_bootstrap, "--default-toolchain", "isolated Windows rustup bootstrap"); require(rustup_bootstrap, "target", "pinned Windows WASM target provisioning")
    quality_support = normalized(text(APP_ROOT / "scripts" / "windows" / "quality_gate_support.py")); require(quality_support, "def _runnable(path: str | None, *args: str, env:", "Windows QA executable probe"); require(quality_support, "(cargo, \"llvm-cov\", \"--version\")", "cargo-llvm-cov Cargo-subcommand probe"); require(quality_support, 'WASM_PACK_VERSION = "0.14.0"', "pinned Windows wasm-pack bootstrap"); require(quality_support, 'GHOST_TALK_WASM_PACK_EXE', "exact Windows wasm-pack path"); require(app_cmd_matrix, "ensure_wasm_pack", "Windows wasm-pack bootstrap"); require(app_cmd_matrix, 'env.get("GHOST_TALK_WASM_PACK_EXE")', "exact Windows wasm-pack invocation")
    if not (app_cmd.find("scripts/windows/_python-env.cmd") < app_cmd.find("scripts/windows/_rust-env.cmd") < app_cmd.find("scripts/windows/_msvc-env.cmd")): fail("application Windows launcher must resolve Python and Rust before MSVC setup can rewrite PATH")
    if re.search(r'(?s)if not errorlevel 1 \([^)]*set "EXIT_CODE=%ERRORLEVEL%"', app_cmd):
        fail("application Windows launcher captures stale %ERRORLEVEL% inside a parenthesized block")
    wasm_target_sh = normalized(text(WASM_TARGET_SH))
    wasm_target_cmd = normalized(text(WASM_TARGET_CMD))
    # Target setup must bind installation and verification to the checkout's
    # resolved toolchain and prove actual compilation, not parse rustup UI text.
    require(wasm_target_sh, "rustup show active-toolchain", "WASM target shell helper"); require(wasm_target_sh, "rustup target add --toolchain", "WASM target shell helper"); require(wasm_target_sh, "rustup run", "WASM target shell helper")
    require(wasm_target_cmd, "show active-toolchain", "WASM target Windows helper"); require(wasm_target_cmd, "target add --toolchain", "WASM target Windows helper"); require(wasm_target_cmd, '"%RUSTUP_EXE%" run', "WASM target Windows helper")
    for source, label in ((wasm_target_sh, "WASM target shell helper"), (wasm_target_cmd, "WASM target Windows helper")):
        require(source, "--crate-type lib", label); require(source, "--emit metadata", label)
        if "target list --installed" in source:
            fail(f"{label} must verify target usability by compiling, not by parsing rustup target-list output")

    # Every first-party Cargo manifest must belong to one of the mandatory
    # workspace/standalone surfaces. A new crate may not silently escape QA.
    manifests = {
        path.resolve()
        for path in REPO_ROOT.rglob("Cargo.toml")
        if not any(part in {"target", "vendor", "external"} for part in path.parts)
    }
    root_members = {path.resolve() for path in (REPO_ROOT / "crates").glob("*/Cargo.toml")}
    app_members = {
        path.resolve()
        for path in (APP_ROOT / "crates").glob("*/Cargo.toml")
        if path.parent.name != "ghost-wasm"
    }
    standalone_wasm = (APP_WASM / "Cargo.toml").resolve()
    covered_manifests = {
        (REPO_ROOT / "Cargo.toml").resolve(),
        (APP_ROOT / "Cargo.toml").resolve(),
        standalone_wasm,
        *root_members,
        *app_members,
    }
    for manifest in sorted(manifests - covered_manifests):
        fail(f"Cargo manifest is not covered by the mandatory run-all matrix: {manifest.relative_to(REPO_ROOT)}")
    for manifest in sorted(covered_manifests - manifests):
        fail(f"mandatory Cargo test surface is missing its manifest: {manifest.relative_to(REPO_ROOT)}")
    check_manifest_test_enablement(manifests)

    rust_files = first_party_rust_files()
    native_tests = 0
    wasm_tests = 0
    integration_files: list[Path] = []
    wasm_test_files: list[Path] = []
    ignored_re = re.compile(
        r"#\s*\[\s*ignore(?:\s*=|\s*\])|#\s*\[\s*cfg_attr\s*\([^]]*\bignore\b[^]]*\)\s*\]",
        re.MULTILINE | re.DOTALL,
    )
    for path in rust_files:
        source = path.read_text(encoding="utf-8", errors="replace")
        if ignored_re.search(source):
            fail(f"ignored Rust test is forbidden in full QA: {path.relative_to(REPO_ROOT)}")
        native = len(re.findall(r"#\s*\[\s*test\s*\]", source))
        wasm = len(re.findall(r"#\s*\[\s*wasm_bindgen_test(?:\([^]]*\))?\s*\]", source))
        native_tests += native
        wasm_tests += wasm
        if wasm:
            wasm_test_files.append(path)
            if ROOT_WASM not in path.parents and APP_WASM not in path.parents:
                fail(
                    f"wasm_bindgen test is outside a mandatory browser-test crate: "
                    f"{path.relative_to(REPO_ROOT)}"
                )
        if "tests" in path.parts and path.parent.name == "tests" and native:
            integration_files.append(path)

    # Root wrapper: SDK workspace, SDK browser adapter, dual feature coverage,
    # then delegation to the full reference-application runner.
    for source, label, arg_guard in (
        (root_sh, "root shell runner", "if (( $# != 0 ))"),
        (root_cmd, "root Windows runner", 'if not "%~1"==""'),
    ):
        require(source, arg_guard, label)
        require(source, "check-test-matrix.py", label)
        require(source, "check-architecture.py", label)
        require(source, "ensure-wasm-target", label)
        require_command_variant(source, "cargo check --workspace", ("--all-targets",), ("--all-features",), label)
        require_command(source, "cargo check --workspace", ("--all-targets", "--all-features"), label)
        if source.find("cargo check --workspace --all-targets") > source.find("ensure-wasm-target"):
            fail(f"{label} must compile native sources before WASM target provisioning")
        require_command_variant(source, "cargo test --workspace", ("--all-targets", "--no-fail-fast"), ("--all-features",), label)
        require_command(source, "cargo test --workspace", ("--all-targets", "--all-features", "--no-fail-fast"), label)
        require_command_variant(source, "cargo test --workspace", ("--doc", "--no-fail-fast"), ("--all-features",), label)
        require_command(source, "cargo test --workspace", ("--doc", "--all-features", "--no-fail-fast"), label)
        require_command_variant(source, "cargo clippy --workspace", ("--all-targets", "-D warnings"), ("--all-features", "--target wasm32-unknown-unknown"), label)
        require_command(source, "cargo clippy --workspace", ("--all-targets", "--all-features", "-D warnings"), label)
        require_command_variant(source, "cargo clippy --workspace", ("--target wasm32-unknown-unknown", "--all-targets", "-D warnings"), ("--all-features",), label)
        require_command(source, "cargo clippy --workspace", ("--target wasm32-unknown-unknown", "--all-targets", "--all-features", "-D warnings"), label)
        require(source, "wasm-pack test --headless", label)
        require(source, "crates/ghost-talk-wasm", label)
        require_command_variant(source, "cargo llvm-cov --workspace", ("--all-targets", "--no-fail-fast", "--lcov"), ("--all-features",), label)
        require_command(source, "cargo llvm-cov --workspace", ("--all-targets", "--all-features", "--no-fail-fast", "--lcov"), label)
        require(source, "root-default.lcov", label)
        require(source, "root-all-features.lcov", label)
        require(source, "GHOST_TALK_ROOT_LCOV_DEFAULT", label)
        require(source, "GHOST_TALK_ROOT_LCOV_ALL_FEATURES", label)
        require(source, "examples/ghost-talk-app/scripts/run-all-tests", label)
        if "cargo test --workspace --lib" in source or "cargo llvm-cov --workspace --lib" in source:
            fail(f"{label} narrows mandatory tests/coverage to --lib")

    # Application wrapper: default/all-feature workspace configs, standalone
    # Yew/WASM workspace, real browser tests, six LCOV reports, one CRAP gate.
    for source, label, arg_guard in (
        (app_sh, "application shell runner", "if (( $# != 0 ))"),
        (app_cmd_matrix, "application Windows runner", 'if not "%~1"==""'),
    ):
        require(source, arg_guard, label)
        require(source, "scripts/check-test-matrix.py", label)
        require(source, "scripts/check-architecture.py", label)
        require(source, "ensure-wasm-target", label)
        require_command_variant(source, "cargo check --workspace", ("--all-targets",), ("--all-features",), label)
        require_command(source, "cargo check --workspace", ("--all-targets", "--all-features"), label)
        require_command(source, "cargo check --manifest-path crates/ghost-wasm/Cargo.toml", ("--all-targets",), label)
        if source.find("cargo check --workspace --all-targets") > source.find("ensure-wasm-target"):
            fail(f"{label} must compile native sources before WASM target provisioning")
        require_command_variant(source, "cargo test --workspace", ("--all-targets", "--no-fail-fast"), ("--all-features",), label)
        require_command(source, "cargo test --workspace", ("--all-targets", "--all-features", "--no-fail-fast"), label)
        require_command_variant(source, "cargo test --workspace", ("--doc", "--no-fail-fast"), ("--all-features",), label)
        require_command(source, "cargo test --workspace", ("--doc", "--all-features", "--no-fail-fast"), label)
        require_command_variant(source, "cargo clippy --workspace", ("--all-targets", "-D warnings"), ("--all-features",), label)
        require_command(source, "cargo clippy --workspace", ("--all-targets", "--all-features", "-D warnings"), label)
        require_command_variant(source, "cargo test --manifest-path crates/ghost-wasm/Cargo.toml", ("--all-targets", "--no-fail-fast"), ("--all-features",), label)
        require_command(source, "cargo test --manifest-path crates/ghost-wasm/Cargo.toml", ("--all-targets", "--all-features", "--no-fail-fast"), label)
        require_command_variant(source, "cargo test --manifest-path crates/ghost-wasm/Cargo.toml", ("--doc", "--no-fail-fast"), ("--all-features",), label)
        require_command(source, "cargo test --manifest-path crates/ghost-wasm/Cargo.toml", ("--doc", "--all-features", "--no-fail-fast"), label)
        require_command_variant(source, "cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml", ("--all-targets", "-D warnings"), ("--all-features", "--target wasm32-unknown-unknown"), label)
        require_command(source, "cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml", ("--all-targets", "--all-features", "-D warnings"), label)
        require_command_variant(source, "cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml", ("--target wasm32-unknown-unknown", "--all-targets", "-D warnings"), ("--all-features",), label)
        require_command(source, "cargo clippy --manifest-path crates/ghost-wasm/Cargo.toml", ("--target wasm32-unknown-unknown", "--all-targets", "--all-features", "-D warnings"), label)
        require(source, "wasm-pack test --headless", label)
        require(source, "crates/ghost-wasm", label)
        require_command_variant(source, "cargo llvm-cov --workspace", ("--all-targets", "--no-fail-fast", "--lcov"), ("--all-features",), label)
        require_command(source, "cargo llvm-cov --workspace", ("--all-targets", "--all-features", "--no-fail-fast", "--lcov"), label)
        require_command_variant(source, "cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml", ("--all-targets", "--no-fail-fast", "--lcov"), ("--all-features",), label)
        require_command(source, "cargo llvm-cov --manifest-path crates/ghost-wasm/Cargo.toml", ("--all-targets", "--all-features", "--no-fail-fast", "--lcov"), label)
        for report in (
            "root-default.lcov",
            "root-all-features.lcov",
            "workspace-default.lcov",
            "workspace-all-features.lcov",
            "ghost-wasm-default.lcov",
            "ghost-wasm-all-features.lcov",
        ):
            require(source, report, label)
        require(source, "scripts/check-crap.py", label)
        require(source, "--max-crap 25", label)
        require(source, "--unmeasured-report", label)
        if "cargo test --workspace --lib" in source or "cargo llvm-cov --workspace --lib" in source:
            fail(f"{label} narrows mandatory tests/coverage to --lib")

    if integration_files:
        for source, label in ((app_sh, "application shell runner"), (app_cmd_matrix, "application Windows runner")):
            require_command(source, "cargo test --workspace", ("--all-targets", "--no-fail-fast"), label)
    if wasm_test_files:
        require(root_sh, "wasm-pack test --headless", "root shell runner")
        require(root_cmd, "wasm-pack test --headless", "root Windows runner")
        require(app_sh, "wasm-pack test --headless", "application shell runner")
        require(app_cmd_matrix, "wasm-pack test --headless", "application Windows runner")

    if errors:
        print("Ghost Talk test-matrix guard: FAIL", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print(
        "PASS: complete test matrix is wired "
        f"({native_tests} native #[test] attributes, {wasm_tests} wasm_bindgen tests, "
        f"{len(integration_files)} integration file(s), {len(wasm_test_files)} wasm test file(s))"
    )
    print("PASS: default + all-feature all-target tests and doctests are mandatory for native workspaces")
    print("PASS: root SDK WASM and application WASM real-browser test surfaces are mandatory")
    print("PASS: root/app/standalone-WASM default + all-feature LCOV all feed one CRAP<=25 gate")
    print(f"PASS: all {len(manifests)} first-party Cargo manifests are covered by a mandatory workspace/standalone test surface")
    print("PASS: ignored/disabled tests, --lib-only narrowing, and external full-suite test filters are forbidden")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
