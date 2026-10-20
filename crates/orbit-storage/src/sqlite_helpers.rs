//! Internal SQLite persistence implementation; not a public API boundary.
use super::*;

pub(super) fn temporary_sibling(destination: &Path, operation: &str) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("orbit.db");
    destination.with_file_name(format!(
        ".{file_name}.orbit-{operation}-{}.partial",
        process::id()
    ))
}

pub(super) fn sync_parent(path: &Path) -> Result<(), RepositoryError> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        let directory = fs::File::open(parent).map_err(io_repo_error)?;
        directory.sync_all().map_err(io_repo_error)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(super) fn io_repo_error(error: std::io::Error) -> RepositoryError {
    RepositoryError::Other(format!("filesystem error: {error}"))
}

pub(super) fn repo_error(error: rusqlite::Error) -> RepositoryError {
    match error {
        rusqlite::Error::FromSqlConversionFailure(_, _, source) => RepositoryError::CorruptData {
            detail: source.to_string(),
        },
        other => RepositoryError::Other(other.to_string()),
    }
}

pub(super) fn to_sql_error(error: impl ToString) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}

pub(super) fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
    )
}
