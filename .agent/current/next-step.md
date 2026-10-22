---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0009
status: ready
milestone_id: MILESTONE-0028
advances_exit_criteria: [EC-009]
---

# Next Deterministic Step

## Objective

Make attachments server-authoritative in connected mode: durable upload intents,
content integrity verification, authorized download for another client, safe
retry after interruption, and no duplicate or corrupted content.

## Why this is next

Basic projects, rich task details, ordered acceptance criteria, local attachments,
workflow terminology and MCP parity are implemented and verified. The only
blocking M28 product gap is required scenario D: connected attachment
acceptance. Today every connected attachment operation fails closed with an
explicit Unavailable message, which is honest but incomplete.

## Preconditions

- Local attachment store and metadata are stable (schema 20).
- The server authority already owns project/task commands and revisions.
- No external authorization is needed; a disposable local server is enough for
  acceptance.

## Inputs

- `crates/orbit-application/src/attachment_commands.rs`
- `crates/orbit-runtime/src/attachment_store.rs`
- `crates/orbit-server/src/main.rs` (authoritative routes)
- `crates/orbit-client/src/attachments.rs` and `unified/rich_client.rs`
- `crates/orbit-storage/src/store_core.rs` (schema 20 attachment tables)

## Ordered actions

1. Add server routes to stage, accept and read attachment content with explicit
   project/task authorization and integrity checks.
2. Add a durable client-side upload intent/outbox record with retry, checksum
   verification and idempotent attachment identity.
3. Implement upload/download in `AuthorityConnection`, removing the Unavailable
   refusal only when the server actually supports the operation.
4. Add focused tests for interrupted upload, lost acknowledgement, retry,
   duplicate content and cross-client retrieval.
5. Run required scenario D against a disposable server and record PTY/HTTP
   evidence; update docs and GetDone records.

## Expected outputs

- Server-authoritative attachment metadata and content.
- Safe retry and GC behaviour across clients.
- Passing scenario D acceptance evidence.

## Acceptance criteria

- [ ] NS-001: Connected upload is accepted, integrity-verified and visible to a
  second authorized client.
- [ ] NS-002: An offline-staged attachment synchronises exactly once after
  reconnect; an interrupted upload retries without duplication or corruption.
- [ ] NS-003: Unauthorized or out-of-scope attachment access is rejected.
- [ ] NS-004: Local-authoritative behaviour is unchanged and connected operations
  never report false success.

## Validation

```sh
cargo test --offline -p orbit-runtime -p orbit-client -p orbit-server -p orbit-storage
python3 -m pytest -q tests/acceptance/tui tests/acceptance/mcp
python3 -m pytest -q tests/acceptance/connected
```

## Stop conditions

- Stop if server authorization cannot be enforced per attachment owner.
- Do not weaken local attachment safety to reuse code in connected mode.

## Out of scope

- Public release verification (M27).
- Live Herdr agent launch acceptance.
- Attachment preview inside terminals without external viewers.
