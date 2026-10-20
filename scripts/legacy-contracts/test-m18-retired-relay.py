#!/usr/bin/env python3
"""Guard the production API against the deprecated peer-event relay.

The semantic history replay remains internal to the authority command ledger.
This check does not establish compiled Rust acceptance.
"""
from pathlib import Path
import re

root = Path(__file__).resolve().parents[2]
store = (root / "crates/orbit-runtime/src/store.rs").read_text()
server = (root / "crates/orbit-server/src/main.rs").read_text()
host = (root / "crates/orbit-runtime/src/lib.rs").read_text()
client = (root / "crates/orbit-client/src/unified.rs").read_text()
for method in ("append", "commit_semantic", "pull", "history", "generation", "authority_projection", "event_count"):
    pattern = rf"#\[cfg\(test\)\]\s+pub fn {method}\("
    assert re.search(pattern, store), f"Legacy {method} still available in production runtime"
assert "pub struct AppendResult" in store and re.search(
    r"#\[cfg\(test\)\]\s+#\[derive\(Debug, Clone, serde::Serialize\)\]\s+pub struct AppendResult", store
)
# No authenticated peer-event publishing endpoint should exist.
for bad in ('/v1/relay', '/v1/events/append', '/v1/relay/append'):
    assert bad not in server, f"Obsolete relay endpoint: {bad}"
assert "pub fn append(" not in host
assert "fn append_relay(" not in client
print("Legacy peer-event ingestion is excluded from production interfaces: PASS")
