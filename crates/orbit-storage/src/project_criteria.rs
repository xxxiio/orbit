//! Internal SQLite persistence implementation; not a public API boundary.
use super::*;

impl SqliteStore {
    pub(crate) fn load_project_criteria(
        &self,
        mut project: Project,
    ) -> Result<Project, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT criterion_key, description, required, status FROM project_success_criteria WHERE project_id = ?1 ORDER BY position"
        ).map_err(repo_error)?;
        let rows = statement
            .query_map([project.id().as_str()], |row| {
                let key: String = row.get(0)?;
                let description: String = row.get(1)?;
                let required: i64 = row.get(2)?;
                let status: String = row.get(3)?;
                let mut criterion = ProjectSuccessCriterion::new(key, description, required != 0)
                    .map_err(to_sql_error)?;
                criterion.set_status(SuccessCriterionStatus::parse(&status).map_err(to_sql_error)?);
                Ok(criterion)
            })
            .map_err(repo_error)?;
        let criteria = collect_rows(rows)?;
        project
            .replace_success_criteria(criteria)
            .map_err(|error| RepositoryError::CorruptData {
                detail: error.to_string(),
            })?;
        Ok(project)
    }
}
