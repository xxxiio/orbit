//! Attachment metadata persistence. Content bytes live outside SQLite.
use super::super::*;
use orbit_domain::{Attachment, AttachmentOwner, AttachmentOwnerKind};

impl AttachmentRepository for SqliteStore {
    fn get_attachment(&self, id: &AttachmentId) -> Result<Option<Attachment>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, owner_kind, owner_id, filename, content_type, size_bytes, \
                 checksum, object_hash, created_at_ms, description \
                 FROM attachments WHERE id = ?1",
                [id.as_str()],
                attachment_from_row,
            )
            .optional()
            .map_err(repo_error)
    }

    fn list_attachments(
        &self,
        owner: &AttachmentOwner,
    ) -> Result<Vec<Attachment>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, owner_kind, owner_id, filename, content_type, size_bytes, \
                 checksum, object_hash, created_at_ms, description \
                 FROM attachments WHERE owner_kind = ?1 AND owner_id = ?2 \
                 ORDER BY created_at_ms, id",
            )
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map(
                    params![owner.kind().as_str(), owner.id()],
                    attachment_from_row,
                )
                .map_err(repo_error)?,
        )
    }

    fn save_attachment(&mut self, attachment: &Attachment) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT INTO attachments\
                 (id, object_hash, owner_kind, owner_id, filename, content_type, size_bytes, \
                  checksum, description, created_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    attachment.id().as_str(),
                    attachment.object(),
                    attachment.owner().kind().as_str(),
                    attachment.owner().id(),
                    attachment.filename(),
                    attachment.content_type(),
                    i64::try_from(attachment.size_bytes()).map_err(|_| {
                        RepositoryError::CorruptData {
                            detail: "attachment size exceeds SQLite integer range".to_owned(),
                        }
                    })?,
                    attachment.checksum(),
                    attachment.description(),
                    attachment.created_at_ms(),
                ],
            )
            .map_err(|error| {
                if is_unique_violation(&error) {
                    RepositoryError::Conflict
                } else {
                    repo_error(error)
                }
            })?;
        Ok(())
    }

    fn delete_attachment(&self, id: &AttachmentId) -> Result<bool, RepositoryError> {
        let affected = self
            .connection
            .execute("DELETE FROM attachments WHERE id = ?1", [id.as_str()])
            .map_err(repo_error)?;
        Ok(affected > 0)
    }

    fn attachment_object(
        &self,
        object_hash: &str,
    ) -> Result<Option<AttachmentObjectRecord>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT object_hash, size_bytes, created_at_ms FROM attachment_objects \
                 WHERE object_hash = ?1",
                [object_hash],
                |row| {
                    let size: i64 = row.get(1)?;
                    Ok(AttachmentObjectRecord {
                        object_hash: row.get(0)?,
                        size_bytes: u64::try_from(size).unwrap_or(0),
                        created_at_ms: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(repo_error)
    }

    fn record_attachment_object(
        &mut self,
        object: &AttachmentObjectRecord,
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "INSERT OR IGNORE INTO attachment_objects(object_hash, size_bytes, created_at_ms) \
                 VALUES (?1, ?2, ?3)",
                params![
                    object.object_hash,
                    i64::try_from(object.size_bytes).map_err(|_| RepositoryError::CorruptData {
                        detail: "attachment size exceeds SQLite integer range".to_owned(),
                    })?,
                    object.created_at_ms,
                ],
            )
            .map_err(repo_error)?;
        Ok(())
    }

    fn attachment_object_referenced(&self, object_hash: &str) -> Result<bool, RepositoryError> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM attachments WHERE object_hash = ?1)",
                [object_hash],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .map_err(repo_error)
    }

    fn delete_attachment_object(&mut self, object_hash: &str) -> Result<bool, RepositoryError> {
        let affected = self
            .connection
            .execute(
                "DELETE FROM attachment_objects WHERE object_hash = ?1",
                [object_hash],
            )
            .map_err(repo_error)?;
        Ok(affected > 0)
    }

    fn unreferenced_attachment_objects(
        &self,
    ) -> Result<Vec<AttachmentObjectRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT object_hash, size_bytes, created_at_ms FROM attachment_objects \
                 WHERE object_hash NOT IN (SELECT DISTINCT object_hash FROM attachments)",
            )
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([], |row| {
                    let size: i64 = row.get(1)?;
                    Ok(AttachmentObjectRecord {
                        object_hash: row.get(0)?,
                        size_bytes: u64::try_from(size).unwrap_or(0),
                        created_at_ms: row.get(2)?,
                    })
                })
                .map_err(repo_error)?,
        )
    }
}

fn attachment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Attachment> {
    let id: String = row.get(0)?;
    let owner_kind: String = row.get(1)?;
    let owner_id: String = row.get(2)?;
    let filename: String = row.get(3)?;
    let content_type: String = row.get(4)?;
    let size_bytes: i64 = row.get(5)?;
    let checksum: String = row.get(6)?;
    let object: String = row.get(7)?;
    let created_at_ms: i64 = row.get(8)?;
    let description: Option<String> = row.get(9)?;

    let owner_kind = AttachmentOwnerKind::parse(&owner_kind)
        .ok_or_else(|| to_sql_error("unknown attachment owner kind"))?;
    let owner = AttachmentOwner::new(owner_kind, owner_id).map_err(to_sql_error)?;
    Attachment::new(
        AttachmentId::new(id).map_err(to_sql_error)?,
        owner,
        filename,
        content_type,
        u64::try_from(size_bytes).map_err(|_| to_sql_error("invalid attachment size"))?,
        checksum,
        object,
        created_at_ms,
        description,
    )
    .map_err(to_sql_error)
}
