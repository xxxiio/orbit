//! Domain repositories using the shared local SQLite connection.
use super::super::*;

impl TaskRepository for SqliteStore {
    fn get(&self, id: &TaskId) -> Result<Option<Task>, RepositoryError> {
        let mut task = self
            .connection
            .query_row(
                "SELECT id, title, status, project_id, milestone_id, estimated_minutes, earliest_start_at_ms, deadline_at_ms, description, priority FROM tasks WHERE id = ?1",
                [id.as_str()],
                task_from_row,
            )
            .optional()
            .map_err(repo_error)?;
        if let Some(task) = task.as_mut() {
            self.apply_task_labels(std::slice::from_mut(task))?;
        }
        Ok(task)
    }

    fn list(&self) -> Result<Vec<Task>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, title, status, project_id, milestone_id, estimated_minutes, earliest_start_at_ms, deadline_at_ms, description, priority FROM tasks ORDER BY id")
            .map_err(repo_error)?;
        let mut tasks = collect_rows(statement.query_map([], task_from_row).map_err(repo_error)?)?;
        self.apply_task_labels(&mut tasks)?;
        Ok(tasks)
    }

    fn save(&mut self, task: Task) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                r#"INSERT INTO tasks(id, title, status, project_id, milestone_id, estimated_minutes, earliest_start_at_ms, deadline_at_ms, description, priority)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                   ON CONFLICT(id) DO UPDATE SET
                     title = excluded.title,
                     status = excluded.status,
                     project_id = excluded.project_id,
                     milestone_id = excluded.milestone_id,
                     estimated_minutes = excluded.estimated_minutes,
                     earliest_start_at_ms = excluded.earliest_start_at_ms,
                     deadline_at_ms = excluded.deadline_at_ms,
                     description = excluded.description,
                     priority = excluded.priority"#,
                params![
                    task.id().as_str(),
                    task.title(),
                    task.status().as_str(),
                    task.scope().project_id().map(ProjectId::as_str),
                    task.scope().milestone_id().map(MilestoneId::as_str),
                    task.planning().estimated_minutes().map(i64::from),
                    task.planning().earliest_start_at_ms(),
                    task.planning().deadline_at_ms(),
                    task.details().description(),
                    task.details().priority().map(|priority| priority.as_str()),
                ],
            )
            .map_err(repo_error)?;
        self.replace_task_labels(task.id(), task.details().labels())?;
        Ok(())
    }
}

impl SqliteStore {
    fn replace_task_labels(
        &mut self,
        id: &TaskId,
        labels: &[String],
    ) -> Result<(), RepositoryError> {
        self.connection
            .execute("DELETE FROM task_labels WHERE task_id = ?1", [id.as_str()])
            .map_err(repo_error)?;
        for label in labels {
            self.connection
                .execute(
                    "INSERT OR IGNORE INTO task_labels(task_id, label) VALUES (?1, ?2)",
                    params![id.as_str(), label],
                )
                .map_err(repo_error)?;
        }
        Ok(())
    }

    fn apply_task_labels(&self, tasks: &mut [Task]) -> Result<(), RepositoryError> {
        if tasks.is_empty() {
            return Ok(());
        }
        let mut statement = self
            .connection
            .prepare("SELECT label FROM task_labels WHERE task_id = ?1 ORDER BY label")
            .map_err(repo_error)?;
        for task in tasks.iter_mut() {
            let labels: Vec<String> = statement
                .query_map([task.id().as_str()], |row| row.get::<_, String>(0))
                .map_err(repo_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(repo_error)?;
            let mut details = task.details().clone();
            details
                .set_labels(labels)
                .map_err(|error| RepositoryError::CorruptData {
                    detail: error.to_string(),
                })?;
            task.set_details(details);
        }
        Ok(())
    }
}

impl ProjectRepository for SqliteStore {
    fn get(&self, id: &ProjectId) -> Result<Option<Project>, RepositoryError> {
        let project = self.connection
            .query_row(
                "SELECT id, name, status, mode, objective, progress_signals FROM projects WHERE id = ?1",
                [id.as_str()],
                project_from_row,
            )
            .optional()
            .map_err(repo_error)?;
        project
            .map(|project| self.load_project_criteria(project))
            .transpose()
    }

    fn list(&self) -> Result<Vec<Project>, RepositoryError> {
        let projects = {
            let mut statement = self.connection
                .prepare("SELECT id, name, status, mode, objective, progress_signals FROM projects ORDER BY id")
                .map_err(repo_error)?;
            collect_rows(
                statement
                    .query_map([], project_from_row)
                    .map_err(repo_error)?,
            )?
        };
        projects
            .into_iter()
            .map(|project| self.load_project_criteria(project))
            .collect()
    }

