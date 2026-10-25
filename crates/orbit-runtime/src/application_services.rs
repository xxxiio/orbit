//! Shared rich application use cases owned by the authoritative runtime.
//!
//! Transport adapters must invoke these use cases rather than independently
//! opening repositories or reconstructing project/task business rules.
//! The application UnitOfWork makes each entity/activity update atomic.
use orbit_application::{
    AcceptanceCriteriaCommand, ApplicationError, ApplyValidatedHabitPlanCommand,
    ApplyValidatedPlanCommand, AssociateAttachmentCommand, AttachmentRemoval,
    ChangeProjectEvidenceCommand, Clock, ConfigureProjectStrategyWithActivityCommand,
    CreateHabitWithActivityCommand, CreateProjectWithActivityCommand,
    CreateRoutineWithActivityCommand, CreateScheduleBlockWithActivityCommand,
    CreateTaskWithActivityCommand, ImportAttachmentCommand, MetricObservation, MetricTargetRecord,
    RecordHabitOccurrenceWithActivityCommand, RemoveAttachmentCommand,
    RescheduleBlockWithActivityCommand, SetHabitActiveWithActivityCommand,
    SetProjectCriterionStatusWithActivityCommand, SetRoutineHabitMembershipWithActivityCommand,
    SetTaskPlanningWithActivityCommand, StartTaskWithActivityCommand,
    TransitionMilestoneWithActivityCommand, TransitionProjectWithActivityCommand,
    TransitionTaskWithActivityCommand, UpdateTaskDetailsCommand, apply_acceptance_criteria_change,
    apply_validated_habit_plan, apply_validated_plan, associate_existing_attachment_with_activity,
    bind_project_evidence, configure_project_strategy_with_activity, create_habit_with_activity,
    create_project_with_activity, create_routine_with_activity,
    create_schedule_block_with_activity, create_task_with_activity,
    import_attachment_with_activity, record_habit_occurrence_with_activity,
    record_metric_observation, remove_attachment_with_activity, reschedule_block_with_activity,
    set_habit_active_with_activity, set_metric_target, set_project_criterion_status_with_activity,
    set_routine_habit_membership_with_activity, set_task_planning_with_activity,
    start_task_with_activity, transition_milestone_with_activity, transition_project_with_activity,
    transition_task_with_activity, unbind_project_evidence, update_task_details_with_activity,
};
use orbit_domain::{
    AcceptanceCriteria, Attachment, Habit, HabitOccurrence, Project, Routine, ScheduleBlock, Task,
};
use orbit_storage::SqliteStore;

use crate::attachment_store::AttachmentObjectStore;

struct RuntimeClock;
impl Clock for RuntimeClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or(0)
    }
}

/// Host-owned diagnostic snapshot, independent of transport and repository handles.
/// The CLI, embedded clients, and administrative adapters can consume this
/// without being allowed to query or mutate the underlying SQLite connection.
#[derive(Debug, Clone)]
pub struct ApplicationDiagnostics {
    pub schema_version: i64,
    pub integrity: orbit_storage::StorageIntegrityReport,
    pub concurrency: orbit_storage::SqliteConcurrencyPolicy,
    pub projects: usize,
    pub tasks: usize,
    pub habits: usize,
    pub routines: usize,
}

