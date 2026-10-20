//! Application ports and use cases for Orbit Core.
//!
//! This crate defines semantic use cases consumed by CLI, MCP, storage adapters,
//! and graphical clients. It depends on the pure domain model plus the
//! deterministic planner contract for validated plan application; it remains
//! independent from storage, UI, AI providers, and transport adapters.

mod activity_commands;
pub mod agent_runtime;
mod error;
mod habit_commands;
mod habit_occurrence_commands;
mod habit_occurrence_queries;
mod habit_queries;
mod knowledge_queries;
mod metric_evidence_commands;
mod milestone_commands;
mod milestone_queries;
mod plan_commands;
mod ports;
mod project_commands;
mod project_evidence_commands;
mod project_evidence_queries;
mod project_health_queries;
mod project_queries;
mod routine_commands;
mod routine_queries;
mod schedule_commands;
mod schedule_queries;
mod sync_materialization;
mod sync_outbox;
mod task_commands;
mod task_queries;
mod task_scope;

pub use activity_commands::{RecordActivityCommand, record_activity};
pub use agent_runtime::{
    AgentInspector, AgentLaunchOutcome, AgentLaunchRequest, AgentLauncher, AgentMessageOutcome,
    AgentMessenger, AgentRuntime, AgentSessionObservation,
};
pub use error::{ApplicationError, RepositoryError};
pub use habit_commands::{
    CreateHabitCommand, CreateHabitWithActivityCommand, SetHabitActiveWithActivityCommand,
    create_habit, create_habit_with_activity, pause_habit, resume_habit,
    set_habit_active_with_activity,
};
pub use habit_occurrence_commands::{
    RecordHabitOccurrenceWithActivityCommand, record_habit_occurrence_with_activity,
};
pub use habit_occurrence_queries::{habit_streak_summary, list_habit_occurrences};
pub use habit_queries::{get_habit, list_habits};
pub use knowledge_queries::KnowledgeTermOccurrence;
pub use milestone_commands::{
    CreateMilestoneCommand, TransitionMilestoneCommand, TransitionMilestoneWithActivityCommand,
    create_milestone, transition_milestone, transition_milestone_with_activity,
};
pub use milestone_queries::{get_milestone, list_project_milestones};
pub use plan_commands::{
    ApplyValidatedHabitPlanCommand, ApplyValidatedPlanCommand, HabitPlanApplicationIdentity,
    PlanApplicationIdentity, apply_validated_habit_plan, apply_validated_plan,
};
pub use ports::{
    ActivityRepository, Clock, HabitOccurrenceRepository, HabitRepository, MilestoneRepository,
    ProjectEvidenceBinding, ProjectEvidenceRepository, ProjectRepository, RoutineRepository,
    ScheduleRepository, TaskRepository, UnitOfWork,
};
pub use project_commands::{
    ConfigureProjectStrategyCommand, ConfigureProjectStrategyWithActivityCommand,
    CreateProjectCommand, CreateProjectWithActivityCommand,
    SetProjectCriterionStatusWithActivityCommand, TransitionProjectCommand,
    TransitionProjectWithActivityCommand, configure_project_strategy_with_activity, create_project,
    create_project_with_activity, set_project_criterion_status_with_activity, transition_project,
    transition_project_with_activity,
};
pub use project_evidence_commands::{
    ChangeProjectEvidenceCommand, bind_project_evidence, list_project_evidence,
    unbind_project_evidence,
};
pub use project_evidence_queries::{
    BoundHabitEvidence, ProjectEvidenceReport, get_project_evidence_report,
};
pub use project_health_queries::{
    MilestoneProgressSnapshot, ProjectHealthSnapshot, get_project_health_snapshot,
};
pub use project_queries::{get_project, list_projects};
pub use routine_commands::{
    CreateRoutineWithActivityCommand, SetRoutineHabitMembershipWithActivityCommand,
    create_routine_with_activity, set_routine_habit_membership_with_activity,
};
pub use routine_queries::{get_routine, list_routines};
pub use schedule_commands::{
    CreateScheduleBlockCommand, CreateScheduleBlockWithActivityCommand,
    RescheduleBlockWithActivityCommand, create_schedule_block, create_schedule_block_with_activity,
    reschedule_block, reschedule_block_with_activity, validate_schedule_target,
};
pub use schedule_queries::{get_schedule_block, list_schedule_between};
pub use sync_materialization::{
    InboundMaterializationReport, MaterializedSemanticEntity, MaterializedSemanticField,
    materialize_inbound_semantic_state,
};
pub use sync_outbox::{
    PendingConflictResolution, PendingSemanticChange, ReservedConflictResolution,
    ReservedSemanticChange, SemanticEntityKind, SemanticFieldMutation, SyncFieldVersion,
    SyncOutboxRepository, enqueue_if_sync_enabled, field as sync_field,
};
pub use task_commands::{
    CreateTaskCommand, CreateTaskWithActivityCommand, SetTaskPlanningCommand,
    SetTaskPlanningWithActivityCommand, StartTaskWithActivityCommand, TransitionTaskCommand,
    TransitionTaskWithActivityCommand, create_task, create_task_with_activity, set_task_planning,
    set_task_planning_with_activity, start_task_with_activity, transition_task,
    transition_task_with_activity,
};
pub use task_queries::{get_task, list_tasks, list_tasks_by_status};
pub use task_scope::validate_task_scope;

