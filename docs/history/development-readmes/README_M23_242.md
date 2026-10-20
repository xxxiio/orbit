# Development 242 — isolated launch and durable result handoffs

## Delivery

- `orbit agent start TASK --repository /absolute/path --worktree` requests a Herdr-managed worktree with a deterministic `orbit/...` branch. The execution option is stored on the job before preparing or claiming launch.
- `orbit agent handoff JOB --file result.json` records an immutable, structured handoff; `orbit agent handoffs JOB` retrieves those handoffs without connecting to Herdr. Required JSON object keys: `summary`, `validation`, `next_steps`. Optional fields include `commits`, `changed_files`, `decisions`, `blockers`, and `acceptance_criteria`.
- Existing jobs default to `worktree=false`; new SQLite schema v17 migrates v13–v16 workspaces, creating the new execution policy and handoff tables.
- Agent handoffs do **not** mutate task status and do not establish acceptance. Duplicate handoff identifiers with conflicting content fail closed.
- Commands remain exclusively under `orbit agent`. Herdr is optional for ordinary Orbit workflows.

## Verification limits

The execution environment lacks `cargo`, `rustc`, and `herdr`; therefore this package is *not compiled or live tested*. SQLite DDL and constraint smoke checks are run separately. No production-readiness or executable acceptance claim is made. This is **provisional source-scope milestone progress**, not accepted runtime progress.

## Remaining acceptance

Authenticated server-mode executor leasing, end-to-end runnable Herdr launch and state reconciliation, project catch-up, MCP integration and explicit acceptance review remain outstanding. Security boundary: the server must never run arbitrary remote shell commands.

## Milestone accounting

M23 prior reported source-scope estimate: 26%. This iteration adds isolated Git worktree creation and durable implementation handoffs. Provisional source-scope estimate: 32% (+6 points); Rust executable acceptance remains unverified, with M15 still pending.
