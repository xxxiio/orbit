# Orbit — Current Roadmap

Updated: 2026-10-20. This is the **product and implementation roadmap**, not a
second progress ledger. GetDone records under `.agent/` own detailed milestone
status, work items, acceptance evidence, and next steps. Historical milestone
notes remain under `docs/history/`; do not create milestone-specific `README_M*.md`
files.

## Product direction

Orbit is a **local-first, agent-aware, extensible project workflow framework**.
It supplies stable projects, work-item identities, activity history,
dependency/authority rules, agent-job lifecycles and capability execution; users
design project variations through reusable **Project Blueprints**. Development is
the default bundled blueprint, while learning, habits and personal workflows are
optional blueprint/capability packages. A blueprint does not require a compiled
plugin. Plugins extend the set of available capabilities, actions, validations,
and specialized views.

**A project is an instance; a blueprint defines its work-item schemas, states,
transitions, defaults, views, and permitted actions; a capability/plugin
implements optional behavior.** Do not force every recurring occurrence, lesson,
or agent session into a generic task. Preserve strongly typed core identity,
consistency, and security boundaries; validate custom property values against
versioned schemas.

## Verification snapshot (2026-10-20)

The M26 packaging/documentation package was integrated into the repaired M15
source tree. The incoming M26 Rust source was an older, unformatted snapshot that
did not compile; the local Rust source and its M15 fixes were preserved. The
current tree was validated independently of the package's claims:

- `cargo fmt --all -- --check` — pass.
- `cargo test --workspace --locked` — **1,235 passed, 0 failed** (27 targets,
  including doc-test targets).
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — pass.
- `./scripts/m15-full-automated.sh --profile full` — **37 passed (38 with opt-in live Herdr), 0 failed,
  0 not_run** (full profile, including packaging/documentation checks and a real
  release-archive build/install smoke).
- Live Herdr acceptance — pass with Herdr 0.9.5 and a real `opencode` agent in a
  disposable workspace (launch, monitoring, Git evidence, handoff, review, no
  automatic task completion).

| Milestone | Source implementation | Executable acceptance | Remaining work |
|---|---|---|---|
| **M22** Planning | Complete and compiled: planner, portfolio, scenario, planning briefs, handoffs, CLI/TUI/MCP/server surfaces | Partial: planning metadata and schedules are covered by the local smoke; 34 source-contract tests; no dedicated portfolio/scenario/approval end-to-end test | Add executable portfolio, scenario and approval coverage; exercise connected planning paths |
| **M23** Agent-aware execution | Complete and compiled: agent jobs, authority modules, Herdr adapter, handoffs, outbox, server routes, machine worker scripts | Local path live-verified (opencode + Herdr 0.9.5); deterministic fake-runtime contract; 46 worker tests | Remote machine-worker full cycle against the real Rust server; `codex` path; live resume/continue; Herdr version matrix |
| **M24** Architecture consolidation | Complete: one authoritative host, split authority modules, client outbox vs machine journal, worker transport | Compiles, 1,235 tests, fmt/clippy clean; historical milestone scripts moved to `scripts/legacy-contracts/` and excluded | Maintain module boundaries; do not reintroduce provider-specific business rules |
| **M25** Project Blueprints | **Partial**: I1/I2 complete; I3 materially incomplete; I4 partial; I5 partial | Local + connected blueprint smoke passes (install, binding, typed properties, guards, templates, offline replay, receipts, restart) | Capability installation/action request/execute; connected MCP outbox/receipt parity; blueprint upgrade path; habit recurrence extraction; lost-ack blueprint E2E |
| **M26** Packaging, installation and documentation | Implemented: install/uninstall, release packaging/verification, environment doctor, installation diagnostics, user/release/CLI/MCP documentation, packaging regression tests | **Executable-verified** for unsigned Linux x86_64 archives: synthetic packaging suite, real CLI build/package/verify/install/uninstall, documentation contracts | Distribution signing/authenticated publishing; cross-platform packaging; optional MCP/TUI/server launch smoke; installed-version upgrade test |
| **M15** Release acceptance | Acceptance harness and workflows (not a feature source) | **Executable-verified** for the Linux local/loopback/local-Herdr scope: 38/38 with live Herdr (37/37 base full profile), packaging smoke | TLS/Caddy ingress, browser deployment, remote-machine Herdr, macOS/Windows, signing, real version-to-version database upgrade |

## Sequence and priorities

1. **M25-I3 capability actions (highest priority).** Make the already-written
   trusted capability path reachable: trusted host composition supplies installed
   capabilities and grants; add an explicit action request/status surface; compose
   the local `agent.launch` adapter only when configured; prove claim/result,
   duplicate replay, refusal, and ambiguous-outcome fencing with a deterministic
   fake runtime. This is wiring, not a new framework.
2. **M25-I5 parity and upgrade.** Add the connected server action route, route
   connected MCP writes through the canonical outbox/receipts, add an explicit
   blueprint upgrade/preview operation, and exercise a lost-acknowledgement
   blueprint replay end to end.
3. **M25-I4 domain extraction.** Move habit recurrence/streak behavior behind an
   opt-in capability package without forcing every occurrence into a core task.
4. **M15 deployment gates.** TLS ingress, remote-machine execution, cross-platform
   packaging, signing, and real upgrade testing. These are release gates, not
   blockers for local or loopback internal testing.
