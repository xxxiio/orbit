//! Ordered task acceptance criteria persistence.
use super::super::*;
use orbit_domain::AcceptanceCriterion;
impl AcceptanceCriteriaRepository for SqliteStore {
    fn list_acceptance_criteria(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<AcceptanceCriterion>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT criterion_id, text, completed_at_ms FROM task_acceptance_criteria \
                 WHERE task_id = ?1 ORDER BY position",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map([task_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })
            .map_err(repo_error)?;
        let mut criteria = Vec::new();
        for row in rows {
            let (id, text, completed_at_ms) = row.map_err(repo_error)?;
            let criterion = AcceptanceCriterion::restored(
                AcceptanceCriterionId::new(id).map_err(|error| RepositoryError::CorruptData {
                    detail: error.to_string(),
                })?,
                text,
                completed_at_ms,
            )
            .map_err(|error| RepositoryError::CorruptData {
                detail: error.to_string(),
            })?;
            criteria.push(criterion);
        }
        Ok(criteria)
    }

    fn replace_acceptance_criteria(
        &mut self,
        task_id: &TaskId,
        criteria: &AcceptanceCriteria,
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                "DELETE FROM task_acceptance_criteria WHERE task_id = ?1",
                [task_id.as_str()],
            )
            .map_err(repo_error)?;
        for (position, criterion) in criteria.items().iter().enumerate() {
            self.connection
                .execute(
                    "INSERT INTO task_acceptance_criteria\
                     (task_id, criterion_id, position, text, completed_at_ms) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        task_id.as_str(),
                        criterion.id().as_str(),
                        position as i64,
                        criterion.text(),
                        criterion.completed_at_ms(),
                    ],
                )
                .map_err(repo_error)?;
        }
        Ok(())
    }
}
