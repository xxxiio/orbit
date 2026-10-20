#!/usr/bin/env python3
"""Source regression guard for M20 idempotent cross-transport domain intentions."""
from pathlib import Path
r=Path(__file__).resolve().parents[2]
a=(r/'crates/orbit-client/src/authority.rs').read_text()
u=(r/'crates/orbit-client/src/unified.rs').read_text()
c=(r/'crates/orbit-client/src/connected.rs').read_text()
for symbol in ('pub fn accepted_receipt(', 'pub fn command_status(', 'old_kind==kind.as_str() && &old_action==action', 'command ID already used for another intention', 'project receipt identity mismatch', 'task receipt identity mismatch'):
 assert symbol in a,symbol
for symbol in ('fn remote_disposition(', 'remote_disposition(replica, &command.command_id)', 'remote_disposition(replica, &id)', 'pub fn command_status(', 'pub fn accepted_receipt('):
 assert symbol in u,symbol
assert 'pub fn command_status(' in c and 'pub fn accepted_receipt(' in c
for test in ('domain_retries_preserve_one_ordinal', 'accepted_receipts_are_durable', 'conflicted_requeue_does_not_clear_conflict', 'remote_unified_commands_are_idempotent'):
 assert test in a+u,test
print('M20 idempotent domain outbox: SOURCE GUARD PASS (Rust execution deferred to M15)')
