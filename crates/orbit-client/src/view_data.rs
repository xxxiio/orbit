//! Application-facing resolution of read-only UI view data.
//!
//! The TUI never opens SQLite or recomputes project/task rules. This module
//! maps the shared application/client contracts into renderer-neutral rows.
use crate::{ClientError, ClientErrorCode, OrbitClient};
use orbit_blueprint::presentation::BlueprintView;
use orbit_blueprint::{Blueprint, BlueprintBinding, BlueprintWorkItem};
use orbit_ui::data::*;
use orbit_ui::model::format_epoch_ms;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn invalid(message: impl Into<String>) -> ViewDataError {
    ViewDataError::invalid(message)
}

/// Convert Unix epoch milliseconds to a UTC `YYYY-MM-DD` civil date.
///
/// Dates are display metadata only; clients must not reinterpret them as local
/// time. Implemented with the proleptic Gregorian civil-from-days algorithm so
/// the facade needs no date dependency.
pub fn civil_date_from_ms(epoch_ms: i64) -> String {
    let days = epoch_ms.div_euclid(86_400_000);
    // Howard Hinnant's `civil_from_days`, relative to 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Human-readable UTC due date for a task deadline, empty when unset.
pub fn due_date_label(deadline_at_ms: Option<i64>) -> String {
    deadline_at_ms.map(civil_date_from_ms).unwrap_or_default()
}

/// Application-owned list of domain workflow targets. The renderer only
/// displays these offers; `set_task_status` revalidates them.
pub fn allowed_task_statuses(status: &str) -> Vec<String> {
    let Ok(current) = crate::facade_conversions::parse_task_status(status) else {
        return Vec::new();
    };
    orbit_domain::TaskStatus::ALL
        .into_iter()
        .filter(|next| current.can_transition_to(*next) && *next != current)
        .map(|next| next.as_str().to_owned())
        .collect()
}

fn client_error(error: ClientError) -> ViewDataError {
    ViewDataError::new(format!("{:?}", error.code).to_lowercase(), error.message)
}

impl OrbitClient {
    /// Workflow targets offered for a task status. This delegates to the domain
    /// transition table; it is not a permission grant.
    pub fn task_allowed_statuses(&self, status: &str) -> Vec<String> {
        allowed_task_statuses(status)
    }

    pub fn project_blueprint_binding(
        &self,
        project_id: &str,
    ) -> Result<Option<BlueprintBinding>, ClientError> {
        self.runtime
            .active_project_blueprint(project_id)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn project_blueprint_definition(
        &self,
        project_id: &str,
    ) -> Result<Option<Blueprint>, ClientError> {
        let Some(binding) = self.project_blueprint_binding(project_id)? else {
            return Ok(None);
        };
        self.runtime
            .project_blueprint(&binding.blueprint_id, binding.version)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn install_bundled_blueprint(&mut self, id: &str) -> Result<Blueprint, ClientError> {
        self.runtime
            .install_bundled_project_blueprint(id)
            .map_err(|error| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: format!("{error:?}"),
            })
    }

    pub fn bind_project_blueprint(
        &mut self,
        project_id: &str,
        blueprint_id: &str,
        version: u32,
    ) -> Result<BlueprintBinding, ClientError> {
        self.runtime
            .activate_project_blueprint(project_id, blueprint_id, version)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Conflict,
                message: format!("{error:?}"),
            })
    }

    pub fn list_blueprint_work_items(
        &self,
        project_id: &str,
    ) -> Result<Vec<BlueprintWorkItem>, ClientError> {
        self.runtime
            .project_work_items(project_id)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn blueprint_work_item_prerequisites(
        &self,
        item_id: &str,
    ) -> Result<Vec<String>, ClientError> {
        self.runtime
            .project_work_item_prerequisites(item_id)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn create_blueprint_work_item(
        &mut self,
        project_id: &str,
        kind: &str,
        properties: BTreeMap<String, serde_json::Value>,
    ) -> Result<BlueprintWorkItem, ClientError> {
        let id = super::generate_id("item");
        self.runtime
            .create_project_work_item(&id, project_id, kind, properties)
            .map_err(|error| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: format!("{error:?}"),
            })
    }

    pub fn update_blueprint_work_item(
        &mut self,
        id: &str,
        expected_revision: u64,
        changes: BTreeMap<String, serde_json::Value>,
    ) -> Result<BlueprintWorkItem, ClientError> {
        self.runtime
            .update_project_work_item_properties(id, expected_revision, changes)
            .map_err(|error| ClientError {
                code: ClientErrorCode::InvalidInput,
                message: format!("{error:?}"),
            })
    }

    pub fn list_blueprint_versions(&self) -> Result<Vec<(String, u32)>, ClientError> {
        self.runtime
            .available_blueprint_versions(None)
            .map_err(|error| ClientError {
                code: ClientErrorCode::Unavailable,
                message: format!("{error:?}"),
            })
    }

    pub fn transition_blueprint_work_item(
        &mut self,
        id: &str,
        to: &str,
        expected_revision: u64,
    ) -> Result<BlueprintWorkItem, ClientError> {
        self.runtime
            .transition_project_work_item(id, to, expected_revision, &BTreeSet::new())
            .map_err(|error| ClientError {
                code: ClientErrorCode::Conflict,
                message: format!("{error:?}"),
            })
    }

    pub fn list_agent_jobs(&self) -> Result<Vec<AgentJobRow>, ClientError> {
        let overview = self
            .runtime
            .local_agent_overview()
            .map_err(|message| ClientError {
                code: ClientErrorCode::Unavailable,
                message,
            })?;
        let mut jobs = Vec::new();
        for row in overview {
            let Some(id) = row.get("job_id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let handoff_count = row
                .get("handoff_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            let review_decision = row
                .get("review_decision")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let needs_attention = row
                .get("needs_attention")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let Some(view) = self
                .runtime
                .local_agent_job(id)
                .map_err(|message| ClientError {
                    code: ClientErrorCode::Unavailable,
                    message,
                })?
            else {
                continue;
            };
            jobs.push(AgentJobRow {
                id: view.id,
                project_id: view.project_id,
                task_id: view.task_id,
                state: view.state,
                agent_kind: view.agent_kind,
                machine_ref: view.machine_ref,
                runtime_kind: view.runtime_kind,
                updated_at_ms: view.updated_at_ms,
                runtime_session_ref: view.runtime_session_ref,
                handoff_count,
                review_decision,
                needs_attention,
            });
        }
        Ok(jobs)
    }

    pub fn agent_job_detail(&self, job_id: &str) -> Result<Option<AgentJobDetail>, ClientError> {
        let Some(job) = self
            .list_agent_jobs()?
            .into_iter()
            .find(|job| job.id == job_id)
        else {
            return Ok(None);
        };
        let handoffs = self
            .runtime
            .local_agent_handoffs(job_id)
            .map_err(|message| ClientError {
                code: ClientErrorCode::Unavailable,
                message,
            })?;
        let review = self
            .runtime
            .local_agent_review(job_id)
            .map_err(|message| ClientError {
                code: ClientErrorCode::Unavailable,
                message,
            })?;
        Ok(Some(AgentJobDetail {
            job,
            handoffs,
            review,
        }))
    }

    /// Read-only bounded project projection for blueprint views.
    pub fn project_view_data(
        &self,
        project_id: &str,
        observed_at_ms: i64,
    ) -> Result<ProjectData, ViewDataError> {
        let projects = self.list_projects().map_err(client_error)?;
        let project = projects
            .iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| invalid(format!("unknown project: {project_id}")))?;
        let evidence = self
            .project_evidence_summary(project_id, observed_at_ms)
            .ok();
        let mut data = ProjectData {
            project: ProjectRow {
                id: project.id.clone(),
                name: project.name.clone(),
                status: project.status.clone(),
                mode: project.mode.clone(),
                objective: project.objective.clone(),
                completion_ready: project.completion_ready,
                criteria_satisfied: project
                    .success_criteria
                    .iter()
                    .filter(|criterion| {
                        criterion.status == "satisfied" || criterion.status == "waived"
                    })
                    .count(),
                criteria_total: project.success_criteria.len(),
                health: None,
                evidence: evidence.clone(),
            },
            revision: None,
            pending_commands: 0,
            observed_at_ms,
            ..Default::default()
        };
        let tasks = self.list_tasks().map_err(client_error)?;
        let readiness = self
            .runtime
            .task_readiness(observed_at_ms)
            .ok()
            .map(|snapshot| {
                snapshot
                    .tasks
                    .into_iter()
                    .map(|task| (task.task_id.clone(), task))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        data.tasks = tasks
            .into_iter()
            .filter(|task| task.project_id.as_deref() == Some(project_id))
            .map(|task| {
                let ready = readiness.get(&task.id);
                TaskRow {
                    id: task.id,
                    title: task.title,
                    status: task.status.clone(),
                    priority: task.priority.clone().unwrap_or_default(),
                    due_date: due_date_label(task.deadline_at_ms),
                    acceptance_total: 0,
                    acceptance_completed: 0,
                    project_id: task.project_id,
                    milestone_id: task.milestone_id,
                    estimated_minutes: task.estimated_minutes,
                    earliest_start_at_ms: task.earliest_start_at_ms,
                    deadline_at_ms: task.deadline_at_ms,
                    actionable: ready.is_some_and(|row| row.actionable),
                    readiness: ready
                        .map(|row| format!("{:?}", row.reason).to_lowercase())
                        .unwrap_or_else(|| task.status.clone()),
                    prerequisite_ids: ready
                        .map(|row| row.prerequisite_ids.clone())
                        .unwrap_or_default(),
                    blocking_task_ids: ready
                        .map(|row| row.blocking_task_ids.clone())
                        .unwrap_or_default(),
                    allowed_statuses: allowed_task_statuses(&task.status),
                }
            })
            .collect();

        let blueprint = self.project_blueprint_definition(project_id).ok().flatten();
        let mut work_items = Vec::new();
        for item in self
            .list_blueprint_work_items(project_id)
            .map_err(client_error)?
        {
            let prerequisite_ids = self
                .blueprint_work_item_prerequisites(&item.id)
                .unwrap_or_default();
            let allowed_transitions = blueprint
                .as_ref()
                .and_then(|blueprint| blueprint.item_kind(&item.kind).ok())
                .map(|kind| {
                    kind.transitions
                        .iter()
                        .filter(|edge| edge.from == item.state)
                        .map(|edge| edge.to.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            work_items.push(WorkItemRow {
                id: item.id,
                project_id: item.project_id,
                kind: item.kind,
                state: item.state,
                revision: item.revision,
                properties: item
                    .properties
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            key,
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string()),
                        )
                    })
                    .collect(),
                prerequisite_ids,
                allowed_transitions,
            });
        }
        data.work_items = work_items;

        data.milestones = self
            .runtime
            .milestones()
            .map(|snapshot| {
                snapshot
                    .milestones
                    .into_iter()
                    .filter(|milestone| milestone.project_id == project_id)
                    .map(|milestone| {
                        let scoped: Vec<&TaskRow> = data
                            .tasks
                            .iter()
                            .filter(|task| task.milestone_id.as_deref() == Some(&milestone.id))
                            .collect();
                        let done = scoped.iter().filter(|task| task.status == "done").count();
                        MilestoneRow {
                            id: milestone.id,
                            project_id: milestone.project_id,
                            name: milestone.name,
                            status: milestone.status,
                            task_total: scoped.len(),
                            task_done: done,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        data.knowledge = self
            .list_knowledge()
            .map_err(client_error)?
            .into_iter()
            .take(200)
            .map(|item| KnowledgeRow {
                id: item.id,
                kind: item.kind,
                title: item.title,
                status: item.status,
                provenance: item.provenance,
                updated_at_ms: item.updated_at_ms,
            })
            .collect();

        let cutoff = observed_at_ms.saturating_sub(28 * 24 * 60 * 60 * 1000);
        data.habits = self
            .list_habits()
            .map_err(client_error)?
            .into_iter()
            .map(|habit| {
                let occurrences = self.habit_history(habit.id.clone()).unwrap_or_default();
                let recent: Vec<_> = occurrences
                    .iter()
                    .filter(|occurrence| occurrence.occurred_at_ms >= cutoff)
                    .collect();
                HabitRow {
                    id: habit.id,
                    name: habit.name,
                    cadence: habit.cadence,
                    active: habit.active,
                    occurrences_28d: recent.len(),
                    last_occurred_at_ms: recent.iter().map(|row| row.occurred_at_ms).max(),
                }
            })
            .collect();

        if let Ok((revision, _, _, _, evidence)) = self.runtime.snapshot() {
            data.revision = Some(revision);
            let mut observations: BTreeMap<String, Vec<MetricObservationRow>> = BTreeMap::new();
            for observation in evidence.metric_observations {
                observations
                    .entry(observation.metric_id)
                    .or_default()
                    .push(MetricObservationRow {
                        observed_at_ms: observation.observed_at_ms,
                        value: observation.value,
                    });
            }
            for rows in observations.values_mut() {
                rows.sort_by_key(|row| row.observed_at_ms);
            }
            data.metrics = evidence
                .metrics
                .into_iter()
                .map(|metric| {
                    let rows = observations.remove(&metric.id).unwrap_or_default();
                    let latest = rows.last().map(|row| row.value);
                    MetricRow {
                        id: metric.id,
                        baseline: metric.baseline,
                        target: metric.target,
                        direction: metric.direction,
                        latest,
                        observations: rows,
                    }
                })
                .collect();
        }

        data.agent_jobs = self
            .list_agent_jobs()
            .map_err(client_error)?
            .into_iter()
            .filter(|job| job.project_id == project_id)
            .collect();

        data.schedule = self
            .list_schedule(
                observed_at_ms.saturating_sub(365 * 24 * 60 * 60 * 1000),
                observed_at_ms.saturating_add(365 * 24 * 60 * 60 * 1000),
            )
            .map_err(client_error)?
            .into_iter()
            .filter(|block| {
                block.target_id.as_deref() == Some(project_id)
                    || block.target_id.as_deref().is_some_and(|id| {
                        data.tasks.iter().any(|task| task.id == id)
                            || data.work_items.iter().any(|item| item.id == id)
                    })
            })
            .map(|block| ScheduleRow {
                id: block.id,
                title: block.title,
                starts_at_ms: block.starts_at_ms,
                ends_at_ms: block.ends_at_ms,
                flexibility: block.flexibility,
                target_kind: block.target_kind,
                target_id: block.target_id,
            })
            .collect();
        Ok(data)
    }

    pub fn workspace_view_data(&self) -> Result<WorkspaceData, ViewDataError> {
        let observed_at_ms = now_ms();
        let projects = self.list_projects().map_err(client_error)?;
        let tasks = self.list_tasks().map_err(client_error)?;
        let readiness = self
            .runtime
            .task_readiness(observed_at_ms)
            .ok()
            .map(|snapshot| {
                snapshot
                    .tasks
                    .into_iter()
                    .map(|task| (task.task_id.clone(), task))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let task_row = |task: crate::TaskDto| {
            let ready = readiness.get(&task.id);
            TaskRow {
                id: task.id,
                title: task.title,
                status: task.status.clone(),
                priority: task.priority.clone().unwrap_or_default(),
                due_date: due_date_label(task.deadline_at_ms),
                acceptance_total: 0,
                acceptance_completed: 0,
                project_id: task.project_id,
                milestone_id: task.milestone_id,
                estimated_minutes: task.estimated_minutes,
                earliest_start_at_ms: task.earliest_start_at_ms,
                deadline_at_ms: task.deadline_at_ms,
                actionable: ready.is_some_and(|row| row.actionable),
                readiness: ready
                    .map(|row| format!("{:?}", row.reason).to_lowercase())
                    .unwrap_or_else(|| task.status.clone()),
                prerequisite_ids: ready
                    .map(|row| row.prerequisite_ids.clone())
                    .unwrap_or_default(),
                blocking_task_ids: ready
                    .map(|row| row.blocking_task_ids.clone())
                    .unwrap_or_default(),
                allowed_statuses: allowed_task_statuses(&task.status),
            }
        };
        let all_tasks: Vec<TaskRow> = tasks.iter().cloned().map(task_row).collect();
        let inbox = all_tasks
            .iter()
            .filter(|task| task.status == "inbox")
            .cloned()
            .collect();
        let projects = projects
            .into_iter()
            .take(100)
            .map(|project| ProjectRow {
                id: project.id.clone(),
                name: project.name.clone(),
                status: project.status.clone(),
                mode: project.mode.clone(),
                objective: project.objective.clone(),
                completion_ready: project.completion_ready,
                criteria_satisfied: project
                    .success_criteria
                    .iter()
                    .filter(|criterion| {
                        criterion.status == "satisfied" || criterion.status == "waived"
                    })
                    .count(),
                criteria_total: project.success_criteria.len(),
                health: None,
                evidence: self
                    .project_evidence_summary(&project.id, observed_at_ms)
                    .ok(),
            })
            .collect();
        let cutoff = observed_at_ms.saturating_sub(28 * 24 * 60 * 60 * 1000);
        let active_habits = self
            .list_habits()
            .map_err(client_error)?
            .into_iter()
            .filter(|habit| habit.active)
            .map(|habit| {
                let occurrences = self.habit_history(habit.id.clone()).unwrap_or_default();
                let recent: Vec<_> = occurrences
                    .iter()
                    .filter(|occurrence| occurrence.occurred_at_ms >= cutoff)
                    .collect();
                HabitRow {
                    id: habit.id,
                    name: habit.name,
                    cadence: habit.cadence,
                    active: habit.active,
                    occurrences_28d: recent.len(),
                    last_occurred_at_ms: recent.iter().map(|row| row.occurred_at_ms).max(),
                }
            })
            .collect();
        let mut recent_knowledge: Vec<KnowledgeRow> = self
            .list_knowledge()
            .map_err(client_error)?
            .into_iter()
            .map(|item| KnowledgeRow {
                id: item.id,
                kind: item.kind,
                title: item.title,
                status: item.status,
                provenance: item.provenance,
                updated_at_ms: item.updated_at_ms,
            })
            .collect();
        recent_knowledge.sort_by_key(|item| std::cmp::Reverse(item.updated_at_ms));
        recent_knowledge.truncate(200);
        Ok(WorkspaceData {
            projects,
            tasks: all_tasks,
            inbox,
            active_habits,
            recent_knowledge,
            agent_jobs: self.list_agent_jobs().map_err(client_error)?,
            pending_commands: 0,
            revision: self.runtime.snapshot().ok().map(|snapshot| snapshot.0),
        })
    }

    /// Resolve a pinned blueprint or generate the legacy fallback. This is the
    /// only bridge between application data and renderer-neutral presentation.
    pub fn resolve_project_views(
        &self,
        project_id: &str,
        registry: &orbit_ui::ComponentRegistry,
        observed_at_ms: i64,
    ) -> Result<(ProjectData, Vec<orbit_ui::ResolvedView>, bool), ViewDataError> {
        let data = self.project_view_data(project_id, observed_at_ms)?;
        let blueprint = self
            .project_blueprint_definition(project_id)
            .map_err(client_error)?;
        let (views, generated) =
            orbit_ui::resolve_blueprint_views(blueprint.as_ref(), &data, registry);
        Ok((data, views, generated))
    }

    /// Read-only view definitions for a pinned blueprint. Useful for tests and
    /// for showing exactly what the installed version declares.
    pub fn project_view_definitions(
        &self,
        project_id: &str,
    ) -> Result<Vec<BlueprintView>, ClientError> {
        Ok(self
            .project_blueprint_definition(project_id)?
            .map(|blueprint| blueprint.views)
            .unwrap_or_default())
    }
}

impl orbit_ui::ViewDataSource for OrbitClient {
    fn workspace(&self) -> Result<WorkspaceData, ViewDataError> {
        self.workspace_view_data()
    }
    fn project(&self, project_id: &str) -> Result<ProjectData, ViewDataError> {
        self.project_view_data(project_id, now_ms())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentJobDetail {
    pub job: AgentJobRow,
    pub handoffs: Vec<serde_json::Value>,
    pub review: Option<serde_json::Value>,
}

pub fn describe_time(ms: i64) -> String {
    format_epoch_ms(ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_ui::ComponentRegistry;

    fn client() -> OrbitClient {
        OrbitClient::open(":memory:").unwrap()
    }

    #[test]
    fn civil_dates_render_utc_days_without_a_date_dependency() {
        assert_eq!(civil_date_from_ms(0), "1970-01-01");
        assert_eq!(civil_date_from_ms(86_399_999), "1970-01-01");
        assert_eq!(civil_date_from_ms(86_400_000), "1970-01-02");
        assert_eq!(civil_date_from_ms(1_700_000_000_000), "2023-11-14");
        assert_eq!(civil_date_from_ms(1_709_164_800_000), "2024-02-29");
        assert_eq!(civil_date_from_ms(-86_400_000), "1969-12-31");
        assert_eq!(due_date_label(None), "");
        assert_eq!(due_date_label(Some(1_700_000_000_000)), "2023-11-14");
    }

    #[test]
    fn created_project_resolves_its_pinned_default_workflow_views() {
        let mut client = client();
        let project = client
            .create_project(crate::CreateProjectRequest {
                id: None,
                name: "Legacy".into(),
            })
            .unwrap();
        let registry = ComponentRegistry::builtin();
        let (data, views, generated) = client
            .resolve_project_views(&project.id, &registry, now_ms())
            .unwrap();
        assert_eq!(data.project.name, "Legacy");
        // Creation binds the bundled `basic` workflow, so views are declared
        // rather than generated for an unbound legacy project.
        assert!(!generated);
        assert!(views.iter().any(|view| view.id == "tasks"));
        assert!(views.iter().all(|view| !view.components.is_empty()));
    }

    #[test]
    fn task_rows_carry_application_owned_transition_offers() {
        let mut client = client();
        let project = client
            .create_project(crate::CreateProjectRequest {
                id: None,
                name: "Workflow".into(),
            })
            .unwrap();
        let task = client
            .create_task(crate::CreateTaskRequest {
                id: None,
                title: "Ready".into(),
                project_id: Some(project.id.clone()),
                milestone_id: None,
            })
            .unwrap();
        client
            .set_task_status(crate::SetTaskStatusRequest {
                id: task.id.clone(),
                status: "ready".into(),
            })
            .unwrap();
        let data = client.project_view_data(&project.id, now_ms()).unwrap();
        let row = data.tasks.iter().find(|row| row.id == task.id).unwrap();
        assert!(row.allowed_statuses.contains(&"in_progress".to_string()));
        assert!(row.allowed_statuses.contains(&"blocked".to_string()));
        assert!(!row.allowed_statuses.contains(&"done".to_string()));
    }

    #[test]
    fn blueprint_work_items_are_project_scoped_and_resolved() {
        let mut client = client();
        let project = client
            .create_project(crate::CreateProjectRequest {
                id: None,
                name: "Research".into(),
            })
            .unwrap();
        client
            .apply_workflow(project.id.clone(), "research".into(), None)
            .unwrap();
        let item = client
            .create_blueprint_work_item(
                &project.id,
                "hypothesis",
                BTreeMap::from([(
                    "question".into(),
                    serde_json::json!("Does the view resolve?"),
                )]),
            )
            .unwrap();
        let data = client.project_view_data(&project.id, now_ms()).unwrap();
        assert_eq!(data.work_items.len(), 1);
        assert_eq!(data.work_items[0].id, item.id);
        assert!(
            data.work_items[0]
                .allowed_transitions
                .contains(&"investigating".to_string())
        );
    }
}
