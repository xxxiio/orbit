//! Convert a read-only portfolio comparison into a concrete, revision-fenced
//! schedule proposal. The conversion never writes; the existing approval
//! ledger owns all mutations, receipts and stale-revision rejection.
use crate::{
    AuthoritativeHost,
    planning_approval::{PlanReview, ScheduleApprovalItem, ScheduleApprovalRequest},
    planning_scenarios::{ScenarioCompareRequest, ScenarioComparison, ScenarioResult},
    schedule_planning::FreeWindow,
    store::AuthorityError,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioExecutionRequest {
    pub comparison: ScenarioCompareRequest,
    pub scenario_name: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub command_id: String,
    pub identity_prefix: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioExecutionReview {
    pub selected_scenario: String,
    pub comparison: ScenarioComparison,
    pub plan: ScheduleApprovalRequest,
    pub review: PlanReview,
    pub warnings: Vec<String>,
    pub read_only: bool,
}

fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// Converts the exact chosen tasks, in their comparison order, into contiguous
/// free windows. No task is split; no unplaced task is silently omitted.
/// Its output is only a proposed wire request, not an authoritative command.
pub fn allocate_scenario(
    selected: &ScenarioResult,
    windows: &[FreeWindow],
    revision: i64,
    request: &ScenarioExecutionRequest,
) -> Result<ScheduleApprovalRequest, &'static str> {
    if !valid_id(&request.command_id)
        || !valid_id(&request.identity_prefix)
        || revision != request.comparison.expected_revision
    {
        return Err("invalid or stale scenario identity");
    }
    if request.starts_at_ms < 0
        || request.ends_at_ms <= request.starts_at_ms
        || request
            .ends_at_ms
            .checked_sub(request.starts_at_ms)
            .is_none_or(|n| n > 7 * 86_400_000)
    {
        return Err("invalid scenario schedule window");
    }
    if selected.chosen.is_empty() || selected.chosen.len() > 100 {
        return Err("scenario requires 1 to 100 selected tasks");
    }
    let mut free = windows.to_vec();
    free.sort_by_key(|window| window.starts_at_ms);
    let mut previous = request.starts_at_ms;
    for window in &free {
        if window.starts_at_ms < previous
            || window.ends_at_ms > request.ends_at_ms
            || window.ends_at_ms <= window.starts_at_ms
            || window.minutes != (window.ends_at_ms - window.starts_at_ms) / 60_000
        {
            return Err("invalid or overlapping free windows");
        }
        previous = window.ends_at_ms;
    }
    let mut seen = BTreeSet::new();
    let mut items = Vec::with_capacity(selected.chosen.len());
    for task in &selected.chosen {
        if !seen.insert(task.task_id.clone()) || task.minutes <= 0 || task.minutes > 10_080 {
            return Err("duplicate or invalid scenario task");
        }
        let duration = task
            .minutes
            .checked_mul(60_000)
            .ok_or("task duration overflow")?;
        let Some(window) = free
            .iter_mut()
            .find(|window| window.ends_at_ms - window.starts_at_ms >= duration)
        else {
            return Err("selected scenario cannot fit available calendar windows");
        };
        let start = window.starts_at_ms;
        let end = start.checked_add(duration).ok_or("schedule overflow")?;
        window.starts_at_ms = end;
        window.minutes = (window.ends_at_ms - end) / 60_000;
        let ordinal = items.len() + 1;
        items.push(ScheduleApprovalItem {
            task_id: task.task_id.clone(),
            starts_at_ms: start,
            ends_at_ms: end,
            schedule_block_id: format!("{}-block-{}", request.identity_prefix, ordinal),
            activity_id: format!("{}-activity-{}", request.identity_prefix, ordinal),
        });
    }
    Ok(ScheduleApprovalRequest {
        command_id: request.command_id.clone(),
        expected_revision: revision,
        window_start_ms: request.starts_at_ms,
        window_end_ms: request.ends_at_ms,
        items,
    })
}

impl AuthoritativeHost {
    /// All reads and final approval-review validation are performed while the
    /// transport holds the authority lock. The final approval still repeats
    /// revision, content, and calendar checks at authoritative execution.
    pub fn review_scenario_execution(
        &self,
        actor: &str,
        request: &ScenarioExecutionRequest,
    ) -> Result<ScenarioExecutionReview, AuthorityError> {
        if self.snapshot()?.0 != request.comparison.expected_revision {
            return Err(AuthorityError::StaleRevision);
        }
        if request.comparison.baseline.now_ms != request.starts_at_ms {
            return Err(AuthorityError::Invalid(
                "scenario evaluation time must equal scheduling start",
            ));
        }
        let comparison = self.compare_planning_scenarios(&request.comparison)?;
        let selected = if request.scenario_name == "baseline" {
            &comparison.baseline
        } else {
            comparison
                .alternatives
                .iter()
                .find(|s| s.name == request.scenario_name)
                .ok_or(AuthorityError::Invalid("unknown planning scenario"))?
        };
        let schedule = self.schedule_plan(&crate::schedule_planning::SchedulePlanRequest {
            starts_at_ms: request.starts_at_ms,
            ends_at_ms: request.ends_at_ms,
            max_tasks: 0,
            project_id: request.comparison.baseline.project_id.clone(),
            default_minutes: request.comparison.baseline.default_minutes,
            include_unsized: request.comparison.baseline.include_unsized,
        })?;
        if schedule.task_revision != request.comparison.expected_revision
            || schedule.schedule_revision != request.comparison.expected_revision
        {
            return Err(AuthorityError::StaleRevision);
        }
        let plan = allocate_scenario(
            selected,
            &schedule.free_windows,
            schedule.task_revision,
            request,
        )
        .map_err(AuthorityError::Invalid)?;
        let review = self.review_schedule_request(actor, &plan)?;
        Ok(ScenarioExecutionReview {
            selected_scenario: request.scenario_name.clone(),
            comparison,
            plan,
            review,
            warnings: schedule.warnings,
            read_only: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> ScenarioExecutionRequest {
        ScenarioExecutionRequest {
            comparison: ScenarioCompareRequest {
                expected_revision: 7,
                baseline: crate::portfolio_capacity::PortfolioCapacityRequest {
                    now_ms: 0,
                    available_minutes: 120,
                    max_tasks: 4,
                    project_id: None,
                    include_unassigned: true,
                    include_unsized: false,
                    default_minutes: None,
                },
                alternatives: vec![],
            },
            scenario_name: "baseline".into(),
            starts_at_ms: 0,
            ends_at_ms: 7_200_000,
            command_id: "scenario-command".into(),
            identity_prefix: "scenario-123".into(),
        }
    }
    fn selection(durations: &[i64]) -> ScenarioResult {
        let chosen = durations
            .iter()
            .enumerate()
            .map(|(i, d)| crate::portfolio_capacity::PortfolioCapacityItem {
                task_id: format!("task-{i}"),
                title: format!("Task {i}"),
                project_id: None,
                minutes: *d,
                portfolio_rank: i + 1,
                immediate_unlocks: 0,
                cross_project_downstream: 0,
                rationale: vec![],
            })
            .collect();
        ScenarioResult {
            name: "baseline".into(),
            capacity_minutes: 120,
            allocated_minutes: 0,
            remaining_minutes: 120,
            chosen,
            excluded: vec![],
            newly_selected: vec![],
            displaced: vec![],
            explanation: vec![],
        }
    }
    fn window(a: i64, b: i64) -> FreeWindow {
        FreeWindow {
            starts_at_ms: a,
            ends_at_ms: b,
            minutes: (b - a) / 60_000,
        }
    }
    #[test]
    fn places_tasks_into_separate_free_windows() {
        let result = allocate_scenario(
            &selection(&[30, 30]),
            &[window(0, 1_800_000), window(3_600_000, 5_400_000)],
            7,
            &request(),
        )
        .unwrap();
        assert_eq!(result.items[0].starts_at_ms, 0);
        assert_eq!(result.items[1].starts_at_ms, 3_600_000);
        assert_eq!(result.items[1].ends_at_ms, 5_400_000);
    }
    #[test]
    fn refuses_unsplittable_work() {
        assert!(
            allocate_scenario(
                &selection(&[60]),
                &[window(0, 1_800_000), window(3_600_000, 5_400_000)],
                7,
                &request()
            )
            .is_err()
        );
    }
    #[test]
    fn refuses_stale_revision() {
        assert!(
            allocate_scenario(&selection(&[10]), &[window(0, 7_200_000)], 8, &request()).is_err()
        );
    }
    #[test]
    fn refuses_duplicate_tasks() {
        let mut selection = selection(&[10, 10]);
        selection.chosen[1].task_id = "task-0".into();
        assert!(allocate_scenario(&selection, &[window(0, 7_200_000)], 7, &request()).is_err());
    }
    #[test]
    fn refuses_overlapping_windows() {
        assert!(
            allocate_scenario(
                &selection(&[10]),
                &[window(0, 2_400_000), window(1_800_000, 3_600_000)],
                7,
                &request()
            )
            .is_err()
        );
    }
    #[test]
    fn rejects_empty_plan() {
        assert!(
            allocate_scenario(&selection(&[]), &[window(0, 7_200_000)], 7, &request()).is_err()
        );
    }
    #[test]
    fn rejects_invalid_identity() {
        let mut req = request();
        req.identity_prefix = "unsafe/id".into();
        assert!(allocate_scenario(&selection(&[10]), &[window(0, 7_200_000)], 7, &req).is_err());
    }
    #[test]
    fn deterministic_identities() {
        let a = allocate_scenario(
            &selection(&[20, 20]),
            &[window(0, 7_200_000)],
            7,
            &request(),
        )
        .unwrap();
        let b = allocate_scenario(
            &selection(&[20, 20]),
            &[window(0, 7_200_000)],
            7,
            &request(),
        )
        .unwrap();
        assert_eq!(a, b);
    }
}

#[cfg(test)]
mod authority_workflow_tests {
    use super::*;
    use crate::{
        domain_commands::{TaskAction, TaskCommand},
        planning_approval::{ApprovedScheduleRequest, PlanApproval},
    };
    fn ready_host() -> AuthoritativeHost {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "create-work".into(),
                expected_revision: 0,
                action: TaskAction::Create {
                    task_id: "work".into(),
                    title: "Do work".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "estimate-work".into(),
                expected_revision: 1,
                action: TaskAction::SetPlanning {
                    task_id: "work".into(),
                    estimated_minutes: Some(30),
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                },
            },
        )
        .unwrap();
        host.task_command(
            "device-a",
            TaskCommand {
                command_id: "ready-work".into(),
                expected_revision: 2,
                action: TaskAction::ChangeStatus {
                    task_id: "work".into(),
                    status: "ready".into(),
                },
            },
        )
        .unwrap();
        host
    }
    fn request() -> ScenarioExecutionRequest {
        ScenarioExecutionRequest {
            comparison: ScenarioCompareRequest {
                expected_revision: 3,
                baseline: crate::portfolio_capacity::PortfolioCapacityRequest {
                    now_ms: 0,
                    available_minutes: 60,
                    max_tasks: 1,
                    project_id: None,
                    include_unassigned: true,
                    include_unsized: false,
                    default_minutes: None,
                },
                alternatives: vec![],
            },
            scenario_name: "baseline".into(),
            starts_at_ms: 0,
            ends_at_ms: 3_600_000,
            command_id: "scenario-review-work".into(),
            identity_prefix: "scenario-review-work".into(),
        }
    }
    #[test]
    fn review_then_explicit_approval_creates_one_calendar_reservation() {
        let mut host = ready_host();
        let reviewed = host
            .review_scenario_execution("device-a", &request())
            .unwrap();
        assert!(reviewed.read_only);
        assert_eq!(reviewed.review.scheduled_slots.len(), 1);
        assert_eq!(reviewed.review.scheduled_slots[0].task_id, "work");
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .is_empty()
        );
        let approved = ApprovedScheduleRequest {
            plan: reviewed.plan,
            approval: PlanApproval {
                review: reviewed.review,
                approved: true,
            },
        };
        let receipt = host
            .execute_schedule_request("device-a", approved.clone())
            .unwrap();
        assert!(!receipt.replayed);
        assert_eq!(receipt.accepted_revision, 4);
        let replay = host.execute_schedule_request("device-a", approved).unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.accepted_revision, 4);
        assert_eq!(
            host.application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn cannot_execute_unapproved_scenario() {
        let mut host = ready_host();
        let reviewed = host
            .review_scenario_execution("device-a", &request())
            .unwrap();
        assert!(
            host.execute_schedule_request(
                "device-a",
                ApprovedScheduleRequest {
                    plan: reviewed.plan,
                    approval: PlanApproval {
                        review: reviewed.review,
                        approved: false
                    }
                }
            )
            .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
        assert!(
            host.application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn rejects_stale_comparison_after_concurrent_revision() {
        let mut host = ready_host();
        let request = request();
        host.task_command(
            "device-b",
            TaskCommand {
                command_id: "later-work".into(),
                expected_revision: 3,
                action: TaskAction::Create {
                    task_id: "later".into(),
                    title: "Later work".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        assert!(
            host.review_scenario_execution("device-a", &request)
                .is_err()
        );
    }
    #[test]
    fn rejects_unknown_selected_alternative() {
        let host = ready_host();
        let mut request = request();
        request.scenario_name = "missing".into();
        assert!(
            host.review_scenario_execution("device-a", &request)
                .is_err()
        );
    }
    #[test]
    fn no_silent_execution_when_window_is_too_short() {
        let host = ready_host();
        let mut request = request();
        request.ends_at_ms = 15 * 60_000;
        assert!(
            host.review_scenario_execution("device-a", &request)
                .is_err()
        );
        assert_eq!(host.snapshot().unwrap().0, 3);
    }
}

#[cfg(test)]
mod scenario_edge_tests {
    use super::*;
    use crate::portfolio_capacity::PortfolioCapacityItem;
    fn request() -> ScenarioExecutionRequest {
        ScenarioExecutionRequest {
            comparison: ScenarioCompareRequest {
                expected_revision: 4,
                baseline: crate::portfolio_capacity::PortfolioCapacityRequest {
                    now_ms: 0,
                    available_minutes: 180,
                    max_tasks: 10,
                    project_id: None,
                    include_unassigned: true,
                    include_unsized: false,
                    default_minutes: None,
                },
                alternatives: vec![],
            },
            scenario_name: "baseline".into(),
            starts_at_ms: 0,
            ends_at_ms: 18_000_000,
            command_id: "command-safe".into(),
            identity_prefix: "stable-identity".into(),
        }
    }
    fn scenario(minutes: Vec<i64>) -> ScenarioResult {
        ScenarioResult {
            name: "baseline".into(),
            capacity_minutes: 180,
            allocated_minutes: 0,
            remaining_minutes: 180,
            chosen: minutes
                .into_iter()
                .enumerate()
                .map(|(i, minutes)| PortfolioCapacityItem {
                    task_id: format!("task-{i}"),
                    title: format!("Work {i}"),
                    project_id: None,
                    minutes,
                    portfolio_rank: i + 1,
                    immediate_unlocks: 0,
                    cross_project_downstream: 0,
                    rationale: vec![],
                })
                .collect(),
            excluded: vec![],
            newly_selected: vec![],
            displaced: vec![],
            explanation: vec![],
        }
    }
    fn free(start: i64, end: i64) -> FreeWindow {
        FreeWindow {
            starts_at_ms: start,
            ends_at_ms: end,
            minutes: (end - start) / 60_000,
        }
    }
    #[test]
    fn deterministic_first_fit_does_not_overlap() {
        let request = request();
        let proposal = allocate_scenario(
            &scenario(vec![30, 45, 30, 20]),
            &[
                free(0, 1_800_000),
                free(3_600_000, 8_100_000),
                free(9_000_000, 16_200_000),
            ],
            4,
            &request,
        )
        .unwrap();
        assert_eq!(proposal.items.len(), 4);
        assert_eq!(proposal.items[0].starts_at_ms, 0);
        assert_eq!(proposal.items[1].starts_at_ms, 3_600_000);
        for (i, a) in proposal.items.iter().enumerate() {
            assert_eq!(
                a.ends_at_ms - a.starts_at_ms,
                (scenario(vec![30, 45, 30, 20]).chosen[i].minutes) * 60_000
            );
            for b in proposal.items.iter().skip(i + 1) {
                assert!(a.ends_at_ms <= b.starts_at_ms || b.ends_at_ms <= a.starts_at_ms);
            }
        }
    }
    #[test]
    fn rejects_window_outside_requested_bounds() {
        let request = request();
        assert!(
            allocate_scenario(
                &scenario(vec![30]),
                &[free(-60_000, 3_600_000)],
                4,
                &request
            )
            .is_err()
        );
        assert!(
            allocate_scenario(&scenario(vec![30]), &[free(0, 18_060_000)], 4, &request).is_err()
        );
    }
    #[test]
    fn rejects_inconsistent_free_window_metadata() {
        let request = request();
        let mut window = free(0, 3_600_000);
        window.minutes = 5;
        assert!(allocate_scenario(&scenario(vec![30]), &[window], 4, &request).is_err());
    }
    #[test]
    fn rejects_zero_or_negative_task_duration() {
        let request = request();
        assert!(allocate_scenario(&scenario(vec![0]), &[free(0, 3_600_000)], 4, &request).is_err());
        assert!(
            allocate_scenario(&scenario(vec![-1]), &[free(0, 3_600_000)], 4, &request).is_err()
        );
    }
    #[test]
    fn rejects_more_than_hundred_reservations() {
        let request = request();
        assert!(
            allocate_scenario(&scenario(vec![1; 101]), &[free(0, 18_000_000)], 4, &request)
                .is_err()
        );
    }
    #[test]
    fn rejects_malformed_command_identity() {
        let mut request = request();
        request.command_id = "unsafe/command".into();
        assert!(
            allocate_scenario(&scenario(vec![30]), &[free(0, 3_600_000)], 4, &request).is_err()
        );
    }
    #[test]
    fn rejected_allocation_does_not_modify_original_windows() {
        let request = request();
        let windows = vec![free(0, 1_800_000)];
        let original = windows.clone();
        assert!(allocate_scenario(&scenario(vec![15, 30]), &windows, 4, &request).is_err());
        assert_eq!(windows, original);
    }
    #[test]
    fn no_identity_collision_across_one_review() {
        let request = request();
        let plan = allocate_scenario(
            &scenario(vec![10, 10, 10]),
            &[free(0, 3_600_000)],
            4,
            &request,
        )
        .unwrap();
        let block_ids: std::collections::BTreeSet<_> =
            plan.items.iter().map(|i| &i.schedule_block_id).collect();
        let activity_ids: std::collections::BTreeSet<_> =
            plan.items.iter().map(|i| &i.activity_id).collect();
        assert_eq!(block_ids.len(), 3);
        assert_eq!(activity_ids.len(), 3);
    }
}

/// A side-by-side calendar feasibility report. Every entry is a read-only
/// reviewed option or an explicit rejection reason. Users must choose and
/// separately approve exactly one option; no 'best choice' is auto-applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioCalendarOption {
    pub name: String,
    pub feasible: bool,
    pub preview: Option<ScenarioExecutionReview>,
    pub rejection: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioCalendarOptions {
    pub expected_revision: i64,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub options: Vec<ScenarioCalendarOption>,
    pub read_only: bool,
}
impl AuthoritativeHost {
    /// Evaluate the baseline and all named alternatives using one accepted
    /// global revision fence. All results are only reviews, never writes.
    /// Infeasible alternatives are reported rather than aborting other viable
    /// options, so the user can make an informed selection.
    pub fn review_scenario_options(
        &self,
        actor: &str,
        request: &ScenarioExecutionRequest,
    ) -> Result<ScenarioCalendarOptions, AuthorityError> {
        if self.snapshot()?.0 != request.comparison.expected_revision {
            return Err(AuthorityError::StaleRevision);
        }
        // Validate the complete comparison before constructing any plan.
        // This also checks duplicate names, visibility and invalid pins.
        self.compare_planning_scenarios(&request.comparison)?;
        let mut names = vec!["baseline".to_owned()];
        names.extend(
            request
                .comparison
                .alternatives
                .iter()
                .map(|a| a.name.clone()),
        );
        let mut options = Vec::with_capacity(names.len());
        for name in names {
            let mut one = request.clone();
            one.scenario_name = name.clone();
            let result = self.review_scenario_execution(actor, &one);
            match result {
                Ok(preview) => options.push(ScenarioCalendarOption {
                    name,
                    feasible: true,
                    preview: Some(preview),
                    rejection: None,
                }),
                Err(AuthorityError::Invalid(reason)) => options.push(ScenarioCalendarOption {
                    name,
                    feasible: false,
                    preview: None,
                    rejection: Some(reason.to_owned()),
                }),
                Err(other) => return Err(other),
            }
        }
        if self.snapshot()?.0 != request.comparison.expected_revision {
            return Err(AuthorityError::StaleRevision);
        }
        Ok(ScenarioCalendarOptions {
            expected_revision: request.comparison.expected_revision,
            window_start_ms: request.starts_at_ms,
            window_end_ms: request.ends_at_ms,
            options,
            read_only: true,
        })
    }
}

#[cfg(test)]
mod options_tests {
    use super::*;
    use crate::domain_commands::{TaskAction, TaskCommand};
    fn host() -> AuthoritativeHost {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        for (id, command_id, revision) in [("one", "create-one", 0), ("two", "create-two", 2)] {
            host.task_command(
                "actor",
                TaskCommand {
                    command_id: command_id.into(),
                    expected_revision: revision,
                    action: TaskAction::Create {
                        task_id: id.into(),
                        title: id.into(),
                        project_id: None,
                    },
                },
            )
            .unwrap();
            host.task_command(
                "actor",
                TaskCommand {
                    command_id: format!("estimate-{id}"),
                    expected_revision: revision + 1,
                    action: TaskAction::SetPlanning {
                        task_id: id.into(),
                        estimated_minutes: Some(30),
                        earliest_start_at_ms: None,
                        deadline_at_ms: None,
                    },
                },
            )
            .unwrap();
        }
        host.task_command(
            "actor",
            TaskCommand {
                command_id: "ready-one".into(),
                expected_revision: 4,
                action: TaskAction::ChangeStatus {
                    task_id: "one".into(),
                    status: "ready".into(),
                },
            },
        )
        .unwrap();
        host.task_command(
            "actor",
            TaskCommand {
                command_id: "ready-two".into(),
                expected_revision: 5,
                action: TaskAction::ChangeStatus {
                    task_id: "two".into(),
                    status: "ready".into(),
                },
            },
        )
        .unwrap();
        host
    }
    fn request() -> ScenarioExecutionRequest {
        ScenarioExecutionRequest {
            comparison: ScenarioCompareRequest {
                expected_revision: 6,
                baseline: crate::portfolio_capacity::PortfolioCapacityRequest {
                    now_ms: 0,
                    available_minutes: 60,
                    max_tasks: 2,
                    project_id: None,
                    include_unassigned: true,
                    include_unsized: false,
                    default_minutes: None,
                },
                alternatives: vec![crate::planning_scenarios::ScenarioChoice {
                    name: "short-list".into(),
                    pinned_task_ids: vec![],
                    excluded_task_ids: vec!["two".into()],
                    available_minutes: Some(30),
                    max_tasks: Some(1),
                }],
            },
            scenario_name: "baseline".into(),
            starts_at_ms: 0,
            ends_at_ms: 3_600_000,
            command_id: "option-review".into(),
            identity_prefix: "option-review".into(),
        }
    }
    #[test]
    fn reviews_baseline_and_alternative_without_writes() {
        let host = host();
        let report = host.review_scenario_options("actor", &request()).unwrap();
        assert!(report.read_only);
        assert_eq!(report.options.len(), 2);
        assert!(report.options.iter().all(|r| r.feasible));
        assert_eq!(
            report.options[0].preview.as_ref().unwrap().plan.items.len(),
            2
        );
        assert_eq!(
            report.options[1].preview.as_ref().unwrap().plan.items.len(),
            1
        );
        assert_eq!(host.snapshot().unwrap().0, 6);
        assert!(
            host.application_queries()
                .schedule_between(0, 3_600_000)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn infeasible_baseline_does_not_hide_viable_alternative() {
        let host = host();
        let mut request = request();
        request.ends_at_ms = 1_800_000;
        let report = host.review_scenario_options("actor", &request).unwrap();
        assert!(!report.options[0].feasible);
        assert!(report.options[0].rejection.is_some());
        assert!(report.options[1].feasible);
        assert_eq!(host.snapshot().unwrap().0, 6);
    }
    #[test]
    fn all_options_reject_after_revision_advances() {
        let mut host = host();
        let request = request();
        host.task_command(
            "other",
            TaskCommand {
                command_id: "concurrent".into(),
                expected_revision: 6,
                action: TaskAction::Create {
                    task_id: "three".into(),
                    title: "Three".into(),
                    project_id: None,
                },
            },
        )
        .unwrap();
        assert!(host.review_scenario_options("actor", &request).is_err());
    }
}

#[cfg(test)]
mod option_transport_contract_tests {
    use super::*;
    #[test]
    fn option_result_json_distinguishes_rejected_and_reviewable_plans() {
        let response = ScenarioCalendarOptions {
            expected_revision: 7,
            window_start_ms: 0,
            window_end_ms: 3_600_000,
            read_only: true,
            options: vec![ScenarioCalendarOption {
                name: "baseline".into(),
                feasible: false,
                preview: None,
                rejection: Some("task requires a longer uninterrupted window".into()),
            }],
        };
        let encoded = serde_json::to_string(&response).unwrap();
        let roundtrip: ScenarioCalendarOptions = serde_json::from_str(&encoded).unwrap();
        assert_eq!(roundtrip, response);
        assert_eq!(roundtrip.options[0].name, "baseline");
        assert!(roundtrip.options[0].preview.is_none());
        assert!(roundtrip.read_only);
    }
}
