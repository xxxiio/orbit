# M20 Development 188 — Milestone-linked authoritative tasks (partial gate)

This source iteration closes a concrete gap between the milestone lifecycle and the task authority: tasks can now be attached to an existing milestone on their own project, detached, and read with their milestone assignment in the accepted snapshot. Moving an attached task to a different project is rejected until the milestone is explicitly detached. The existing milestone move/delete constraints are now reachable through canonical task commands.

This does **not** close M20. Canonical project-strategy commands, CLI/TUI/MCP integration, and executable Rust acceptance remain incomplete. The milestone percentage is held at 83% pending runnable verification. Do not treat source checks as compiled tests.
