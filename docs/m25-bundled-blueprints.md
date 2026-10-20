# Built-in project blueprint packages (M25)

The optional blueprint packages under `blueprints/` are declarative manifests, not Rust domain subclasses or privileged plugins. `development`, `learning`, `habits`, and `research` use the same versioned schema, property validation, guarded workflow transitions, and deterministic template planning as user-authored blueprints.

`orbit_blueprint::bundled::BUNDLED_BLUEPRINTS` supplies installation metadata and `bundled_blueprint(id)` parses a manifest. The authoritative host exposes `install_bundled_project_blueprint(id)` for explicit publication; it **never** binds an existing project or changes its pinned version. Applications must separately request binding through the existing authority method.

The habits package deliberately models activities and explicit check-ins only. It does not pretend to implement a recurrence scheduler, automatic streaks, or reminders. Those remain optional capability plugins. The learning package does not require cloud models or automatic assessment. Development workflows likewise require no Git/Herdr installation unless the user opts into that capability separately.

The package manifest format is also available to third-party authors through JSON/YAML import, so installing a built-in blueprint is not a privileged workflow type. Actions declared in blueprints are offers, not grants or automatic external executions.

This is M25 source implementation. Executable release acceptance of the Rust workspace and server remains tracked under M15.
