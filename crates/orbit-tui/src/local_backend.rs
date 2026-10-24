//! Local deployment construction for the shared shell.
//!
//! This is the only place the TUI opens an embedded local authority. It lives
//! in its own module so `backend.rs` (which also owns the connected
//! constructors) cannot reach a local store or the local facade by accident:
//! connected deployments are remote-only through
//! `AuthorityConnection::remote`.
use crate::backend::Backend;
use orbit_client::{ClientError, OrbitClient};

impl Backend {
    /// Open the embedded local authority. The local authority commits
    /// synchronously, so the shell has no outbox or per-command receipt here.
    pub fn open_local(path: impl AsRef<std::path::Path>) -> Result<Self, ClientError> {
        Ok(Self::Local(OrbitClient::open(path)?))
    }
}
