# M20 Development 203 — Embedded client read-path consolidation

The local TUI now obtains Markdown term references and project evidence through its
existing `OrbitClient` instance. Previously it created a fresh `AuthoritativeHost`
for each term search and, worse, for every project during each render. This
duplicated connection initialization and allowed TUI-specific application
business calculations to diverge from the client interface.

The shared client now owns both read operations and translates errors into client
contracts. Local-authoritative mode is preserved. Connected mode is unchanged.

This is a source-only consolidation. Rust compilation and broad acceptance are
reserved for M15; M20 completion has **not** been established.
