#!/usr/bin/env python3
"""Structural contract checks; does not replace compiled Rust acceptance."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
commands=(root/'crates/orbit-cli/src/commands.rs').read_text()
replica=(root/'crates/orbit-cli/src/authority_replica.rs').read_text()
http=(root/'crates/orbit-cli/src/authority_http.rs').read_text()
client=(root/'crates/orbit-client/src/authority.rs').read_text()
server=(root/'crates/orbit-server/src/main.rs').read_text()
families={'Schedule':'schedule','Routine':'routine','Milestone':'milestone','Strategy':'strategy'}
checks=[]
def check(name,cond):
    checks.append((name,bool(cond)))
for enum,route in families.items():
    check(f'{route}: replica CLI enumerated',enum in commands.split('pub enum ReplicaKind')[1].split('}')[0])
    check(f'{route}: maps to canonical queue',f'ReplicaKind::{enum}=>CommandKind::{enum}' in replica)
    check(f'{route}: typed direct invocation',f'authority_{route if route!="routine" else "routines"}::{enum}Command' in http if route!='milestone' else 'authority_milestones::MilestoneCommand' in http)
    check(f'{route}: outbox transport',f'Self::{enum}=>"{route}-commands"' in client)
    check(f'{route}: server route',f'/v1/authoritative/{route}-commands' in server)
for name in ['projects','tasks','habits','habit_occurrences','metrics','metric_observations','schedule','routines','milestones','strategies']:
    check(f'accepted {name}',f'"{name}"' in http and f'"{name}"' in client)
check('receipt inspection', 'replica.command_status(&command_id)' in replica)
check('strict wire deserialization','serde_json::from_value::<orbit_runtime::authority_strategy::StrategyCommand>' in http)
check('no premature offline acceptance','"pending_changes_included":false' in replica)
check('no credentials in body','"token":' not in http)
for name,ok in checks:
    print(('PASS' if ok else 'FAIL')+': '+name)
assert all(v for _,v in checks)
print(f'{len(checks)} M20 CLI integration source checks passed (not compiled Rust tests)')
