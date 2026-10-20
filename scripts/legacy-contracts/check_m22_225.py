"""Source-level regression guard for scenario-to-calendar approval plumbing.

This does not compile or execute Rust; Cargo acceptance remains M15 work.
"""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]

def source(name):
    return (ROOT/name).read_text()

core=source('crates/orbit-runtime/src/scenario_execution.rs')
approval=source('crates/orbit-runtime/src/planning_approval.rs')
capabilities=source('crates/orbit-runtime/src/capabilities.rs')
server=source('crates/orbit-server/src/main.rs')
cli=source('crates/orbit-cli/src/lib.rs')
cli_commands=source('crates/orbit-cli/src/commands.rs')
connected=source('crates/orbit-client/src/connected.rs')
unified=source('crates/orbit-client/src/unified.rs')
local_mcp=source('crates/orbit-mcp/src/embedded_backend.rs')
remote_mcp=source('crates/orbit-mcp/src/connected_backend.rs')
mcp=source('crates/orbit-mcp/src/main.rs')

checks={
    'scenario module is exported': 'pub mod scenario_execution;' in source('crates/orbit-runtime/src/lib.rs'),
    'scenario comparison is reconstructed server side': 'self.compare_planning_scenarios(&request.comparison)?' in core,
    'calendar windows are accepted projections': 'self.schedule_plan(' in core,
    'review tied to global accepted revision': 'self.snapshot()?.0 !=request.comparison.expected_revision' in core,
    'same revision for tasks and calendar': 'schedule.schedule_revision!=request.comparison.expected_revision' in core,
    'review delegates to existing authoritative gate': 'self.review_schedule_request(actor,&plan)?' in core,
    'unschedulable selection is rejected': 'selected scenario cannot fit available calendar windows' in core,
    'selected work cannot silently disappear': 'scenario requires 1 to 100 selected tasks' in core,
    'duplicate selected task detection': 'duplicate or invalid scenario task' in core,
    'explicit approval required by ledger': 'if !approval.approved' in approval,
    'server review authenticated': 'let actor=device(&state,&headers)?;' in server,
    'review route exists': '/v1/authoritative/scenario-execution-review' in server,
    'existing apply route remains separate': '/v1/authoritative/plan-apply-approved' in server,
    'CLI review exists': 'TaskSubcommand::ReviewScenario(args)' in cli,
    'CLI apply remains explicit': 'TaskSubcommand::ApplyReviewedSchedule(' in cli,
    'CLI review requires input file': 'struct TaskReviewScenarioArgs' in cli_commands,
    'online client validates response': 'scenario execution review contract mismatch' in connected,
    'no offline remote fallback': 'scenario review requires a live authoritative server' in unified,
    'no offline schedule application': 'approved plan requires a live authoritative server' in unified,
    'object schema arguments supported': '"object"=>value.is_object()' in capabilities,
    'two-step MCP contracts registered': all(x in capabilities for x in ('planning.review_scenario','planning.apply_approved_scenario')),
    'local MCP review and apply': all(x in local_mcp for x in ('planning.review_scenario','planning.apply_approved_scenario')),
    'connected MCP review and apply': all(x in remote_mcp for x in ('planning.review_scenario','planning.apply_approved_scenario')),
    'MCP approval requires separate confirm': all('args.get("confirm")' in s for s in (local_mcp,remote_mcp)),
    'MCP approval checks write permissions': all('self.policy.write_allowed()?' in s for s in (local_mcp,remote_mcp)),
    'MCP tools discovered in both modes': mcp.count('mcp_tool("planning.review_scenario")')==2 and mcp.count('mcp_tool("planning.apply_approved_scenario")')==2,
    'behavioral Rust tests included': core.count('#[test]')>=21,
}
for title,ok in checks.items():
    print(('PASS' if ok else 'FAIL'),title)
assert all(checks.values()), 'source integration check failed'
print(f'{len(checks)} source checks passed (NOT Rust compilation)')
