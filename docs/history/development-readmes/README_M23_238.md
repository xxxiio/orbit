> **Historical checkpoint only.** Not current CLI or deployment instructions. See the [root README](../../../README.md) and [current architecture](../../architecture.md).

# Development 238 — Herdr execution vertical slice (source-only)

## Delivered

- Optional `HerdrRuntime` adapter with a swappable `HerdrCommandRunner` test seam.
- Documented Herdr CLI shape: `--machine`, `workspace create`, `agent start`, `agent prompt`, and `agent get`.
- Deterministic session naming derived from the durable launch claim token.
- A machine-side `dispatch_prepared_job` coordinator: claim first, invoke once, record confirmed/ambiguous outcomes.
- Opt-in `orbit-agent-executor` binary for **fully local authoritative** SQLite mode only.
- Unit-test source for remote machine routing, uncertain transport failure, and rejected agent kinds.

## Local smoke workflow (requires Rust and Herdr, not verified in this environment)

1. Create a task in an authority database, reserve an agent job, and accept the `prepare` transition.
2. Run `cargo test -p orbit-runtime` and `cargo build -p orbit-runtime --bin orbit-agent-executor`.
3. Ensure `herdr` and `codex` or `opencode` are installed on the target machine; register remote Herdr machines explicitly if needed.
4. Invoke `orbit-agent-executor --local-authority <db> <job> <prepared-revision> <unique-claim-token> <current-epoch-ms>`.
5. Inspect the resulting Herdr agent; repeated invocations with the same claim do not start another agent.

## Acceptance gaps / safety boundaries

- **Not a validated end-to-end workflow:** No Rust toolchain or Herdr executable was available here.
- No GUI `Start with Agent` action, authenticated executor lease, remote command dispatch service, or connected-mode replica projection yet.
- No automatic Git worktree creation or resumption yet; the current adapter starts in the requested existing checkout.
- The adapter must run only on an authorized machine-side executor. Do not expose the binary as an unauthenticated server action or run against an offline client replica.
- Once workspace creation is attempted, every subsequent error is ambiguous, and the durable claim must not be replayed.
- Herdr `idle`/`done` states do not establish task completion; review and acceptance remain separate.

## Progress

M23 previously reported at 21% (provisional). Development 238 adds a substantial source slice, but no new milestone percentage is credited before compilation and workflow acceptance. M23-I2 remains open. Internal-testing readiness: **not established**.
