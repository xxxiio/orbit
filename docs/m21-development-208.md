# M21 — Development 208: Multi-client convergence and reviewed conflicts

## Why this is the next substantial feature

Development 207 coordinated replay, accepted-state refresh and Markdown mirroring,
but it did not independently check whether a second client advanced the server
after the refresh. It also allowed conflict replacement without linking the
replacement to a particular reviewed accepted revision. These are two parts of
one end-to-end multi-client reconciliation workflow.

## Implemented interfaces

- `AuthorityReplica::convergence_status()`: offline-safe accepted revision,
  fingerprint, outbox states, Markdown count, and first unresolved command.
- `AuthorityReplica::observe_convergence()`: compares live server revision and
  history fingerprint with locally accepted state without rewriting either.
- `AuthorityReplica::converge()`: runs reconnect/replay/accepted refresh/Markdown
  mirror, then makes a second point-in-time server observation.
- `AuthorityReplica::conflict_review()` and `review_conflict_online()`:
  captures the conflicted command, original ordering position, accepted
  revision and history fingerprint. Online review refreshes first.
- `replace_reviewed_conflict()` and `_online()`: validates that the command and
  accepted history still match the review, and replaces within one SQLite
  transaction. Server acceptance is still pending, never implied.
- The replacement freezes the **reviewed** expected revision. Another client's
  later commit therefore causes authoritative optimistic-concurrency rejection
  rather than silent automatic rebasing.
- File operations with separate content-revision semantics are explicitly
  excluded from the domain-revision replacement path.
- The shared connected client and unified `AuthorityConnection` expose these
  operations. CLI, MCP and TUI have new online convergence/review pathways.

## Operator commands

Use `orbit sync replica --db <replica> --server <url> convergence` for an offline
read-only report. Use `observe-convergence` for a live comparison and `converge`
for replay, refresh, Markdown mirroring and a final server observation.

For conflicts, use `review-conflict-online --command-id <id>` to obtain the
review token (which includes the rejected action and accepted revision).
Save the JSON response to a secure local review file and submit a deliberate
replacement with `resolve-reviewed --review-file <path> --replacement-id <id>
--kind <family> --action-json <payload>`.

The MCP connected backend exposes `authority.convergence`,
`authority.observe_convergence`, `authority.converge`,
`authority.review_online`, `authority.conflict_review`, and
`authority.resolve_reviewed` with its existing scoped permission checks.

## Precise claims and remaining limitations

Convergence is **observed at a point in time**, not a persistent lease or
promise that other clients cannot write. Domain conflict replacements remain
subject to the server's accepted revision check. A successful Markdown mirror
is not a transactionally frozen server filesystem snapshot. Remote batch
acceptance is ordered, not all-or-nothing.

The earlier compatibility `replace_conflict` API remains available for older
callers; new interfaces should use the snapshot-bound review path. Retiring
that old API and achieving executable two-client acceptance are separate tasks.

## Scope and checks

Focused Rust scenarios cover stale review tokens, fingerprint mismatch,
replacements retaining original ordering, rejection without mutation,
independent two-replica pending queues, HTTP observation races, complete
reconnect cycles, conflicted cycles, and Markdown mirroring failures.

Broad Cargo compilation and acceptance remain part of M15 per the project's
feature-first roadmap. The implementation progress estimate does not claim
those scenarios have been executed.

## Internal-testing readiness

Not verified: runnable Rust builds, smoke workflows, and no known blocking
data-loss issues are prerequisites for internal-testing approval.
