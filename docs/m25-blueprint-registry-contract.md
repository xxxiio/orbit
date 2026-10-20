# M25 blueprint registry contract (Development 280)

A manifest is an immutable `(id,version)` pair after publication. Registries reject
re-publication with different content. Projects pin an explicit version; loading a
new revision **never** upgrades existing projects. Legacy projects remain unbound
until an authorized caller explicitly activates a published blueprint.

`AuthoritativeHost` is the application entry point for publish/import, version
listing, export, and binding. Callers must obtain installed capabilities from
trusted host configuration rather than claims inside uploaded manifests.

The JSON document is the currently supported portable interchange format.
YAML parsing, default blueprint installation policy, connected-client APIs,
workflow execution, and offline authority commands are subsequent M25 scope.

No automatic project migration is performed. A project bound to a blueprint
with revision 1 remains on revision 1 until a future explicit upgrade path.
Broad Rust compilation and integrated verification remain M15 scope.
