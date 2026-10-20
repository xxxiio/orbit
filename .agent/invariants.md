---
template: invariant-register
template_version: 1.0.0
template_digest: "a3082d617e3b1ed122a5785f4244c7576a4f2ee7b5b690c225620ab0e0513245"
project_owned: true
record_contract: invariant-register
record_schema_version: 1
status: current
---

# System Invariants

## Invariants

| ID | Invariant | Scope | Enforcement | Review trigger | Status |
|---|---|---|---|---|---|
| INV-0001 | A client pending command or preview is never treated as accepted authoritative state | Connected client and server | Command/receipt tests, offline and lost-ack smokes | Any change to outbox, receipts or replay | active |
| INV-0002 | Agent job/session completion never completes an Orbit task automatically | Agent runtime, Herdr, worker and review surfaces | `agent_execution_contract`; live Herdr smoke asserts the task stays open | Any agent review/completion change | active |
| INV-0003 | A blueprint manifest cannot grant itself capability execution rights | Blueprint publish and capability execution | Blueprint validation and empty-capability refusal tests | Any change to publish or capability grants | active |
| INV-0004 | Installers remove or overwrite only files recorded in an Orbit-owned manifest | Install/uninstall/release scripts | Packaging manifest-safety and transaction tests | Any installer or manifest change | active |
| INV-0005 | Acceptance tests use disposable databases, prefixes and workspaces; never user data | Tests and acceptance harness | Runner and smoke design; review of every new test | Any new acceptance check | active |
