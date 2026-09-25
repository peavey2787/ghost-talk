"""Repository shape, module resolution, complexity, and duplication checks."""
from __future__ import annotations

import re
from collections import Counter
from pathlib import Path

from .common import (
    APP_ROOT, REPO_ROOT, APP_CRATES, SDK_CRATES, WASM, NATIVE, HYDRA, DOMAIN,
    MAX_LINES, NORMAL_FILE_LINES, MAX_FUNCTION_LINES, MAX_COMPLEXITY,
    NORMAL_FUNCTION_LINES, NORMAL_COMPLEXITY, MAX_RS_FILES_PER_DIRECTORY,
    MAX_COMPLEXITY_REVIEW_ENTRIES,
    fail, rel, text, sanitize_rust, function_bodies, structural_complexity,
)


def check_repository_shape(files: list[Path]) -> None:
    for path in files:
        source = path.read_text(encoding="utf-8", errors="replace")
        if "include!(" in sanitize_rust(source):
            fail(f"textual include! pseudo-module remains: {rel(path)}")
        cross_root = re.search(r"(?<!super::)super::(?:wallet_commands|hydra_commands|kaspa_gateway|kaspa_wallet_events|peer_commands|mailbox_commands|NativeAppState|run_blocking|storage_root)\b", sanitize_rust(source))
        if cross_root:
            fail(f"{rel(path)} uses depth-sensitive cross-subsystem path `{cross_root.group(0)}`; use explicit `crate::...` ownership")
        if re.match(r"^\d+_.*\.rs$", path.name):
            fail(f"numeric-order source filename remains: {rel(path)}")
        if "_and_" in path.stem:
            fail(f"compound multi-responsibility source filename remains: {rel(path)}")
        suppression_re = re.compile(
            r"\ballow\s*\([^)]*\b(?:dead_code|unused_imports|unused_variables|unreachable_code|clippy::too_many_arguments)\b[^)]*\)"
        )
        for match in suppression_re.finditer(source):
            line = source.count("\n", 0, match.start()) + 1
            fail(
                f"{rel(path)}:{line} suppresses compiler/Clippy debt with "
                f"`{match.group(0)}` instead of removing the underlying code problem"
            )
    directories = list(APP_CRATES.glob("*/src/**")) + list(SDK_CRATES.glob("*/src/**"))
    for directory in directories:
        if not directory.is_dir():
            continue
        count = len(list(directory.glob("*.rs")))
        if count > MAX_RS_FILES_PER_DIRECTORY:
            fail(
                f"{rel(directory)}: {count} Rust files exceeds folder organization limit "
                f"{MAX_RS_FILES_PER_DIRECTORY}"
            )
        if directory.name.endswith("_parts") or directory.name == "lib_parts":
            fail(f"pseudo-module parts directory remains: {rel(directory)}")



def check_quality_script_shape() -> None:
    """Apply the same SRP/naming limits to first-party QA infrastructure."""
    roots = (REPO_ROOT / "scripts", APP_ROOT / "scripts")
    seen: set[Path] = set()
    for root in roots:
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*")):
            if not path.is_file() or path.suffix.lower() not in {".py", ".sh", ".cmd"}:
                continue
            resolved = path.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            source = path.read_text(encoding="utf-8", errors="replace")
            lines = source.count("\n") + (0 if source.endswith("\n") else 1)
            if lines >= NORMAL_FILE_LINES:
                fail(
                    f"{rel(path)}: {lines} lines crosses the strict QA-script SRP target "
                    f"(< {NORMAL_FILE_LINES})"
                )
            if "_and_" in path.stem:
                fail(f"compound multi-responsibility QA script filename remains: {rel(path)}")

