---
template: acceptance-report
template_version: 2.0.0
template_digest: "cf2288fa6e58729b6f3b34306207c7168c34b100111c216cda2530187e055f83"
project_owned: true
record_contract: acceptance-report
record_schema_version: 1
status: passed
task_id: TASK-0014
---

# Current Acceptance Report

## Task

TASK-0014 — M32 I5–I6: dogfooding, final acceptance, installation and TUI handoff.

## Gate results

| Gate ID | Gate | Status | Evidence |
|---|---|---|---|
| G-0001 | Persistent-workspace dogfooding with an honest record | pass | Nine sessions on one database (run 9 resumed run 8); 23 steps each; captures, step JSON and a trace under `.slim/deepwork/m32-web-completeness/i5-dogfood/`; `.agent/evidence/m32-i5-acceptance.md`. Labelled automated interactive dogfooding, with the human-operator gaps listed. |
| G-0002 | Blocker fixed in the owning layer with regression proof | pass | `orbit_planner::deadline_bound_at` used by the planner's proposer and validator, `schedule_planning`, `portfolio_schedule` and `planning_approval`; the session's plan step (06b) now reviews and applies on the same workspace. Six revert-and-run proofs across planner, runtime and client. |
| G-0003 | Full browser acceptance of every added operation | pass | 136 passed, 5 skipped (opt-in captures and the opt-in dogfooding session), 0 failed, against an isolated server per worker. |
| G-0004 | Reproducible clean install | pass | Lockfile-only directory: plain `npm ci` fails on this host (Windows npm shim under WSL, no native Linux npm); `--ignore-scripts --os=linux --cpu=x64` installs 291 correct packages, typechecks, builds, and the `dist` is byte-identical (16 files); the documented loop was run end to end. Host limitation recorded, tooling unchanged. |
| G-0005 | TUI specification and documentation | pass | `docs/guides/tui-redesign-spec.md` rewritten (18 patterns; 6/3/1/8) from proven usage with named shared APIs and A1–A23; three stale doc claims corrected; `tests/documentation` 39 passed; terminology 256 files clean. No TUI change. |

## Commands run

```text
cargo test --workspace --offline                       # 1,844 passed, 0 failed, 31 binaries
cargo clippy -p orbit-planner -p orbit-runtime -p orbit-server -p orbit-storage -p orbit-cli -p orbit-blueprint   --all-targets --offline -- -D warnings               # clean
cargo fmt --all -- --check                             # clean
cd web && tsc --noEmit                                 # exit 0
cd web && vitest run                                   # 60 files, 393 tests
cd web && node scripts/check-terminology.mjs           # 256 files
cd web && vite build                                   # built
cd web && playwright test                              # 136 passed, 5 skipped, 0 failed
ORBIT_DOGFOOD=1 playwright test tests/e2e/dogfooding   # 1 passed (23 steps), plan step now ok
python3 -m pytest -q tests/documentation               # 39 passed
```

## Checks not run

- Plain `npm ci` on a Linux-native npm, an offline install from an empty cache, and a
  clean-machine Playwright run (needs the browser libraries bundle). Recorded as
  unverified rather than assumed.
- Live Herdr/model acceptance (unchanged external requirement).
- M15/M27 release qualification (deliberately not regenerated).

## Waivers

- None.

## Residual risk

- Three low-severity dogfooding findings remain open and recorded: Enter swallowed
  while a criterion save is pending (D-04), a second click needed to build a day
  plan (D-05), and a row's middle target plus a missing "Blocked from In progress"
  explanation (D-06/06b).
- The clean-install proof depends on a documented workaround for this host's npm.
- A capability request is once per item revision by design; the panel now says so
  instead of offering the control.
