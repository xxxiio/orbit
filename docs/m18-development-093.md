# M18 development 093 — Atomic authoritative snapshot and offline cache safety

## Delivered

- Authenticated `GET /v1/authoritative/snapshot` for canonical projects, tasks and their single server revision.
- A single SQLite read transaction for both collections; neither a client-side revision polling loop nor potentially inconsistent pair of HTTP reads is needed.
- Offline Python bridge refresh now uses this endpoint and atomically replaces both accepted collection caches.
- Fail-closed behavior for incomplete snapshots, malformed revision values and revision rewind following a restored/replaced server. Existing offline reads and pending commands are retained.
- Added a Rust store regression for snapshot contents and revision, plus Python regression tests for atomic refresh, malformed responses and rewind protection.

## Limits / following M18 work

This is **not** the complete offline Rust desktop/mobile replica. Pending commands are not yet rebased after conflicts, and a restored server retaining a revision but changing history needs a stronger history identity/checkpoint contract. Canonical habits/metrics, browser/agent cutover, migration/import of populated histories, and retiring built-in Git remain open. The bridge still operates as an opt-in Python tool.

Rust/Cargo checks are not available in this environment; new Rust code remains uncompiled. The focused Python tests are executable here. M18 source-progress estimate: **68%**, up from 65%. M15 stays 8% until executable acceptance evidence is available.
