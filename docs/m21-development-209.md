# M21 Development 209 — Snapshot-bound batch reconciliation

## Why this implementation is the next priority

M21's next uncompleted connected-client workflow is conflict convergence after multiple independently edited clients. Development 208 introduced single-command, snapshot-bound reconciliation, but older unreviewed compatibility paths remained reachable from the CLI, MCP and shared client. The remaining high-priority functionality is safe, durable reconciliation for more than one conflict, without creating a locally half-staged resolution batch. This iteration implements that capability rather than another status wrapper.

## Delivered implementation

- Eliminates `AuthorityReplica::replace_conflict`, `AuthorityConnection::resolve_conflict`, and the local-only shortcut on `ConnectedOrbitClient`.
- Removes the unsafe `sync replica replace-conflict` CLI action. Operators instead use reviewed, revision-bound resolution.
- Retains the MCP `authority.resolve` name only as a *safe alias* of `authority.resolve_reviewed`; a full review token is now required.
- Introduces `ReviewedReplacement` and `AuthorityReplica::replace_reviewed_batch`.
- Preflights up to 100 distinct reviewed decisions before database mutation. Checks original conflict state, ordinal, family, rejected action, accepted revision and history fingerprint, unused replacement IDs and action-envelope purity.
- Preserves command ordering and original rejected audit rows, and freezes each replacement's reviewed revision.
- Commits the entire local batch in one SQLite transaction; remote acceptance remains sequential and may still fail partway through server replay.
- Exposes online refresh + atomic reviewed-batch staging through `AuthorityConnection` and the native replica CLI (`resolve-reviewed-batch --file ...`).
- Exposes the same semantics through connected MCP (`authority.resolve_reviewed_batch`), with whole-request policy validation before dispatch.
- Adds Rust regression cases for stale accepted state, duplicate IDs, cross-family changes, rollback, unchanged accepted snapshots, durable restart and separation of two client replicas.

## Example CLI reviewed batch manifest

```json
{
  "resolutions": [
    {
      "review": {
        "rejected_id": "old-command",
        "kind": "task",
        "rejected_action": {"type":"change_status","task_id":"t1","status":"ready"},
        "ordinal": 1,
        "accepted_revision": 42,
        "history_fingerprint": "<64 lowercase hexadecimal characters>"
      },
      "replacement_id": "reviewed-command",
      "family": "task",
      "action": {"type":"change_status","task_id":"t1","status":"done"}
    }
  ]
}
```

The review fields must be obtained from the connected client's `review-conflict-online` result, not invented. The example fingerprint is a placeholder, not a working token.

## Important boundaries

- Accepted state never incorporates a queued replacement until the server issues an accepted receipt.
- SQLite batch staging is atomic, but remote processing of that batch is not a distributed transaction.
- Concurrent writes after the online review are handled by freezing the reviewed server revision; the server must reject stale actions rather than auto-rebasing.
- Markdown and folder conflicts require content-specific file revision semantics and are not silently resolved by a domain revision token.
- Rust toolchain compilation and exhaustive executable acceptance remain assigned to M15. This milestone reports source implementation only.

## Milestone coverage

M21 advances from provisional 80% to 85% implementation coverage because a coherent previously uncompleted multi-conflict reconciliation workflow is now implemented across the replica, shared client, CLI and MCP. This is **not** a compiled acceptance score. The remaining M21 work involves ensuring all supported domain and Markdown convergence paths present consistent state across clients and finalizing implementation closure.

## Internal-testing readiness

Not established. No runnable Rust build or successful integrated smoke workflow has been verified this iteration. M15 remains responsible for broad build and release acceptance, with targeted source guards in this implementation pass.
