# M25 Project Blueprints — implementation record

## Development 279 — I1 foundation

Implemented a headless `orbit-blueprint` Rust crate with strict JSON manifests, typed custom field declarations, workflow state/transition reference validation, capability availability checks, an immutable in-memory registry, and version-pinned project bindings. Added `orbit-storage` persistence operations for published versions and project bindings, preserving the existing `ProjectMode` and task schema. Added a development blueprint JSON example and focused crate tests.

**Scope boundary:** Blueprint workflows are definitions only; they do not alter task transitions yet. Existing projects are not automatically rebound. YAML import, application service/CLI/MCP/HTTP wiring, bulk export and default binding for existing projects are not yet implemented. These remain in I1/I5 as appropriate. Broad compilation and release integration belong to M15.

M25 implementation progress: **0% → 10% (provisional source scope)**. This is the first substantial I1 slice; I1 is not complete. M15 remains at historically reported 5% acceptance.

GetDone is the intended progress system; no separate milestone README was created. The supplied source archive does not contain an identifiable `.planning`/GetDone store; this note records changes without claiming a GetDone update.
