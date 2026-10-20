# Development 289 — Blueprint CLI management

Added the first complete public local-authority `orbit blueprint` command group:
version discovery, bundled installation, JSON publication, inspection, export,
explicit project binding, work-item creation/inspection, guarded transition,
property update, and atomic template application. All operations delegate through
`AuthoritativeHost`, without opening an independent persistence authority.

M25 provisional source progress: 64% → 70% (+6 percentage points).
M15 remains at 5%. Broad Rust compilation, full server integration, and live
acceptance belong to M15. Connected API, MCP, and offline blueprint commands remain
outstanding under M25 feature implementation.