    fn save(&mut self, project: Project) -> Result<(), RepositoryError> {
        let signals = project
            .progress_signals()
            .iter()
            .map(|signal| signal.as_str())
            .collect::<Vec<_>>()
            .join(",");
        self.connection.execute(
            r#"INSERT INTO projects(id, name, status, mode, objective, progress_signals) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
               ON CONFLICT(id) DO UPDATE SET name = excluded.name, status = excluded.status,
                 mode = excluded.mode, objective = excluded.objective, progress_signals = excluded.progress_signals"#,
            params![project.id().as_str(), project.name(), project.status().as_str(), project.mode().as_str(), project.objective(), signals],
        ).map_err(repo_error)?;
        self.connection
            .execute(
                "DELETE FROM project_success_criteria WHERE project_id = ?1",
                [project.id().as_str()],
            )
            .map_err(repo_error)?;
        for (position, criterion) in project.success_criteria().iter().enumerate() {
            self.connection.execute(
                "INSERT INTO project_success_criteria(project_id, position, criterion_key, description, required, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![project.id().as_str(), position as i64, criterion.key(), criterion.description(), if criterion.required() { 1 } else { 0 }, criterion.status().as_str()],
            ).map_err(repo_error)?;
        }
        Ok(())
    }
}

impl MetricEvidenceRepository for SqliteStore {
    fn get_metric_target(
        &self,
        metric_id: &str,
    ) -> Result<Option<MetricTargetRecord>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT baseline, target, direction FROM metric_targets WHERE metric_id = ?1",
                [metric_id],
                |row| {
                    let direction: String = row.get(2)?;
                    let direction = match direction.as_str() {
                        "at_least" => orbit_progress::MetricTargetDirection::AtLeast,
                        "at_most" => orbit_progress::MetricTargetDirection::AtMost,
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    };
                    Ok(MetricTargetRecord {
                        metric_id: metric_id.to_string(),
                        baseline: row.get(0)?,
                        target: row.get(1)?,
                        direction,
                    })
                },
            )
            .optional()
            .map_err(repo_error)
    }
    fn put_metric_target(&mut self, target: &MetricTargetRecord) -> Result<(), RepositoryError> {
        if target.metric_id.trim() != target.metric_id || target.metric_id.is_empty() {
            return Err(RepositoryError::Other("invalid metric identifier".into()));
        }
        if orbit_progress::metric_target_evidence(
            target.baseline,
            target.baseline,
            target.target,
            target.direction,
        )
        .is_err()
        {
            return Err(RepositoryError::Other(
                "invalid metric target direction or baseline".into(),
            ));
        }
        let direction = match target.direction {
            orbit_progress::MetricTargetDirection::AtLeast => "at_least",
            orbit_progress::MetricTargetDirection::AtMost => "at_most",
        };
        self.connection.execute("INSERT INTO metric_targets(metric_id, baseline, target, direction) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(metric_id) DO UPDATE SET baseline=excluded.baseline, target=excluded.target, direction=excluded.direction", params![target.metric_id, target.baseline, target.target, direction]).map_err(repo_error)?;
        Ok(())
    }
    fn append_metric_observation(
        &mut self,
        observation: &MetricObservation,
    ) -> Result<(), RepositoryError> {
        self.connection.execute("INSERT INTO metric_observations(metric_id, observed_at_ms, value) VALUES (?1, ?2, ?3)", params![observation.metric_id, observation.observed_at_ms, observation.value]).map_err(repo_error)?;
        Ok(())
    }
    fn put_metric_observation(
        &mut self,
        observation: &MetricObservation,
    ) -> Result<(), RepositoryError> {
        self.connection.execute(
            "INSERT INTO metric_observations(metric_id, observed_at_ms, value) VALUES (?1, ?2, ?3) ON CONFLICT(metric_id, observed_at_ms) DO UPDATE SET value=excluded.value",
            params![observation.metric_id, observation.observed_at_ms, observation.value],
        ).map_err(repo_error)?;
        Ok(())
    }
    fn latest_metric_observation(
        &self,
        metric_id: &str,
        as_of_ms: i64,
    ) -> Result<Option<MetricObservation>, RepositoryError> {
        self.connection.query_row("SELECT observed_at_ms, value FROM metric_observations WHERE metric_id=?1 AND observed_at_ms<=?2 ORDER BY observed_at_ms DESC LIMIT 1", params![metric_id, as_of_ms], |row| Ok(MetricObservation {metric_id: metric_id.to_string(), observed_at_ms: row.get(0)?, value: row.get(1)?})).optional().map_err(repo_error)
    }
}

