"""Shared paths, parser helpers, thresholds, and failure collection."""
from __future__ import annotations

import re
from pathlib import Path

APP_ROOT = Path(__file__).resolve().parents[2]
REPO_ROOT = Path(__file__).resolve().parents[4]
APP_CRATES = APP_ROOT / "crates"
SDK_CRATES = REPO_ROOT / "crates"
WASM = APP_CRATES / "ghost-wasm" / "src"
NATIVE = APP_CRATES / "ghost-talk-native" / "src"
HYDRA = APP_CRATES / "ghost-hydra" / "src"
DOMAIN = APP_CRATES / "ghost-domain" / "src"
MAX_LINES = 350
NORMAL_FILE_LINES = 300
MAX_FUNCTION_LINES = 75
MAX_COMPLEXITY = 8
NORMAL_FUNCTION_LINES = 50
NORMAL_COMPLEXITY = 6
MAX_RS_FILES_PER_DIRECTORY = 12
MAX_COMPLEXITY_REVIEW_ENTRIES = 32

errors: list[str] = []
RAW_STRING_START = re.compile(r'(?:br|r)(?P<hashes>#{0,16})"')

def fail(message: str) -> None:
    errors.append(message)


def rel(path: Path) -> str:
    return str(path.relative_to(REPO_ROOT)).replace("\\", "/")


def text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        fail(f"missing required file: {rel(path)}")
        return ""


def production_rust_files() -> list[Path]:
    return sorted({*APP_CRATES.glob("*/src/**/*.rs"), *SDK_CRATES.glob("*/src/**/*.rs")})


def sanitize_rust(source: str) -> str:
    """Blank comments and quoted literals while preserving byte/newline positions."""
    out = list(source)
    i = 0
    n = len(source)
    block_depth = 0
    while i < n:
        if block_depth:
            if source.startswith("/*", i):
                out[i:i + 2] = "  "
                block_depth += 1
                i += 2
            elif source.startswith("*/", i):
                out[i:i + 2] = "  "
                block_depth -= 1
                i += 2
            else:
                if source[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        if source.startswith("//", i):
            j = source.find("\n", i)
            if j < 0:
                j = n
            for k in range(i, j):
                out[k] = " "
            i = j
            continue
        if source.startswith("/*", i):
            out[i:i + 2] = "  "
            block_depth = 1
            i += 2
            continue
        raw = RAW_STRING_START.match(source, i)
        if raw:
            hashes = raw.group("hashes")
            opening = raw.group(0)
            end_marker = '"' + hashes
            j = source.find(end_marker, i + len(opening))
            if j < 0:
                j = n - len(end_marker)
            stop = min(n, j + len(end_marker))
            for k in range(i, stop):
                if source[k] != "\n":
                    out[k] = " "
            i = stop
            continue
        if source.startswith("b'", i):
            out[i:i + 2] = "  "
            i += 2
            escaped = False
            while i < n:
                ch = source[i]
                if ch != "\n":
                    out[i] = " "
                if escaped:
                    escaped = False
                elif ch == "\\":
                    escaped = True
                elif ch == "'":
                    i += 1
                    break
                i += 1
            continue
        if source[i] in ('"', "'"):
            quote = source[i]
            # Apostrophe + identifier is a Rust lifetime, not a character literal.
            if (
                quote == "'"
                and i + 1 < n
                and (source[i + 1].isalpha() or source[i + 1] == "_")
                and not (i + 2 < n and source[i + 2] == "'")
            ):
                i += 1
                continue
            out[i] = " "
            i += 1
            escaped = False
            while i < n:
                ch = source[i]
                if ch != "\n":
                    out[i] = " "
                if escaped:
                    escaped = False
                elif ch == "\\":
                    escaped = True
                elif ch == quote:
                    i += 1
                    break
                i += 1
            continue
        i += 1
    return "".join(out)


def function_bodies(source: str):
    clean = sanitize_rust(source)
    fn_re = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^{};]*>)?\s*\(")
    for match in fn_re.finditer(clean):
        brace = clean.find("{", match.end())
        semi = clean.find(";", match.end())
        if brace < 0 or (semi >= 0 and semi < brace):
            continue
        depth = 0
        end = brace
        for end in range(brace, len(clean)):
            ch = clean[end]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    break
        if depth != 0:
            continue
        start_line = source.count("\n", 0, match.start()) + 1
        end_line = source.count("\n", 0, end) + 1
        yield match.group(1), start_line, end_line, clean[brace + 1:end]


def structural_complexity(body: str) -> int:
    # Deterministic McCabe-style estimate. Each explicit branch/loop and match
    # arm contributes one decision. `matches!` and closures intentionally do not.
    branches = len(re.findall(r"\b(?:if|for|while|loop)\b", body))
    arms = len(re.findall(r"=>", body))
    return 1 + branches + arms

