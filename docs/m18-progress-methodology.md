# M18: evidence-based progress rebasing (2026-10-13)

## Why the historical 89% is retired
The old M18 number was a subjective estimate that stayed fixed while the scope
changed; it did not distinguish source wiring, build validity, or accepted domain
parity. Do not carry 89% forward as an implementation or acceptance measurement.

## Reproducible evidence
Run `python3 scripts/legacy-contracts/m18-progress.py` for the summary or `--json` for each
source/negative check. The retained machine-readable result is
`docs/m18-progress-evidence.json`.

The eight weighted acceptance categories total 100 points: one authoritative
runtime (20), Markdown authority (15), folder organization (10), client parity
(15), remote offline outbox (10), local feature parity (15), executable tests
(10), internal-testing readiness (5).

This version grants at most 60% of each non-blocked *source-evidence* category;
failed architecture gates receive zero. It deliberately grants zero for runtime
acceptance and internal-testing readiness until executable evidence is supplied.
The resulting source-evidence score is **not** a % completion estimate and is
not comparable to the historical 89%. It is a conservative audit metric.

## Why the milestone is currently blocked
- `OrbitRuntime` + `SqliteStore` still coexist with `AuthoritativeHost` and
  `RelayStore`, so single authoritative storage and local feature parity fail.
- Rust toolchain is not installed here; no workspace compile, runtime smoke,
  or crash acceptance can be established.
- Full folder, MCP, CLI/TUI and remote offline parity require live acceptance.
- Flutter is deferred to Atomic App and is not an M18 blocker.

## Reporting convention
Report **historical M1–M17 estimates** as such and report M18 using this evidence
score plus an explicit acceptance status. Do not synthesize a new overall
implementation percentage from incomparable metrics. Once source and runtime
acceptance gates are demonstrably satisfied, replace the provisional model with
measured passing acceptance criteria.
