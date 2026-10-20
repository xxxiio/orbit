#!/usr/bin/env python3
"""Source-contract smoke checks; not a substitute for compiling Rust."""
from pathlib import Path
root=Path(__file__).resolve().parents[2]
ui=(root/'crates/orbit-tui/src/planning_ui.rs').read_text()
local=(root/'crates/orbit-tui/src/main.rs').read_text()
remote=(root/'crates/orbit-tui/src/connected.rs').read_text()
checks={
 'dependency request from reviewed handoff':'pub fn dependency_request_from_handoff' in ui,
 'dependency preview is read-only':'no dependency edge or task state was changed' in ui,
 'dependency removal requires explicit parent':'missing accepted prerequisite' in ui,
 'dependency add not guessed':'PreviewOperation::DependencyRemoval {return Ok(None)}' in ui,
 'calendar request from accepted window':'pub fn calendar_request_from_handoff' in ui,
 'calendar has separate approval':'separately review exact slots and approve' in ui,
 'calendar preview target is scoped':'plan.slots.iter().find(|s|s.task_id==preview.task_id)' in ui,
 'calendar excludes foreign notes':'plan.unplaced.iter().filter(|s|s.task_id==preview.task_id)' in ui,
 'reject task revision mismatch':'plan.task_revision!=handoff.revision' in ui,
 'reject schedule revision mismatch':'plan.schedule_revision!=handoff.revision' in ui,
 'local dependency preview':'self.client.preview_task_dependency(&request)' in local,
 'local calendar preview':'self.client.schedule_plan(&request)' in local,
 'connected dependency preview':'preview_dependency(' in remote,
 'connected calendar preview':'plan_schedule(' in remote,
 'connected snapshots accepted only':'self.replica.snapshot()' in remote,
 'tests for stale revision':'fn schedule_revision_change_must_reject_preview' in ui,
 'tests for foreign task redaction':'fn calendar_view_only_shows_the_selected_task' in ui,
 'tests for read-only changes':'fn dependency_preview_presents_before_and_after' in ui,
}
for name,passed in checks.items():
 print(('PASS' if passed else 'FAIL'),name)
assert all(checks.values())
print(f'{len(checks)} source-contract checks passed; Rust compiler execution NOT performed')
