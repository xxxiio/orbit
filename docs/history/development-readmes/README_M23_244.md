# Development 244 — agent review attention and deterministic catch-up

Adds `orbit agent overview` for cross-project, persisted-state catch-up without LLMs or a live Herdr connection. Each job shows its project, task, lifecycle state, handoff count, review decision, attention boolean, and deterministic recommended next action. Pending handoff reviews and changes-requested jobs now appear in the existing `orbit agent attention` query, even if the runtime is still running. No command automatically marks tasks complete.

Verification performed: SQLite schema initialization and direct pending-review/changes-requested attention queries. Rust compilation/tests and live Herdr were unavailable; regression test added but unexecuted. Next priority remains runnable end-to-end build plus automatic evidence at job completion, rather than more disconnected infrastructure changes.

M23: previous provisional source-scope estimate 38%; no additional milestone acceptance percentage claimed pending executable validation and a complete feature increment. M15 acceptance pending; M22 source scope 100%.
