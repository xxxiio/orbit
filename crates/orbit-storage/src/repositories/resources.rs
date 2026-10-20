//! Scheduling and activity repository adapters; knowledge assets live in Markdown.
use super::super::*;

// Legacy SQL knowledge adapters are intentionally disabled. File-backed
// attachments are published via the Markdown vault's content-addressed store.
impl ScheduleRepository for SqliteStore {
    fn get(&self, id: &ScheduleBlockId) -> Result<Option<ScheduleBlock>, RepositoryError> {
        self.connection
            .query_row(
                r#"SELECT id, title, starts_at_ms, duration_minutes, flexibility, target_kind, target_id
                   FROM schedule_blocks WHERE id = ?1"#,
                [id.as_str()],
                schedule_from_row,
            )
            .optional()
            .map_err(repo_error)
    }

    fn list_between(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<ScheduleBlock>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                r#"SELECT id, title, starts_at_ms, duration_minutes, flexibility, target_kind, target_id
                   FROM schedule_blocks
                   WHERE starts_at_ms < ?2
                     AND (starts_at_ms + duration_minutes * 60000) > ?1
                   ORDER BY starts_at_ms, id"#,
            )
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map(params![start_ms, end_ms], schedule_from_row)
                .map_err(repo_error)?,
        )
    }

    fn save(&mut self, block: ScheduleBlock) -> Result<(), RepositoryError> {
        let flexibility = match block.flexibility() {
            ScheduleFlexibility::Fixed => "fixed",
            ScheduleFlexibility::Movable => "movable",
        };
        let (target_kind, target_id): (&str, Option<&str>) = match block.target() {
            ScheduleTarget::Standalone => ("standalone", None),
            ScheduleTarget::Task(id) => ("task", Some(id.as_str())),
            ScheduleTarget::Habit(id) => ("habit", Some(id.as_str())),
        };
        self.connection
            .execute(
                r#"INSERT INTO schedule_blocks(
                     id, title, starts_at_ms, duration_minutes, flexibility, target_kind, target_id
                   ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                   ON CONFLICT(id) DO UPDATE SET
                     title = excluded.title,
                     starts_at_ms = excluded.starts_at_ms,
                     duration_minutes = excluded.duration_minutes,
                     flexibility = excluded.flexibility,
                     target_kind = excluded.target_kind,
                     target_id = excluded.target_id"#,
                params![
                    block.id().as_str(),
                    block.title(),
                    block.starts_at_ms(),
                    block.duration_minutes(),
                    flexibility,
                    target_kind,
                    target_id
                ],
            )
            .map_err(repo_error)?;
        Ok(())
    }
}

impl ActivityRepository for SqliteStore {
    fn get(&self, id: &ActivityId) -> Result<Option<ActivityRecord>, RepositoryError> {
        self.connection
            .query_row(
                r#"SELECT id, subject_kind, subject_id, activity_kind, activity_kind_value,
                          origin_kind, origin_value, occurred_at_ms, summary
                   FROM activities WHERE id = ?1"#,
                [id.as_str()],
                activity_from_row,
            )
            .optional()
            .map_err(repo_error)
    }

    fn list(&self) -> Result<Vec<ActivityRecord>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                r#"SELECT id, subject_kind, subject_id, activity_kind, activity_kind_value,
                          origin_kind, origin_value, occurred_at_ms, summary
                   FROM activities ORDER BY sequence"#,
            )
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([], activity_from_row)
                .map_err(repo_error)?,
        )
    }

    fn append(&mut self, activity: ActivityRecord) -> Result<(), RepositoryError> {
        let (subject_kind, subject_id) = encode_subject(activity.subject());
        let (activity_kind, activity_kind_value) = encode_activity_kind(activity.kind());
        let (origin_kind, origin_value) = encode_origin(activity.origin());

        let result = self.connection.execute(
            r#"INSERT INTO activities(
                 id, subject_kind, subject_id, activity_kind, activity_kind_value,
                 origin_kind, origin_value, occurred_at_ms, summary
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                activity.id().as_str(),
                subject_kind,
                subject_id,
                activity_kind,
                activity_kind_value,
                origin_kind,
                origin_value,
                activity.occurred_at_ms(),
                activity.summary()
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(RepositoryError::Conflict),
            Err(error) => Err(repo_error(error)),
        }
    }
}
