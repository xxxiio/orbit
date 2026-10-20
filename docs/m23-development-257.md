# Development 257 — job-scoped MCP reporting (feature implementation)

This development pass begins closing the agent-facing MCP gap identified in the original M23 scope. The existing MCP server now exposes `agent.job_context` and `agent.handoff_submit` through the existing `orbit-mcp` executable, with no additional public CLI.

The job tools are **opt-in, job-bound**: the host must set `ORBIT_MCP_JOB_ID` for the specific launched job. Both tools fail closed if the identifier is missing or doesn't match the requested job, and enforce the existing MCP project allowlist. Handoff submission additionally requires developer mode, a nonempty idempotency key (`handoff_id`), a JSON object payload no larger than 64 KiB, and a nonnegative timestamp. Handoffs are persisted by `AuthoritativeHost`, not by an MCP-owned repository. They do not mark tasks complete. `agent.job_context` returns the durable task brief, previous handoff, review and prior session information.

Example MCP tool arguments:

- `agent.job_context`: `{"job_id":"job-123"}`
- `agent.handoff_submit`: `{"job_id":"job-123","handoff_id":"job-123-progress-01","at_ms":1792300000000,"payload":{"summary":"Work in progress","blockers":["Review needed"],"next_steps":["Review diff"]}}`

**Connected mode:** Both tools now use the authenticated `AuthorityHttp` transport, with the exact job's owner-checked resume-context and handoff endpoints; no direct replica writes, no shell execution, and no silent local fallback. Connected reporting deliberately fails closed when offline instead of claiming that an unaccepted handoff was accepted. The normal accepted-command outbox does not yet carry offline agent-job commands. Herdr existing-session reconnection and offline agent-command projection remain to be audited and implemented before M23 is called source-complete.

M23 remains at 97% *previously reported provisional source progress* for this partial scope pass; source completion has not been claimed. M15 owns broad Rust compilation, end-to-end validation, live Herdr checks, and fixes found by those checks. Local feature checks are retained here; no executable Rust acceptance is claimed.

## Live session continuation (local)

Added `orbit agent continue <job-id> --message "..."` for an already-confirmed, active, local Herdr agent. It targets the persisted Orbit-managed agent session and only invokes `herdr agent prompt` (never `agent start`). Unknown, disconnected, nonlocal, terminal and unconfirmed jobs fail closed; transport loss is reported as uncertain delivery, never auto-retried. The source has a Herdr fake-runner test for the prompt-only contract. For remote sessions, machine-side continuation authorization is still not implemented; the connected CLI explicitly rejects this command.
