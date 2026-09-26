#!/usr/bin/env python3
"""Calculate per-function CRAP from combined repository LCOV reports.

CRAP(m) = CC(m)^2 * (1 - coverage(m))^3 + CC(m)

Every first-party production function is written to the report as measured or
unmeasured. Ownership/security-critical host-executable functions must have LCOV
measurement; target-specific browser adapters remain explicitly unmeasured and
are validated by the mandatory wasm-bindgen browser test gate instead.
"""
from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from pathlib import Path

APP_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = APP_ROOT.parents[1]
ARCH = APP_ROOT / "scripts" / "check-architecture.py"


def load_architecture_helpers():
    spec = importlib.util.spec_from_file_location("ghost_architecture", ARCH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    scripts_dir = str(ARCH.parent)
    inserted = scripts_dir not in sys.path
    if inserted:
        sys.path.insert(0, scripts_dir)
    try:
        spec.loader.exec_module(module)
    finally:
        if inserted:
            sys.path.remove(scripts_dir)
    return module


def canonical_source(raw: str) -> Path:
    path = Path(raw)
    if path.is_absolute():
        return path.resolve()
    candidates = (REPO_ROOT / path, APP_ROOT / path)
    for candidate in candidates:
        if candidate.exists():
            return candidate.resolve()
    return (APP_ROOT / path).resolve()


def display_path(path: Path) -> str:
    resolved = path.resolve()
    try:
        return str(resolved.relative_to(REPO_ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return str(resolved)


def parse_lcov(paths: list[Path]) -> dict[Path, dict[int, int]]:
    coverage: dict[Path, dict[int, int]] = {}
    for report in paths:
        if not report.exists():
            raise FileNotFoundError(f"LCOV report does not exist: {report}")
        if report.stat().st_size == 0:
            raise ValueError(f"LCOV report is empty: {report}")
        current: Path | None = None
        for raw_line in report.read_text(encoding="utf-8", errors="replace").splitlines():
            if raw_line.startswith("SF:"):
                current = canonical_source(raw_line[3:])
                coverage.setdefault(current, {})
                continue
            if current is None or not raw_line.startswith("DA:"):
                continue
            fields = raw_line[3:].split(",")
            if len(fields) < 2:
                continue
            try:
                line = int(fields[0])
                hits = int(fields[1])
            except ValueError:
                continue
            coverage[current][line] = max(coverage[current].get(line, 0), hits)
    return coverage


def crap(cc: int, fraction: float) -> float:
    return (cc * cc) * ((1.0 - fraction) ** 3) + cc


CRITICAL_PREFIXES = tuple(
    (APP_ROOT / "crates" / crate / "src").resolve()
    for crate in ("ghost-domain", "ghost-chat", "ghost-contacts", "ghost-rooms", "ghost-protocol")
)
CRITICAL_FILES = {
    (APP_ROOT / "crates/ghost-runtime/src/mailbox.rs").resolve(),
}
CRITICAL_FUNCTIONS = {
    ((APP_ROOT / "crates/ghost-runtime/src/wallet.rs").resolve(), "merge_progress"),
    ((APP_ROOT / "crates/ghost-runtime/src/wallet.rs").resolve(), "reconcile"),
    ((APP_ROOT / "crates/ghost-kaspa/src/wallet/consolidation.rs").resolve(), "consolidation_replan_fee"),
    ((APP_ROOT / "crates/ghost-talk-native/src/persistence/profile_merge.rs").resolve(), "merge_profile_patch"),
}


def is_critical(path: Path, name: str) -> bool:
    resolved = path.resolve()
    if resolved in CRITICAL_FILES or (resolved, name) in CRITICAL_FUNCTIONS:
        return True
    return any(resolved == prefix or prefix in resolved.parents for prefix in CRITICAL_PREFIXES)



def validate_critical_targets(arch) -> None:
    missing: list[str] = []
    for prefix in CRITICAL_PREFIXES:
        if not prefix.is_dir():
            missing.append(f"critical source prefix is missing: {display_path(prefix)}")
    for path in CRITICAL_FILES:
        if not path.is_file():
            missing.append(f"critical source file is missing: {display_path(path)}")
    for path, function in CRITICAL_FUNCTIONS:
        if not path.is_file():
            missing.append(f"critical function source is missing: {display_path(path)}::{function}")
            continue
        source = path.read_text(encoding="utf-8", errors="replace")
        names = {name for name, _start, _end, _body in arch.function_bodies(source)}
        if function not in names:
            missing.append(f"critical function rule is stale: {display_path(path)}::{function}")
    if missing:
        raise ValueError("; ".join(missing))


def test_only_line_ranges(source: str, arch) -> list[tuple[int, int]]:
    """Return line ranges compiled only for tests inside a production source file."""
    clean = arch.sanitize_rust(source)
    attr_mod = re.compile(
        r"#\s*\[\s*cfg\s*\([^]]*\btest\b[^]]*\)\s*\]\s*mod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{",
        re.MULTILINE | re.DOTALL,
    )
    ranges: list[tuple[int, int]] = []
    for match in attr_mod.finditer(clean):
        brace = clean.find("{", match.start(), match.end())
        if brace < 0:
            continue
        depth = 0
        end = brace
        for end in range(brace, len(clean)):
            if clean[end] == "{":
                depth += 1
            elif clean[end] == "}":
                depth -= 1
                if depth == 0:
                    break
        if depth == 0:
            start_line = source.count("\n", 0, match.start()) + 1
            end_line = source.count("\n", 0, end) + 1
            ranges.append((start_line, end_line))
    return ranges


def is_test_only_function(source_path: Path, source: str, start_line: int, ranges: list[tuple[int, int]]) -> bool:
    if source_path.stem.endswith(("test", "tests")) or "tests" in source_path.parts:
        return True
    if any(start <= start_line <= end for start, end in ranges):
        return True
    lines = source.splitlines()
    index = max(0, start_line - 2)
    prefix = "\n".join(lines[max(0, index - 6): index + 1])
    return bool(
        re.search(r"#\s*\[\s*(?:test|wasm_bindgen_test(?:\([^]]*\))?)\s*\]", prefix)
        or re.search(r"#\s*\[\s*cfg\s*\([^]]*\btest\b[^]]*\)\s*\]", prefix)
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("lcov", nargs="+", type=Path)
    parser.add_argument("--max-crap", type=float, default=25.0)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--unmeasured-report", type=Path)
    args = parser.parse_args()

    arch = load_architecture_helpers()
    try:
        validate_critical_targets(arch)
    except ValueError as error:
        print(f"CRAP gate: FAIL - {error}")
        return 1
    line_coverage = parse_lcov(args.lcov)
    measured = []
    unmeasured = []
    critical_unmeasured = []
    violations = []

    for source_path in arch.production_rust_files():
        resolved = source_path.resolve()
        source = source_path.read_text(encoding="utf-8", errors="replace")
        covered_lines = line_coverage.get(resolved, {})
        test_ranges = test_only_line_ranges(source, arch)
        for name, start, end, body in arch.function_bodies(source):
            if is_test_only_function(source_path, source, start, test_ranges):
                continue
            cc = arch.structural_complexity(body)
            executable = [line for line in covered_lines if start <= line <= end]
            if not executable:
                row = (source_path, start, name, cc)
                unmeasured.append(row)
                if is_critical(source_path, name):
                    critical_unmeasured.append(row)
                continue
            covered = sum(1 for line in executable if covered_lines[line] > 0)
            fraction = covered / len(executable)
            score = crap(cc, fraction)
            row = (source_path, start, name, cc, fraction, score)
            measured.append(row)
            if score > args.max_crap:
                violations.append(row)

    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        with args.report.open("w", encoding="utf-8") as handle:
            handle.write("status\tfile\tline\tfunction\tcc\tcoverage\tcrap\n")
            measured_keys = {(path.resolve(), line, name) for path, line, name, *_ in measured}
            violation_keys = {(path.resolve(), line, name) for path, line, name, *_ in violations}
            for path, line, name, cc, fraction, score in sorted(measured, key=lambda row: (display_path(row[0]), row[1], row[2])):
                key = (path.resolve(), line, name)
                status = "violation" if key in violation_keys else "measured"
                handle.write(f"{status}\t{display_path(path)}\t{line}\t{name}\t{cc}\t{fraction:.4f}\t{score:.2f}\n")
            critical_keys = {(path.resolve(), line, name) for path, line, name, _cc in critical_unmeasured}
            for path, line, name, cc in sorted(unmeasured, key=lambda row: (display_path(row[0]), row[1], row[2])):
                key = (path.resolve(), line, name)
                status = "critical-unmeasured" if key in critical_keys else "unmeasured"
                if key not in measured_keys:
                    handle.write(f"{status}\t{display_path(path)}\t{line}\t{name}\t{cc}\t\t\n")

    if args.unmeasured_report:
        args.unmeasured_report.parent.mkdir(parents=True, exist_ok=True)
        critical_keys = {(path.resolve(), line, name) for path, line, name, _cc in critical_unmeasured}
        with args.unmeasured_report.open("w", encoding="utf-8") as handle:
            handle.write("status\tfile\tline\tfunction\tcc\n")
            for path, line, name, cc in sorted(unmeasured, key=lambda row: (display_path(row[0]), row[1], row[2])):
                key = (path.resolve(), line, name)
                status = "critical-unmeasured" if key in critical_keys else "unmeasured"
                handle.write(f"{status}\t{display_path(path)}\t{line}\t{name}\t{cc}\n")

    if not measured:
        print("CRAP gate: FAIL - combined LCOV contained no executable lines for repository Rust functions")
        return 1

    if critical_unmeasured:
        print(f"CRAP gate: FAIL - {len(critical_unmeasured)} critical functions have no executable LCOV measurement")
        for path, line, name, cc in critical_unmeasured[:100]:
            print(f"  {display_path(path)}:{line} {name}: CC={cc} unmeasured")
        return 1

    if violations:
        print(f"CRAP gate: FAIL - {len(violations)} measured functions exceed {args.max_crap:.1f}")
        for path, line, name, cc, fraction, score in sorted(violations, key=lambda row: row[5], reverse=True)[:100]:
            print(f"  {display_path(path)}:{line} {name}: CC={cc} coverage={fraction:.1%} CRAP={score:.2f}")
        return 1

    worst = max(score for *_prefix, score in measured)
    print(
        f"PASS: measured CRAP <= {args.max_crap:.1f} "
        f"({len(measured)} functions measured, {len(unmeasured)} explicitly unmeasured, worst={worst:.2f})"
    )
    if args.unmeasured_report:
        print(f"PASS: unmeasured-function audit written to {args.unmeasured_report}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
