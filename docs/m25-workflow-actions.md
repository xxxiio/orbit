# M25 — Declarative workflow action contract

Transitions may now declare `actions` with an ID, a registered capability ID, a description, and a fixed JSON parameter map. An action is **offered** by a blueprint, not executed when the transition occurs. A valid manifest must declare each action capability in `required_capabilities`, and installation must provide it.

The blueprint crate offers `offered_actions` for discovery and `plan_explicit_action` for *explicit*, host-authorized invocation planning. `plan_explicit_action` requires an available capability, a host grant, and a caller-supplied command ID. It returns a description of an intention, not permission to bypass server authorization or dispatch an external effect.

The authoritative application is responsible for validating the project's pinned blueprint, the item and transition revision, the user's permissions, the unique command ID, and idempotent dispatch. There is deliberately no plugin execution inside `orbit-blueprint`. In particular, moving a work item into `implementing` does not launch Herdr. Additional server-side action command routing and plugin invocation remain M25 implementation work.

Example transition action:

```json
{"from":"ready","to":"implementing","guards":[],"actions":[{"id":"start_agent","capability":"agent.launch","description":"Start an implementation agent","parameters":{"agent":"codex"}}]}
```

This is a manifest format example; it requires `agent.launch` in the blueprint's `required_capabilities` and an installed provider and explicit authority grant at invocation.