impl ProjectEvidenceRepository for SqliteStore {
    fn list_project_evidence(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ProjectEvidenceBinding>, RepositoryError> {
        let mut statement = self.connection.prepare(
            "SELECT evidence_kind, evidence_id FROM project_evidence_bindings WHERE project_id = ?1 ORDER BY evidence_kind, evidence_id"
        ).map_err(repo_error)?;
        let rows = statement
            .query_map([project_id.as_str()], |row| {
                let kind: String = row.get(0)?;
                let id: String = row.get(1)?;
                match kind.as_str() {
                    "habit" => Ok(ProjectEvidenceBinding::Habit(
                        HabitId::new(id).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    )),
                    "metric" => Ok(ProjectEvidenceBinding::Metric(id)),
                    _ => Err(rusqlite::Error::InvalidQuery),
                }
            })
            .map_err(repo_error)?;
        collect_rows(rows)
    }

    fn add_project_evidence(
        &mut self,
        project_id: &ProjectId,
        binding: &ProjectEvidenceBinding,
    ) -> Result<(), RepositoryError> {
        let (kind, id) = match binding {
            ProjectEvidenceBinding::Habit(id) => ("habit", id.as_str()),
            ProjectEvidenceBinding::Metric(id) => ("metric", id.as_str()),
        };
        if id.trim().is_empty() {
            return Err(RepositoryError::CorruptData {
                detail: "blank evidence identifier".into(),
            });
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO project_evidence_bindings(project_id, evidence_kind, evidence_id) VALUES (?1, ?2, ?3)",
            params![project_id.as_str(), kind, id]
        ).map_err(repo_error)?;
        Ok(())
    }

    fn remove_project_evidence(
        &mut self,
        project_id: &ProjectId,
        binding: &ProjectEvidenceBinding,
    ) -> Result<(), RepositoryError> {
        let (kind, id) = match binding {
            ProjectEvidenceBinding::Habit(id) => ("habit", id.as_str()),
            ProjectEvidenceBinding::Metric(id) => ("metric", id.as_str()),
        };
        self.connection.execute(
            "DELETE FROM project_evidence_bindings WHERE project_id = ?1 AND evidence_kind = ?2 AND evidence_id = ?3",
            params![project_id.as_str(), kind, id]
        ).map_err(repo_error)?;
        Ok(())
    }
}

impl MilestoneRepository for SqliteStore {
    fn get(&self, id: &MilestoneId) -> Result<Option<Milestone>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, project_id, name, status FROM milestones WHERE id = ?1",
                [id.as_str()],
                milestone_from_row,
            )
            .optional()
            .map_err(repo_error)
    }

    fn list_for_project(&self, project_id: &ProjectId) -> Result<Vec<Milestone>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, project_id, name, status FROM milestones WHERE project_id = ?1 ORDER BY id",
            )
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([project_id.as_str()], milestone_from_row)
                .map_err(repo_error)?,
        )
    }

    fn save(&mut self, milestone: Milestone) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                r#"INSERT INTO milestones(id, project_id, name, status) VALUES (?1, ?2, ?3, ?4)
                   ON CONFLICT(id) DO UPDATE SET
                     project_id = excluded.project_id,
                     name = excluded.name,
                     status = excluded.status"#,
                params![
                    milestone.id().as_str(),
                    milestone.project_id().as_str(),
                    milestone.name(),
                    milestone.status().as_str()
                ],
            )
            .map_err(repo_error)?;
        Ok(())
    }
}

impl HabitRepository for SqliteStore {
    fn get(&self, id: &HabitId) -> Result<Option<Habit>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, name, cadence_kind, target_per_week, active FROM habits WHERE id = ?1",
                [id.as_str()],
                habit_from_row,
            )
            .optional()
            .map_err(repo_error)
    }

    fn list(&self) -> Result<Vec<Habit>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, cadence_kind, target_per_week, active FROM habits ORDER BY id",
            )
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([], habit_from_row)
                .map_err(repo_error)?,
        )
    }

    fn save(&mut self, habit: Habit) -> Result<(), RepositoryError> {
        let (cadence_kind, target_per_week): (&str, Option<u8>) = match habit.cadence() {
            HabitCadence::Daily => ("daily", None),
            HabitCadence::Weekly { target_per_week } => ("weekly", Some(target_per_week)),
        };
        self.connection
            .execute(
                r#"INSERT INTO habits(id, name, cadence_kind, target_per_week, active)
                   VALUES (?1, ?2, ?3, ?4, ?5)
                   ON CONFLICT(id) DO UPDATE SET
                     name = excluded.name,
                     cadence_kind = excluded.cadence_kind,
                     target_per_week = excluded.target_per_week,
                     active = excluded.active"#,
                params![
                    habit.id().as_str(),
                    habit.name(),
                    cadence_kind,
                    target_per_week,
                    if habit.is_active() { 1_i64 } else { 0_i64 }
                ],
            )
            .map_err(repo_error)?;
        Ok(())
    }
}

