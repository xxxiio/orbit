---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0030
status: active
---

# Milestone: TUI workspace simplification and executable Blueprint capabilities

## Intended outcome

The TUI workspace is a real two-region terminal workspace with an explicit,
dismissible item inspector, and a trusted Blueprint capability can be installed,
authorized for a project, discovered as an offered action, explicitly
requested, claimed, executed and reconciled through public CLI/MCP/TUI surfaces
with a durable receipt — closing M25 EC-003 through a supported user workflow
rather than a test-only primitive.

## Why this milestone matters

M28 completed the project-centric TUI and M29 completed its interaction quality.
Two gaps then remained in the highest-priority unfinished work:

1. The workspace permanently reserved a third Details column even when the user
   was not inspecting anything, so the primary content paid for a panel nobody
   asked to see. That is a layout/state-model defect, not a missing feature.
2. M25 EC-003 was the only remaining blueprint requirement that blocked the
   extension model from being demonstrable to users: executor primitives existed
   in `crates/orbit-runtime/src/blueprint_execution.rs`, but every publication
   path supplied an empty installed-capability set and no CLI/MCP/HTTP surface
   exposed install, authorize, offer, request, execute or inspect.

M30 addresses both without reopening M22–M29, without a second job engine and
without granting a manifest execution authority.

## Scope

- Remove the permanent docked Details region; two regions by default.
- Explicit inspector state: `Enter`/`D` open, `Esc` closes, full-screen fallback
  on very small terminals, focus restoration, selection preservation,
  missing-entity fallback, edit round trip, no hidden focus in `Tab` traversal.
- Durable trusted-capability installation, separate project-scoped
  authorization, project-scoped grants, and an installed-capability catalog
  derived from trusted host composition.
- Public composition API for registering trusted capabilities and their
  executors, with descriptors, actions, input schema, required permissions,
  compatibility requirements and fail-closed semantics.
- Action offer discovery, explicit requests with revision fencing, single-claim
  execution, durable receipts, reconciliation of uncertain outcomes.
- One deterministic built-in reference capability that proves the whole path
  locally without Python, Herdr, network access or external services.
- CLI capability/action commands (and MCP tools where the existing blueprint
  tool boundary makes that safe), with connected mode explicitly refusing the
  local-only path rather than bypassing the outbox.
- Project-centric TUI integration: offered capability actions appear in the
  contextual action menu, the command palette and the selected-entity
  inspector, with availability, missing requirements, inputs, confirmation and
  accurate execution disposition.
- Focused Rust tests, a real-terminal PTY scenario, an isolated-database
  end-to-end acceptance workflow including restart, documentation and records.

### Iterations

