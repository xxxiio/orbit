---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0008
status: blocked
milestone_id: MILESTONE-0027
advances_exit_criteria: [EC-001, EC-002]
---

# Next Deterministic Step

## Objective

Run the authorized release workflow and verify the real GitHub-hosted installer
and update path against published release assets.

## Why this is next

M28 functional recovery is complete and the local/loopback profile passes 42/42.
The only remaining unfinished milestone work is M27's real release verification,
which is blocked by maintainer authorization and write access.

## Preconditions

- A maintainer with write access explicitly authorizes a draft/test release.
- The release workflow is merged on `main`.
- The local account has release permissions or a maintainer runs the workflow.

## Inputs

- `.github/workflows/release.yml`
- `install.sh`
- `scripts/release/validate-release-version.sh`
- `docs/releasing/release-checklist.md`

## Ordered actions

1. Obtain authorization and a write-capable environment.
2. Push the source and create an approved test tag.
3. Confirm the gate and native builds produce all canonical archives.
4. Verify checksums/provenance on the draft release.
5. Run the curl installer and `orbit init`/`doctor`/`update --check` on Linux.
6. Record evidence without claiming macOS or stable-public readiness.

## Expected outputs

- A real GitHub Actions run and draft release.
- A verified install/update transcript and updated M27 evidence.

## Acceptance criteria

- [ ] NS-001: The authorized workflow passes and produces all assets.
- [ ] NS-002: Installed binary and update path work against the real release.
- [ ] NS-003: Evidence separates fixture acceptance from public readiness.

## Validation

```sh
gh release view TAG --repo xxxiio/orbit
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
~/.local/bin/orbit init
~/.local/bin/orbit doctor
~/.local/bin/orbit update --check
```

## Stop conditions

- Stop without authorization or write access.
- Stop if any release gate or native build fails.
- Do not publish a stable release without separate approval.

## Out of scope

- M28 TUI work.
- macOS runtime verification without a runner.
- Windows, signing keys or hosted documentation.
