#!/usr/bin/env python3
"""Source-only guard against reintroduction of unsafe conflict-replacement routes."""
from pathlib import Path

root = Path(__file__).resolve().parents[2] / "crates"
replica = (root / "orbit-client/src/authority.rs").read_text()
unified = (root / "orbit-client/src/unified.rs").read_text()
connected = (root / "orbit-mcp/src/connected_backend.rs").read_text()
cli = (root / "orbit-cli/src/authority_replica.rs").read_text()
commands = (root / "orbit-cli/src/commands.rs").read_text()
tests = (root / "orbit-client/src/conflict_batch_tests.rs").read_text()

assert "pub fn replace_conflict(" not in replica
assert "pub fn resolve_conflict(" not in unified
assert "ReplicaAction::ReplaceConflict" not in cli
assert "ReplaceConflict {" not in commands
assert "pub fn replace_reviewed_batch(" in replica
assert "pub fn replace_reviewed_batch_online(" in replica
assert "pub fn resolve_reviewed_batch(" in unified
assert "authority.resolve_reviewed_batch" in connected
assert '"authority.resolve_reviewed" | "authority.resolve"' in connected
assert "resolve_reviewed_batch_online" in cli or "replace_reviewed_batch_online" in cli
assert "rejected command changed since review" in replica
assert "accepted snapshot missing" in replica
assert "file conflicts require content-specific revision review" in replica
assert "atomic_local_staging" in connected and "atomic_local_staging" in cli
assert tests.count("#[test]") >= 20
print("M21 reviewed-batch source guard: PASS; Rust execution deferred to M15")
