# Development 231: M23-I1 authority checkpoint

- Validate queued agent reservations against a real task, project scope, and executable task state in the same SQLite transaction.
- Claim a prepared job exactly once for an external launch, including durable claim token and audit event.
- Record uncertain launch outcomes separately from confirmed Herdr session identity. Never blindly retry an ambiguous launch.
- List jobs by owning project; preserve separation between job state and Orbit task status.
- Add Rust tests for non-ready task, missing task, claim uniqueness and ambiguous result reconciliation.

Verification: Python sqlite3 DDL/constraint smoke tests passed; Rust cargo toolchain is unavailable, so compilation and Rust tests **have not run**. No accepted-command wiring, agent runtime, or user-facing launch is claimed.

M23: 13% provisional, up from 8% (+5 percentage points). M23-I1: 65% provisional. M22: 100% source scope (unchanged). M15: acceptance pending (unchanged). I1 remains open.
