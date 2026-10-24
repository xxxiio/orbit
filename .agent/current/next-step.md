---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0009
status: completed
milestone_id: MILESTONE-0028
advances_exit_criteria: [EC-009]
---

# Next Deterministic Step

## Objective

Make attachments server-authoritative in connected mode: durable upload intents,
content integrity verification, authorized download for another client, safe
retry after interruption, and no duplicate or corrupted content.

## Why this is next

It was the only blocking M28 product gap after the local usability recovery. The
server now accepts attachments, clients stage offline and flush exactly once, and
Scenario D passes.

## Preconditions

- Local attachment store and metadata are stable (schema 20).
- The server authority owns project/task commands and revisions.
- A disposable local server is enough for acceptance.

## Inputs

- `crates/orbit-application/src/attachment_commands.rs`
- `crates/orbit-runtime/src/attachment_store.rs`
- `crates/orbit-server/src/main.rs`
- `crates/orbit-client/src/attachments_remote.rs`
- `tests/acceptance/connected/test_attachments.py`

## Ordered actions

1. Add authenticated server routes to import/list/download/remove/associate
   attachment content and metadata.
2. Add durable client upload intents with ordered flush, checksum verification
   and idempotent retry.
3. Implement CLI/TUI attachment operations with explicit pending/conflict state.
4. Add Scenario D coverage for cross-client retrieval, offline staging, duplicate
   and truncated retries, removal replay and metadata-only ledger payloads.
5. Wire the connected checks into release evidence and document the model.

## Expected outputs

- Server-authoritative attachment metadata and content.
- Safe retry and explicit pending/accepted semantics.
- Passing Scenario D evidence.

## Acceptance criteria

- [x] NS-001: Connected upload is accepted, integrity-verified and visible to a
  second authorized client.
- [x] NS-002: An offline-staged attachment synchronises exactly once after
  reconnect; an interrupted upload retries without duplication or corruption.
- [x] NS-003: Every attachment route requires an enrolled device and validates
  the owner entity; the authority is single-tenant, so per-project ACLs are
  documented and tracked separately (TODO-0013).
- [x] NS-004: Local-authoritative behaviour is unchanged and connected operations
  never report false success.

## Validation

```sh
cargo test --offline -p orbit-runtime -p orbit-client -p orbit-server -p orbit-storage
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp tests/acceptance/connected
python3 deploy/server/smoke-attachments.py
python3 deploy/server/smoke-offline-authority.py
```

### Evidence

- Connected suite: 6 passed; server attachment smoke PASS; offline authority
  smoke PASS; runtime ledger-size/retry tests pass.
- M15 profile: 45/47 product checks pass (release workflow withheld).

## Stop conditions

- Stop if server authorization cannot be enforced per attachment owner.
- Do not weaken local attachment safety to reuse code in connected mode.

## Out of scope

- Per-device project ACLs (single-tenant authority).
- Server-side attachment GC endpoint.
- Public release verification (M27).
- Live Herdr agent launch acceptance.
