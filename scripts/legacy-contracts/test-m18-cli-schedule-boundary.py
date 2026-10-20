#!/usr/bin/env python3
"""Source-contract regression guard for M18 schedule transport routing."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
cli = (root / 'crates/orbit-cli/src/lib.rs').read_text()
services = (root / 'crates/orbit-runtime/src/application_services.rs').read_text()
assert 'run_schedule_host(&mut host, schedule.command)' in cli
assert 'fn run_schedule(' not in cli
assert 'Command::Schedule(_) => unreachable!' in cli
for token in ('create_schedule_block_accepted(', 'reschedule_block_accepted(', 'application_queries().schedule_between('):
    assert token in cli, token
for token in ('pub(crate) fn reschedule_block(', 'pub fn schedule_between('):
    assert token in services, token
print('M18 shared scheduling boundary: passed')
