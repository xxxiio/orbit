---
template: milestone-plan
template_version: 2.0.0
template_digest: "737b1e57a6469c8843d3ed28294d21fe5a61750b2222084a7eaac19644fa392f"
project_owned: true
record_contract: milestone-plan
record_schema_version: 1
id: MILESTONE-0035
status: proposed
---

# Milestone: M35 — Orbit Self-Dogfooding: MCP, Workflows and Herdr

## Intended outcome

Orbit is used as the persistent project-management system for its own
development: its current milestones and tasks are registered in an "Orbit
Development" project, an authorized MCP client creates and updates real work
through Orbit, and a Herdr-managed coding agent completes one bounded task
through the full create → launch → evidence → in-Orbit review → explicit
accept/reject loop. The complete record of that work is present in a verified
workspace backup and survives an isolated restore.

## Why this milestone matters

M34 made workspace backup and recovery reliable, and MCP, agent-job and Herdr
interfaces exist throughout the tree. The project mission is for Orbit to manage
its own development, but interface existence is not proof of a working live
integration. M35 must exercise the real loop against actual MCP and Herdr
processes and record honestly what worked, what failed and what remains an
external boundary. It must not assume live integrations work because an interface
exists, and it must not start until the maintainer has reviewed the M34 completion
evidence and explicitly approved it.

## Scope

- Review M34 completion evidence, the `web/evidence/m34-recovery/` rehearsal and
  the documentation walkthrough; obtain explicit maintainer approval to start.
- Create an "Orbit Development" project in a persistent Orbit workspace and
  register the current milestones and tasks (including M34 completion and M35).
- Connect an explicitly authorized MCP client and create/update at least one real
  task through MCP, with the accepted change auditable in Orbit.
- Launch a Herdr-managed coding agent from an authorized Orbit agent-job record for
  one bounded, reviewable task, in an isolated branch/worktree.
- Collect the implementation and test evidence into Orbit and review the agent
  result in Orbit.
- Make an explicit accept or reject decision; agent completion never completes the
  task by itself.
- Verify the whole M35 record is present in a verified workspace backup and
  survives an isolated restore.
- Record honestly which integrations were exercised against actual processes
  versus interfaces only, including all external-boundary limitations.

## Non-goals

- Public-release qualification or any claim that live MCP/Herdr integrations are
  generally supported.
- TUI redesign; Docker/cloud/Redis/broker dependencies; multi-tenant work.
- Automatically completing Orbit tasks from agent completion, or auto-executing
  external actions after recovery.
- Reopening accepted M33/M34 scope, or editing the frozen M34 acceptance evidence.
- Expanding MCP tooling or Herdr adapters beyond what the dogfooding loop needs.

## Deliverables

- A persistent Orbit workspace and "Orbit Development" project with the current
  milestones/tasks registered from the real project records.
- Recorded MCP session evidence: authorization/scope, a created or updated task,
  the accepted command/receipt and a read-back in Orbit.
- A recorded Herdr agent-job lifecycle: authorized launch, bounded branch/worktree,
  monitoring, handoff and review, with no automatic task completion.
- Collected implementation and test evidence attached to the bounded task.
- An explicit review decision (accept or reject) recorded in Orbit.
- A verified workspace backup containing the M35 record, plus an isolated-restore
  proof that it survives.
- A journal/evidence record that separates live-tested behaviour from
  interface-only capabilities and lists every external limitation.

## Dependencies

- MILESTONE-0034 is complete (reliable workspace backup/restore, verified).
- Explicit maintainer approval after reviewing the M34 evidence and walkthrough
  (NEXT-0023). No M35 implementation before that approval.
- A persistent local workspace and an authorized MCP client allowed by the
  existing MCP policy.
- A configured, authorized Herdr runtime and a disposable repository/worktree with
  a bounded task whose acceptance can be reviewed.
- The existing agent-job, workflow and review surfaces; no new cloud dependency.

## Risks

