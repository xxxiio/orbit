---
template: project-agents
template_version: 2.3.0
template_digest: "bd89549bf0bd53d7e5fc031ffa3196ef63040a3ac425f4d0108b8337e225c8bb"
project_owned: true
---

# Project Agent Instructions

Before development:

1. Read `.agent/skills-reference.md` and verify `.agent/skills.lock.json`.
2. Read `skill/START-HERE.md` from the shared checkout.
3. Classify the implementation through `skill/workflow-router.md`, including whether the
   request uses task workflow or project workflow.
4. Always read stable project context and command guidance relevant to the work.
5. In task workflow, do not read or update current-task, roadmap, plan, status, handoff,
   or next-step records unless the request specifically depends on them.
6. In project workflow, read and maintain the current/planning/continuation records
   required by the active goal, including `.agent/current/next-step.md`.
7. Load only the selected recurring guidance; load policies, references, and prior journal
   entries only when their trigger or historical relevance applies.
8. Record completed work and durable findings in the journal in both workflows.

Treat the shared skill pack as read-only unless the task explicitly changes it.

Project-owned `.agent/` records follow the controlled project-record contract in the
shared skill pack. Run `getdone-validate-project` before claiming completion.
