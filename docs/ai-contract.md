# Orbit AI integration contract

Orbit AI is optional. Orbit execution, planning, sync, knowledge, CLI, and embedded client behavior must remain usable with AI disabled.

## Direction

Orbit-owned AI features call provider adapters through `orbit-ai`. External coding/automation agents interact with Orbit through MCP; MCP and built-in AI do not depend on each other.

## BYOK and local AI

`AiProviderConfig` describes provider identity, endpoint, model, capabilities, auth mode, and whether a configuration is local-only. Raw credentials are not part of the configuration object; it contains only an opaque secret reference that platform composition resolves from secure storage at invocation time.

Initial provider identities include OpenAI, Anthropic, Gemini, OpenAI-compatible endpoints, and Ollama. No provider is mandatory.

Local-only configurations fail closed when configured with a non-loopback endpoint.

## Provider capabilities

Capabilities are explicit rather than inferred:

- chat
- tools
- structured output
- embeddings
- vision

An Orbit workflow must check the capabilities it requires before invocation.

## State mutation rule

AI is never an authoritative persistence writer. AI may return text, structured classifications, or proposed task/schedule/knowledge changes. Any mutation must be represented as a typed proposal/command and pass normal Orbit deterministic validation/application rules before canonical state changes.

## Data disclosure

A later access-policy/context-disclosure layer will determine which Orbit context is eligible for a provider invocation. Cloud-provider use is a user choice. When context is sent to a user-selected external provider, that provider's data handling applies; Orbit itself does not need a first-party backend to store the user's canonical personal data.
