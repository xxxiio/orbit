# M18 development 091 — Durable offline authority command bridge

Adds `scripts/orbit-authority-offline.py`, an opt-in prototype command outbox
for connected project/task workflows. It is explicitly **not** a substitute for
Orbit's integrated local SQLite replica, which remains future M18 work.

## Supported workflow

```sh
python3 scripts/orbit-authority-offline.py create-project project-1 'My Project'
python3 scripts/orbit-authority-offline.py create-task task-1 'Prepare review' --project-id project-1
python3 scripts/orbit-authority-offline.py status
ORBIT_SYNC_HTTP_TOKEN='...' python3 scripts/orbit-authority-offline.py sync --server https://orbit.example.com
```

Commands are durably queued in a dedicated SQLite database with stable IDs.
The first attempted submission saves its expected server revision before POST;
retries preserve this exact envelope for server-side idempotency. On an
unacknowledged network failure commands remain pending. Conflicts stop the
queue (no implicit rebase or silent overwriting); rejected commands require
explicit resolution. Accepted receipts are saved durably.

This bridge supports creation of projects and tasks and task-status transitions.
It does not cache canonical server projections, merge offline edits, or offer
habit/metric commands. No automatic client cutover or Git retirement has occurred.

## Checks

`python3 scripts/test-authority-offline.py` — 5 tests passed here.
Rust compilation and live server integration remain unverified; M15 acceptance
must cover them after the M18 architecture migration.

M18 source-progress estimate: **62%** (was 60%). Other 17 milestones unchanged.
