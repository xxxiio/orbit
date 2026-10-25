#!/usr/bin/env python3
"""M25 source artifact inventory; complements, never replaces, compiled tests."""
from pathlib import Path
import json
import sys
ROOT = Path(__file__).resolve().parents[1]
EXPECTED = ('development', 'learning', 'habits', 'research')
for item in EXPECTED:
    path = ROOT / 'blueprints' / (item + '.json')
    if not path.is_file():
        raise SystemExit(f'MISSING blueprint: {path}')
    value = json.loads(path.read_text(encoding='utf-8'))
    if value.get('id') != item or not isinstance(value.get('version'), int):
        raise SystemExit(f'INVALID blueprint id/version: {path}')
    print(f'OK bundled blueprint {item}@{value["version"]}')
required = {
    'crates/orbit-blueprint/src/lib.rs': ('pub struct Blueprint',),
    'crates/orbit-cli/src/blueprint_commands.rs': ('pub enum BlueprintSubcommand', 'Validate {', 'Import {', 'ApplyTemplate {'),
    'crates/orbit-runtime/src/blueprint_execution.rs': ('BlueprintExecutorRegistry',),
    'crates/orbit-runtime/src/capability_composition.rs': (
        'TrustedComposition',
        'request_capability_action',
        'execute_capability_action',
        'reconcile_capability_action',
    ),
    'crates/orbit-storage/src/capability_installs.rs': (
        'install_capability',
        'grant_capability',
        'record_capability_receipt',
    ),
    'crates/orbit-storage/src/blueprint_command_receipts.rs': (),
    'crates/orbit-mcp/src/blueprint_tools.rs': (),
    'blueprints/examples/capability-reference.json': (
        'orbit.reference.work-item-note',
    ),
}
for rel, tokens in required.items():
    path = ROOT / rel
    if not path.is_file():
        raise SystemExit(f'MISSING source integration: {rel}')
    source = path.read_text(encoding='utf-8')
    for token in tokens:
        if token not in source:
            raise SystemExit(f'MISSING expected definition {token!r}: {rel}')
print('OK blueprint source inventory (not executable acceptance)')