- Live MCP or Herdr may be unavailable or behave differently from the implemented
  interfaces; record the failure honestly instead of substituting a fixture.
- A Herdr session may outlive or diverge from the Orbit job record; the external
  session/worktree/credentials are outside the backup and restore boundary.
- Dogfooding against the only persistent workspace is risky; take and verify a
  backup first and rehearse the recovery path before agent work.
- An agent may produce incomplete or incorrect work; the human review must be
  explicit and must not rely on agent self-reporting.
- MCP and Herdr are external boundaries: their processes, sessions, credentials
  and repositories are not restored by M34 recovery and must not be assumed live.

## Ordered implementation sequence

1. Maintainer walkthrough and review of the M34 completion evidence and
   documentation, then explicit approval to start M35 (NEXT-0023).
2. Prepare the persistent workspace: create the "Orbit Development" project,
   register the current milestones/tasks and take a verified backup.
3. Connect the authorized MCP client, create/update one real task through MCP,
   and confirm the accepted change and receipt in Orbit.
4. Create and launch a bounded, authorized Herdr agent job for one reviewable task
   in an isolated branch/worktree; monitor it and collect implementation/test
   evidence.
5. Review the agent result in Orbit and record an explicit accept or reject.
6. Verify the complete M35 record is in a workspace backup and survives an
   isolated restore; write the honest integration/limitations acceptance.

## Exit criteria

- [ ] EC-001: The maintainer has reviewed the M34 completion evidence and the
  documentation walkthrough and explicitly approved starting M35; no M35
  implementation work began before that approval.
- [ ] EC-002: A persistent Orbit workspace contains an "Orbit Development" project
  with the current milestones and tasks registered from the real project records,
  and a verified backup of it exists.
- [ ] EC-003: An explicitly authorized MCP client creates or updates at least one
  real task through Orbit; the accepted change and its receipt are readable and
  auditable in Orbit.
- [ ] EC-004: A Herdr-managed coding agent is launched from an authorized,
  bounded Orbit agent-job record and works in an isolated branch/worktree; the
  launch, monitoring, handoff and review states are recorded and agent completion
  does not complete the Orbit task.
- [ ] EC-005: Implementation and test evidence produced by the bounded task is
  collected into Orbit and reviewable there.
- [ ] EC-006: The reviewer records an explicit accept or reject decision in Orbit;
  the task state reflects the human decision rather than agent completion.
- [ ] EC-007: The complete M35 record — project, tasks, agent jobs,
  handoffs/reviews, receipts and activity — is present in a verified workspace
  backup and survives an isolated restore.
- [ ] EC-008: M35 acceptance states honestly which capabilities were exercised
  against actual MCP/Herdr processes and which remain interface-only or external
  boundaries; no claim rests on interface existence alone.

## Evidence

- EC-001: Not yet produced — awaiting the maintainer walkthrough/review and the
  explicit approval recorded by NEXT-0023.
- EC-002: Not yet produced — planned: registered project/milestone/task records in
  a persistent workspace plus a verified backup of it.
- EC-003: Not yet produced — planned: recorded MCP authorization, a real
  created/updated task, its accepted command/receipt and a read-back.
- EC-004: Not yet produced — planned: a real Herdr agent-job lifecycle transcript
  with bounded branch/worktree evidence and no automatic task completion.
- EC-005: Not yet produced — planned: implementation/test evidence attached to the
  bounded task and visible in Orbit.
- EC-006: Not yet produced — planned: an explicit review decision recorded in
  Orbit with the resulting task state.
- EC-007: Not yet produced — planned: a verified backup containing the M35 record
  plus an isolated-restore proof.
- EC-008: Not yet produced — planned: the final acceptance record separating
  live-tested behaviour from interface-only capabilities and external boundaries.

## Remaining work

- All exit criteria are unmet. M35 is proposed and has not started; implementation
  waits for maintainer approval (NEXT-0023). No live MCP/Herdr integration is
  claimed by this milestone record.

## Next milestone

None yet; M36 is not planned.
