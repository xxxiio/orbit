---
template: dependency-register
template_version: 2.0.0
template_digest: "ec93524b18a5f226fcf7a0e496dc3b949eed9fbf69314bd8be7f70f53ad4abbf"
project_owned: true
record_contract: dependency-register
record_schema_version: 1
status: current
---

# Dependency Register

| ID | Work item | Dependency | Type | Owner | Required by | Verification | Status |
|---|---|---|---|---|---|---|---|
| DEP-0001 | M25-I3 capability actions | Existing blueprint action intent storage and executor registry | internal | maintainers | NEXT-0001 | Source inspection; `blueprint_execution.rs` and `blueprint_actions.rs` compile | available |
| DEP-0002 | M25-I3 deterministic acceptance | Fake agent runtime with durable launch fencing | internal | maintainers | NEXT-0001 | Deterministic tests in `agent_execution_contract` | available |
| DEP-0003 | TLS/self-hosted acceptance | Docker daemon plus disposable certificates | external | operators | Deferred M15 gates | `docker version` and an ingress smoke | blocked |
| DEP-0004 | Remote-machine agent acceptance | Second enrolled machine with Herdr | external | operators | Deferred M15 gates | Real launch/reconnect/review smoke | blocked |
| DEP-0005 | Public release | Signing key/channel and target-platform builders | external | release owners | Deferred M26 gates | Signed archive verification and target install | pending |
