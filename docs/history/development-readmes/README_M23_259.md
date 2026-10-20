# Orbit Development 259 — Connected existing-session continuation (source implementation)

- Added owner-scoped `orbit agent continue JOB --message ...` for connected mode.
- Added a durable, idempotent session prompt queue in authoritative SQLite schema v19.
- Added machine-scoped pending prompt inbox, atomic dispatch claim, and outcome reporting.
- Extended the trusted machine worker to send prompts to existing Herdr sessions only.
- Ambiguous/lost acknowledgments never cause automatic duplicate delivery.
- These source changes do not provide offline command acceptance or offline replica replay.

Broad compilation, end-to-end testing, and defect fixes from validation belong to M15.