/// Read-only counterpart to ApplicationServices. All transports resolve
/// application entities against the same host-owned database connection.
pub struct ApplicationQueries<'a> {
    store: &'a SqliteStore,
}
impl<'a> ApplicationQueries<'a> {
    pub(crate) fn new(store: &'a SqliteStore) -> Self {
        Self { store }
    }
    /// Used only by the Markdown cutover regression to ensure note bodies are
    /// not silently reintroduced into the retired SQL knowledge repository.
    pub fn legacy_knowledge_rows_empty(&self) -> Result<bool, ApplicationError> {
        let count: i64 = self
            .store
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_items'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| {
                ApplicationError::from(orbit_application::RepositoryError::Other(e.to_string()))
            })?;
        Ok(count == 0)
    }

    pub fn diagnostics(&self) -> Result<ApplicationDiagnostics, ApplicationError> {
        Ok(ApplicationDiagnostics {
            schema_version: self
                .store
                .schema_version()
                .map_err(ApplicationError::from)?,
            integrity: self.store.integrity_report(),
            concurrency: self
                .store
                .concurrency_policy()
                .map_err(ApplicationError::from)?,
            projects: self.projects()?.len(),
            tasks: self.tasks()?.len(),
            habits: self.habits()?.len(),
            routines: self.routines()?.len(),
        })
    }
    /// Domain-level entity and evidence queries; adapters never need the SQLite handle.
    pub fn project(
        &self,
        id: &orbit_domain::ProjectId,
    ) -> Result<orbit_domain::Project, ApplicationError> {
        orbit_application::get_project(self.store, id)
    }
    pub fn task(&self, id: &orbit_domain::TaskId) -> Result<orbit_domain::Task, ApplicationError> {
        orbit_application::get_task(self.store, id)
    }
    pub fn milestone(
        &self,
        id: &orbit_domain::MilestoneId,
    ) -> Result<orbit_domain::Milestone, ApplicationError> {
        orbit_application::get_milestone(self.store, id)
    }
    pub fn project_evidence(
        &self,
        id: &orbit_domain::ProjectId,
    ) -> Result<Vec<orbit_application::ProjectEvidenceBinding>, ApplicationError> {
        orbit_application::list_project_evidence(self.store, id)
    }
    pub fn project_evidence_report(
        &self,
        id: &orbit_domain::ProjectId,
        periods: &[orbit_domain::HabitPeriod],
        as_of_ms: i64,
    ) -> Result<orbit_application::CombinedProjectEvidenceReport, ApplicationError> {
        let recurrence = self.habit_recurrence_status_for_project(id)?;
        orbit_application::get_combined_project_evidence_report(
            self.store,
            id,
            periods,
            as_of_ms,
            &recurrence,
        )
    }
    pub fn project_health(
        &self,
        id: &orbit_domain::ProjectId,
        lookback_minutes: u64,
        policy: orbit_progress::HealthPolicy,
    ) -> Result<orbit_application::ProjectHealthSnapshot, ApplicationError> {
        orbit_application::get_project_health_snapshot(
            self.store,
            &RuntimeClock,
            id,
            lookback_minutes,
            policy,
        )
    }
    pub fn mode_aware_project_health(
        &self,
        id: &orbit_domain::ProjectId,
        periods: &[orbit_domain::HabitPeriod],
        as_of_ms: i64,
        lookback_minutes: u64,
        policy: orbit_progress::HealthPolicy,
    ) -> Result<orbit_application::ModeAwareProjectHealthSnapshot, ApplicationError> {
        let recurrence = self.habit_recurrence_status_for_project(id)?;
        orbit_application::get_mode_aware_project_health_snapshot(
            self.store,
            &RuntimeClock,
            id,
            periods,
            as_of_ms,
            lookback_minutes,
            policy,
            &recurrence,
        )
    }

    /// The opt-in gate for habit recurrence at the host/projection boundary.
    ///
    /// A project-scoped read requires both a durable installation and a grant for
    /// that exact project, matching `capability_status` semantics. This is the
    /// seam documented in the habits guide: without it, evidence reports return an
    /// explicit unavailable status instead of computing streaks in core.
    fn habit_recurrence_status_for_project(
        &self,
        project_id: &orbit_domain::ProjectId,
    ) -> Result<orbit_habit_capability::HabitRecurrenceStatus, ApplicationError> {
        let installed: bool = self
            .store
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM orbit_capability_installations WHERE capability_id=?1)",
                [orbit_habit_capability::CAPABILITY_ID],
                |row| row.get(0),
            )
            .map_err(|error| {
                ApplicationError::Repository(orbit_application::RepositoryError::Other(
                    error.to_string(),
                ))
            })?;
        if !installed {
            return Ok(orbit_habit_capability::HabitRecurrenceStatus::not_installed());
        }
        let authorized: bool = self
            .store
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM orbit_capability_grants
                 WHERE capability_id=?1 AND project_id=?2)",
                [orbit_habit_capability::CAPABILITY_ID, project_id.as_str()],
                |row| row.get(0),
            )
            .map_err(|error| {
                ApplicationError::Repository(orbit_application::RepositoryError::Other(
                    error.to_string(),
                ))
            })?;
        Ok(if authorized {
            orbit_habit_capability::HabitRecurrenceStatus::available()
        } else {
            orbit_habit_capability::HabitRecurrenceStatus::not_authorized(project_id.as_str())
        })
    }
    pub fn activities(&self) -> Result<Vec<orbit_domain::ActivityRecord>, ApplicationError> {
        use orbit_application::ActivityRepository;
        ActivityRepository::list(self.store).map_err(ApplicationError::from)
    }
    pub fn projects(&self) -> Result<Vec<orbit_domain::Project>, ApplicationError> {
        orbit_application::list_projects(self.store)
    }
    pub fn tasks(&self) -> Result<Vec<orbit_domain::Task>, ApplicationError> {
        orbit_application::list_tasks(self.store)
    }
    pub fn habit(&self, id: &orbit_domain::HabitId) -> Result<Habit, ApplicationError> {
        orbit_application::get_habit(self.store, id)
    }
    pub fn habits(&self) -> Result<Vec<orbit_domain::Habit>, ApplicationError> {
        orbit_application::list_habits(self.store)
    }
    pub fn habit_occurrences(
        &self,
        id: &orbit_domain::HabitId,
    ) -> Result<Vec<orbit_domain::HabitOccurrence>, ApplicationError> {
        orbit_application::list_habit_occurrences(self.store, id)
    }
    pub fn routines(&self) -> Result<Vec<orbit_domain::Routine>, ApplicationError> {
        orbit_application::list_routines(self.store)
    }
    pub fn schedule_block(
        &self,
        id: &orbit_domain::ScheduleBlockId,
    ) -> Result<ScheduleBlock, ApplicationError> {
        orbit_application::get_schedule_block(self.store, id)
    }
    pub fn routine(&self, id: &orbit_domain::RoutineId) -> Result<Routine, ApplicationError> {
        orbit_application::get_routine(self.store, id)
    }
    pub fn schedule_between(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<ScheduleBlock>, ApplicationError> {
        orbit_application::list_schedule_between(self.store, from_ms, to_ms)
    }
    pub fn project_milestones(
        &self,
        id: &orbit_domain::ProjectId,
    ) -> Result<Vec<orbit_domain::Milestone>, ApplicationError> {
        orbit_application::list_project_milestones(self.store, id)
    }
}

