#!/usr/bin/env python3
"""Static boundary checks for M22 development 219 (not Rust acceptance)."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
def check(rel,*patterns):
    text=(root/rel).read_text()
    for pattern in patterns:
        assert pattern in text,(rel,pattern)
check('crates/orbit-runtime/src/capabilities.rs',
      'task.dependencies.add','task.dependencies.remove','task.dependencies.preview',
      'task.graph','task.next','pub fn validate_call','unknown_mutation_argument')
check('crates/orbit-runtime/src/task_dependency_preview.rs',
      'pub fn preview_dependency','analyze_task_graph','changed_tasks',
      'requires_revision_check','transitive_cycles_are_rejected')
check('crates/orbit-client/src/unified.rs','pub fn preview_task_dependency',
      'dependency preview requires accepted authoritative state')
check('crates/orbit-mcp/src/embedded_backend.rs',
      '"task.dependencies.add" | "task.dependencies.remove"',
      '"task.dependencies.preview"','"task.graph"','"task.next"')
check('crates/orbit-mcp/src/connected_backend.rs',
      '"task.dependencies.add" | "task.dependencies.remove"',
      '"task.dependencies.preview"',
      'TaskAction::AddPrerequisite {task_id,prerequisite_id}',
      'self.check_existing_task(prerequisite_id)')
check('crates/orbit-mcp/src/main.rs',
      'capabilities::validate_call','mcp_tool("task.dependencies.preview")',
      'project_scoped_dependency_writes_check_both_endpoints')
check('crates/orbit-cli/src/commands.rs','PreviewDependency(TaskPreviewDependencyArgs)')
check('crates/orbit-cli/src/lib.rs','TaskSubcommand::PreviewDependency(args)')
print('M22 Development 219: source boundary checks passed (Rust not executed)')