5. **Later, un-numbered:** Visual blueprint designer in the separate Atomic App
   Flutter project; third-party plugin packaging, isolation and marketplace.
   Deferred until headless contracts and the capability boundary are stable.

## M25 — Configurable Project Blueprints

Goal: users can design a project variation without writing Rust, create a project
from it, and have the **same versioned workflow rules** enforced in both
local-authoritative and self-hosted server-authoritative modes. The first
implementation is headless: Rust engine, CLI/MCP, server contracts. Existing
development behaviors remain available as default blueprints while personal/habit
features become opt-in packages. No cloud requirement.

| Iteration | Status | Evidence / remaining work |
|---|---|---|
| **I1: Blueprint model and registry** | Complete | Versioned manifests, registry, project binding, export/import, validator; blueprint crate tests and local/connected smoke |
| **I2: Typed properties and lifecycle** | Complete | Typed fields, states, transitions, guards, templates, event history; smoke covers typed properties, guards, templates, capability refusal |
| **I3: Composable capabilities and safe actions** | **Partial** | Local CLI install/authorize/request/execute/reconcile and the deterministic reference capability are implemented, including the installation gate on publication (M25 EC-003). Connected parity (EC-004), previewable upgrades (EC-005) and habit extraction (EC-006) remain open; MCP exposes no capability tools. |
| **I4: Bundled packs and domain extraction** | Partial | Four manifests exist; habit recurrence/streak calculation remains core, not an opt-in capability package |
| **I5: Public headless surfaces** | Partial | CLI and HTTP blueprint surfaces plus receipts exist; connected MCP writes bypass the outbox/receipts; no blueprint upgrade operation; lost-ack blueprint replay lacks an E2E test |

Design constraints remain unchanged:

- **Blueprints vs plugins:** a blueprint is declarative and user-editable; a
  plugin registers new optional capabilities. `development`, `learning`, and
  `habit` are configurations, not hardcoded `ProjectKind` branches.
- **No over-generalization:** retain core Project and WorkItem identifiers,
  relationships and event histories; model recurrence occurrences, assessments,
  sessions, and Git evidence with appropriate extension data/services instead of
  coercing everything to Task.
- **Versions:** immutable published blueprint versions; projects bind to a
  version; upgrades are explicit, previewable and atomic. No silent remapping.
- **Guards:** deterministic, restricted declarative rules; no arbitrary
  YAML-embedded shell commands. Extensions run only via registered,
  permission-checked capabilities.
- **Authority:** local SQLite is canonical in local mode; server SQLite is
  canonical in connected mode. Client-side previews are not accepted state.
  Existing offline command ordering and idempotency remain authoritative.
- **Agent execution:** generic AgentJob/authorization/handoff remains core; Herdr
  stays optional. Agent completion never automatically closes a work item.
- **Privacy/deployment:** local-first, no mandatory hosted registry, cloud AI or
  remote plugins. Optional built-in packs can be disabled.

## M15 — Release acceptance evidence

Current Linux x86_64 acceptance run (2026-10-20, Rust 1.99, Python 3.12):

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
./scripts/m15-full-automated.sh --profile full
python3 deploy/server/smoke-herdr-live.py --orbit target/debug/orbit --confirm-live --timeout 180
```

- Full profile: **37/37 checks passed (38/38 with live Herdr)**, including Rust format/check/test/clippy/
  build, Python worker and contract suites, documentation contracts, installation/
  release-archive contracts, a real release-archive build/install smoke, local
  CLI persistence, M25 blueprint checks, server task/domain smokes, offline
  replica replay, server durability, blueprint workflows, and the deterministic
  agent-execution contract.
- The final evidence bundle was produced with `--live-herdr` and contains 38
  checks; a base full profile without live Herdr contains 37.
- The earlier Development 298 narrative reported 27/27 and 1,232 tests before the
  M26 packaging/documentation checks and real-archive smoke were added. Those
  counts are superseded; the historical note remains in `docs/history/`.

Still unverified for M15: TLS/Caddy ingress and browser deployment (Docker is
unavailable here), remote-machine Herdr through `agent-machine-worker.py` against
the real Rust server, macOS/Windows/cross-compiled targets, distribution signing,
and a real installed-version database upgrade.

## Internal-testing readiness

| Scope | Status | Basis |
|---|---|---|
| Local internal testing | **Ready** | Runnable build and installed archive; local CLI persistence with restart; 1,235 Rust tests; 38/38 acceptance profile with live Herdr; no known blocking data-loss issue |
| Connected internal testing | **Ready on loopback HTTP only** | Server task/domain/durability/offline/blueprint smokes pass; TLS/self-hosted network ingress is untested; connected model is single-user trust (any enrolled device can call mutations) |
| Agent-enabled internal testing | **Ready for local opencode + Herdr 0.9.5 only** | Live Herdr acceptance passed; fake-runtime contract and worker tests pass; remote machine-worker and `codex` paths remain unverified |
| Public release | **Not ready** | No signing/authenticated distribution, no TLS ingress test, no cross-platform or installed-version upgrade evidence, and M25 capability actions/upgrade remain incomplete |

Local internal testing is not blocked on optional public distribution
infrastructure. Public-release claims require the separate gates above.

## Progress reporting convention

Report source implementation and executable acceptance separately. Do not infer
completion from documentation or source-shape tests. GetDone records carry
criterion-level evidence; historical journal entries do not override current
verified state.
