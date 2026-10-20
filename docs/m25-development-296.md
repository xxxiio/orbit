# Development 296 — Concrete capability composition

This source increment introduces a trusted `LocalAgentLaunchCapability` implementing
`BlueprintCapabilityExecutor` and delegates to `AuthoritativeHost::start_local_agent_job`.
The executor interface now receives the authoritative host and durable action
request, allowing adapters to verify cross-domain associations before effects.

Safety: registered capability only; trusted repository allowlist; existing
application-level agent checks; work-item/task project equality; action claim
before launch; command-ID-as-job-ID; ambiguity requires reconciliation.

This is not a generic plugin loader, connected machine launcher, or automatic
workflow-transition effect. Those scopes are intentionally excluded. M15 owns
broader acceptance and resulting fixes.
