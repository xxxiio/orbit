//! Trusted composition member for the opt-in habit recurrence capability.
//!
//! The algorithm lives in `orbit-habit-capability`; this module only adapts it to
//! the blueprint capability executor contract. Registration makes the capability
//! visible to offers and status, but it never installs or authorizes it.

use crate::AuthoritativeHost;
use crate::blueprint_execution::{BlueprintCapabilityExecutor, ExecutionOutcome};
use crate::capability_composition::{CapabilityActionSpec, TrustedCapability, TrustedComposition};
use orbit_blueprint::{ActionInvocation, CapabilityDescriptor, CapabilityPermission};
use orbit_storage::blueprint_actions::BlueprintActionRequest;
use std::collections::BTreeSet;

pub use orbit_habit_capability::{ACTION_EVALUATE_PERIODS, CAPABILITY_ID, CONTRACT_VERSION};

/// The trusted contract advertised by the habit recurrence package.
pub fn habit_recurrence_capability() -> TrustedCapability {
    TrustedCapability {
        descriptor: CapabilityDescriptor {
            id: CAPABILITY_ID.to_owned(),
            provider: "orbit".to_owned(),
            permission: CapabilityPermission::Read,
            enabled: true,
        },
        actions: vec![CapabilityActionSpec {
            action_id: ACTION_EVALUATE_PERIODS.to_owned(),
            description:
                "Evaluate caller-supplied habit periods and occurrences into a streak summary"
                    .to_owned(),
            input_schema: orbit_habit_capability::input_schema(),
            external_effect: false,
            requires: Vec::new(),
        }],
        requires: BTreeSet::new(),
    }
}

/// Add the habit recurrence capability to a trusted composition. Installation and
/// project authorization remain separate explicit operations.
pub fn register_habit_capability(composition: &mut TrustedComposition) -> Result<(), String> {
    composition.register(
        habit_recurrence_capability(),
        Box::new(HabitRecurrenceExecutor),
    )
}

struct HabitRecurrenceExecutor;

impl BlueprintCapabilityExecutor for HabitRecurrenceExecutor {
    fn capability_id(&self) -> &str {
        CAPABILITY_ID
    }

