"""Plain `#[test]` functions inside wasm32-only application modules never run.

Native `cargo test` does not compile those modules and the browser runner
only executes `wasm_bindgen_test`, so such tests are silently dead.
"""
from __future__ import annotations

import re
from functools import cache
from pathlib import Path

WASM_ONLY_MOD = re.compile(r'#\[cfg\(target_arch = "wasm32"\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+);')


@cache
def wasm_only_modules(app_wasm: Path) -> tuple[Path, ...]:
    """Top-level application-WASM modules compiled only for wasm32."""
    src = app_wasm / "src"
    names = WASM_ONLY_MOD.findall((src / "lib.rs").read_text(encoding="utf-8"))
    return tuple(path for name in names for path in (src / name, src / f"{name}.rs"))


def dead_plain_test_failure(path: Path, native_tests: int, app_wasm: Path, repo_root: Path) -> str | None:
    roots = wasm_only_modules(app_wasm)
    if native_tests == 0 or not any(path == root or root in path.parents for root in roots):
        return None
    return f"plain #[test] in a wasm32-only module never runs; use wasm_bindgen_test: {path.relative_to(repo_root)}"
