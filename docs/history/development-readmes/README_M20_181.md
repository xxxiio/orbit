# Orbit 181 — Project bootstrap workflow

## Selection and priority
The existing embedded/remote command foundation can create individual projects and tasks, but creating a usable project with its initial task plan required callers to manually sequence revisions, prepare retry-stable command IDs, and detect obvious invalid manifests after partial commits. A complete project launch is a core daily workflow and takes precedence over independent hardening, release tooling, and additional dashboards.

## Delivered
- `ProjectBootstrap` and `BootstrapTask` provide one typed user-facing project launch manifest.
- `into_workflow` rejects invalid project and task IDs, empty names, oversized inputs, duplicate task IDs, excessively large manifests, command-ID overflow, and revision overflow **before any side effects**.
- Generated stable command identities and consecutive expected revisions allow the existing authoritative domain command engine to commit project and initial tasks in order or queue them durably on remote replicas.
- `AuthorityConnection::bootstrap_project` exposes one operation through embedded and remote clients, preserving `WorkflowFailure`'s partial-commit reporting. It does not claim atomicity.
- Rust behavioral tests cover embedded launch, retry receipts, accepted-state revision, remote pending isolation and preflight failure. These tests are added but could not be executed because Cargo/rustc are missing.

## Important limits and follow-up
Remote launch returns pending until the self-hosted authority accepts each command. An interrupted remote/embedded launch may have an accepted prefix: callers must inspect the returned `WorkflowFailure` and retry with identical command IDs. This workflow is not an alternative authority: it composes existing canonical project and task commands. The next feature priority remains rich-domain planning, routines, schedules, strategy, and milestones transport.

## Validation and readiness
Python source-contract smoke passed; Rust compilation/integration and two-client acceptance remain unverified. Do not declare internal-testing ready until a runnable build, core smoke workflows and no known blocking data-loss issue are established. Flutter remains deferred to Atomic App.

## Provisional progress
M20: 77% implementation estimate (previous 75%). This is not a verified acceptance percentage. M15 8%, M18 99%, M19 99%, M21 65%, M22 10% remain unchanged and provisional. No higher-priority unfinished category was displaced by this iteration.
