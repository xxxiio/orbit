# Design a custom project workflow

Project Blueprints describe *how* a project operates; they are not separate hardcoded Rust project types. A blueprint can define work-item types, typed properties, allowed states and transitions, declarative guards, prerequisites and optional actions. Plugin capabilities are separate and must be authorized by the host.

## Workflow

1. Explore available examples: `orbit blueprint bundled`.
2. Export an existing definition: `orbit blueprint export development 1 --file development.yaml`.
3. Copy the file, change its identifier and version, then adjust work-item definitions, property defaults and transitions. Leave required capability declarations out unless the runtime provides them.
4. Validate without publishing: `orbit blueprint validate my-workflow.yaml`.
5. Publish: `orbit blueprint import my-workflow.yaml`.
6. Bind a project explicitly to the new blueprint ID and version: `orbit blueprint bind PROJECT-ID BLUEPRINT-ID VERSION`.
7. Create items with `orbit blueprint create-item PROJECT-ID ITEM-ID TYPE --properties props.json`; inspect using `orbit blueprint item ITEM-ID`.
8. Transition only along permitted edges, using the item's current revision: `orbit blueprint transition ITEM-ID STATE --revision REVISION`.
9. Apply a validated starter template: `orbit blueprint apply-template PROJECT-ID TEMPLATE-ID INSTANCE-NAMESPACE`.
10. If the blueprint offers a trusted capability action, install the capability before publishing, then authorize it for the project and request the action explicitly:

   ```sh
   orbit blueprint capability install CAPABILITY-ID
   orbit blueprint import my-workflow.yaml
   orbit blueprint capability authorize CAPABILITY-ID --project PROJECT-ID --confirm
   orbit blueprint action offers ITEM-ID
   orbit blueprint action request ITEM-ID --action ACTION-ID --revision REVISION \
     --command COMMAND-ID --input '{"note":"first durable note"}'
   orbit blueprint action execute COMMAND-ID
   orbit blueprint item ITEM-ID
   ```

   `orbit blueprint action offers` reports `available` or an unavailable reason code. A request is durable intent; only `execute` dispatches the trusted provider. See [Build your own project workflow](custom-blueprints.md#4-offer-a-trusted-capability-action) for a complete runnable example and the [developer guide](../developers/extensions-and-capabilities.md) for provider rules.

These IDs are placeholders, not literal values. Use `orbit blueprint --help` for the installed build's current options.

## Design practices

- Publish immutable versions; project bindings remain pinned until changed explicitly.
- Prefer small lifecycle graphs and explicit guards over arbitrary scripts.
- Create separate templates for reusable starter structures.
- Do not expect state transitions to launch agents automatically: external actions require explicit authority acceptance, permission grants and executor availability.
- In connected mode, an offline edit can be pending rather than applied. Inspect `orbit blueprint pending` and synchronize with `orbit blueprint flush`.
