# Orbit 143 — retirement of peer-event production entry points

## Architectural decision

Orbit has one authoritative application runtime using `SqliteStore` and a Markdown knowledge vault. A remote client invokes the same logical application commands over HTTP; an embedded client invokes them directly. Offline remote writes remain pending until server acceptance. Peer-event append/pull and migration-stage semantic commit APIs do **not** belong to the production runtime.

## Code changes

The obsolete peer-event `append`, raw `pull`, migration-only `commit_semantic`, and their legacy projection/history inspection helpers are now test-only, not callable from production Rust builds. Canonical project/task/evidence acceptance, the server history fingerprint endpoint, Markdown operations, device authentication and offline outbox remain present. `scripts/legacy-contracts/test-m18-retired-relay.py` guards against accidentally bringing back the retired entry points, and `scripts/legacy-contracts/m18-acceptance.sh` executes this check.

The existing internal `relay_events` table, the `RelayStore` wrapper, and semantic-history replay are **not removed**: canonical command acceptance still depends on them. This change is a cleanup of obsolete **public production capabilities**, not complete persistence unification.

## Unfinished M18 requirements

- Eliminate the remaining independent command-ledger/repository business semantics, not merely rename `RelayStore`.
- Replace direct rich-application mutations that bypass canonical revision/receipt validation with the same shared application services for embedded and remote callers.
- Prove planning/project-mode/habit/metric/knowledge feature parity through real executable tests.
- Compile Rust workspace and run end-to-end command, HTTP, offline, Markdown recovery tests (the environment has no Rust toolchain).
- Verify no known blocking data-loss issues before internal testing.

Flutter is deferred to Atomic App.

## Progress reporting

The last automated source-evidence score was 51/100. This cleanup does not satisfy any previously missing major feature-acceptance category, so **no percentage gain is asserted**. Historical milestone estimates remain informational only. M18 completion and internal-testing readiness are **not established**.
