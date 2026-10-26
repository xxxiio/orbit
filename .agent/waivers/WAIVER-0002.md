---
template: acceptance-waiver
template_version: 2.0.0
template_digest: "1c11836cf420861a2c2fad8499d4ad51b317263a4427b35ff940cdf74870c828"
project_owned: true
record_contract: acceptance-waiver
record_schema_version: 1
id: WAIVER-0002
status: accepted
task_id: null
---

# Acceptance Waiver

## Requirement

AGENTS.md requires `getdone-validate-project` to pass before completion may be
claimed for project-owned `.agent/` records.

## Reason

The validator cannot run in this environment. It aborts before reading any project
record with:

`error: .agent/skills-reference.md: [Errno 2] No such file or directory:
/home/xliu/miniconda3/lib/python3.12/site-packages/skill/bootstrap/manifests.json`

The installed `skill` package is missing its `bootstrap/manifests.json` manifest, and
the repository has no shared `skill/` checkout to point `--skills-root` at (the shared
skill pack is a separate checkout that is not present here). This is an environment
defect, not a defect in the M31 records: the failure occurs while resolving the skill
reference, before any milestone, journal, status or next-step record is parsed.

## Risk

Project-owned records for this milestone are not machine-validated, so a schema or
front-matter drift in `MILESTONE-0031.md`, the journal entries, `next-step.md` or
`project-status.md` would go unnoticed until the validator is available.

## Compensating control

- Every record was written against the checked-in templates in `.agent/milestones`,
  `.agent/journal` and `.agent/current`, preserving the template/version/digest and
  `record_contract`/`record_schema_version` fields verbatim.
- Records were reviewed by hand against the templates before commit, and the
  `id`/`status`/`milestone_id` cross-references were checked (MILESTONE-0031 ↔ journal
  entries ↔ NEXT-0013).
- `python3 -m pytest -q tests/documentation tests/contracts` passes (82 tests), which
  covers documentation navigation and contract shape within this repository.
- The validator command and its exact failure are recorded in
  `.slim/deepwork/m31-web-app/progress.md` and repeated in the I5 journal entry, so
  the gap is visible rather than silent.

## Owner

M31 orchestrator (single-maintainer project during internal testing).

## Expiry or removal condition

Remove this waiver as soon as either condition holds: the installed `skill` package
ships `skill/bootstrap/manifests.json`, or a shared `skill/` checkout becomes available
in this environment. At that point run
`getdone-validate-project --project-root /home/xliu/dev/orbit` and close this waiver.

## Follow-up TODO

Run the validator, fix any record drift it reports, and delete this waiver in the same
change.
