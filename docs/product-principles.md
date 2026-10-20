# Orbit product and data principles

## Mission

Orbit exists to keep a person's life and useful knowledge organized in one system: commitments, projects, tasks, habits, schedule, progress, and structured knowledge. AI may automate organization and planning, but the product remains useful without AI.

## Structured knowledge, not a generic workspace builder

Orbit is not trying to reproduce all of Notion or Obsidian. It provides a preset semantic knowledge model optimized for ongoing life/work execution. Users get strong defaults instead of designing arbitrary databases or folder conventions before the system becomes useful.

Knowledge and execution are equal first-class subsystems connected by typed relationships.

## Local-first and user-owned content

Canonical user content is local by default. The Flutter application embeds the Rust Orbit runtime; a dedicated Orbit backend process or cloud server is not required for normal use.

Orbit should not require first-party collection or centralized storage of user-content fields such as notes, tasks, project details, habit history, schedules, or attachments.

Operational account/payment metadata for an optional future paid service is a separate concern and must be minimized; it must not be conflated with permission to inspect user content.

## AI is optional and user-selected

AI is an adapter, not the engine.

- Local models can keep AI processing fully local.
- Cloud providers are configured by the user through BYOK or another explicit provider choice.
- Orbit controls which local context is eligible to be sent based on user/project policy and should preview/disclose remote context boundaries where practical.
- If the user chooses a cloud AI provider, that provider may process or retain submitted data according to its own terms. Orbit must state this clearly.
- Orbit itself does not need to centrally collect that content in order to orchestrate the request.
- AI proposals never bypass deterministic Orbit validation before mutation.

## MCP is external-agent access, not built-in AI

Built-in AI means Orbit calls a selected model. MCP means an external agent calls Orbit. Both use the same semantic application/knowledge APIs, and neither is allowed to write canonical persistence directly.

## Sync is optional transport

Orbit syncs semantic changes, not a live SQLite database.

User-managed Git is a supported transport. A future first-party sync service, if offered, must be optional and should be content-blind/end-to-end encrypted so the service does not require plaintext user content or user-held decryption keys.
