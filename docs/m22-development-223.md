# M22 Development 223 — Read-only interactive planning scenarios

Planning scenario comparisons operate only on one accepted authoritative revision. No task or calendar command is submitted; users explicitly approve future mutations through existing authoritative interfaces.

## Contract

- Baseline uses the existing portfolio rank with a fixed available-minute and task-count budget.
- Up to eight named alternatives allow explicit pins, exclusions, alternate budgets and task limits.
- A pinned task must be visible, actionable, and schedulable, or the comparison fails rather than pretending the pin succeeded.
- Strict accepted-revision fencing prevents applying decisions to unseen task state.
- A project scope uses the existing portfolio action redaction policy, and invalid pins do not disclose hidden task details.
- Returned differences include selected and displaced task IDs, rationale and excluded-task reasons.
- No persisted state, command, reservation, or outbound network action is modified by the comparison.
- Both local host and remote accepted-snapshot clients use the same deterministic calculation.

## CLI

`orbit task compare-scenarios --input scenarios.json`

The JSON file has `baseline` (PortfolioCapacityRequest), `expected_revision`, and `alternatives` (ScenarioChoice array). JSON output is available through the CLI's global `--json` mode.

## Server

`POST /v1/authoritative/planning-scenarios` accepts the same JSON with the existing device authentication. HTTP POST here is a *read-only evaluation*, not a mutation.

## Acceptance state

This iteration includes pure Rust unit tests but they have not been executed because Cargo is not available in the current execution environment. Broad executable acceptance remains assigned to M15. New scenario workflows are source implemented, not runtime certified.