pub use metric_evidence_commands::{record_metric_observation, set_metric_target};
pub use ports::{MetricEvidenceRepository, MetricObservation, MetricTargetRecord};
pub use project_evidence_queries::{
    BoundMetricEvidence, CombinedProjectEvidenceReport, get_combined_project_evidence_report,
};
pub use project_health_queries::{
    ModeAwareProjectHealthSnapshot, get_mode_aware_project_health_snapshot,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use orbit_domain::{
        ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Habit,
        HabitCadence, HabitId, Milestone, MilestoneId, MilestoneStatus, Project, ProjectId,
        ProjectStatus, ScheduleBlock, ScheduleBlockId, ScheduleFlexibility, ScheduleTarget, Task,
        TaskId, TaskScope, TaskStatus,
    };

    use super::*;

    #[derive(Default)]
    struct InMemoryTaskRepository {
        tasks: BTreeMap<TaskId, Task>,
    }

    impl TaskRepository for InMemoryTaskRepository {
        fn get(&self, id: &TaskId) -> Result<Option<Task>, RepositoryError> {
            Ok(self.tasks.get(id).cloned())
        }
        fn list(&self) -> Result<Vec<Task>, RepositoryError> {
            Ok(self.tasks.values().cloned().collect())
        }
        fn save(&mut self, task: Task) -> Result<(), RepositoryError> {
            self.tasks.insert(task.id().clone(), task);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryProjectRepository {
        projects: BTreeMap<ProjectId, Project>,
    }

    impl ProjectRepository for InMemoryProjectRepository {
        fn get(&self, id: &ProjectId) -> Result<Option<Project>, RepositoryError> {
            Ok(self.projects.get(id).cloned())
        }
        fn list(&self) -> Result<Vec<Project>, RepositoryError> {
            Ok(self.projects.values().cloned().collect())
        }
        fn save(&mut self, project: Project) -> Result<(), RepositoryError> {
            self.projects.insert(project.id().clone(), project);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryMilestoneRepository {
        milestones: BTreeMap<MilestoneId, Milestone>,
    }

    impl MilestoneRepository for InMemoryMilestoneRepository {
        fn get(&self, id: &MilestoneId) -> Result<Option<Milestone>, RepositoryError> {
            Ok(self.milestones.get(id).cloned())
        }
        fn list_for_project(
            &self,
            project_id: &ProjectId,
        ) -> Result<Vec<Milestone>, RepositoryError> {
            Ok(self
                .milestones
                .values()
                .filter(|milestone| milestone.belongs_to(project_id))
                .cloned()
                .collect())
        }
        fn save(&mut self, milestone: Milestone) -> Result<(), RepositoryError> {
            self.milestones.insert(milestone.id().clone(), milestone);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryHabitRepository {
        habits: BTreeMap<HabitId, Habit>,
    }

    impl HabitRepository for InMemoryHabitRepository {
        fn get(&self, id: &HabitId) -> Result<Option<Habit>, RepositoryError> {
            Ok(self.habits.get(id).cloned())
        }
        fn list(&self) -> Result<Vec<Habit>, RepositoryError> {
            Ok(self.habits.values().cloned().collect())
        }
        fn save(&mut self, habit: Habit) -> Result<(), RepositoryError> {
            self.habits.insert(habit.id().clone(), habit);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryScheduleRepository {
        blocks: BTreeMap<ScheduleBlockId, ScheduleBlock>,
    }

    impl ScheduleRepository for InMemoryScheduleRepository {
        fn get(&self, id: &ScheduleBlockId) -> Result<Option<ScheduleBlock>, RepositoryError> {
            Ok(self.blocks.get(id).cloned())
        }
        fn list_between(
            &self,
            start_ms: i64,
            end_ms: i64,
        ) -> Result<Vec<ScheduleBlock>, RepositoryError> {
            Ok(self
                .blocks
                .values()
                .filter(|block| block.starts_at_ms() < end_ms && block.ends_at_ms() > start_ms)
                .cloned()
                .collect())
        }
        fn save(&mut self, block: ScheduleBlock) -> Result<(), RepositoryError> {
            self.blocks.insert(block.id().clone(), block);
            Ok(())
        }
    }

    #[derive(Default)]
    struct InMemoryActivityRepository {
        activities: BTreeMap<ActivityId, ActivityRecord>,
    }

    impl ActivityRepository for InMemoryActivityRepository {
        fn get(&self, id: &ActivityId) -> Result<Option<ActivityRecord>, RepositoryError> {
            Ok(self.activities.get(id).cloned())
        }
        fn list(&self) -> Result<Vec<ActivityRecord>, RepositoryError> {
            Ok(self.activities.values().cloned().collect())
        }
        fn append(&mut self, activity: ActivityRecord) -> Result<(), RepositoryError> {
            self.activities.insert(activity.id().clone(), activity);
            Ok(())
        }
    }

    struct FixedClock(i64);
    impl Clock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    fn task_id(value: &str) -> TaskId {
        TaskId::new(value).expect("valid task id")
    }

    fn empty_context() -> (InMemoryProjectRepository, InMemoryMilestoneRepository) {
        (
            InMemoryProjectRepository::default(),
            InMemoryMilestoneRepository::default(),
        )
    }

    #[test]
    fn create_task_persists_a_standalone_task() {
        let mut tasks = InMemoryTaskRepository::default();
        let (projects, milestones) = empty_context();
        let created = create_task(
            &mut tasks,
            &projects,
            &milestones,
            CreateTaskCommand {
                id: task_id("task-1"),
                title: "Renew passport".to_owned(),
                scope: TaskScope::standalone(),
            },
        )
        .expect("task should be created");

        let stored = get_task(&tasks, created.id()).expect("task should be persisted");
        assert!(stored.scope().is_standalone());
        assert_eq!(stored.status(), TaskStatus::Inbox);
    }

    #[test]
    fn duplicate_task_ids_are_rejected_by_application_service() {
        let mut tasks = InMemoryTaskRepository::default();
        let (projects, milestones) = empty_context();
        let id = task_id("task-1");
        create_task(
            &mut tasks,
            &projects,
            &milestones,
            CreateTaskCommand {
                id: id.clone(),
                title: "First".to_owned(),
                scope: TaskScope::standalone(),
            },
        )
        .expect("first task should be created");

        let result = create_task(
            &mut tasks,
            &projects,
            &milestones,
            CreateTaskCommand {
                id: id.clone(),
                title: "Duplicate".to_owned(),
                scope: TaskScope::standalone(),
            },
        );

        assert_eq!(
            result,
            Err(ApplicationError::AlreadyExists {
                entity: "task",
                id: id.to_string(),
            })
        );
    }

    #[test]
    fn project_milestone_and_task_relationships_are_validated() {
        let mut tasks = InMemoryTaskRepository::default();
        let mut projects = InMemoryProjectRepository::default();
        let mut milestones = InMemoryMilestoneRepository::default();
        let project_a = ProjectId::new("project-a").expect("valid project id");
        let project_b = ProjectId::new("project-b").expect("valid project id");
        let milestone_id = MilestoneId::new("milestone-1").expect("valid milestone id");

        create_project(
            &mut projects,
            CreateProjectCommand {
                id: project_a.clone(),
                name: "A".to_owned(),
            },
        )
        .expect("project A");
        create_project(
            &mut projects,
            CreateProjectCommand {
                id: project_b.clone(),
                name: "B".to_owned(),
            },
        )
        .expect("project B");
        create_milestone(
            &mut milestones,
            &projects,
            CreateMilestoneCommand {
                id: milestone_id.clone(),
                project_id: project_a,
                name: "M1".to_owned(),
            },
        )
        .expect("milestone");

        let result = create_task(
            &mut tasks,
            &projects,
            &milestones,
            CreateTaskCommand {
                id: task_id("task-1"),
                title: "Wrongly scoped".to_owned(),
                scope: TaskScope::in_milestone(project_b, milestone_id),
            },
        );

        assert_eq!(
            result,
            Err(ApplicationError::InconsistentReference {
                relation: "task_milestone_project",
            })
        );
    }

    #[test]
    fn project_and_milestone_commands_route_through_domain_lifecycles() {
        let mut projects = InMemoryProjectRepository::default();
        let mut milestones = InMemoryMilestoneRepository::default();
        let project_id = ProjectId::new("project-1").expect("valid project id");
        let milestone_id = MilestoneId::new("milestone-1").expect("valid milestone id");

        create_project(
            &mut projects,
            CreateProjectCommand {
                id: project_id.clone(),
                name: "Orbit".to_owned(),
            },
        )
        .expect("project should be created");
        transition_project(
            &mut projects,
            TransitionProjectCommand {
                id: project_id.clone(),
                status: ProjectStatus::Active,
            },
        )
        .expect("project should activate");

        create_milestone(
            &mut milestones,
            &projects,
            CreateMilestoneCommand {
                id: milestone_id.clone(),
                project_id: project_id.clone(),
                name: "Core".to_owned(),
            },
        )
        .expect("milestone should be created");
        transition_milestone(
            &mut milestones,
            TransitionMilestoneCommand {
                id: milestone_id.clone(),
                status: MilestoneStatus::InProgress,
            },
        )
        .expect("milestone should start");

        assert_eq!(
            get_project(&projects, &project_id)
                .expect("project")
                .status(),
            ProjectStatus::Active
        );
        assert_eq!(
            get_milestone(&milestones, &milestone_id)
                .expect("milestone")
                .status(),
            MilestoneStatus::InProgress
        );
    }

    #[test]
    fn habit_commands_keep_habits_independent_from_tasks() {
        let mut habits = InMemoryHabitRepository::default();
        let id = HabitId::new("habit-1").expect("valid habit id");

        create_habit(
            &mut habits,
            CreateHabitCommand {
                id: id.clone(),
                name: "Exercise".to_owned(),
                cadence: HabitCadence::Weekly { target_per_week: 4 },
            },
        )
        .expect("habit should be created");
        pause_habit(&mut habits, &id).expect("habit should pause");
        assert!(!get_habit(&habits, &id).expect("habit").is_active());
        resume_habit(&mut habits, &id).expect("habit should resume");
        assert!(get_habit(&habits, &id).expect("habit").is_active());
    }

    #[test]
    fn schedule_targets_must_exist_and_fixed_blocks_remain_fixed() {
        let mut schedules = InMemoryScheduleRepository::default();
        let tasks = InMemoryTaskRepository::default();
        let habits = InMemoryHabitRepository::default();
        let missing_task = task_id("missing-task");

        let result = create_schedule_block(
            &mut schedules,
            &tasks,
            &habits,
            CreateScheduleBlockCommand {
                id: ScheduleBlockId::new("block-1").expect("valid block id"),
                title: "Focus".to_owned(),
                starts_at_ms: 100,
                duration_minutes: 30,
                flexibility: ScheduleFlexibility::Movable,
                target: ScheduleTarget::Task(missing_task.clone()),
            },
        );
        assert_eq!(
            result,
            Err(ApplicationError::NotFound {
                entity: "task",
                id: missing_task.to_string(),
            })
        );

        let block_id = ScheduleBlockId::new("block-2").expect("valid block id");
        create_schedule_block(
            &mut schedules,
            &tasks,
            &habits,
            CreateScheduleBlockCommand {
                id: block_id.clone(),
                title: "Appointment".to_owned(),
                starts_at_ms: 100,
                duration_minutes: 30,
                flexibility: ScheduleFlexibility::Fixed,
                target: ScheduleTarget::Standalone,
            },
        )
        .expect("standalone fixed block should be valid");

        assert!(reschedule_block(&mut schedules, &block_id, 200).is_err());
    }

    #[test]
    fn task_status_queries_remain_application_queries() {
        let mut tasks = InMemoryTaskRepository::default();
        let (projects, milestones) = empty_context();
        let first = task_id("task-1");
        let second = task_id("task-2");
        for (id, title) in [(first.clone(), "One"), (second.clone(), "Two")] {
            create_task(
                &mut tasks,
                &projects,
                &milestones,
                CreateTaskCommand {
                    id,
                    title: title.to_owned(),
                    scope: TaskScope::standalone(),
                },
            )
            .expect("task should be created");
        }
        transition_task(
            &mut tasks,
            TransitionTaskCommand {
                id: first.clone(),
                status: TaskStatus::Ready,
            },
        )
        .expect("task should transition");

        let ready = list_tasks_by_status(&tasks, TaskStatus::Ready).expect("query should succeed");
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id(), &first);
    }

    #[test]
    fn record_activity_uses_the_injected_clock() {
        let mut activities = InMemoryActivityRepository::default();
        let clock = FixedClock(1_234_567);
        let id = ActivityId::new("activity-1").expect("valid activity id");

        let record = record_activity(
            &mut activities,
            &clock,
            RecordActivityCommand {
                id,
                subject: ActivitySubject::Task(task_id("task-1")),
                kind: ActivityKind::Created,
                origin: ActivityOrigin::Human,
                summary: "Task captured".to_owned(),
            },
        )
        .expect("activity should be recorded");

        assert_eq!(record.occurred_at_ms(), 1_234_567);
        assert_eq!(activities.list().expect("list succeeds").len(), 1);
    }
}
