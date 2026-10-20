# Orbit documentation

Start with the task you want to accomplish. Orbit's Rust CLI runs locally with SQLite; a server, Herdr, machine worker and Python are optional.

## New users

- [Installation](getting-started/installation.md) — local CLI and optional binaries
- [Verified release archives](releasing/binary-distribution.md) — offline binary bundles and checksums
- [Platforms and components](releasing/platforms-and-components.md) — requirements and unverified targets
- [Source completion and first-run handoff](releasing/source-completion.md) — installation choices and validation boundaries
- [Release checklist](releasing/release-checklist.md) — evidence-driven distribution readiness
- [Local quickstart](getting-started/quickstart.md) — complete local CLI tutorial
- [Connected mode onboarding](getting-started/connected-mode.md) — authority, enrollment and offline replicas
- [Your first project](getting-started/first-project.md) — create and inspect work
- [Installation components](guides/installation-components.md) — choose only what you need
- [Troubleshooting](guides/troubleshooting.md) — common failures and diagnostics

## Everyday workflows

- [Research project workflow](guides/research-workflow.md) — research tasks and optional blueprint
- [Projects, tasks and planning](guides/projects-and-planning.md)
- [Blueprints versus tasks](guides/blueprints-vs-tasks.md)
- [Custom workflow walkthrough](guides/blueprint-walkthrough.md)
- [Agent execution](guides/agent-workflow-tutorial.md)
- [Review, continue and resume agents](guides/agent-review-and-resumption.md)
- [Offline mode and synchronization](guides/offline-operation.md)

## Optional connected and agent operation

- [Self-hosting](guides/self-hosting.md)
- [Optional machine worker](guides/worker-deployment.md)
- [MCP integration](guides/mcp-integration.md)

## Reference and extension

- [CLI reference](reference/cli.md)
- [MCP reference](reference/mcp.md)
- [Trusted provider example](developers/capability-provider-example.md)
- [Upgrades and recovery](releasing/upgrades-and-recovery.md)
- [Capability/provider authoring](developers/extensions-and-capabilities.md)
- [Contributing and testing](contributing/testing.md)

Historical milestone notes remain in the repository for provenance; they are not a replacement for current user guides. Verify specific command options using the installed binary's `--help` when working across versions.

## Verification status

See [the roadmap and verification gates](roadmap.md) for milestone state and the
full list of open gates. In short: Linux **local**, **loopback connected** and
**local-Herdr** agent workflows are verified on Linux x86_64 by the M15 profile
(37/37 checks, 38/38 with opt-in live Herdr, 1235 Rust tests, including a real release-archive
build/verify/install smoke). TLS/self-hosted ingress (Docker/Caddy),
cross-platform (macOS/Windows) targets, remote-machine agent execution,
artifact signing, and real version-to-version upgrades are **not yet verified**.
Nothing here claims public-release readiness.

- [Installation inspection and local directory preparation](getting-started/installation.md#prepare-and-inspect-a-local-installation)

For local installation failures, start with [`orbit-env.sh --doctor`](getting-started/installation.md#diagnose-a-local-installation).