def check_real_module_resolution(files: list[Path]) -> None:
    path_mod_re = re.compile(
        r'#\[path\s*=\s*"([^"]+)"\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;'
    )
    mod_re = re.compile(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")
    for path in files:
        raw = path.read_text(encoding="utf-8", errors="replace")
        source = sanitize_rust(raw)
        path_modules = {name for _target, name in path_mod_re.findall(raw)}
        for target, name in path_mod_re.findall(raw):
            fail(f"{rel(path)} uses forbidden #[path] alias `{name}` -> {target}; use the semantic module tree")
        module_base = path.parent if path.name in {"lib.rs", "main.rs", "mod.rs"} else path.parent / path.stem
        for name in mod_re.findall(source):
            if name in path_modules:
                continue
            candidates = (module_base / f"{name}.rs", module_base / name / "mod.rs")
            if not any(candidate.exists() for candidate in candidates):
                fail(f"{rel(path)} declares unresolved module `{name}`")


def check_local_reexport_symbols(files: list[Path]) -> None:
    """Reject stale simple re-exports from local sibling modules.

    Rust/Cargo remains authoritative for name resolution; this narrow guard
    catches the common refactor failure where a module was split/renamed but a
    `pub use sibling::{...}` list still names an item that disappeared.
    """
    grouped_re = re.compile(
        r"pub(?:\([^)]*\))?\s+use\s+(?:self::)?([A-Za-z_][A-Za-z0-9_]*)::\{([^}]+)\}\s*;",
        re.MULTILINE | re.DOTALL,
    )
    single_re = re.compile(
        r"pub(?:\([^)]*\))?\s+use\s+(?:self::)?([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)(?:\s+as\s+[A-Za-z_][A-Za-z0-9_]*)?\s*;"
    )
    for path in files:
        raw = path.read_text(encoding="utf-8", errors="replace")
        module_base = path.parent if path.name in {"lib.rs", "main.rs", "mod.rs"} else path.parent / path.stem

        def target_source(module: str) -> str | None:
            candidates = (module_base / f"{module}.rs", module_base / module / "mod.rs")
            target = next((candidate for candidate in candidates if candidate.exists()), None)
            return None if target is None else target.read_text(encoding="utf-8", errors="replace")

        for match in grouped_re.finditer(raw):
            module, items = match.groups()
            target = target_source(module)
            if target is None:
                continue
            for item in items.split(","):
                item = item.strip()
                if not item or item == "self" or "::{" in item or item == "*":
                    continue
                name = item.split(" as ", 1)[0].strip()
                if "::" in name:
                    name = name.rsplit("::", 1)[-1]
                if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) and not re.search(rf"\b{re.escape(name)}\b", target):
                    line = raw.count("\n", 0, match.start()) + 1
                    fail(f"{rel(path)}:{line} stale local re-export `{module}::{name}`")
        for match in single_re.finditer(raw):
            module, name = match.groups()
            target = target_source(module)
            if target is not None and not re.search(rf"\b{re.escape(name)}\b", target):
                line = raw.count("\n", 0, match.start()) + 1
                fail(f"{rel(path)}:{line} stale local re-export `{module}::{name}`")


def check_exact_function_duplication(files: list[Path]) -> None:
    """Reject nontrivial byte-structure duplication across first-party functions."""
    bodies: dict[str, list[tuple[Path, str, int]]] = {}
    for path in files:
        source = path.read_text(encoding="utf-8", errors="replace")
        for name, start, _end, body in function_bodies(source):
            normalized = re.sub(r"\s+", " ", body).strip()
            # Tiny getters/adapters are clearer inline; only enforce DRY once a
            # copied body is substantial enough to represent real logic.
            if len(normalized) < 70:
                continue
            bodies.setdefault(normalized, []).append((path, name, start))
    for copies in bodies.values():
        if len(copies) < 2:
            continue
        locations = ", ".join(f"{rel(path)}:{line}::{name}" for path, name, line in copies)
        fail(f"nontrivial exact function-body duplication remains: {locations}")


