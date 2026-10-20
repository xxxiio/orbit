# Development 273 — Framework module ownership and test isolation

M24 source implementation progress: 83% → 90% (provisional +7 percentage points).

## Scope and priority

Continue approved M24-I4 consolidation before M15 release acceptance. No higher-priority unfinished feature scope was identified in the approved M24 roadmap; this change tightens production-code module ownership and separates large crate-root test suites without changing the authority design.

## Changes

- `orbit-client/src/facade_conversions.rs` now owns status, cadence and planning DTO conversions; the existing `orbit-client` root facade imports these functions and retains the same public types and methods.
- `orbit-client/src/client_facade_tests.rs` contains the extracted embedded-client regression suites. The test module remains nested under the client facade for private helper access.
- `orbit-runtime/src/store_tests.rs` contains nine authority store regression modules; non-test `latest_task_field_version` remains with the authority implementation. The authority store's original public API is unchanged.
- Removed production/test co-location that made ordinary source navigation and changes unnecessarily expensive.

## Scope preserved

No SQLite schema, HTTP contract, CLI argument, machine worker, or new execution path was introduced. The `orbit` executable remains the only public CLI. The server alone remains connected-mode authority, and Herdr execution remains machine-side and optional.

## Checks and acceptance boundary

File-based extraction and static references were inspected; no cargo/rustc executable is installed in the current environment. Full Rust compilation, end-to-end validation and corrections discovered by those checks remain M15 scope. This report does not claim runnable acceptance.

## Remaining M24-I4

Remove obsolete code only where callers can be demonstrated absent; audit active documentation and ensure compatibility shims that still have callers remain until migration or intentional breaking change. Close M24 implementation only when those source-level criteria are met.

## Progress

M22 100%; M23 100% source scope; M24 90% provisional source scope; M15 release acceptance pending. Next M24 iteration should deliver at least five percentage points or finish the remaining ten; no artificial partial checkpoint.
