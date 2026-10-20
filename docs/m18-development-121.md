# Orbit Development 121 — Unified Knowledge Client Operations

## Priority
M18 U2 remains highest-priority unfinished work: shared domain/persistence behavior. This iteration reuses existing canonical knowledge CRUD across embedded and HTTP-backed native clients, without adding another persistence facade. Full repository consolidation remains unfinished.

## Changes
- EmbeddedAuthorityClient delegates knowledge read/create/update to AuthoritativeHost.
- AuthorityConnection adds knowledge create/update plus accepted-state reads.
- Remote knowledge writes use durable frozen-revision outbox with independent create/update command kinds and correct HTTP endpoints.
- Remote knowledge reads query authenticated HTTP; offline reads fail explicitly rather than fabricate accepted state.
- Outbox schema allows knowledge commands, including its existing rebuild path.
- Added Rust regression test for embedded receipts and remote pending semantics.

## Limits
- Rust compilation and executable Rust tests have not been verified in this environment.
- Remote knowledge responses are not cached for offline access.
- Rich local knowledge relations, resources and search remain in legacy storage; one complete canonical persistence implementation is not achieved.
- MCP coverage and live HTTP parity require end-to-end verification.
- Flutter remains assigned to Atomic App.

## Internal-testing assessment
Not ready: successful Rust build, smoke workflows and no blocking data loss issue have not been established.

## Next
Prioritize migration of remaining rich knowledge repositories and local planning services into canonical domain persistence and eliminate competing local storage, with end-to-end compile/test evidence.
