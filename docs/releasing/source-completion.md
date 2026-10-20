# Source-completion handoff

This page summarizes implemented interfaces, not a certified production
release. See the [documentation index](../README.md) for the user guides.

## Choose an installation

1. **Published-style binary install:** use the root installer:
   `curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh`.
   No Rust, Python, Herdr or server is required.
2. **Offline archive:** obtain a trusted archive and its matching `.sha256`;
   run `./scripts/verify-release.sh ARCHIVE` then
   `./scripts/install.sh --from-archive ARCHIVE`.
3. **Source CLI:** `./scripts/install.sh` builds with Cargo and installs the
   CLI. Optional components use `--components cli,mcp,tui,server`.
4. **Connected deployment:** follow [self-hosting](../guides/self-hosting.md).
   Server SQLite is authoritative; client SQLite is a replica plus outbox.
5. **Agent worker:** follow [worker deployment](../guides/worker-deployment.md).
   Herdr and Python are optional and must be configured explicitly.

## First-run path

```sh
orbit init
orbit doctor
orbit project add "First project"
```

- [Initialization](../getting-started/initialization.md) covers the layout,
  configuration and precedence.
- [Local quickstart](../getting-started/quickstart.md) covers projects and tasks.
- [Agent tutorial](../guides/agent-workflow-tutorial.md) and the
  [blueprint walkthrough](../guides/blueprint-walkthrough.md) cover extended
  workflows.
- `orbit update --check` inspects updates without mutating anything.

## Release acceptance status

Linux x86_64 local, loopback-connected and local-Herdr M15 workflows are the
only runtime-verified scope. That includes a real release-archive
build/verify/install smoke. The GitHub workflow creates draft releases and
requires manual authorization to publish; no public release is claimed here.

The following remain separate, unverified gates: macOS and Windows runtime
behavior, aarch64 runtime behavior, TLS ingress, remote-machine agent execution,
artifact signing, and real version-to-version upgrades. Treat them as open work,
not as implied by the Linux pass. For exact gates use the
[release checklist](release-checklist.md).
