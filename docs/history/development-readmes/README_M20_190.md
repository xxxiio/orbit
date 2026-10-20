# M20 / Development 190 — Connected CLI integration for rich domains

## Selection rationale

Scheduling, routines, milestones and strategies already have canonical Rust authority
commands, authoritative server routes and durable replica outbox support, but the
public CLI's `sync replica queue` only exposed project, task and evidence. That made
four completed source-level domain capabilities inaccessible to users of the CLI.
Exposing the existing transport, validated direct commands, accepted reads and
receipt visibility is more urgent than standalone hardening, new workflow wrappers,
or release preparation.

## Usable workflows

An enrolled device can now perform all of the following through CLI:

1. `orbit sync authority --server https://HOST --device DEVICE --token TOKEN snapshot`
   fetches all accepted server projections, validating revision, fingerprint and
   presence of all ten accepted-domain arrays.
2. `orbit sync authority ... domain-read --kind strategy` reads the authoritative
   project strategies. Other supported values: `schedule`, `routine`, `milestone`.
3. `orbit sync authority ... domain-command --kind strategy --command-id change-1
   --revision 5 --action-json '{"type":"set_objective","project_id":"p1","objective":"Ship"}'`
   verifies the exact canonical StrategyCommand wire type before posting the
   mutation. The same interface supports schedule, routine and milestone actions.
4. `orbit sync replica --db replica.sqlite --server https://HOST queue --kind
   schedule --command-id create-1 --action-json '{...}'` durably stages the
   intended schedule change while disconnected. The CLI also accepts routine,
   milestone, strategy, Markdown and folder command families.
5. `orbit sync replica ... flush` submits in preserved command order; `refresh`
   updates the accepted projection only after server acknowledgment.
6. `orbit sync replica ... accepted --family schedule` shows accepted schedule
   blocks independently of local unresolved intentions.
7. `orbit sync replica ... receipt --command-id create-1` exposes accepted,
   pending, conflicted or superseded command state and any stored receipt.

## Acceptance contract

- Rich direct commands deserialize into the same strict Rust command types as
  the server handlers before any network operation.
- Direct command identifiers and revisions are validated; action JSON cannot
  smuggle envelope fields.
- Replicas distinguish uninitialized accepted state from an empty accepted
  workspace; pending modifications do not appear in accepted domain reads.
- The existing durable queue and command receipt logic are reused unchanged.
- The CLI cannot declare an operation accepted solely because it was queued.
- The server's token is never included in output JSON or diagnostic strings.

## Validation

- `python scripts/legacy-contracts/check_m20_190.py` — 34 structural checks passed.
- `python scripts/legacy-contracts/check_m20_189.py` — 22 source contracts passed.
- `python scripts/legacy-contracts/test-m20-unified-accepted-reads.py` — passed.
- Added Rust command-parsing, wire-schema, accepted-snapshot and receipt tests.
- **No Rust toolchain is installed in this environment**; the Rust tests were
  neither compiled nor run. These checks do not establish binary readiness.
- MCP and TUI remain outside this iteration and are not claimed complete.

## Next highest-priority scope

Run a real Cargo workspace build and tests, repair any compilation failures,
exercise two connected clients against the self-hosted server, and wire the
remaining rich-domain workflows through MCP and TUI. Those acceptance gaps
must be resolved before closing M20. No higher-priority unfinished feature
category justifies unrelated release or hardening work.

## Internal testing

**Not ready / not verified**: no runnable validated Rust build, no cross-client
smoke verification, and thus no evidenced assessment of blocking data-loss issues.
