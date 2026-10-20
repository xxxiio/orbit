# Orbit Development 270 — Runtime Capability Segregation

M24-I3 source refactoring only. The launch coordinator now depends on `AgentLauncher`; the monitoring coordinator depends on `AgentInspector`. Existing `AgentRuntime` adapters remain supported by blanket compatibility bridges, avoiding a flag-day migration. `AgentMessenger` separates continuation from launch/inspection; Herdr implements it, and ambiguous prompt delivery is explicit. No new public CLI, database schema, or server capability is introduced.

Broad compilation/integration/live Herdr acceptance and fixes discovered there remain in M15.
