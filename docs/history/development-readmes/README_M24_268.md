> **Historical checkpoint only.** Not current CLI or deployment instructions. See the [root README](../../../README.md) and [current architecture](../../architecture.md).

# Development 268 — Offline transport journal ownership

M24: 52% → 58% provisional implementation scope (+6 percentage points). The CLI-specific agent queue mechanics were moved into `orbit-client::offline_command_journal`, retaining the v1 JSON file format and local command behavior. Added FIFO, replay, binding, and old-file-format unit coverage. Canonical SQLite agent queue convergence remains in M24-I3; executable acceptance belongs to M15. See `docs/m24-offline-command-consolidation.md`.
