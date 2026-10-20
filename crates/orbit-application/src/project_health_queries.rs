use orbit_domain::{ActivitySubject, ProjectId, ProjectMode, ProjectProgressSignal, TaskStatus};
use orbit_progress::{
    BlockerExposure, CanonicalTaskProgress, HealthPolicy, ProgressRollup, ProjectHealthProjection,
    ProjectProgressContribution, ProjectProgressEvidence, RecentActivityEvidence,
    RollupWeightPolicy, canonical_milestone_rollup, canonical_project_rollup,
    project_health_projection, recent_activity_evidence,
};

use crate::{
    ActivityRepository, ApplicationError, Clock, MilestoneRepository, ProjectRepository,
    TaskRepository,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneProgressSnapshot {
    pub milestone_id: orbit_domain::MilestoneId,
    pub progress: Option<orbit_domain::ProgressBasisPoints>,
    pub included_tasks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHealthSnapshot {
    pub project_id: ProjectId,
    pub mode: ProjectMode,
    pub tracked_signals: Vec<ProjectProgressSignal>,
    pub completion_ready: bool,
    pub required_criteria_total: u32,
    pub required_criteria_satisfied: u32,
    pub rollup: ProgressRollup,
    pub milestones: Vec<MilestoneProgressSnapshot>,
    pub evidence: ProjectProgressEvidence,
    pub recent_activity: RecentActivityEvidence,
    pub health: ProjectHealthProjection,
    /// Current canonical rollup derives unfinished task progress from status
    /// unless a future stored normalized progress source is supplied.
    pub status_derived_progress: bool,
}

pub fn get_project_health_snapshot<R>(
    repository: &R,
    clock: &impl Clock,
    project_id: &ProjectId,
    activity_lookback_minutes: u64,
    health_policy: HealthPolicy,
) -> Result<ProjectHealthSnapshot, ApplicationError>
where
    R: ProjectRepository + MilestoneRepository + TaskRepository + ActivityRepository,
{
    let project = ProjectRepository::get(repository, project_id)?.ok_or_else(|| {
        ApplicationError::NotFound {
            entity: "project",
            id: project_id.to_string(),
        }
    })?;
    let milestones = MilestoneRepository::list_for_project(repository, project_id)?;
    let tasks: Vec<_> = TaskRepository::list(repository)?
        .into_iter()
        .filter(|task| task.scope().project_id() == Some(project_id))
        .collect();

    let mut milestone_snapshots = Vec::new();
    let mut project_items = Vec::new();
    for milestone in &milestones {
        let milestone_tasks: Vec<_> = tasks
            .iter()
            .filter(|task| task.scope().milestone_id() == Some(milestone.id()))
            .map(|task| CanonicalTaskProgress::from_task(task, None))
            .collect();
        let rollup = canonical_milestone_rollup(
            milestone,
            &milestone_tasks,
            RollupWeightPolicy::EstimateMinutes,
        )?;
        milestone_snapshots.push(MilestoneProgressSnapshot {
            milestone_id: milestone.id().clone(),
            progress: rollup.progress,
            included_tasks: rollup.included_tasks,
        });
        project_items.push(ProjectProgressContribution::Milestone {
            progress: rollup.progress,
            weight: u32::try_from(rollup.total_weight.max(1)).unwrap_or(u32::MAX),
        });
    }

    for task in tasks
        .iter()
        .filter(|task| task.scope().milestone_id().is_none())
    {
        project_items.push(ProjectProgressContribution::DirectTask(
            CanonicalTaskProgress::from_task(task, None),
        ));
    }

    let rollup = canonical_project_rollup(
        &project,
        &project_items,
        RollupWeightPolicy::EstimateMinutes,
    )?;

    let blocked_tasks = tasks
        .iter()
        .filter(|task| task.status() == TaskStatus::Blocked)
        .count() as u32;
    let blocked_milestones = milestones
        .iter()
        .filter(|milestone| milestone.status() == orbit_domain::MilestoneStatus::Blocked)
        .count() as u32;
    let blocker_exposure = BlockerExposure {
        open_blockers: 0,
        blocked_tasks,
        blocked_milestones,
        oldest_open_blocker_age_minutes: None,
    };

    let evidence =
        ProjectProgressEvidence::new(rollup.progress, None, None, blocker_exposure, None, None)?;

    let task_ids: std::collections::HashSet<_> =
        tasks.iter().map(|task| task.id().clone()).collect();
    let milestone_ids: std::collections::HashSet<_> = milestones
        .iter()
        .map(|milestone| milestone.id().clone())
        .collect();
    let activity_times: Vec<_> = ActivityRepository::list(repository)?
        .into_iter()
        .filter(|activity| match activity.subject() {
            ActivitySubject::Project(id) => id == project_id,
            ActivitySubject::Milestone(id) => milestone_ids.contains(id),
            ActivitySubject::Task(id) => task_ids.contains(id),
            _ => false,
        })
        .map(|activity| activity.occurred_at_ms())
        .collect();
    let recent_activity =
        recent_activity_evidence(clock.now_ms(), &activity_times, activity_lookback_minutes)?;
    let health = project_health_projection(evidence, None, Some(recent_activity), health_policy);

    let required_criteria_total = project
        .success_criteria()
        .iter()
        .filter(|criterion| criterion.required())
        .count() as u32;
    let required_criteria_satisfied = project
        .success_criteria()
        .iter()
        .filter(|criterion| criterion.required() && criterion.is_complete())
        .count() as u32;
    Ok(ProjectHealthSnapshot {
        project_id: project_id.clone(),
        mode: project.mode(),
        tracked_signals: project.progress_signals().to_vec(),
        completion_ready: project.required_completion_ready(),
        required_criteria_total,
        required_criteria_satisfied,
        rollup,
        milestones: milestone_snapshots,
        evidence,
        recent_activity,
        health,
        status_derived_progress: true,
    })
}

/// A mode/signal-aware progress surface. Canonical project completion still depends
/// on domain success criteria, never on a derived score or an AI assessment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeAwareProjectHealthSnapshot {
    pub canonical: ProjectHealthSnapshot,
    pub bound_evidence: crate::CombinedProjectEvidenceReport,
    /// Equal weight across selected, available signal families (not across each raw event).
    pub selected_progress: Option<orbit_domain::ProgressBasisPoints>,
    pub available_signals: Vec<ProjectProgressSignal>,
    pub unavailable_signals: Vec<ProjectProgressSignal>,
}

pub fn get_mode_aware_project_health_snapshot<R>(
    repository: &R,
    clock: &impl Clock,
    project_id: &ProjectId,
    periods: &[orbit_domain::HabitPeriod],
    as_of_ms: i64,
    activity_lookback_minutes: u64,
    health_policy: HealthPolicy,
) -> Result<ModeAwareProjectHealthSnapshot, ApplicationError>
where
    R: ProjectRepository
        + MilestoneRepository
        + TaskRepository
        + ActivityRepository
        + crate::ProjectEvidenceRepository
        + crate::HabitRepository
        + crate::HabitOccurrenceRepository
        + crate::MetricEvidenceRepository,
{
    if periods.iter().any(|p| p.end_ms > as_of_ms) {
        return Err(ApplicationError::InconsistentReference {
            relation: "evidence_as_of_window",
        });
    }
    let canonical = get_project_health_snapshot(
        repository,
        clock,
        project_id,
        activity_lookback_minutes,
        health_policy,
    )?;
    let bound_evidence =
        crate::get_combined_project_evidence_report(repository, project_id, periods, as_of_ms)?;
    let mut available_signals = Vec::new();
    let mut unavailable_signals = Vec::new();
    let mut values = Vec::new();
    for signal in &canonical.tracked_signals {
        let value = match signal {
            ProjectProgressSignal::Tasks | ProjectProgressSignal::Milestones => {
                Some(canonical.rollup.progress)
            }
            ProjectProgressSignal::HabitConsistency => bound_evidence.habits.habit_consistency,
            ProjectProgressSignal::MetricValue => {
                let observed: Vec<u16> = bound_evidence
                    .metrics
                    .iter()
                    .filter_map(|m| m.evaluation.map(|e| e.progress.value()))
                    .collect();
                average_signal_values(&observed)?
            }
            // These are descriptive/operational signals and must not be invented
            // as numeric progress in the absence of a dedicated source contract.
            _ => None,
        };
        if let Some(value) = value {
            available_signals.push(*signal);
            values.push(value.value());
        } else {
            unavailable_signals.push(*signal);
        }
    }
    Ok(ModeAwareProjectHealthSnapshot {
        canonical,
        bound_evidence,
        selected_progress: average_signal_values(&values)?,
        available_signals,
        unavailable_signals,
    })
}

fn average_signal_values(
    values: &[u16],
) -> Result<Option<orbit_domain::ProgressBasisPoints>, ApplicationError> {
    if values.is_empty() {
        return Ok(None);
    }
    let total: u64 = values.iter().map(|v| u64::from(*v)).sum();
    let mean = (total + values.len() as u64 / 2) / values.len() as u64;
    Ok(Some(orbit_domain::ProgressBasisPoints::new(mean as u16)?))
}

#[cfg(test)]
mod mode_aware_tests {
    use super::*;
    #[test]
    fn no_observations_is_unknown_not_zero() {
        assert_eq!(average_signal_values(&[]).unwrap(), None);
        assert_eq!(
            average_signal_values(&[2000, 8000])
                .unwrap()
                .unwrap()
                .value(),
            5000
        );
    }
    #[test]
    fn averages_signal_families_without_overflow() {
        assert_eq!(
            average_signal_values(&[10000, 10000, 10000])
                .unwrap()
                .unwrap()
                .value(),
            10000
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use orbit_domain::{
        ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Milestone,
        MilestoneId, MilestoneStatus, Project, ProjectId, ProjectStatus, Task, TaskId,
        TaskPlanning, TaskScope, TaskStatus,
    };
    use orbit_progress::{HealthPolicy, ProjectHealthReason, ProjectHealthState};

    use super::*;
    use crate::RepositoryError;

    #[derive(Default)]
    struct Repo {
        projects: BTreeMap<ProjectId, Project>,
        milestones: BTreeMap<MilestoneId, Milestone>,
        tasks: BTreeMap<TaskId, Task>,
        activities: BTreeMap<ActivityId, ActivityRecord>,
    }

    impl ProjectRepository for Repo {
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

    impl MilestoneRepository for Repo {
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
                .filter(|m| m.project_id() == project_id)
                .cloned()
                .collect())
        }
        fn save(&mut self, milestone: Milestone) -> Result<(), RepositoryError> {
            self.milestones.insert(milestone.id().clone(), milestone);
            Ok(())
        }
    }

    impl TaskRepository for Repo {
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

    impl ActivityRepository for Repo {
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

    #[test]
    fn project_health_query_rolls_up_canonical_entities_and_activity() {
        let project_id = ProjectId::new("p1").unwrap();
        let mut project = Project::new(project_id.clone(), "Orbit").unwrap();
        project.transition_to(ProjectStatus::Active).unwrap();
        let milestone_id = MilestoneId::new("m1").unwrap();
        let mut milestone = Milestone::new(milestone_id.clone(), project_id.clone(), "M1").unwrap();
        milestone
            .transition_to(MilestoneStatus::InProgress)
            .unwrap();

        let mut done = Task::new(
            TaskId::new("t1").unwrap(),
            "Done",
            TaskScope::in_milestone(project_id.clone(), milestone_id.clone()),
        )
        .unwrap();
        done.set_planning(TaskPlanning::new(Some(30), None, None).unwrap());
        done.transition_to(TaskStatus::Ready).unwrap();
        done.transition_to(TaskStatus::InProgress).unwrap();
        done.transition_to(TaskStatus::Done).unwrap();

        let mut blocked = Task::new(
            TaskId::new("t2").unwrap(),
            "Blocked",
            TaskScope::in_milestone(project_id.clone(), milestone_id.clone()),
        )
        .unwrap();
        blocked.set_planning(TaskPlanning::new(Some(30), None, None).unwrap());
        blocked.transition_to(TaskStatus::Ready).unwrap();
        blocked.transition_to(TaskStatus::Blocked).unwrap();

        let activity = ActivityRecord::new(
            ActivityId::new("a1").unwrap(),
            ActivitySubject::Task(done.id().clone()),
            ActivityKind::Completed,
            ActivityOrigin::Human,
            9 * 60_000,
            "Done completed",
        )
        .unwrap();

        let mut repo = Repo::default();
        repo.projects.insert(project_id.clone(), project);
        repo.milestones.insert(milestone_id, milestone);
        repo.tasks.insert(done.id().clone(), done);
        repo.tasks.insert(blocked.id().clone(), blocked);
        repo.activities.insert(activity.id().clone(), activity);

        let snapshot = get_project_health_snapshot(
            &repo,
            &FixedClock(10 * 60_000),
            &project_id,
            60,
            HealthPolicy::default(),
        )
        .unwrap();

        assert_eq!(snapshot.rollup.progress.value(), 5_000);
        assert_eq!(snapshot.evidence.blocker_exposure.blocked_tasks, 1);
        assert_eq!(snapshot.recent_activity.activity_count, 1);
        assert_eq!(snapshot.health.state, ProjectHealthState::NeedsAttention);
        assert!(
            snapshot
                .health
                .reasons
                .contains(&ProjectHealthReason::BlockedEntities)
        );
        assert!(snapshot.status_derived_progress);
    }
}
