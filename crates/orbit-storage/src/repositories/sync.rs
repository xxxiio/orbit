//! Durable semantic sync outbox repository implementation.
//! Transaction orchestration remains in the UnitOfWork implementation.
use super::super::*;

impl SyncOutboxRepository for SqliteStore {
    fn initialize_sync_device(&mut self, device_id: &str) -> Result<(), RepositoryError> {
        if device_id.trim().is_empty() {
            return Err(RepositoryError::Other(
                "sync device id cannot be blank".into(),
            ));
        }
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT device_id FROM sync_local_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        match existing {
            Some(existing) if existing == device_id => Ok(()),
            Some(_) => Err(RepositoryError::Conflict),
            None => {
                self.connection.execute(
                    "INSERT INTO sync_local_state(id, device_id, next_sequence) VALUES (1, ?1, 1)",
                    [device_id],
                ).map_err(repo_error)?;
                Ok(())
            }
        }
    }

    fn sync_device(&self) -> Result<Option<(String, u64)>, RepositoryError> {
        let row: Option<(String, i64)> = self
            .connection
            .query_row(
                "SELECT device_id, next_sequence FROM sync_local_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(repo_error)?;
        row.map(|(device, next)| {
            let next = u64::try_from(next).map_err(|_| RepositoryError::CorruptData {
                detail: "invalid sync next_sequence".into(),
            })?;
            if next == 0 {
                return Err(RepositoryError::CorruptData {
                    detail: "sync next_sequence must be positive".into(),
                });
            }
            Ok((device, next))
        })
        .transpose()
    }

    fn enqueue_semantic_change(
        &mut self,
        recorded_at_ms: i64,
        change: PendingSemanticChange,
    ) -> Result<ReservedSemanticChange, RepositoryError> {
        self.connection
            .execute_batch("SAVEPOINT orbit_sync_enqueue")
            .map_err(repo_error)?;
        let result = (|| -> Result<ReservedSemanticChange, RepositoryError> {
            let (device_id, next_sequence) = self
                .sync_device()?
                .ok_or_else(|| RepositoryError::Other("sync device is not initialized".into()))?;
            let sequence = i64::try_from(next_sequence)
                .map_err(|_| RepositoryError::Other("sync sequence overflow".into()))?;
            self.connection.execute(
                "INSERT INTO sync_outbox(device_id, sequence, recorded_at_ms, entity_kind, entity_id, exported_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![device_id.as_str(), sequence, recorded_at_ms, change.entity_kind.as_str(), change.entity_id.as_str()],
            ).map_err(repo_error)?;
            for (position, mutation) in change.mutations.iter().enumerate() {
                let base: Option<(String, i64)> = self.connection.query_row(
                    "SELECT device_id, sequence FROM sync_field_versions WHERE entity_kind = ?1 AND entity_id = ?2 AND field = ?3",
                    params![change.entity_kind.as_str(), change.entity_id.as_str(), mutation.field.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                ).optional().map_err(repo_error)?;
                self.connection.execute(
                    "INSERT INTO sync_outbox_fields(device_id, sequence, position, field, value, base_device_id, base_sequence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![device_id.as_str(), sequence, position as i64, mutation.field.as_str(), mutation.value.as_deref(), base.as_ref().map(|v| v.0.as_str()), base.as_ref().map(|v| v.1)],
                ).map_err(repo_error)?;
            }
            let following = next_sequence
                .checked_add(1)
                .ok_or_else(|| RepositoryError::Other("sync sequence overflow".into()))?;
            self.connection.execute(
                "UPDATE sync_local_state SET next_sequence = ?1 WHERE id = 1 AND device_id = ?2",
                params![i64::try_from(following).map_err(|_| RepositoryError::Other("sync sequence overflow".into()))?, device_id.as_str()],
            ).map_err(repo_error)?;
            Ok(ReservedSemanticChange {
                device_id,
                sequence: next_sequence,
                recorded_at_ms,
                change,
                exported_at_ms: None,
            })
        })();
        match result {
            Ok(value) => {
                self.connection
                    .execute_batch("RELEASE SAVEPOINT orbit_sync_enqueue")
                    .map_err(repo_error)?;
                Ok(value)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK TO SAVEPOINT orbit_sync_enqueue; RELEASE SAVEPOINT orbit_sync_enqueue;");
                Err(error)
            }
        }
    }

    fn list_pending_semantic_changes(
        &self,
    ) -> Result<Vec<ReservedSemanticChange>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT device_id, sequence, recorded_at_ms, entity_kind, entity_id, exported_at_ms FROM sync_outbox WHERE exported_at_ms IS NULL ORDER BY device_id, sequence"
        ).map_err(repo_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })
            .map_err(repo_error)?;
        let mut result = Vec::new();
        for row in rows {
            let (device_id, sequence_raw, recorded_at_ms, kind_raw, entity_id, exported_at_ms) =
                row.map_err(repo_error)?;
            let sequence =
                u64::try_from(sequence_raw).map_err(|_| RepositoryError::CorruptData {
                    detail: "invalid sync outbox sequence".into(),
                })?;
            let mut field_statement = self.connection.prepare(
                "SELECT field, value, base_device_id, base_sequence FROM sync_outbox_fields WHERE device_id = ?1 AND sequence = ?2 ORDER BY position"
            ).map_err(repo_error)?;
            let field_rows = field_statement
                .query_map(params![device_id.as_str(), sequence_raw], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                })
                .map_err(repo_error)?;
            let mutations = field_rows
                .map(|row| {
                    let (field, value, base_device_id, base_sequence_raw) =
                        row.map_err(repo_error)?;
                    let base_sequence = base_sequence_raw
                        .map(|value| {
                            u64::try_from(value).map_err(|_| RepositoryError::CorruptData {
                                detail: "invalid sync base sequence".into(),
                            })
                        })
                        .transpose()?;
                    Ok(SemanticFieldMutation {
                        field,
                        value,
                        base_device_id,
                        base_sequence,
                    })
                })
                .collect::<Result<Vec<_>, RepositoryError>>()?;
            let change = PendingSemanticChange::new(
                SemanticEntityKind::parse(&kind_raw)?,
                entity_id,
                mutations,
            )?;
            result.push(ReservedSemanticChange {
                device_id,
                sequence,
                recorded_at_ms,
                change,
                exported_at_ms,
            });
        }
        Ok(result)
    }

    fn enqueue_conflict_resolution(
        &mut self,
        recorded_at_ms: i64,
        resolution: PendingConflictResolution,
    ) -> Result<ReservedConflictResolution, RepositoryError> {
        self.connection
            .execute_batch("SAVEPOINT orbit_sync_resolution_enqueue")
            .map_err(repo_error)?;
        let result = (|| -> Result<ReservedConflictResolution, RepositoryError> {
            let (device_id, next_sequence) = self
                .sync_device()?
                .ok_or_else(|| RepositoryError::Other("sync device is not initialized".into()))?;
            let sequence = i64::try_from(next_sequence)
                .map_err(|_| RepositoryError::Other("sync sequence overflow".into()))?;
            self.connection.execute(
                "INSERT INTO sync_resolution_outbox(device_id, sequence, recorded_at_ms, conflict_id_json, value, exported_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![device_id.as_str(), sequence, recorded_at_ms, resolution.conflict_id_json.as_str(), resolution.value.as_deref()],
            ).map_err(repo_error)?;
            let following = next_sequence
                .checked_add(1)
                .ok_or_else(|| RepositoryError::Other("sync sequence overflow".into()))?;
            self.connection.execute(
                "UPDATE sync_local_state SET next_sequence = ?1 WHERE id = 1 AND device_id = ?2",
                params![i64::try_from(following).map_err(|_| RepositoryError::Other("sync sequence overflow".into()))?, device_id.as_str()],
            ).map_err(repo_error)?;
            Ok(ReservedConflictResolution {
                device_id,
                sequence: next_sequence,
                recorded_at_ms,
                resolution,
                exported_at_ms: None,
            })
        })();
        match result {
            Ok(value) => {
                self.connection
                    .execute_batch("RELEASE SAVEPOINT orbit_sync_resolution_enqueue")
                    .map_err(repo_error)?;
                Ok(value)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK TO SAVEPOINT orbit_sync_resolution_enqueue; RELEASE SAVEPOINT orbit_sync_resolution_enqueue;");
                Err(error)
            }
        }
    }

    fn list_pending_conflict_resolutions(
        &self,
    ) -> Result<Vec<ReservedConflictResolution>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT device_id, sequence, recorded_at_ms, conflict_id_json, value, exported_at_ms FROM sync_resolution_outbox WHERE exported_at_ms IS NULL ORDER BY device_id, sequence"
        ).map_err(repo_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })
            .map_err(repo_error)?;
        rows.map(|row| {
            let (device_id, sequence_raw, recorded_at_ms, conflict_id_json, value, exported_at_ms) =
                row.map_err(repo_error)?;
            let sequence =
                u64::try_from(sequence_raw).map_err(|_| RepositoryError::CorruptData {
                    detail: "invalid sync resolution sequence".into(),
                })?;
            Ok(ReservedConflictResolution {
                device_id,
                sequence,
                recorded_at_ms,
                resolution: PendingConflictResolution::new(conflict_id_json, value)?,
                exported_at_ms,
            })
        })
        .collect()
    }

    fn mark_conflict_resolution_exported(
        &mut self,
        device_id: &str,
        sequence: u64,
        exported_at_ms: i64,
    ) -> Result<(), RepositoryError> {
        let changed = self.connection.execute(
            "UPDATE sync_resolution_outbox SET exported_at_ms = COALESCE(exported_at_ms, ?3) WHERE device_id = ?1 AND sequence = ?2",
            params![device_id, i64::try_from(sequence).map_err(|_| RepositoryError::Other("sync sequence overflow".into()))?, exported_at_ms],
        ).map_err(repo_error)?;
        if changed == 0 {
            return Err(RepositoryError::Unavailable);
        }
        Ok(())
    }

    fn mark_semantic_change_exported(
        &mut self,
        device_id: &str,
        sequence: u64,
        exported_at_ms: i64,
    ) -> Result<(), RepositoryError> {
        let changed = self.connection.execute(
            "UPDATE sync_outbox SET exported_at_ms = COALESCE(exported_at_ms, ?3) WHERE device_id = ?1 AND sequence = ?2",
            params![device_id, i64::try_from(sequence).map_err(|_| RepositoryError::Other("sync sequence overflow".into()))?, exported_at_ms],
        ).map_err(repo_error)?;
        if changed == 0 {
            return Err(RepositoryError::Unavailable);
        }
        Ok(())
    }

    fn set_known_sync_field_version(
        &mut self,
        version: SyncFieldVersion,
    ) -> Result<(), RepositoryError> {
        if version.entity_id.trim().is_empty()
            || version.field.trim().is_empty()
            || version.device_id.trim().is_empty()
            || version.sequence == 0
        {
            return Err(RepositoryError::Other("invalid sync field version".into()));
        }
        self.connection.execute(
            r#"INSERT INTO sync_field_versions(entity_kind, entity_id, field, device_id, sequence)
               VALUES (?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(entity_kind, entity_id, field) DO UPDATE SET
                 device_id = excluded.device_id, sequence = excluded.sequence"#,
            params![
                version.entity_kind.as_str(),
                version.entity_id,
                version.field,
                version.device_id,
                i64::try_from(version.sequence).map_err(|_| RepositoryError::Other("sync sequence overflow".into()))?,
            ],
        ).map_err(repo_error)?;
        Ok(())
    }
}