impl RoutineRepository for SqliteStore {
    fn get(&self, id: &RoutineId) -> Result<Option<Routine>, RepositoryError> {
        let name: Option<String> = self
            .connection
            .query_row(
                "SELECT name FROM routines WHERE id = ?1",
                [id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        let Some(name) = name else {
            return Ok(None);
        };
        let mut routine =
            Routine::new(id.clone(), name).map_err(|e| RepositoryError::CorruptData {
                detail: e.to_string(),
            })?;
        let mut statement = self
            .connection
            .prepare("SELECT habit_id FROM routine_habits WHERE routine_id = ?1 ORDER BY position")
            .map_err(repo_error)?;
        let rows = statement
            .query_map([id.as_str()], |row| row.get::<_, String>(0))
            .map_err(repo_error)?;
        for row in rows {
            let habit_id = habit_id_from_string(row.map_err(repo_error)?).map_err(|e| {
                RepositoryError::CorruptData {
                    detail: e.to_owned(),
                }
            })?;
            routine
                .add_habit(habit_id)
                .map_err(|e| RepositoryError::CorruptData {
                    detail: e.to_string(),
                })?;
        }
        Ok(Some(routine))
    }

    fn list(&self) -> Result<Vec<Routine>, RepositoryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM routines ORDER BY id")
            .map_err(repo_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(repo_error)?;
        let mut result = Vec::new();
        for row in rows {
            let id = routine_id_from_string(row.map_err(repo_error)?).map_err(|e| {
                RepositoryError::CorruptData {
                    detail: e.to_owned(),
                }
            })?;
            if let Some(routine) = RoutineRepository::get(self, &id)? {
                result.push(routine);
            }
        }
        Ok(result)
    }

    fn save(&mut self, routine: Routine) -> Result<(), RepositoryError> {
        self.connection
            .execute(
                r#"INSERT INTO routines(id, name) VALUES (?1, ?2)
               ON CONFLICT(id) DO UPDATE SET name = excluded.name"#,
                params![routine.id().as_str(), routine.name()],
            )
            .map_err(repo_error)?;
        self.connection
            .execute(
                "DELETE FROM routine_habits WHERE routine_id = ?1",
                [routine.id().as_str()],
            )
            .map_err(repo_error)?;
        for (position, habit_id) in routine.habit_ids().iter().enumerate() {
            self.connection.execute(
                "INSERT INTO routine_habits(routine_id, habit_id, position) VALUES (?1, ?2, ?3)",
                params![routine.id().as_str(), habit_id.as_str(), position as i64],
            ).map_err(repo_error)?;
        }
        Ok(())
    }
}

impl HabitOccurrenceRepository for SqliteStore {
    fn get(&self, id: &HabitOccurrenceId) -> Result<Option<HabitOccurrence>, RepositoryError> {
        self.connection
            .query_row(
                "SELECT id, habit_id, occurred_at_ms, duration_minutes FROM habit_occurrences WHERE id = ?1",
                [id.as_str()],
                habit_occurrence_from_row,
            )
            .optional()
            .map_err(repo_error)
    }

    fn list_for_habit(&self, habit_id: &HabitId) -> Result<Vec<HabitOccurrence>, RepositoryError> {
        let mut statement = self.connection
            .prepare("SELECT id, habit_id, occurred_at_ms, duration_minutes FROM habit_occurrences WHERE habit_id = ?1 ORDER BY occurred_at_ms, id")
            .map_err(repo_error)?;
        collect_rows(
            statement
                .query_map([habit_id.as_str()], habit_occurrence_from_row)
                .map_err(repo_error)?,
        )
    }

    fn append(&mut self, occurrence: HabitOccurrence) -> Result<(), RepositoryError> {
        let result = self.connection.execute(
            "INSERT INTO habit_occurrences(id, habit_id, occurred_at_ms, duration_minutes) VALUES (?1, ?2, ?3, ?4)",
            params![occurrence.id().as_str(), occurrence.habit_id().as_str(), occurrence.occurred_at_ms(), occurrence.duration_minutes()],
        );
        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(RepositoryError::Conflict),
            Err(error) => Err(repo_error(error)),
        }
    }
}
