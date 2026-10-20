#!/usr/bin/env python3
"""Source-level wiring regression; not a replacement for cargo test."""
from pathlib import Path

root = Path(__file__).resolve().parents[2]

def read(path):
    return (root / path).read_text(encoding="utf-8")

schedule = read("crates/orbit-runtime/src/authority_schedule.rs")
server = read("crates/orbit-server/src/main.rs")
runtime = read("crates/orbit-runtime/src/lib.rs")
embedded = read("crates/orbit-client/src/embedded_authority.rs")
connected = read("crates/orbit-client/src/connected.rs")
unified = read("crates/orbit-client/src/unified.rs")
outbox = read("crates/orbit-client/src/authority.rs")

assert "authority_schedule_command(" in schedule
assert "TransactionBehavior::Immediate" in schedule
assert "pending.commit(&tx, revision, semantic, &receipt)?" in schedule
assert schedule.index("pending.commit(&tx, revision, semantic, &receipt)?") < schedule.index("tx.commit()?;\n        Ok(result)")
assert "ensure_free(&tx, *starts_at_ms, end, None)?" in schedule
assert "ensure_free(&tx, *starts_at_ms, end, Some(&id))?" in schedule
assert "fixed schedule cannot move" in schedule
assert "fixed schedule cannot resize" in schedule
assert "target does not exist" in schedule
assert "ScheduleAction::Delete" in schedule
assert "authority_schedule_conflicts(" in schedule
assert schedule.count("#[test]") >= 25

for route in [
    '/v1/authoritative/schedule',
    '/v1/authoritative/schedule-conflicts',
    '/v1/authoritative/schedule-commands',
]:
    assert route in server, route
assert 'schedule:schedule.blocks' in server
assert 'pub mod authority_schedule;' in runtime
assert 'pub fn schedule_command(' in runtime
assert 'pub fn schedule_command(' in embedded
assert 'pub fn schedule_command(' in unified
assert 'pub fn schedule(&self)' in unified
assert '"schedule": schedule.blocks' in unified
assert '"schedule": workspace.schedule' in unified
assert 'CommandKind::Schedule' in connected
assert 'schedule:collection(&value,"schedule")?' in connected
assert '"schedule"=>CommandKind::Schedule' in outbox
assert '"schedule-commands"' in outbox
assert 'schedule receipt identity mismatch' in outbox
assert '"metric_observations","schedule"' in outbox
print("M20 authoritative schedule source contract: PASS; Rust execution not verified")