def check_implicit_sibling_imports(files: list[Path]) -> None:
    """Reject wildcard coupling between repository modules.

    External framework preludes remain allowed because they are explicit API
    surfaces. Test modules may use `use super::*` for concise fixture access.
    Production modules must name sibling dependencies so a refactor cannot
    silently change another module's namespace.
    """
    patterns = (
        re.compile(r"(?m)^\s*use\s+super::\*\s*;"),
        re.compile(r"(?m)^\s*use\s+super::[A-Za-z_][A-Za-z0-9_]*::\*\s*;"),
        re.compile(r"(?ms)^\s*use\s+super::\{[^;]*::\*[^;]*\}\s*;"),
        re.compile(r"(?m)^\s*pub(?:\([^)]*\))?\s+use\s+(?:self::)?[A-Za-z_][A-Za-z0-9_]*::\*\s*;"),
    )
    for path in files:
        if path.stem.endswith("test") or path.stem.endswith("tests") or "tests" in path.parts:
            continue
        source = path.read_text(encoding="utf-8", errors="replace")
        clean = sanitize_rust(source)
        test_marker = source.find("#[cfg(test)]")
        for pattern in patterns:
            for match in pattern.finditer(clean):
                if test_marker >= 0 and match.start() > test_marker:
                    continue
                line = source.count("\n", 0, match.start()) + 1
                fail(f"{rel(path)}:{line} production module uses wildcard sibling/re-export coupling")


def check_sizes_and_complexity(files: list[Path]) -> None:
    review_doc = text(APP_ROOT / "docs" / "spec" / "complexity-review.md")
    row_re = re.compile(r"^\| `([^`]+)` \|\s*(\d+)\s*\|\s*(\d+)\s*\|", re.MULTILINE)
    documented = {key: (int(lines), int(cc)) for key, lines, cc in row_re.findall(review_doc)}
    current_reviews: dict[str, tuple[int, int]] = {}
    for path in files:
        source = path.read_text(encoding="utf-8", errors="replace")
        lines = source.count("\n") + (0 if source.endswith("\n") else 1)
        if lines > MAX_LINES:
            fail(f"{rel(path)}: {lines} lines exceeds {MAX_LINES}")
        if lines >= NORMAL_FILE_LINES:
            fail(
                f"{rel(path)}: {lines} lines crosses the strict SRP source target "
                f"(< {NORMAL_FILE_LINES})"
            )
        for name, start, end, body in function_bodies(source):
            length = end - start + 1
            if length > MAX_FUNCTION_LINES:
                fail(f"{rel(path)}:{start} fn {name}: {length} lines exceeds {MAX_FUNCTION_LINES}")
            score = structural_complexity(body)
            if score > MAX_COMPLEXITY:
                fail(f"{rel(path)}:{start} fn {name}: structural complexity {score} exceeds {MAX_COMPLEXITY}")
            if length >= NORMAL_FUNCTION_LINES or score > NORMAL_COMPLEXITY:
                review_key = f"{rel(path)}::{name}"
                current_reviews[review_key] = (length, score)
                if review_key not in documented:
                    fail(
                        f"{rel(path)}:{start} fn {name}: {length} lines / complexity {score} "
                        "crosses the normal SRP review target without a documented review"
                    )
                elif documented[review_key] != (length, score):
                    fail(
                        f"{review_key}: complexity review records {documented[review_key]} but "
                        f"current source measures {(length, score)}"
                    )
    for stale in sorted(set(documented) - set(current_reviews)):
        fail(f"stale complexity-review entry no longer crosses a normal target: {stale}")
    if len(current_reviews) > MAX_COMPLEXITY_REVIEW_ENTRIES:
        fail(
            f"complexity-review exemption budget exceeded: {len(current_reviews)} entries "
            f"> {MAX_COMPLEXITY_REVIEW_ENTRIES}; refactor instead of growing the exception list"
        )


def declared_types(path: Path) -> set[str]:
    source = sanitize_rust(path.read_text(encoding="utf-8", errors="replace"))
    return set(re.findall(r"\b(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)", source))


def check_duplicate_domain_types(files: list[Path]) -> None:
    owners: dict[str, set[str]] = {}
    for path in files:
        crate = next((part for part in path.parts if part.startswith("ghost-")), None)
        if crate is None:
            continue
        for name in declared_types(path):
            owners.setdefault(name, set()).add(crate)
    duplicates = {name: crates for name, crates in owners.items() if len(crates) > 1}
    allowed = {"Error", "Config", "State"}
    for name, crates in sorted(duplicates.items()):
        if name in allowed:
            continue
        fail(f"duplicated domain/API type `{name}` across crates: {', '.join(sorted(crates))}")

