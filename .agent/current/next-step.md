---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0010
status: completed
milestone_id: MILESTONE-0029
advances_exit_criteria: [EC-001, EC-002, EC-003, EC-004, EC-005, EC-006, EC-007, EC-008, EC-009, EC-010]
---

# Next Deterministic Step

## Objective

Deliver M29: restore the TUI's working context, make actions and forms
discoverable and safe, adapt layout from 60x18 to 160x45, consolidate local and
connected operation into one shell, extract the oversized modules and improve
measured interaction latency without changing M28 product capabilities.

## Why this is next

M28 completed the functional surfaces; daily use exposed interaction defects and
maintenance concentration. M29 was the only product-quality work that could be
completed locally and materially improves every project. It is now complete;
no further milestone is authorised by this record.

## Preconditions

- MILESTONE-0028 completed with its regression tests.
- A local `orbit-server` binary for connected recovery acceptance.
- Ratatui/Crossterm renderer and the existing PTY harness.

## Inputs

- `crates/orbit-tui/src/{main.rs,input/,app/,render/,backend.rs,ui_state.rs}`
- `crates/orbit-client/src/unified.rs`
- `tests/acceptance/tui/`
- `scripts/tui-fixture.py`, `tests/acceptance/tui/perf_probe.py`

## Ordered actions

1. Persist and restore the stable selected entity, component and scroll offsets.
2. Add the compact action bar, grouped menu, ranked palette, quick edit and
   field-level form validation.
3. Implement component-aware layout, Kanban windowing, table priority and a
   restrained no-colour-safe theme.
4. Port pending/conflict inspection and the record console into the shared
   shell; delete the legacy connected presentation.
5. Extract `app/`, `input/`, `render/`; measure the realistic fixture and fix
   only measured hot spots.
6. Run PTY Scenarios A–G, capture real screens, update docs and records.

## Expected outputs

- Durable navigation context and a coherent daily-use interface.
- One shell for local and connected operation with honest pending state.
- Maintainable module boundaries and recorded performance evidence.

## Acceptance criteria

- [x] NS-001: Selection, component, focus and scroll restore across view changes
  and restart, with safe fallback for deleted/reordered items and corrupt files.
- [x] NS-002: High-value actions are discoverable with keys that match their
  behaviour; the palette ranks and refuses disabled actions.
- [x] NS-003: Forms and quick edit behave predictably and preserve input.
- [x] NS-004: Boards, tables and details stay readable at every supported size.
- [x] NS-005: One shared shell handles local and connected operation; the legacy
  presentation is deleted without capability loss.
- [x] NS-006: The event loop is simple, dispatch has one owner, and measured
  performance improves where the data justified it.
- [x] NS-007: PTY Scenarios A–G pass; documentation and captures match behaviour.

## Validation

```sh
cargo test --workspace --offline
cargo clippy --offline -p orbit-domain -p orbit-application -p orbit-storage \
  -p orbit-runtime -p orbit-sync -p orbit-blueprint -p orbit-client -p orbit-ui \
  -p orbit-tui -p orbit-mcp -p orbit-cli -p orbit-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m pytest -q tests/contracts
python3 -m pytest -q tests/acceptance/tui
ORBIT_TUI_FULL_FIXTURE=1 python3 -m pytest -q tests/acceptance/tui/test_m29_fixture.py
python3 scripts/tui-captures.py
./scripts/m15-full-automated.sh --profile full
```

Observed: workspace 1,563 passed; contracts 43; TUI 54 passed/1 skipped; full
fixture 2 passed; 21 captures; M15 45/47 (withheld release workflow).

## Stop conditions

- A blocking data-loss, authority or everyday-use defect remains.
- PTY Scenarios A–G cannot run against the real binary.
- Documentation claims exceed measured behaviour.

None of these applied at completion.

## Out of scope

- New domain capabilities or a Flutter renderer.
- Per-device ACLs, server attachment GC and live Herdr acceptance.
- Public release verification.
