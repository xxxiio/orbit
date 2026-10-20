#!/usr/bin/env python3
"""Source boundary checks for M22 Development 227 (not a Rust compiler)."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
files={
 'runtime':root/'crates/orbit-runtime/src/planning_actions.rs',
 'host':root/'crates/orbit-runtime/src/lib.rs',
 'client':root/'crates/orbit-client/src/unified.rs',
 'cli':root/'crates/orbit-cli/src/lib.rs',
 'cliargs':root/'crates/orbit-cli/src/commands.rs',
 'server':root/'crates/orbit-server/src/main.rs',
 'localmcp':root/'crates/orbit-mcp/src/embedded_backend.rs',
 'remotemcp':root/'crates/orbit-mcp/src/connected_backend.rs',
 'mcplist':root/'crates/orbit-mcp/src/main.rs',
 'registry':root/'crates/orbit-runtime/src/capabilities.rs',
 'localtui':root/'crates/orbit-tui/src/main.rs',
 'remotetui':root/'crates/orbit-tui/src/connected.rs',
}
src={key:value.read_text() for key,value in files.items()}
checks={
 'read-only action metadata':all(x in src['runtime'] for x in ('pub executable:bool','requires_confirmation:bool','reviewed_revision:i64')),
 'revision-bound resolution': 'selection.reviewed_revision!=brief.revision' in src['runtime'],
 'recomputed task menu':'let menu=task_actions(brief,request)?' in src['runtime'],
 'recomputed group menu':'let menu=group_actions(brief,request)?' in src['runtime'],
 'reject forged selection': 'selected.kind!=selection.kind' in src['runtime'] and 'selected.prerequisite_id!=selection.prerequisite_id' in src['runtime'],
 'reject unavailable mutation':'ActionAvailability::Unavailable {return Err' in src['runtime'],
 'do not expose hidden blocker identities':'visible.contains_key(id.as_str())' in src['runtime'],
 'core authoritative revision fence':'self.tasks()?.0!=brief.revision' in src['host'],
 'connected accepted snapshot fence':'if revision!=brief.revision' in src['client'],
 'local cli menus':all(x in src['cli'] for x in ('TaskSubcommand::PlanningActions','TaskSubcommand::PlanningGroupActions')),
 'local cli resolves':all(x in src['cli'] for x in ('TaskSubcommand::ResolvePlanningAction','TaskSubcommand::ResolvePlanningGroupAction')),
 'CLI command declarations':all(x in src['cliargs'] for x in ('ResolvePlanningAction','ResolvePlanningGroupAction')),
 'authenticated HTTP menus':'device(&state,&headers)?' in src['server'] and '/v1/authoritative/planning-task-actions' in src['server'],
 'read-only HTTP resolution':all(x in src['server'] for x in ('authoritative_resolve_task_action','authoritative_resolve_group_action')),
 'local MCP actions':all(x in src['localmcp'] for x in ('planning_resolve_task_action','planning_group_actions')),
 'connected MCP actions':all(x in src['remotemcp'] for x in ('"planning.resolve_task_action"','"planning.group_actions"')),
 'MCP advertised both modes':src['mcplist'].count('mcp_tool("planning.resolve_task_action")')==2,
 'registry requires selection':'const ACTION_SELECTION:' in src['registry'],
 'local TUI action navigation': 'today_actions(&mut self)' in src['localtui'],
 'connected TUI action navigation': 'preview_planning_actions(&mut self)' in src['remotetui'],
 'Rust regression tests':'mod action_selection_tests' in src['runtime'] and 'mod planning_action_contract_tests' in src['registry'],
}
for label,passed in checks.items():
 print(('PASS' if passed else 'FAIL')+' '+label)
assert all(checks.values()),'M22 Development 227 source boundary checks failed'
print(f'{len(checks)} source checks passed (Rust compilation not run)')
