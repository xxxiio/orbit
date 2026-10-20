# Orbit 103 — Browser canonical workspace

The server-hosted browser console now has an explicit **authoritative workspace** section.
It uses an enrolled device bearer token (separate from the admin/worker token),
reads all six collections from `/v1/authoritative/snapshot`, and performs
revision-checked direct commands against the shared canonical SQLite history.
Projects, tasks, habits, metrics, task status changes, habit occurrences and
metric observations are covered. This path does not use local-worker reports or
approval-proposal storage. A 409 conflict is shown for human review and the
console requires a new snapshot before the next attempt; it does not auto-rebase.

The existing admin reporting/automation/proposal console remains functional but
is explicitly a **legacy worker-managed view**, not authoritative data. The
browser is online-only and has no offline replica. Credentials remain only in
page memory. Host only on loopback or behind TLS. Existing server endpoints
continue to enforce device enrollment and token authentication.

No Dart/Flutter sources are included in this repository snapshot, so this is
browser integration, not Flutter screen integration. Cargo/runtime acceptance
is not available in the current build environment. M18 source estimate: 91%;
M15 release acceptance: 8%.