- I1 (this record's exit criteria): TUI two-region workspace + explicit inspector,
  trusted capability execution end to end, TUI capability actions, docs/tests.
  Status: implemented and verified; see Evidence.
- I2 (M25 EC-004): connected MCP blueprint writes through the canonical outbox,
  receipts, offline replay and lost-ack recovery. Delivered: queue-first
  `blueprint_write`, `accepted`/`pending`/`conflict` dispositions, 8 MCP tests and
  a real server/MCP acceptance case; no direct `.post(` remains.
- I3 (M25 EC-005): previewable fingerprint-fenced upgrades. Delivered:
  `preview_blueprint_upgrade`/`apply_blueprint_upgrade`, typed diffs and
  blockers, CLI `upgrade-preview|upgrade`, 13 runtime + 2 CLI tests and 60
  local CLI acceptance checks; publish/install never rebind.
- I4 (M25 EC-006): habit recurrence owned by the opt-in `orbit-habit-capability`
  package (`orbit.habit.recurrence`) with install/authorize gating; core domain
  keeps only occurrence/period types. Delivered with package, gating and docs.


## Non-goals

- Reopening M22, M23, M24, M26, M28 or M29.
- A broad TUI redesign, a new top-level tab, or a Flutter renderer.
- Arbitrary untrusted plugin loading, user-authored executables, a plugin
  marketplace or dynamic libraries.
- Rebuilding blueprint schemas, attachments, acceptance criteria or the offline
  outbox.
- Connected capability parity (M25 EC-004), previewable upgrades (EC-005) and
  habit recurrence extraction (EC-006).
- Public GitHub Release publication and M27 deployment verification.
- A new domain schema for unrelated project data.

## Deliverables

- `crates/orbit-tui/src/{app,render,input}` two-region layout and inspector.
- `crates/orbit-storage/src/capability_installs.rs` installation, grants and
  receipts.
- `crates/orbit-runtime/src/capability_composition.rs` trusted composition and
  host capability API.
- `crates/orbit-cli/src/blueprint_commands.rs` capability/action commands.
- `crates/orbit-client` local capability methods consumed by the TUI.
- `blueprints/examples/capability-reference.json` example manifest.
- TUI inspector and capability-action tests, PTY scenario, capability runtime
  test matrix, and an isolated end-to-end acceptance workflow.
- Updated `docs/guides/tui-workspace.md`, `docs/guides/blueprint-views.md`,
  blueprint authoring and capability reference, and CLI/MCP reference.

## Dependencies

- MILESTONE-0024 authoritative host and command receipts.
- MILESTONE-0028 project-centric TUI shell, contextual actions and forms.
- MILESTONE-0029 interaction-quality, test and PTY infrastructure.

M25 I1/I2 (registry, typed lifecycle, validation) are delivered and reused, but
M25 itself remains open for EC-004..EC-006, so it is not a blocking dependency.

## Risks

- Capability execution is security-sensitive: every path must fail closed and
  re-check authorization at request time, never trust a rendered "available".
- Receipt/claim collisions could duplicate an external effect: only one claim,
  stable command identity, and no automatic retry of `uncertain`.
- Removing the docked pane can strand focus or lose the inspected subject; the
  inspector must own explicit subject/focus state and tolerate deletion.
- TUI integration could tempt direct execution from render handlers; mutations
  must stay behind the client/application boundary.
- Documenting hypothetical API would overstate readiness; only implemented
  command names and keys may be documented.

## Ordered implementation sequence

1. Replace the docked Details pane with the explicit inspector and update layout,
   focus, session state and render-purity tests.
2. Add durable capability installation, grants and receipts to storage.
3. Add the trusted composition, reference capability, offers, request,
   execution, receipt and reconciliation host API.
4. Expose capability/action CLI commands and the example manifest; prove the
   full local workflow in an isolated database.
5. Integrate capability actions into the TUI contextual menu, palette and
   inspector with an input form, confirmation and honest dispositions.
6. Add the PTY scenario, documentation, captures and records; re-run focused and
   regression suites.

## Exit criteria

- [x] EC-001: At 80x24, 120x35 and 160x45 the default TUI has exactly two regions, no permanent Details box appears and the workspace receives the freed width.
- [x] EC-002: `Enter`/`D` open a dismissible inspector and `Esc` closes it with the selection, component, scroll and originating focus preserved; small terminals get a full-screen inspector; deleted entities show a fallback; editing returns to the same inspected entity; no `Tab` stop targets an invisible pane.
- [x] EC-003: A trusted capability can be installed through a public composition surface, authorized per project, offered as a Blueprint action, explicitly requested with revision fencing, claimed once, executed by a registered trusted executor and recorded as a durable receipt that survives restart.
- [x] EC-004: Uninstalled, unauthorized, unsupported-action, invalid-input, revision-mismatch and duplicate-request cases fail closed with distinct, test-covered reasons and no duplicate effect.
- [x] EC-005: Capability actions are discoverable and runnable from the project-centric TUI contextual action menu, command palette and selected-entity inspector, with required inputs, confirmation and accurate available/pending/completed/failed/uncertain dispositions; no mutation originates in the renderer.
- [x] EC-006: A documented CLI (and MCP where implemented) programmatic path performs capability registration/installation and explicit invocation with structured inputs; connected mode refuses rather than bypasses the outbox.
- [x] EC-007: An isolated-database end-to-end acceptance workflow proves create project → install capability → install/bind compatible Blueprint → discover → request → authorize → execute → receipt → restart → verify, plus the inspector PTY scenario and focused runtime tests.
- [x] EC-008: Existing documentation is updated with runnable examples; M25 EC-003 is updated only if EC-003/EC-007 genuinely pass; EC-004/EC-005/EC-006 and M27 remain open.

## Evidence

- EC-001/EC-002: `cargo test -p orbit-tui --offline` → 348 passed; 17 inspector tests
  (layout at 80x24/120x35/160x45, Enter/Esc/D, full-screen boundary, deleted
  entity, edit round trip, render purity, legacy session); PTY
  `python3 -m pytest -q tests/acceptance/tui` → 65 passed, 1 skipped (opt-in
  full fixture), including `test_m30_inspector.py` and `test_m30_capability_ui.py`.
- EC-003/EC-004: `cargo test -p orbit-storage -p orbit-runtime` → 105 + 527
  passed (offers availability matrix, request re-authorization, single claim,
  invalid input, revision mismatch, duplicate command, replay without a second
  note, executor failure, `claimed`/`uncertain` settlement, provider identity
  from the durable installation); `crates/orbit-storage/src/capability_installs.rs`,
  `crates/orbit-runtime/src/capability_composition.rs`.
- EC-005: `crates/orbit-tui/src/app/capability.rs` + 9 tests in
  `m30_capability_action_tests.rs`; inspector renders `CAPABILITY ACTIONS` and
  `RECENT CAPABILITY ACTIONS`; PTY `test_m30_capability_ui.py` drives menu →
  form → pending confirmation → `succeeded` → durable note verified through the
  CLI.
- EC-006: `python3 -m pytest -q tests/acceptance/test_m30_capability_workflow.py`
  → 1 passed against `target/debug/orbit`; CLI `blueprint capability`/`action`
  documented in `docs/reference/cli.md` and `docs/cli-contract.md`; connected
  refusal asserted by the same test and by TUI backend tests; no MCP capability
  tools exist and `docs/reference/mcp.md` says so.
- EC-007: the same acceptance test covers restart and replay; `cargo test
  --workspace --offline` → 1,623 passed across 29 test binaries; contracts 43,
  documentation 39.
- EC-008: `docs/guides/tui-workspace.md`, `docs/guides/blueprint-views.md`,
  `docs/guides/custom-blueprints.md`, `docs/guides/blueprint-walkthrough.md`,
  `docs/developers/extensions-and-capabilities.md`, `docs/reference/{cli,mcp}.md`,
  `docs/{cli,mcp}-contract.md`, `docs/m25-capability-executor.md`; every command
  example was executed against an isolated database. `getdone-validate-project`
  run recorded in the acceptance report.

## Remaining work

- I1–I4 are complete. M25 EC-001..EC-007 are now satisfied; capability execution
  remains local-authority only (connected capability execution is not part of
  M25).
- Connected capability execution, TUI capability status screens and MCP
  capability tools remain follow-ups, not M25 criteria.
- M27 public release verification remains externally blocked.

## Next milestone

MILESTONE-0030 iterations I2–I4 (connected MCP parity, previewable upgrades,
habit capability extraction), then remaining distribution work in MILESTONE-0027.
