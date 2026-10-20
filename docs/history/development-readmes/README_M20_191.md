# M20 / Development 191 — Connected TUI workflow implementation

## Scope

This increment implements connected-mode interactive domain workflows instead of adding another wrapper to the authority connection. The TUI has a dedicated pure command-palette interpreter (`orbit-tui/src/connected_actions.rs`) and a server-authoritative terminal state machine (`orbit-tui/src/connected.rs`). It never constructs the local-authoritative `OrbitClient` in connected mode.

## User-visible workflows

- Switch between accepted projects, tasks, habits, metrics, schedule blocks, routines, milestones, strategies, and unresolved durable commands.
- Browse the last accepted revision, select a record, display its fields and suggested edit-command templates.
- Filter accepted collection records in the selected section without altering the snapshot or showing unaccepted changes as accepted.
- Create, rename, assign and plan tasks, including assigning milestones.
- Create, move, resize, rename, retarget and delete schedule blocks, including fixed/movable policy and target binding.
- Create, rename, replace habit memberships and delete routines.
- Create, rename, change statuses, change project and delete milestones.
- Configure project strategies (objective, mode, progress signals and success criteria), update objectives/signals, and update criterion status.
- Review typed mutation payloads before queuing commands; cancel without writing.
- Compose an ordered `batch` of up to 100 commands, validate all command payloads before starting any queue operation, preview the complete workflow, and queue with explicit acknowledgement of partial failures.
- Inspect pending/conflicted outbox entries and explicitly replace a conflicted command with another same-family intention while preserving the rejected command in audit history.
- Refresh server-accepted state or flush pending intents on demand; offline operations never pretend to be committed.

## Keyboard and palette

In connected mode, `Tab` and `Shift-Tab` select a domain, arrow keys or `j/k` select records, `Enter` inspects one, `e` prepares an editable command, `:` opens the command palette, `?` shows section-specific help, `/` filters records, `x` clears the filter, `o` shows the outbox, `f` flushes, `r` refreshes, and `q` exits. To create a task quickly use `c`.

Palette examples:

```
task.create "Draft release notes"
task.milestone task-1 ship
schedule.create block1 1792108800000 45 movable task task-1 "Documentation"
schedule.move block1 1792112400000
routine.create morning walk,read "Morning ritual"
milestone.create ship project-1 "Ship first beta"
strategy.configure project-1 standard tasks,milestones "Finish beta" acceptance:true:"Beta launched"
strategy.criterion project-1 acceptance satisfied
batch task.create "Investigate latency"; routine.create startup - "Startup checklist"
resolve rejected-command-id schedule.resize block1 30
```

`-` clears optional values and represents an empty routine habit list. A command entered with `:` is validated and previewed; press `y` to queue. A queued command receives a pending disposition and is never displayed as an accepted snapshot mutation. Batch queueing is ordered but not atomic: previous durable commands remain queued if a later queue operation fails. Replacing a conflicted command is explicit and cannot silently discard the original.

## Architecture boundaries

- Parse/validate: `connected_actions::parse_intent` and `parse_batch`, no writes.
- Preview: `Editor::Confirm`, `Editor::BatchConfirm`, or `Editor::Replace`, no writes.
- Durable pending write: `AuthorityReplica::queue` or `replace_conflict`.
- Network acceptance: `AuthorityReplica::flush(AuthorityHttp)`.
- Accepted projections: only `AuthorityReplica::snapshot` updated through refresh.
- Local authoritative mode: unchanged. Flutter: deferred to separate Atomic App project.

## Scope and limitations

This is an **implementation increment**, not a claim of verified M20 completion. MCP integration is still unfinished, and full terminal execution has not been checked against a compiled Rust binary. Cargo compilation, integration acceptance, release validation, and full regression campaigns are scheduled for M15 under the user's requested prioritization. Keep essential safety checks in feature development: all mutations require user confirmation, use canonical server command payloads, and preserve accepted/pending separation.

## Next implementation

Finish connected MCP and remaining domain interaction gaps, not standalone compilation-hardening work. M20 stays open until the intended workflows are implemented; do not use source line totals as a proxy for completion.