    fn execute(
        &mut self,
        _host: &mut AuthoritativeHost,
        request: &BlueprintActionRequest,
        invocation: &ActionInvocation,
    ) -> ExecutionOutcome {
        // The persisted envelope must describe exactly this dispatch, and the
        // input document is re-validated rather than trusted because it was
        // schema-shaped at request time.
        if invocation.capability != CAPABILITY_ID
            || invocation.command_id != request.command_id
            || invocation.action_id != ACTION_EVALUATE_PERIODS
            || request.action_id != ACTION_EVALUATE_PERIODS
        {
            return ExecutionOutcome::Failed;
        }
        let Ok(parameters) = serde_json::to_value(&invocation.parameters) else {
            return ExecutionOutcome::Failed;
        };
        match orbit_habit_capability::evaluate_periods_json(&parameters) {
            Ok(_) => ExecutionOutcome::Succeeded,
            Err(_) => ExecutionOutcome::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_application::{
        HabitOccurrenceRepository, HabitRepository, ProjectEvidenceBinding,
        ProjectEvidenceRepository, ProjectRepository, TaskRepository,
    };
    use orbit_domain::{
        Habit, HabitCadence, HabitId, HabitOccurrence, HabitOccurrenceId, HabitPeriod, Project,
        ProjectId, Task, TaskId, TaskScope,
    };
    use orbit_habit_capability::HabitRecurrenceStatus;

    fn project(host: &mut AuthoritativeHost, id: &str) {
        let project = Project::new(ProjectId::new(id).unwrap(), "Project").expect("project");
        ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
    }

    fn seed_habit_evidence(host: &mut AuthoritativeHost, project_id: &ProjectId) {
        let habit_id = HabitId::new("habit-1").unwrap();
        let habit = Habit::new(habit_id.clone(), "Exercise", HabitCadence::Daily).unwrap();
        HabitRepository::save(&mut host.authority.conn, habit).unwrap();
        let occurrence = HabitOccurrence::new(
            HabitOccurrenceId::new("occurrence-1").unwrap(),
            habit_id.clone(),
            10,
            Some(30),
        )
        .unwrap();
        HabitOccurrenceRepository::append(&mut host.authority.conn, occurrence).unwrap();
        ProjectEvidenceRepository::add_project_evidence(
            &mut host.authority.conn,
            project_id,
            &ProjectEvidenceBinding::Habit(habit_id),
        )
        .unwrap();
    }

    #[test]
    fn registers_contract_without_installing_or_authorizing_it() {
        let composition = TrustedComposition::with_builtin_reference();
        let capability = composition
            .capability(CAPABILITY_ID)
            .expect("habit capability registered");
        assert_eq!(capability.descriptor.provider, "orbit");
        assert_eq!(capability.descriptor.permission, CapabilityPermission::Read);
        assert!(capability.descriptor.enabled);
        assert!(capability.requires.is_empty());
        assert_eq!(capability.actions.len(), 1);
        assert_eq!(capability.actions[0].action_id, ACTION_EVALUATE_PERIODS);
        assert!(!capability.actions[0].external_effect);
        assert_eq!(
            capability.actions[0].input_schema,
            orbit_habit_capability::input_schema()
        );
        assert!(composition.has_executor(CAPABILITY_ID));

        let host = AuthoritativeHost::open(":memory:").unwrap();
        let rows = host.capability_status(&composition, None).unwrap();
        let row = rows
            .iter()
            .find(|row| row.capability_id == CAPABILITY_ID)
            .expect("status row");
        assert!(!row.installed);
        assert!(row.executor_registered);
        assert!(!row.available);
        assert_eq!(row.actions, vec![ACTION_EVALUATE_PERIODS.to_owned()]);
        assert!(row.authorized_projects.is_empty());
        assert!(
            row.unavailable_reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("capability_not_installed")),
            "{:?}",
            row.unavailable_reason
        );
    }

    #[test]
    fn task_flow_works_without_capability_and_habit_evidence_is_explicitly_opt_in() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let composition = TrustedComposition::with_builtin_reference();
        project(&mut host, "project-1");
        let project_id = ProjectId::new("project-1").unwrap();
        TaskRepository::save(
            &mut host.authority.conn,
            Task::new(
                TaskId::new("task-1").unwrap(),
                "Ship",
                TaskScope::in_project(project_id.clone()),
            )
            .unwrap(),
        )
        .unwrap();
        let periods = [HabitPeriod::new(0, 100).unwrap()];

        // A task-only project flow succeeds without the capability: there is no
        // habit binding to evaluate, and the report simply carries the
        // unavailable status.
        let (_, tasks) = host.tasks().unwrap();
        assert!(tasks.iter().any(|task| task.id == "task-1"));
        let task_only = host
            .application_queries()
            .project_evidence_report(&project_id, &periods, 100)
            .unwrap();
        assert!(task_only.habits.habits.is_empty());
        assert!(matches!(
            task_only.habits.habit_recurrence,
            HabitRecurrenceStatus::Unavailable { .. }
        ));

        seed_habit_evidence(&mut host, &project_id);

        // No installation: tasks are unaffected, but recurrence is unavailable.
        let report = host
            .application_queries()
            .project_evidence_report(&project_id, &periods, 100)
            .unwrap();
        assert!(matches!(
            report.habits.habit_recurrence,
            HabitRecurrenceStatus::Unavailable { .. }
        ));
        assert_eq!(report.habits.habits.len(), 1);
        assert!(!report.habits.habits[0].available);
        assert_eq!(report.habits.habits[0].fulfilled_periods, 0);
        assert!(report.habits.habits[0].consistency.is_none());
        assert!(report.habits.habit_consistency.is_none());

        // Installed but not authorized for the project: still unavailable.
        host.install_trusted_capability(&composition, CAPABILITY_ID, 1)
            .unwrap();
        let report = host
            .application_queries()
            .project_evidence_report(&project_id, &periods, 100)
            .unwrap();
        assert!(matches!(
            report.habits.habit_recurrence,
            HabitRecurrenceStatus::Unavailable { .. }
        ));
        assert!(report.habits.habits[0].consistency.is_none());

        // Explicit authorization makes the identical evaluation available.
        host.authorize_trusted_capability(CAPABILITY_ID, "project-1", "tester", 2)
            .unwrap();
        let report = host
            .application_queries()
            .project_evidence_report(&project_id, &periods, 100)
            .unwrap();
        assert_eq!(
            report.habits.habit_recurrence,
            HabitRecurrenceStatus::Available
        );
        assert_eq!(report.habits.habits.len(), 1);
        assert!(report.habits.habits[0].available);
        assert_eq!(report.habits.habits[0].fulfilled_periods, 1);
        assert_eq!(report.habits.habits[0].evaluated_periods, 1);
        assert_eq!(report.habits.habits[0].consistency.unwrap().value(), 10_000);
        assert_eq!(report.habits.habit_consistency.unwrap().value(), 10_000);
    }
}
