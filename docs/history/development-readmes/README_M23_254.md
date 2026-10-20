# Orbit Development 254 — Machine recovery and HTTP contract acceptance

## Priority decision
M23 remains the active milestone after Development 253 (87% provisional).
The next highest-value feature gap is reliable connected execution across
restarts and authority report failures, not new APIs, multi-agent scheduling,
or standalone hardening. Work here closes the operator monitoring/recovery slice
and expands executable protocol-double coverage; M15 release acceptance remains
separate. No higher-priority unfinished user-facing category should precede
establishing this launch-to-monitor workflow.

## Changes
- Recover `report_pending` launch receipts after a lost HTTP acknowledgment,
  using the original claim token and session identity before observations.
- Persist ambiguous Herdr failures even if reporting to the authority fails;
  allow idempotent ambiguous-result replay without relaunching.
- Introduce a per-journal-directory exclusive machine-worker process lock.
- Add an offline, deterministic journal health/attention snapshot.
- Add local HTTP tests exercising actual JSON requests, authentication headers,
  inbox, claim, launch result, observation, and recovery from a lost report.

## Verification
`python -m unittest discover -p 'test_agent_machine*.py' -q`:
18 tests passed (including four HTTP protocol-double tests and journal tests).
`python -m py_compile deploy/server/agent-machine-worker.py`: passed.
Rust compiler and Cargo unavailable; NO Rust build, real server integration,
or live Herdr launch was tested. The HTTP double checks only Python-side wire
behavior and MUST NOT be represented as Rust endpoint acceptance.

## Milestone progress
M22: 100% source scope (unchanged); M23: 87% previously reported provisional
implementation, **not advanced** by source-only checks; M15: pending.
This is a feature implementation and executable worker test increment, but
without exercising the Rust server and Herdr there is not sufficient evidence
to claim another >=5 accepted milestone points. Do not declare I5 closed.

## Highest-priority remaining work
Build the Rust workspace and run authenticated server/client smoke flows;
validate server idempotency of replayed `launch-result`; verify the exact Herdr
CLI JSON and worktree contracts on a real machine; then complete the full
start → monitor → handoff → review acceptance flow.

## Internal testing
Not ready. Requires a runnable Rust build, core smoke workflows and no known
blocking data-loss issue. Unsafe ambiguous job retry remains deliberately
operator-mediated, not silently automated.
