# AI privacy and provider boundary

Orbit AI is optional. The deterministic domain/application/planner stack works with AI disabled.

## Ownership and routing

- Flutter/Atomic Apps calls the embedded Rust `orbit-client` facade.
- `orbit-client` delegates provider behavior to `orbit-ai`.
- `orbit-ai` calls a user-selected local or remote provider directly; no Orbit-hosted proxy is required.
- BYOK credentials remain in the platform secure store. Orbit state contains only opaque secret references.

## Disclosure-first remote calls

Candidate context is categorized before provider invocation. Per-project policy decides which categories are eligible and whether a remote provider is permitted. Remote calls can require explicit confirmation. The resulting disclosure identifies:

- destination: local or remote
- provider
- included projects/source IDs/categories
- omitted items and reasons
- whether confirmation is required

A local-only project never discloses its context to a remote endpoint.

## Supported context categories

- project metadata
- task titles
- task planning metadata
- schedule
- habit metadata
- knowledge metadata
- knowledge bodies
- resource metadata
- activity history

Resource file bytes and device-local resource paths are not context categories and are not implicitly sent.

## Provider boundary

`orbit-ai` supports provider configurations for OpenAI, Anthropic, Gemini, OpenAI-compatible endpoints, and Ollama. Native embedded/CLI composition can use the Rustls-backed direct HTTP transport. Endpoint/model/auth remain configuration, not hard-wired product choices.

## AI cannot mutate canonical state directly

AI planning output is JSON proposal data only. Orbit parses it into a planner proposal and runs the same deterministic validation used for non-AI proposals. Flutter receives a validated proposal DTO for preview. `apply_task_plan` revalidates against current canonical state before persistence.
