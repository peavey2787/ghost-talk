#!/usr/bin/env python3
"""Run the complete Ghost Talk validation stack.

Ordering is intentional: fast/static checks first; exhaustive first-party and
upstream integration gates later; coverage-guided fuzzing is always last.
Kaspa Portal is a registry dependency, so Ghost Talk validates that integration
by compiling/testing the consumer against kaspa-portal 1.0.1. Ghost Talk does
not carry or run a private copy of Kaspa Portal's own repository QA.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PY = sys.executable


def banner(title: str) -> None:
    print(f"\n{'=' * 72}\n{title}\n{'=' * 72}", flush=True)


def run(args: list[str], cwd: Path = ROOT, env: dict[str, str] | None = None) -> None:
    child = os.environ.copy()
    if env:
        child.update(env)
    print("+", " ".join(str(x) for x in args), flush=True)
    completed = subprocess.run(args, cwd=cwd, env=child, check=False)
    if completed.returncode:
        raise SystemExit(completed.returncode)


def require(tool: str, hint: str = "") -> str:
    found = shutil.which(tool)
    if found:
        return found
    message = f"ERROR: required tool is not on PATH: {tool}"
    if hint:
        message += f"\n{hint}"
    raise SystemExit(message)


def npm_install(app: Path, npm: str) -> None:
    if (app / "package-lock.json").is_file():
        run([npm, "ci", "--include=dev"], cwd=app)
    else:
        run([npm, "install", "--include=dev"], cwd=app)


def first_party_examples() -> list[Path]:
    examples = ROOT / "examples"
    return sorted(
        p / "Cargo.toml"
        for p in examples.iterdir()
        if p.is_dir() and (p / "Cargo.toml").is_file()
    )


SECTIONS = ("static", "rust", "examples", "frontend", "hydra", "hygiene", "fuzz")
SECTION_ALIASES = {str(index): name for index, name in enumerate(SECTIONS, start=1)}


def parse_start_section(value: str) -> str:
    normalized = SECTION_ALIASES.get(value.strip().lower(), value.strip().lower())
    if normalized not in SECTIONS:
        valid = ", ".join(SECTIONS)
        raise argparse.ArgumentTypeError(f"unknown section {value!r}; expected one of: {valid}")
    return normalized


def main() -> int:
    parser = argparse.ArgumentParser(description="Run the complete Ghost Talk validation stack")
    parser.add_argument(
        "--from",
        dest="from_section",
        type=parse_start_section,
        default="static",
        metavar="SECTION",
        help="resume at a main section: static, rust, examples, frontend, hydra, hygiene, or fuzz",
    )
    parser.add_argument(
        "--list-sections",
        action="store_true",
        help="print main section names in execution order and exit",
    )
    args = parser.parse_args()
    if args.list_sections:
        for index, name in enumerate(SECTIONS, start=1):
            print(f"{index}: {name}")
        return 0

    start_index = SECTIONS.index(args.from_section)

    def enabled(section: str) -> bool:
        return SECTIONS.index(section) >= start_index

    if start_index:
        print(
            f"Resuming Ghost Talk validation from section {start_index + 1}/{len(SECTIONS)}: {args.from_section}",
            flush=True,
        )

    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

    if enabled("static"):
        banner("1/7 Ghost Talk static architecture/security QA")
        run([PY, "qa/run-all.py", "--static-only"])

    if enabled("rust"):
        require("cargo", "Install Rust 1.95 with rustup before running the complete suite.")
        require("rustup", "Install rustup before running the complete suite.")
        banner("2/7 Ghost Talk Rust formatting, registry integration, lint and first-party tests")
        run(["cargo", "fmt", "--all", "--", "--check"])
        run(["cargo", "check", "--workspace", "--all-targets", "--all-features"])
        run(["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"])
        run(["cargo", "test", "--workspace", "--all-targets", "--all-features"])
        run(["cargo", "test", "--workspace", "--doc", "--all-features"])
        # These compile the consumer against its real upstream dependency boundaries.
        run(["cargo", "check", "-p", "ghost-hydra", "--features", "upstream"])
        run(["cargo", "check", "-p", "ghost-kaspa", "--features", "upstream"])
        run(["rustup", "target", "add", "wasm32-unknown-unknown"])
        run(["cargo", "check", "-p", "ghost-wasm", "--target", "wasm32-unknown-unknown"])

    if enabled("examples"):
        require("cargo", "Install Rust 1.95 with rustup before running the complete suite.")
        banner("3/7 Every standalone Ghost Talk example")
        for manifest in first_party_examples():
            run(["cargo", "test", "--manifest-path", str(manifest), "--all-targets", "--all-features"])

    if enabled("frontend"):
        require("node", "Install Node.js before running the complete suite.")
        npm = require("npm", "Install npm before running the complete suite.")
        banner("4/7 Ghost Talk frontend compile and every declared frontend test")
        app = ROOT / "crates/ghost-app"
        npm_install(app, npm)
        package = json.loads((app / "package.json").read_text(encoding="utf-8"))
        scripts = package.get("scripts", {})
        if "test" in scripts:
            run([npm, "run", "test"], cwd=app)
        run([npm, "run", "build"], cwd=app)

    hydra = ROOT / "external/hydra-msg"
    if enabled("hydra"):
        banner("5/7 HYDRA complete release validation through mutation")
        run([PY, "qa/ci/run_all.py", "--through", "mutation"], cwd=hydra)

    if enabled("hygiene"):
        require("git", "Install Git before running repository hygiene checks.")
        banner("6/7 Final pre-fuzz repository hygiene")
        run([PY, "qa/run-all.py", "--static-only"])
        run([
            "git",
            "-c",
            "core.whitespace=blank-at-eol,blank-at-eof,space-before-tab,cr-at-eol",
            "diff",
            "--check",
        ])

    if enabled("fuzz"):
        banner("7/7 COVERAGE-GUIDED FUZZING — ALWAYS LAST")
        # HYDRA's own deep profile runs every fast target at 100k iterations and its
        # stateful target at 1k by default. It is deliberately the final command.
        run([PY, "qa/ci/run_all.py", "--only", "fuzz", "--deep-fuzz"], cwd=hydra)

    banner("ALL GHOST TALK + UPSTREAM INTEGRATION TESTS PASSED")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
