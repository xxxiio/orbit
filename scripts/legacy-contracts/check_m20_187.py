"""Source-boundary assertions for M20 routine and milestone transport.

This is intentionally not a substitute for compiled Rust acceptance tests.
"""
from pathlib import Path

root = Path(__file__).resolve().parents[2]

def load(relative: str) -> str:
    return (root / relative).read_text()

runtime = load('crates/orbit-runtime/src/lib.rs')
routines = load('crates/orbit-runtime/src/authority_routines.rs')
milestones = load('crates/orbit-runtime/src/authority_milestones.rs')
server = load('crates/orbit-server/src/main.rs')
client = load('crates/orbit-client/src/unified.rs')
replica = load('crates/orbit-client/src/authority.rs')
connected = load('crates/orbit-client/src/connected.rs')
embedded = load('crates/orbit-client/src/embedded_authority.rs')

checks = {
    'canonical routine command and projection': (
        'pub mod authority_routines;' in runtime
        and 'pub fn authority_routine_command' in routines
        and 'pub fn authority_routines' in routines
        and 'pub fn routine_command' in runtime
    ),
    'canonical milestone command and projection': (
        'pub mod authority_milestones;' in runtime
        and 'pub fn authority_milestone_command' in milestones
        and 'pub fn authority_milestones' in milestones
        and 'pub fn milestone_command' in runtime
    ),
    'routine mutation and receipt share transaction': (
        'TransactionBehavior::Immediate' in routines
        and 'pending.commit(&tx,revision,semantic,&receipt)?' in routines
        and 'tx.commit()?' in routines
    ),
    'milestone mutation and receipt share transaction': (
        'TransactionBehavior::Immediate' in milestones
        and 'pending.commit(&tx,revision,semantic,&receipt)?' in milestones
        and 'tx.commit()?' in milestones
    ),
    'routine membership order and uniqueness': (
        'ORDER BY position,habit_id' in routines
        and 'unique.insert(id.as_str())' in routines
        and 'replace_habits(&tx,&id,&habit_ids)?' in routines
    ),
    'milestone relationship and lifecycle validation': (
        'cannot delete milestone with attached tasks' in milestones
        and 'invalid milestone status transition' in milestones
        and 'ensure_project(&tx,&project_id)?' in milestones
    ),
    'server exposes both read and write HTTP routes': all(
        route in server for route in (
            '/v1/authoritative/routines',
            '/v1/authoritative/routine-commands',
            '/v1/authoritative/milestones',
            '/v1/authoritative/milestone-commands',
        )
    ),
    'server accepted snapshot includes both new collections': (
        'routines:routines.routines' in server
        and 'milestones:milestones.milestones' in server
        and 'milestones.revision != revision' in server
    ),
    'embedded client delegates to same canonical host': all(
        signature in embedded for signature in (
            'self.host.routines()', 'self.host.routine_command(&self.actor,command)',
            'self.host.milestones()', 'self.host.milestone_command(&self.actor,command)',
        )
    ),
    'remote outbox accepts routine and milestone command kinds': (
        'CommandKind::Routine' in connected
        and 'CommandKind::Milestone' in connected
        and "'routine','milestone'" in replica
    ),
    'remote receipt identity is checked before acknowledgement': (
        'routine receipt identity mismatch' in replica
        and 'milestone receipt identity mismatch' in replica
        and 'milestone delete receipt must be null' in replica
    ),
    'accepted replica schema includes new collections': (
        '"routines","milestones"' in replica
        and 'routines:collection(&value,"routines")?' in connected
        and 'milestones:collection(&value,"milestones")?' in connected
    ),
    'unified client retains accepted pending separation': (
        'remote_disposition(replica,&id)' in client
        and 'self.accepted_collection("milestones")' in client
        and 'self.accepted_collection("routines")' in client
    ),
    'executable regression cases added': (
        routines.count('#[test]') >= 6
        and milestones.count('#[test]') >= 6
        and 'remote_routine_and_milestone_commands_are_only_pending_until_acknowledged' in client
    ),
}
for name,passed in checks.items():
    print(f'{"PASS" if passed else "FAIL"}: {name}')
if not all(checks.values()):
    raise SystemExit(1)
print(f'{len(checks)} structural checks passed (Rust executable acceptance still required)')
