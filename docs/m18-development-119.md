# Orbit Development 119 — Canonical knowledge updates

## Priority decision
M18 U2 remains the highest priority: remove the split between feature-rich local SqliteStore and canonical AuthoritativeHost without breaking functionality. This iteration extends the existing canonical knowledge path to support edits; it does not create another adapter or claim full repository unification.

## Implementation
- Typed `KnowledgeUpdateCommand` for knowledge item edits.
- `AuthoritativeHost::knowledge_update` delegates to the canonical store.
- Update requires expected revision, existing note, nonempty valid title, and actual content change.
- One immediate SQLite transaction persists record change, semantic change, immutable command receipt, and accepted projection.
- Field mutation `base` uses the latest committed field change ID, enabling correct semantic conflict tracking; it does not use previous field text as the base.
- Duplicate receipts and command-ID collisions enforce actor/payload immutability.
- Regression test covers accepted edit, read-after-write, duplicate receipt, actor conflict, stale revision, missing item, and revision consistency.

## Validation
- 17 offline Python tests passed; Python syntax checks passed; archive inventory verified.
- Rust compiler unavailable here: new Rust code and its regression test are NOT compiled or executed.
- HTTP, MCP and unified transport exposure not yet included; no end-to-end parity claim.

## Remaining highest-priority work
Unify rich local knowledge repositories (metadata, relations, resources, search) and canonical services, then planning/scheduling and other domain capabilities. Verify Rust compilation, embedded/remote/MCP parity, smoke workflows and data-loss blockers before internal testing. Flutter belongs to Atomic App.

## Completion priority assessment
M18 U2 is still highest-priority unfinished work, ahead of release-hardening and Flutter integration. Internal-testing readiness NOT established.
