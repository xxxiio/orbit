# Development 293 — Blueprint command UX

> **Historical checkpoint.** This record describes the state at Development 292.
> Server-side atomic blueprint command receipts and ordinary replay/reconciliation
> were implemented afterward and are exercised by the blueprint smoke. The current
> remaining M25 gaps are capability installation/action execution, connected MCP
> outbox parity, blueprint upgrade, habit-recurrence extraction and a lost-ack E2E
> test. See `docs/roadmap.md`.

- Normal blueprint mutations use canonical durable commands in connected mode, with online flush when available.
- `--offline` queues without contacting the authority; `--require-online` preserves explicit immediate-only behavior.
- Pending/blocked outcomes are surfaced rather than implying success. Local-only commands retain immediate authority behavior.
- Existing advanced queue, pending, and flush commands remain available.
- Next implementation: server-side atomic accepted-command receipts and concrete capability adapter composition.
- Broad Rust and HTTP integration validation belongs to M15; source modifications are not runtime acceptance.
