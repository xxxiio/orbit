# Blueprint CLI (M25)

Blueprint commands are available through the existing `orbit` executable, in **local-authoritative** mode:

```sh
orbit blueprint list
orbit blueprint install development
orbit blueprint import my-blueprint.json
orbit blueprint show development 1
orbit blueprint export development 1 --file development.json
orbit blueprint bind PROJECT_ID development 1
orbit blueprint binding PROJECT_ID
orbit blueprint create-item PROJECT_ID ITEM_ID feature --properties properties.json
orbit blueprint item ITEM_ID
orbit blueprint transition ITEM_ID review --revision 0
orbit blueprint set-properties ITEM_ID --revision 1 --file properties.json
orbit blueprint apply-template PROJECT_ID starter instance-a
```

Use `--json` to obtain structured responses. Import publishes an immutable version;
explicit binding pins a project to a specific published version. Import and install
never bind existing projects automatically. Custom properties are read from JSON
objects and checked by the authoritative blueprint service. No blueprint transition
or template application automatically launches agents.

Connected-mode blueprint commands are intentionally not routed to local SQLite.
Authenticated server, MCP, and connected-mode authority commands remain follow-up
M25 implementation. This module contains only transport argument parsing and
response formatting; blueprint validation and persistence remain host-owned.

## Blueprint authoring formats

Use `orbit blueprint bundled` to list built-in variations. `orbit blueprint import example.yaml` and `orbit blueprint export development 1 --file development.yaml` support YAML; `.yml` is also accepted. JSON remains the default.

In connected mode, mutations automatically stage to the canonical SQLite outbox; use `--offline` to stage without connecting or `--require-online` to require immediate authority acceptance. The legacy `queue` command is not supported.

Validate an authoring file without publishing: `orbit blueprint validate example.yaml` (works without a server). List bundled choices: `orbit blueprint bundled`.
