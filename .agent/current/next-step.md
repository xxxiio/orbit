---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0023
status: ready
milestone_id: MILESTONE-0035
advances_exit_criteria: [EC-001]
---

# Next Deterministic Step

## Objective

Give the maintainer a bounded human walkthrough and review of M34's completion
evidence — the `web/evidence/m34-recovery/` rehearsal and failure-injection records,
the operator documentation, and the running backup/restore experience — and obtain
an explicit approval (or rejection with findings) to start MILESTONE-0035. No M35
implementation work starts before that decision.

## Why this is next

M34 is complete and accepted as internal engineering acceptance: all fifteen exit
criteria pass on recorded evidence, including a 14/14 real recovery rehearsal and
a 22-pass/1-NA failure-injection matrix. M35 (Orbit self-dogfooding with MCP,
Workflows and Herdr) is proposed but deliberately gated on maintainer review,
because it will exercise live external processes whose interfaces existing is not
proof. The next valid action is therefore the human review/approval decision, not
implementation. This is also MILESTONE-0035 EC-001 and its precondition.

## Preconditions

- MILESTONE-0034 is completed with EC-001..EC-015 passing on recorded evidence.
- MILESTONE-0035 is proposed in the roadmap; no active milestone exists.
- `web/evidence/m34-recovery/README.md`, `rehearsal-evidence.json` and
  `failure-injection.json` are present and consistent with the records.
- The documentation set is updated and both documentation suites pass.
- A disposable copy of a workspace and the built `orbit` / `orbit-server` binaries
  are available for a short live demo; the real workspace is not required.

## Inputs

- `.agent/milestones/MILESTONE-0034.md`, `.agent/milestones/MILESTONE-0035.md`.
- `.agent/current/acceptance.md`, `.agent/current/evidence.md`,
  `.agent/journal/2026-10-31-m34-completion.md`.
- `web/evidence/m34-recovery/README.md`, `rehearsal-evidence.json`,
  `failure-injection.json`.
- `docs/guides/backup-and-recovery.md`, `docs/web-app.md` Backup & Restore,
  `docs/releasing/upgrades-and-recovery.md`, `docs/reference/cli.md`.
- `target/debug/orbit` and `target/debug/orbit-server` from the frozen tree.

## Ordered actions

1. Read the M34 completion evidence and the acceptance/evidence records; confirm
   the exit-criteria mapping and the explicitly recorded limitations.
2. Walk through the operator documentation against the shipped CLI and the
   `/settings/backup` page: create, list, inspect, verify, isolated restore,
   replacement plan and automatic-backup policy.
3. Run a short live demonstration on a disposable workspace: create a labeled
   backup, verify it, restore it into an isolated directory, open the restored
   instance, and show the preserved-copy/recovery-journal behaviour and the
   admin-only offline replacement plan.
4. Review `web/evidence/m34-recovery/` directly (rehearsal dataset, counts, hashes
   and failure-injection results) and the recorded limitations (replica snapshots
   refused, connected pending data excluded, download cap, offline replacement,
   free-space measurement, helper-level update evidence).
5. Record the maintainer decision: explicitly approve M35, or reject with concrete
   findings. If rejected, keep M35 proposed and create a follow-up step/records
   change instead of starting implementation.
6. On approval, activate MILESTONE-0035 (status `active`, roadmap
   `current_milestone: MILESTONE-0035`) and create its first implementation step;
   do not begin MCP/Herdr work before that.

## Expected outputs

- A written maintainer review decision on M34 and M35 (approve/reject with
  findings), recorded in a journal entry and reflected in the milestone status.
- Any review-driven record corrections (no implementation claims before approval).
- If approved: MILESTONE-0035 active with a first deterministic implementation step
  (persistent "Orbit Development" project and record registration).

## Acceptance criteria

- [ ] NS-001: The maintainer has seen the M34 evidence (rehearsal, failure
  injection, docs, CLI/Web backup-restore walkthrough) and the decision is recorded
  in a journal entry with concrete findings, if any.
- [ ] NS-002: No M35 implementation work or MCP/Herdr integration claim exists
  before the approval decision.
- [ ] NS-003: On approval, MILESTONE-0035 is `active` and the roadmap
  `current_milestone` matches it; on rejection, M35 stays `proposed` with the
  findings recorded and a corrective step created.
- [ ] NS-004: Records remain structurally valid (`getdone-validate-records` and
  `getdone-validate-project` pass) after the decision is recorded.

## Validation

```sh
cd /home/xliu/dev/orbit
target/debug/orbit --version
target/debug/orbit backup create --label "m34-walkthrough" --db /tmp/m34-walkthrough/orbit.db
target/debug/orbit backup verify /tmp/m34-walkthrough/backups/NEWEST.orbitbak
target/debug/orbit backup restore /tmp/m34-walkthrough/backups/NEWEST.orbitbak --dry-run --to /tmp/m34-walkthrough/restored
python3 -m unittest discover -s tests/documentation
python3 -m pytest -q tests/documentation
getdone-validate-records --project-root /home/xliu/dev/orbit --skills-root /home/xliu/dev/getdone
getdone-validate-project --project-root /home/xliu/dev/orbit --skills-root /home/xliu/dev/getdone
```

## Stop conditions

- Stop if M34 evidence is incomplete, contradictory or not reproducible on the
  frozen tree; fix the record/evidence gap before asking for approval.
- Stop and do not activate M35 if the maintainer rejects or asks for changes;
  record the findings and create a corrective step instead.
- Do not start MCP/Herdr integration work in this step; it begins only after
  approval.

## Out of scope

- Any M35 implementation (persistent project setup, MCP client connection, Herdr
  launch, agent work, review execution) — that starts only after approval.
- Re-running or amending the frozen M34 acceptance evidence; M34 is complete.
- Public-release qualification, TUI redesign or broader integration work.
