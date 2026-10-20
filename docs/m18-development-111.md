# Orbit 111 — Canonical automation report cutover

The previous scheduled automation worker opened a local-only Orbit database through `orbit --db`, which could report state different from server-authoritative clients. That path is deleted. The worker claims/settles jobs through the scheduler API and reads a single canonical snapshot for each accepted job; the entire report is derived from that revision. It never mutates domain state.

Launch with the scheduler credential in `ORBIT_SYNC_HTTP_TOKEN`, a separately enrolled device credential in `ORBIT_AUTHORITY_TOKEN`, and `--device <enrolled-device-id>`:

```bash
python3 deploy/server/automation-worker.py --server https://orbit.example.com --device automation-reader-1 --once
```

The three supported actions remain `daily-briefing`, `habit-summary`, and `project-health-refresh`. The latter now emits per-project task counts rather than invoking a separate local-only project-health algorithm. This deliberate behavior change avoids inventing parity between two different read models. Snapshot schema validation is fail-closed, and report size is limited to 60 KiB. Network redirects are refused.

**Verification:** `python3 -m unittest discover -s deploy/server -p 'test_*worker.py'` passed (12 focused tests). Python compilation passed. No Rust build or live server smoke was run in this authoring environment. The authoritative automation claim/settle lifecycle still needs runtime verification, together with the other M15 gates. Flutter belongs to Atomic App.