/// One executable implementation for the rich local application use cases.
/// This service is hosted inside AuthoritativeHost for either deployment mode.
pub struct ApplicationServices<'a> {
    store: &'a mut SqliteStore,
    attachments: &'a mut AttachmentObjectStore,
}
impl<'a> ApplicationServices<'a> {
    pub(crate) fn new(
        store: &'a mut SqliteStore,
        attachments: &'a mut AttachmentObjectStore,
    ) -> Self {
        Self { store, attachments }
    }

    pub(crate) fn create_routine(
        &mut self,
        cmd: CreateRoutineWithActivityCommand,
    ) -> Result<Routine, ApplicationError> {
        create_routine_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn set_routine_habit_membership(
        &mut self,
        cmd: SetRoutineHabitMembershipWithActivityCommand,
    ) -> Result<Routine, ApplicationError> {
        set_routine_habit_membership_with_activity(self.store, &RuntimeClock, cmd)
    }
    /// Project creation also installs and binds the bundled `basic` workflow.
    /// The authority command engine already holds the enclosing transaction on
    /// this exact connection, so the project row, its activity, the published
    /// manifest and the default binding commit (or roll back) atomically.
    pub(crate) fn create_project(
        &mut self,
        cmd: CreateProjectWithActivityCommand,
    ) -> Result<Project, ApplicationError> {
        let project = create_project_with_activity(self.store, &RuntimeClock, cmd)?;
        crate::default_workflow::ensure_basic_workflow(self.store, project.id().as_str()).map_err(
            |error| {
                ApplicationError::Repository(orbit_application::RepositoryError::Other(format!(
                    "default workflow binding failed: {error:?}"
                )))
            },
        )?;
        Ok(project)
    }
    pub(crate) fn replace_project_workflow(
        &mut self,
        project_id: &orbit_domain::ProjectId,
        blueprint_id: &str,
        version: u32,
    ) -> Result<(), ApplicationError> {
        self.store
            .replace_project_blueprint_binding(project_id, blueprint_id, version)
            .map_err(ApplicationError::from)
    }
    pub(crate) fn remove_project_workflow(
        &mut self,
        project_id: &orbit_domain::ProjectId,
    ) -> Result<bool, ApplicationError> {
        self.store
            .remove_project_blueprint_binding(project_id)
            .map_err(ApplicationError::from)
    }
    pub(crate) fn transition_project(
        &mut self,
        cmd: TransitionProjectWithActivityCommand,
    ) -> Result<Project, ApplicationError> {
        transition_project_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn create_task(
        &mut self,
        cmd: CreateTaskWithActivityCommand,
    ) -> Result<Task, ApplicationError> {
        create_task_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn start_task(
        &mut self,
        cmd: StartTaskWithActivityCommand,
    ) -> Result<Task, ApplicationError> {
        start_task_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn transition_task(
        &mut self,
        cmd: TransitionTaskWithActivityCommand,
    ) -> Result<Task, ApplicationError> {
        transition_task_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn set_task_planning(
        &mut self,
        cmd: SetTaskPlanningWithActivityCommand,
    ) -> Result<Task, ApplicationError> {
        set_task_planning_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn configure_project_strategy(
        &mut self,
        cmd: ConfigureProjectStrategyWithActivityCommand,
    ) -> Result<Project, ApplicationError> {
        configure_project_strategy_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn set_project_criterion_status(
        &mut self,
        cmd: SetProjectCriterionStatusWithActivityCommand,
    ) -> Result<Project, ApplicationError> {
        set_project_criterion_status_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn create_habit(
        &mut self,
        cmd: CreateHabitWithActivityCommand,
    ) -> Result<Habit, ApplicationError> {
        create_habit_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn set_habit_active(
        &mut self,
        cmd: SetHabitActiveWithActivityCommand,
    ) -> Result<Habit, ApplicationError> {
        set_habit_active_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn record_habit_occurrence(
        &mut self,
        cmd: RecordHabitOccurrenceWithActivityCommand,
    ) -> Result<HabitOccurrence, ApplicationError> {
        record_habit_occurrence_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn transition_milestone(
        &mut self,
        cmd: TransitionMilestoneWithActivityCommand,
    ) -> Result<orbit_domain::Milestone, ApplicationError> {
        transition_milestone_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn reschedule_block(
        &mut self,
        cmd: RescheduleBlockWithActivityCommand,
    ) -> Result<ScheduleBlock, ApplicationError> {
        reschedule_block_with_activity(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn create_schedule_block(
        &mut self,
        cmd: CreateScheduleBlockWithActivityCommand,
    ) -> Result<ScheduleBlock, ApplicationError> {
        create_schedule_block_with_activity(self.store, &RuntimeClock, cmd)
    }

    pub(crate) fn change_project_evidence(
        &mut self,
        bind: bool,
        cmd: ChangeProjectEvidenceCommand,
    ) -> Result<(), ApplicationError> {
        if bind {
            bind_project_evidence(self.store, &RuntimeClock, cmd)
        } else {
            unbind_project_evidence(self.store, &RuntimeClock, cmd)
        }
    }
    pub(crate) fn set_metric_target(
        &mut self,
        cmd: MetricTargetRecord,
    ) -> Result<(), ApplicationError> {
        set_metric_target(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn record_metric_observation(
        &mut self,
        cmd: MetricObservation,
    ) -> Result<(), ApplicationError> {
        record_metric_observation(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn apply_task_plan(
        &mut self,
        cmd: ApplyValidatedPlanCommand,
    ) -> Result<Vec<ScheduleBlock>, ApplicationError> {
        apply_validated_plan(self.store, &RuntimeClock, cmd)
    }
    pub(crate) fn apply_habit_plan(
        &mut self,
        cmd: ApplyValidatedHabitPlanCommand,
    ) -> Result<Vec<ScheduleBlock>, ApplicationError> {
        apply_validated_habit_plan(self.store, &RuntimeClock, cmd)
    }

    pub(crate) fn import_attachment(
        &mut self,
        cmd: ImportAttachmentCommand,
    ) -> Result<Attachment, ApplicationError> {
        import_attachment_with_activity(self.store, self.attachments, &RuntimeClock, cmd)
    }

    pub(crate) fn remove_attachment(
        &mut self,
        cmd: RemoveAttachmentCommand,
    ) -> Result<AttachmentRemoval, ApplicationError> {
        remove_attachment_with_activity(self.store, self.attachments, &RuntimeClock, cmd)
    }

    pub(crate) fn associate_attachment(
        &mut self,
        cmd: AssociateAttachmentCommand,
    ) -> Result<Attachment, ApplicationError> {
        associate_existing_attachment_with_activity(
            self.store,
            self.attachments,
            &RuntimeClock,
            cmd,
        )
    }

    pub(crate) fn update_task_details(
        &mut self,
        cmd: UpdateTaskDetailsCommand,
    ) -> Result<Task, ApplicationError> {
        update_task_details_with_activity(self.store, &RuntimeClock, cmd)
    }

    pub(crate) fn apply_acceptance_criteria(
        &mut self,
        cmd: AcceptanceCriteriaCommand,
    ) -> Result<AcceptanceCriteria, ApplicationError> {
        apply_acceptance_criteria_change(self.store, &RuntimeClock, cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_application::{CreateProjectCommand, CreateTaskCommand};
    use orbit_domain::{ActivityId, ActivityOrigin, ProjectId, TaskId, TaskScope};

    fn project_command(id: &str, activity: &str) -> CreateProjectWithActivityCommand {
        CreateProjectWithActivityCommand {
            project: CreateProjectCommand {
                id: ProjectId::new(id.to_owned()).unwrap(),
                name: id.to_owned(),
            },
            activity_id: ActivityId::new(activity.to_owned()).unwrap(),
            origin: ActivityOrigin::Human,
            summary: format!("Create {id}"),
        }
    }

    #[test]
    fn authority_batch_commits_all_project_and_activity_records() {
        let mut host = crate::AuthoritativeHost::open(":memory:").unwrap();
        host.transact_application(|app| {
            app.create_project(project_command("first", "activity-first"))?;
            app.create_project(project_command("second", "activity-second"))?;
            Ok(())
        })
        .unwrap();
        assert_eq!(host.application_queries().projects().unwrap().len(), 2);
        assert_eq!(host.application_queries().activities().unwrap().len(), 2);
    }

    #[test]
    fn authority_batch_rejects_partial_project_and_activity_commit() {
        let mut host = crate::AuthoritativeHost::open(":memory:").unwrap();
        let failed = host.transact_application(|app| {
            app.create_project(project_command("first", "activity-first"))?;
            app.create_project(project_command("first", "activity-second"))?;
            Ok(())
        });
        assert!(failed.is_err());
        assert!(host.application_queries().projects().unwrap().is_empty());
        assert!(host.application_queries().activities().unwrap().is_empty());
        // The connection is reusable immediately after rolling back the batch.
        host.application_services()
            .create_project(project_command("recovered", "activity-recovered"))
            .unwrap();
        assert_eq!(host.application_queries().projects().unwrap().len(), 1);
    }

    fn project_change(id: &str, name: &str) -> orbit_sync::SemanticChange {
        use orbit_sync::{EntityKind, EntityRef, FieldMutation, SemanticChange};
        SemanticChange::new(
            EntityRef::new(EntityKind::Project, id).unwrap(),
            vec![
                FieldMutation::new("name", Some(name.to_owned()), None).unwrap(),
                FieldMutation::new("status", Some("proposed".to_owned()), None).unwrap(),
            ],
        )
        .unwrap()
    }

    #[test]
    fn rich_command_receipt_and_activity_commit_with_canonical_history() {
        let mut host = crate::AuthoritativeHost::open(":memory:").unwrap();
        let (revision, duplicate, created) = host
            .execute_application_command(
                "user",
                "rich-create",
                0,
                "create:project:rich",
                project_change("rich", "rich"),
                |app| {
                    let project = app.create_project(project_command("rich", "rich-activity"))?;
                    Ok(project.name().to_owned())
                },
            )
            .unwrap();
        assert_eq!((revision, duplicate, created.as_str()), (1, false, "rich"));
        assert_eq!(host.application_queries().projects().unwrap().len(), 1);
        assert_eq!(host.application_queries().activities().unwrap().len(), 1);
        assert_eq!(host.snapshot().unwrap().0, 1);
        // A byte-identical retry must NOT invoke the mutating closure.
        let (retry_revision, retry_duplicate, retry_name) = host
            .execute_application_command(
                "user",
                "rich-create",
                0,
                "create:project:rich",
                project_change("rich", "rich"),
                |_app| -> Result<String, ApplicationError> {
                    panic!("duplicate command executed the application use case")
                },
            )
            .unwrap();
        assert_eq!(
            (retry_revision, retry_duplicate, retry_name.as_str()),
            (1, true, "rich")
        );
        assert_eq!(host.application_queries().activities().unwrap().len(), 1);
    }

    #[test]
    fn rich_command_application_error_preserves_revision_and_releases_transaction() {
        let mut host = crate::AuthoritativeHost::open(":memory:").unwrap();
        let failed = host.execute_application_command(
            "user",
            "failing-command",
            0,
            "bad-project",
            project_change("bad", "bad"),
            |app| {
                app.create_project(project_command("bad", "bad-activity"))?;
                app.create_project(project_command("bad", "duplicate-activity"))?;
                Ok("unreachable".to_owned())
            },
        );
        assert!(failed.is_err());
        assert!(host.application_queries().projects().unwrap().is_empty());
        assert!(host.application_queries().activities().unwrap().is_empty());
        assert_eq!(host.snapshot().unwrap().0, 0);
        let valid = host
            .execute_application_command(
                "user",
                "valid-command",
                0,
                "good-project",
                project_change("good", "good"),
                |app| {
                    app.create_project(project_command("good", "good-activity"))?;
                    Ok("good".to_owned())
                },
            )
            .unwrap();
        assert_eq!(valid.0, 1);
    }

    #[test]
    fn rich_command_rejects_cross_domain_id_collision_without_mutation() {
        let mut host = crate::AuthoritativeHost::open(":memory:").unwrap();
        host.execute_application_command(
            "user",
            "same-id",
            0,
            "first",
            project_change("first", "first"),
            |app| {
                app.create_project(project_command("first", "first-activity"))?;
                Ok("first".to_owned())
            },
        )
        .unwrap();
        let collision = host.execute_application_command(
            "user",
            "same-id",
            1,
            "different",
            project_change("second", "second"),
            |_app| -> Result<String, ApplicationError> {
                panic!("collision bypassed immutable receipt check")
            },
        );
        assert!(matches!(
            collision,
            Err(crate::store::AuthorityError::ImmutableConflict)
        ));
        assert_eq!(host.application_queries().projects().unwrap().len(), 1);
        assert_eq!(host.snapshot().unwrap().0, 1);
    }

    #[test]
    fn rich_project_and_task_use_cases_share_host_store_and_activity_transaction() {
        let mut host = crate::AuthoritativeHost::open(":memory:").unwrap();
        let project_id = ProjectId::new("project-m18".to_owned()).unwrap();
        let task_id = TaskId::new("task-m18".to_owned()).unwrap();
        host.application_services()
            .create_project(CreateProjectWithActivityCommand {
                project: CreateProjectCommand {
                    id: project_id.clone(),
                    name: "M18 integration".to_owned(),
                },
                activity_id: ActivityId::new("activity-m18-project".to_owned()).unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Project created".to_owned(),
            })
            .unwrap();
        host.application_services()
            .create_task(CreateTaskWithActivityCommand {
                task: CreateTaskCommand {
                    id: task_id,
                    title: "Consolidate shared runtime".to_owned(),
                    scope: TaskScope::try_new(Some(project_id.clone()), None).unwrap(),
                },
                activity_id: ActivityId::new("activity-m18-task".to_owned()).unwrap(),
                origin: ActivityOrigin::Human,
                summary: "Task created".to_owned(),
            })
            .unwrap();
        let queries = host.application_queries();
        assert_eq!(queries.projects().unwrap().len(), 1);
        assert_eq!(queries.tasks().unwrap().len(), 1);
        assert_eq!(
            queries.project(&project_id).unwrap().name(),
            "M18 integration"
        );
        assert_eq!(queries.activities().unwrap().len(), 2);
        assert!(host.application_schema_version().unwrap() > 0);
    }
}
