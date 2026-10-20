# Orbit 146: Host-owned application query consolidation and roadmap estimates

## Scope
This iteration consolidates the common application read boundary for both the embedded Orbit client and local MCP. These clients now request projects, tasks, habits, routines and project milestones through `AuthoritativeHost::application_queries()` rather than calling those repository query functions directly. ApplicationQueries uses the same SqliteStore owned by the host. No additional connection or database is created.

The host's write-side ApplicationServices remains in place. Existing specialized repository queries (planning, activities, evidence) and the CLI transitional mutation bridge are not yet consolidated. This is not completion of M18.

## Source validation
Run `python scripts/legacy-contracts/test-m18-application-queries.py` and the existing host-composition, Markdown, folder, repository and offline-sync guards. Cargo/rustc were not found in the execution environment; Rust build and runtime parity remain unverified.

## Percentage assessment (October 14, 2026)
These are subjective **implementation coverage estimates**, not release readiness or verified acceptance. A fixed checklist is used rather than measuring number of methods or iterations. The estimates are ranges in spirit; a single rounded point value is supplied for reporting convenience.

| Unfinished milestone | Estimated implementation | Why incomplete |
|---|---:|---|
| M1 domain foundation | 94% | historical estimate; executable acceptance pending |
| M2 local persistence | 96% | historical estimate |
| M3 CLI core | 92% | historical estimate; legacy bridge |
| M4 planning/scheduling | 94% | historical estimate; client parity |
| M5 progress/forecasting | 94% | historical estimate |
| M6 habits | 94% | historical estimate |
| M7 retired Git sync | 92% | historical estimate; cleanup |
| M8 knowledge foundation | 94% | historical estimate; Markdown unification |
| M9 knowledge retrieval | 97% | historical estimate; integration |
| M10 embedded Rust client | 94% | historical estimate; parity |
| M11 local AI/BYOK | 94% | historical estimate |
| M12 MCP/agents | 95% | historical estimate; parity |
| M13 terminal UI | 94% | historical estimate |
| M14 project modes/AI | 97% | historical estimate |
| M15 release readiness | 8% | historical estimate; no verified build |
| M16 self-hosted server | 95% | historical estimate; parity |
| M17 remote agents/web | 98% | historical estimate |
| M18 unified application runtime | 70% | one store owned by host and rich services, but CLI bridge and command semantics not unified |
| M19 Markdown knowledge/folders | 75% | source-rich, but recovery and resource workflows not verified |
| M20 unified clients/transports | 65% | embedded/HTTP/MCP partially connected; full feature parity missing |
| M21 remote offline sync | 65% | queue/cache implemented; executable reconnect and conflict tests missing |
| M22 integrated acceptance | 10% | scripts exist; executable Rust acceptance not performed |

Numbers are planning judgments, **not** computed from a source-evidence script. They must not be averaged into an overall completion number across differing confidence levels. Re-estimate against accepted workflow outcomes at each substantial milestone.
