# Development 258 — Herdr remote session continuation

## Scope delivered

In fully local-authoritative Orbit mode, `orbit agent continue JOB --message ...` now routes to the machine recorded on the job. An existing Orbit-managed Herdr session can therefore be prompted on an explicitly configured remote Herdr machine, without allocating a new session or creating an agent job. The Herdr runtime validates the machine selector and session identity, and sends one `agent prompt` command; ambiguous delivery is **never** retried automatically. Server-authoritative remote continuation requires an executor-issued prompt protocol and remains out of scope here.

## Verification

* `python -m unittest discover -s deploy/server -p 'test_agent_machine*.py' -q`: 28 tests passed.
* New Rust unit test exercises `--machine buildbox agent prompt ...` routing and invalid-machine rejection, but Rust tests have **not** been run (Cargo is unavailable).
* No live Herdr smoke test was performed (M15 acceptance).

## M23 remaining scope

* Authenticated connected-mode prompt requests consumed by the machine worker, with idempotent delivery semantics and acknowledgement ambiguity.
* Offline-compatible pending-command submission/projection for agent operations.
* Confirm CLI/MCP feature parity and roadmap acceptance criteria before declaring M23 complete.

Implementation-scope percentage remains **97% provisional**. This is a targeted implementation change, **not** a completed >=5-point milestone iteration; do not count it as one. Broad compile/integration/live-run acceptance and resulting fixes belong to M15.
