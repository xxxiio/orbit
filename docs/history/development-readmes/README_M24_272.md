# Development 272 — M24 worker internals and documentation consolidation

M24 implementation estimate: **77% → 83% (+6 percentage points)**.

Separated the Herdr worker's read-only Git evidence resolver and the durable
machine crash journal into independent internal modules; kept the existing
worker entrypoint and compatible imports. Replaced out-of-date root and
self-hosted architecture entrypoints with current local/server-authoritative
mode descriptions and retained prior texts as explicitly historical records.

Feature-level checks: 28 machine-worker tests passed and Python module syntax
was checked. Broad Rust integration, live Herdr acceptance and defects found by
those checks are M15 scope. Internal-testing readiness is not established.

See `docs/m24-architecture-consolidation.md` and
`docs/m24-worker-architecture.md` for current responsibilities and remaining I4
work.
