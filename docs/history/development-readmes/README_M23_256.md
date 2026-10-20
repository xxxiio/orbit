# Orbit Development 256 — Machine execution lifecycle and attributable Git evidence

## Scope

This iteration finishes a measurable **machine-worker contract acceptance slice** for
M23: a real HTTP transport to an explicitly simulated authority, a real fake-Herdr
subprocess, real Git repositories, durable launch journal, status reconciliation,
and handoff receipt retries. These tests do **not** exercise the Rust server or
an installed Herdr runtime.

### Delivered

- End-to-end worker contract fixture executes inbox → claim → Herdr workspace/start/prompt
  → launch result → idle observation → persisted Git evidence → review-needed response.
- Lost handoff acknowledgments replay the exact original immutable submission.
- Machine identity checks and subsequent polling prevent duplicate launches.
- The worker journals pre-launch `HEAD` when Git history is available; evidence now
  reports commits and changed filenames since that baseline rather than up to 20
  unrelated historical commits. The baseline is optional for non-Git/legacy jobs;
  in that case new-commit attribution is intentionally unavailable.
- Changed filenames include both committed changes since launch and the current
  working-tree status. Rewritten histories are rejected rather than falsely attributed.
- Handoff idempotency identifiers now incorporate the Orbit job identity, avoiding
  cross-job collisions when two jobs have identical repository snapshots.
- CI contract suite runs machine worker and simulated-protocol tests on changes.

### Validation executed

`python -m unittest discover -s deploy/server -p 'test_agent_machine*.py' -q`

Result: 28 tests passed, including new real-HTTP, subprocess and Git fixtures.

### Outstanding

Rust compilation and tests; Rust server HTTP contract integration; live Herdr smoke;
connected-mode end-to-end start/review acceptance. No task was auto-completed.
Internal testing readiness is **not established**.

### Progress assessment

M23 provisional feature implementation: **92% → 97%** (+5 percentage points)
for end-to-end machine-worker contract and baseline-scoped evidence, **not**
end-to-end runtime acceptance. M22 source implementation stays 100%; M15 release
acceptance remains pending. The remaining 3% is *not* considered complete until
Rust + real Herdr acceptance is demonstrated.
