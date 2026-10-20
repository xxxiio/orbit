#!/usr/bin/env python3
"""Conservative, reproducible M18 scope assessment; not a release certificate.

Unlike historical subjective milestones this scores independently inspectable gates.
Static evidence is capped: source existence can never satisfy an executable gate.
Exit nonzero with --require-complete until every mandatory gate passes.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]

def has(path, *tokens):
    file = ROOT / path
    return file.is_file() and all(t in file.read_text(errors='replace') for t in tokens)

# A passing source check establishes existence, never correctness. The share
# of each gate awarded to source is deliberately disclosed and reviewable.
GATES = [
    ('host_owned_composition', 5, 'Native adapters open one host instead of their own application stores',
     [('crates/orbit-runtime/src/lib.rs','pub fn application_store_mut'),
      ('crates/orbit-client/src/lib.rs','runtime: orbit_runtime::AuthoritativeHost'),
      ('crates/orbit-mcp/src/main.rs','authority: AuthoritativeHost')],
     [('crates/orbit-client/src/lib.rs','store: SqliteStore'),
      ('crates/orbit-mcp/src/main.rs','store: SqliteStore')]),
    ('single_authority', 15, 'Single authoritative persistence and application services',
     [('crates/orbit-runtime/src/lib.rs','pub struct AuthoritativeHost'),
      ('crates/orbit-runtime/src/store.rs','pub(crate) conn: orbit_storage::SqliteStore')],
     [('crates/orbit-runtime/src/lib.rs','application: SqliteStore'),
      ('crates/orbit-client/src/lib.rs','store: SqliteStore')]),
    ('markdown_authority', 15, 'File-authoritative knowledge with durable acceptance',
     [('crates/orbit-runtime/src/markdown_commands.rs','pub fn enqueue'),
      ('crates/orbit-runtime/src/markdown_commands.rs','pub fn accept'),
      ('crates/orbit-runtime/src/knowledge_service.rs','read_by_id')], []),
    ('folder_domain', 10, 'Stable identity and durable folder organization',
     [('crates/orbit-runtime/src/folder_commands.rs','pub fn enqueue'),
      ('crates/orbit-runtime/src/folder_commands.rs','pub fn enqueue_create'),
      ('crates/orbit-runtime/src/knowledge_service.rs','pub fn discover')], []),
    ('client_parity', 15, 'Embedded, HTTP and MCP share canonical host',
     [('crates/orbit-client/src/unified.rs','pub enum AuthorityConnection'),
      ('crates/orbit-mcp/src/main.rs','knowledge.command'),
      ('crates/orbit-runtime/src/lib.rs','execute_markdown_command')], []),
    ('offline', 10, 'Durable remote outbox and conflict handling',
     [('crates/orbit-client/src/unified.rs','flush_markdown'),
      ('crates/orbit-client/src/unified.rs','queue_markdown_command')], []),
    ('local_feature_parity', 15, 'Local planning, projects, habits and metrics use the canonical service',
     [('crates/orbit-client/src/lib.rs','create_project_with_activity'),
      ('crates/orbit-client/src/lib.rs','apply_validated_plan'),
      ('crates/orbit-client/src/lib.rs','create_habit_with_activity')],
     [('crates/orbit-client/src/lib.rs','store: SqliteStore')]),
    ('executable_acceptance', 10, 'Workspace compilation, executable smoke and recovery validation',
     [('scripts/legacy-contracts/m18-acceptance.sh','cargo test --workspace')], []),
    ('internal_testing', 5, 'Runnable packaged build and no known blocking data-loss issue',
     [('scripts/legacy-contracts/m18-acceptance.sh','cargo check --workspace')], []),
]

def assess():
    evidence = []
    for key, weight, label, positive, blockers in GATES:
        checks = [{'file':p,'needle':t,'present':bool(has(p,t))} for p,t in positive]
        negatives = [{'file':p,'needle':t,'still_present':bool(has(p,t))} for p,t in blockers]
        # Source-only credit is limited to 60% of a gate. Architectural blockers
        # explicitly set the credit to zero, regardless of interfaces present.
        source_fraction = sum(c['present'] for c in checks)/len(checks) if checks else 0
        blocked = any(c['still_present'] for c in negatives)
        credit = 0 if blocked else round(weight*0.6*source_fraction,2)
        if key in {'executable_acceptance','internal_testing'}:
            credit = 0  # source wiring does not establish build or runtime evidence
        evidence.append(dict(id=key,description=label,weight=weight,source_checks=checks,
                             blocking_patterns=negatives,source_credit=credit,
                             acceptance='not_verified',complete=False))
    return dict(schema_version=1, methodology='Source evidence capped at 60%; blocked or unexecuted gates receive no full completion credit.',
                source_evidence_score=round(sum(x['source_credit'] for x in evidence),2),
                out_of=100, acceptance='NOT_ESTABLISHED', m18_complete=False,
                rust_available=bool(shutil.which('cargo') and shutil.which('rustc')),
                gates=evidence)

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--json',action='store_true')
    p.add_argument('--require-complete',action='store_true')
    args=p.parse_args()
    data=assess()
    if args.json: print(json.dumps(data,indent=2))
    else:
        print('M18 evidence-based source score: %.2f/100 (NOT milestone completion)' % data['source_evidence_score'])
        print('M18 acceptance:',data['acceptance'])
        print('Rust available:',data['rust_available'])
        for x in data['gates']:
            blockers=', '.join(y['needle'] for y in x['blocking_patterns'] if y['still_present'])
            print(f"  {x['id']:24} {x['source_credit']:>5.2f}/{x['weight']:>2} source credit; blocker={blockers or 'none detected'}")
    return 1 if args.require_complete and not data['m18_complete'] else 0
if __name__=='__main__': raise SystemExit(main())
