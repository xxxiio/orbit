# M20 Development 199 — Reviewed connected workflows and replica audit

## Scope and engineering decision

This increment implements two missing client-level operations across the canonical
connected-mode authority boundary. It does not create another SQL domain store,
server-side business-rule fork, Git sync adapter, or data migration path.

1. **Reviewed offline workflow manifests in the CLI** (`orbit sync replica ...`).
   `preview-workflow --file plan.json` validates a versioned manifest and prints
   a redacted summary without staging it. `stage-workflow --file plan.json`
   stages the complete ordered batch in one SQLite transaction, or none at all.
   Replaying identical command IDs/actions is idempotent; an identity collision
   with different contents causes the complete new batch to roll back.
2. **Single-outbox audit** (`orbit sync replica ... audit`). Checks identity,
   accepted vs pending receipts, family labels, global ordering, frozen
   revisions, action envelopes and snapshot metadata; does not mutate or
   automatically repair accepted data. Reports command metadata but not bodies.
3. **Unified workflow ID preflight** rejects unsafe command identifiers and
   empty action objects before staging.

## Example reviewed manifest

```json
{
  "version": 1,
  "name": "Project kickoff",
  "commands": [
    {"command_id":"kickoff-project","kind":"project","action":{"project_id":"p-1","name":"Planning"}},
    {"command_id":"kickoff-task","kind":"task","action":{"type":"create","task_id":"t-1","title":"Define scope","project_id":"p-1"}}
  ]
}
```

```sh
orbit sync replica --db replica.sqlite --server https://orbit.example \
  preview-workflow --file kickoff.json
orbit sync replica --db replica.sqlite --server https://orbit.example \
  stage-workflow --file kickoff.json
orbit sync replica --db replica.sqlite --server https://orbit.example audit
# Authenticated submission is intentionally separate:
orbit sync replica --db replica.sqlite --server https://orbit.example flush
```

A staged command is **not** an accepted server mutation. Multi-command local
staging is atomic. Remote authority acceptance is ordered but **not atomic**,
so earlier commands can commit before a later server rejection.

## Implementation boundary and verification

The source includes focused behavioral scenarios for retries, atomic rollback,
restart persistence, accepted-state isolation, malformed manifests, receipt
invariants, command ordering, all ten transport families and safe redacted
previews. Broad Rust compilation and runtime acceptance are still deferred to
M15 at the user's direction. The development source has not been compiled.

The M20 implementation percentage is not advanced simply on code volume.
The work does not by itself prove that every legacy mutation path has been
retired or that M20 can be formally closed.
