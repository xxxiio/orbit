---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0007
status: ready
milestone_id: MILESTONE-0028
advances_exit_criteria: [EC-002, EC-006]
---

# Next Deterministic Step

## Objective

Replace the connected command-only mutation surface with project-centric
navigation and the shared action catalogue, while preserving durable pending
commands and accepted-state separation.

## Why this is next

The local project-centric shell, full 42/42 profile and connected read-only
project views are complete. Connected mode still uses its command-centric
sections for navigation and mutations. This is the largest remaining M28
user-visible parity gap.

## Preconditions

- Connected read-only views resolve accepted snapshots through `orbit-ui`.
- The shared action catalogue and resolved view/selection contracts are stable.
- Mutations remain outbox intents until server acceptance.

## Inputs

- `crates/orbit-tui/src/connected.rs`
- `crates/orbit-tui/src/connected_actions.rs`
- `crates/orbit-ui/src/{resolve,actions,model}.rs`
- `crates/orbit-client/src/{unified,authority}.rs`
- `.agent/evidence/m28-terminal-smoke.txt`

## Ordered actions

1. Add project selection and view switching to the connected shell using the
   shared resolved views.
2. Derive contextual actions from the shared catalogue; map each to the existing
   durable connected intent queue.
3. Keep a visible pending/outbox distinction and never present a queued intent
   as accepted.
4. Add connected tests for navigation, action availability, pending state and
   no-snapshot behavior.
5. Update docs, acceptance and evidence.

## Expected outputs

- Connected project navigation and view switching.
- Shared action menus/palette routed to durable intents.
- Tests for accepted/pending separation and no duplicate execution.
- Updated connected user documentation.

## Acceptance criteria

- [ ] NS-001: Connected projects and views are navigable without command syntax.
- [ ] NS-002: Contextual actions are derived from the shared catalogue and remain pending until accepted.
- [ ] NS-003: Offline/no-snapshot behavior is explicit and never invents accepted state.
- [ ] NS-004: Focused tests and docs cover navigation, actions and pending state.

## Validation

```sh
cargo test -p orbit-tui -p orbit-client -p orbit-ui --offline
cargo clippy -p orbit-tui -p orbit-client -p orbit-ui --all-targets --offline -- -D warnings
```

## Stop conditions

- Stop if a connected mutation bypasses the outbox or server acceptance.
- Stop if pending intents are projected as accepted state.
- Stop if the shared action contract must be forked.

## Out of scope

- Blueprint work-item synchronization over connected mode.
- Replacing the local shell.
- Flutter implementation.
