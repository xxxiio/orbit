# Authenticated blueprint authority API (M25)

All endpoints below live under `/v1/authoritative/blueprints` and require the existing authenticated device identity. Commands run against the server's canonical `AuthoritativeHost`; connected clients must not directly mutate their local replicas. An untrusted request never supplies installed capabilities or capability permission grants.

| Method | Path | Inputs | Function |
|---|---|---|---|
| GET | `/` | optional `id` query | List published `(id, version)` pairs |
| GET | `/show` | `id`, `version` query | Load an immutable published definition |
| POST | `/` | `{ "definition": <blueprint> }` | Publish manifest (empty capabilities set) |
| GET | `/bundled` | none | Discover bundled IDs |
| POST | `/install` | `{ "id": "development" }` | Explicitly install known bundled definition |
| POST | `/bind` | `project_id`, `id`, `version` JSON | Pin a project to a version |
| GET | `/binding` | `project_id` query | Read binding |
| GET | `/items` | `id` query | Read work item |
| POST | `/items` | `id`, `project_id`, `kind`, `properties` JSON | Create work item |
| POST | `/transition` | `id`, `to`, `expected_revision` JSON | Transition with *no* untrusted capability grants |
| POST | `/properties` | `id`, `expected_revision`, `changes` JSON | Revise properties |
| POST | `/template` | `project_id`, `template_id`, `namespace` JSON | Atomically apply template |

These are explicit online-authority operations. Offline command projection, MCP exposure, and connected CLI transport are separate M25 work. A publication or transition cannot perform arbitrary plugin execution. Security or finer-grained roles are not implied by device authentication alone; the API currently uses the existing device trust model.

Broad compilation and live integration acceptance belong to M15.
