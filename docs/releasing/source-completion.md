# M26 source-completion handoff

This guide summarizes the **implemented source interfaces**, not a certified production release.

## Choose an installation

1. **Local CLI:** `./scripts/install.sh` builds from Rust source. No server, Python, Herdr or Docker needed to operate local mode.
2. **Prebuilt archive:** obtain a trusted platform-specific archive **and** its matching `.sha256`; run `./scripts/verify-release.sh ARCHIVE` then `./scripts/install.sh --from-archive ARCHIVE`. Checksums alone cannot establish publisher authenticity.
3. **Connected deployment:** follow [self-hosting](../guides/self-hosting.md); configure authentication and TLS deliberately. Local client data is not the connected source of authority.
4. **Agent worker:** follow [worker deployment](../guides/worker-deployment.md). Herdr and Python are optional and must be configured explicitly.

## First-run path

- `./scripts/orbit-env.sh --doctor` inspects a local installation without changing state.
- `orbit --help` shows commands implemented by the installed version.
- [Local quickstart](../getting-started/quickstart.md) covers projects/tasks.
- [Agent workflow](../guides/agent-workflow-tutorial.md) and [Blueprint walkthrough](../guides/blueprint-walkthrough.md) cover extended workflows.

## Release acceptance status

Linux local/loopback/local-Herdr M15 acceptance has passed: the full automated
profile reports 37/37 checks (38/38 with opt-in live Herdr) (0 failed, 0 not run) and the Rust suite reports
1235 passing tests, including a real release-archive build/verify/install smoke.
These results cover a Linux x86_64 host using local SQLite, loopback connected
mode, and a local Herdr daemon.

The following remain separate, currently unverified gates: cross-platform
builds (macOS/Windows), TLS/self-hosted ingress (Docker/Caddy), remote-machine
agent execution, artifact signing, and real version-to-version upgrade testing.
Treat them as open work, not as implied by the local pass. For exact gates use
the [release checklist](release-checklist.md).
