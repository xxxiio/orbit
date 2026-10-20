//! Internal SQLite persistence implementation; not a public API boundary.
use super::*;

// Domain-specific persistence adapters live in repositories/.
// An authoritative command may already hold BEGIN IMMEDIATE on this same
// connection. Rich application use cases must participate in that transaction,
// not try to open a competing transaction or commit the outer command early.
// SQLite savepoints provide the same rollback guarantees in both contexts.
impl UnitOfWork for SqliteStore {
    fn transact<T, F>(&mut self, operation: F) -> Result<T, ApplicationError>
    where
        Self: Sized,
        F: FnOnce(&mut Self) -> Result<T, ApplicationError>,
    {
        // The identical savepoint protocol works under an outer transaction and
        // in autocommit mode. RELEASE in autocommit mode commits this use case;
        // RELEASE under an authority transaction leaves the final commit under
        // the authority command engine's exclusive control.
        self.connection
            .execute_batch("SAVEPOINT orbit_application_uow")
            .map_err(repo_error)?;
        let result = operation(self);
        match result {
            Ok(value) => {
                if let Err(error) = self
                    .connection
                    .execute_batch("RELEASE SAVEPOINT orbit_application_uow")
                {
                    let _ = self.connection.execute_batch("ROLLBACK TO SAVEPOINT orbit_application_uow; RELEASE SAVEPOINT orbit_application_uow");
                    return Err(ApplicationError::Repository(repo_error(error)));
                }
                Ok(value)
            }
            Err(error) => {
                self.connection.execute_batch(
                    "ROLLBACK TO SAVEPOINT orbit_application_uow; RELEASE SAVEPOINT orbit_application_uow",
                ).map_err(repo_error)?;
                Err(error)
            }
        }
    }
}
