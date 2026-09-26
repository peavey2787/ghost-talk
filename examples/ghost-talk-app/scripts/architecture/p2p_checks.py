"""p2p-net and Kaspa boundary checks: clean imports, one ABI family."""
from __future__ import annotations

import re

from .common import APP_CRATES, APP_ROOT, SDK_CRATES, fail, rel, text

RUSTY_KASPA = re.compile(r"(?m)^\s*(kaspa-wrpc-client|kaspa-rpc-core|kaspa-consensus-core|kaspa-addresses|kaspa-txscript|kaspa-hashes|workflow-core)\s*[=.]")
P2P_NET_DEP = re.compile(r'p2p-net\s*=\s*\{[^}]*rev\s*=\s*"([0-9a-f]{40})"')


def check_p2p_net_boundary() -> None:
    """Ghost Talk imports p2p-net, Kaspa Portal and HYDRA as ordinary crates.

    The browser transport (`ghost-p2p`) drives p2p-net's own `WasmNode` in
    process; there is no separately loaded p2p WebAssembly module and no copy
    of p2p-net code. Kaspa access goes only through Kaspa Portal, so no
    Rusty-Kaspa client crates remain in either graph.
    """
    adapter = text(APP_CRATES / "ghost-p2p" / "src" / "node.rs")
    for token in ("p2p_net::wasm::WasmNode", "ghost_node_config", "storage_namespace"):
        if token not in adapter:
            fail(f"Ghost browser p2p adapter does not start p2p-net in process (`{token}` missing)")
    if (SDK_CRATES / "ghost-p2p-web").exists():
        fail("crates/ghost-p2p-web must not return: p2p-net is linked in process")
    trunk = text(APP_CRATES / "ghost-wasm" / "Trunk.toml") + text(APP_CRATES / "ghost-wasm" / "index.html")
    if "p2p-net-web" in trunk or "wasm-pack" in trunk:
        fail("the web build must not package a second p2p-net WebAssembly module")
    realtime = text(SDK_CRATES / "ghost-realtime" / "Cargo.toml")
    if 'default = ["p2p-net"]' not in realtime:
        fail("ghost-realtime must keep p2p-net helpers available to native/Kinesis consumers by default")
    manifests = [APP_ROOT / "Cargo.toml", SDK_CRATES / "ghost-realtime" / "Cargo.toml"]
    manifests += sorted(APP_CRATES.glob("*/Cargo.toml"))
    revisions = set()
    for manifest in manifests:
        body = manifest.read_text(encoding="utf-8")
        if RUSTY_KASPA.search(body):
            fail(f"{rel(manifest)} depends on Rusty-Kaspa directly; use Kaspa Portal")
        revisions.update(P2P_NET_DEP.findall(body))
    if len(revisions) > 1:
        fail(f"p2p-net is pinned to more than one revision: {sorted(revisions)}")
