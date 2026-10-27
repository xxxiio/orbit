//! Portfolio-first calendar proposals built on the shared accepted schedule and
//! task graph. Unlike the standard schedule planner, this places high-leverage
//! tasks first when several actionable tasks compete for one free window.
//!
//! This remains an advisory read. It does not create calendar blocks or consume
//! pending offline commands, and it never changes persisted task priority.
use crate::{
    authority_schedule::ScheduleRecord,
    domain_commands::TaskRecord,
    portfolio_actions::{PortfolioActionRequest, recommend_portfolio_actions},
    schedule_planning::{
        FreeWindow, PlannedSlot, SchedulePlan, SchedulePlanRequest, UnplacedTask, plan_schedule,
    },
    task_dependencies::TaskDependencyEdge,
};
use std::collections::BTreeMap;

/// Schedule accepted, actionable work using portfolio leverage as the primary
/// ordering. The input revision must describe tasks, edges and accepted blocks
/// from the *same* authoritative snapshot.
pub fn plan_portfolio_schedule(
    revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    blocks: &[ScheduleRecord],
    request: &SchedulePlanRequest,
) -> Result<SchedulePlan, &'static str> {
    // Delegate all horizon, estimate-default and accepted-calendar validation
    // to the existing planner. max_tasks=0 gets its verified busy-time union
    // without letting the legacy deadline-first allocator consume windows.
    let mut base = plan_schedule(
        revision,
        revision,
        tasks,
        edges,
        blocks,
        &SchedulePlanRequest {
            starts_at_ms: request.starts_at_ms,
            ends_at_ms: request.ends_at_ms,
            max_tasks: 0,
            project_id: request.project_id.clone(),
            default_minutes: request.default_minutes,
            include_unsized: request.include_unsized,
        },
    )?;
    if request.max_tasks > 10_000 {
        return Err("portfolio task limit exceeds safety bound");
    }
    let actions = recommend_portfolio_actions(
        revision,
        tasks,
        edges,
        &PortfolioActionRequest {
            now_ms: request.starts_at_ms,
            project_id: request.project_id.clone(),
            limit: tasks.len(),
            include_unassigned: request.project_id.is_none(),
        },
    )?;
    let by_id: BTreeMap<&str, &TaskRecord> =
        tasks.iter().map(|task| (task.id.as_str(), task)).collect();
    if by_id.len() != tasks.len() {
        return Err("duplicate accepted task identity");
    }
    let mut windows = base.free_windows.clone();
    let mut slots = Vec::new();
    let mut unplaced = Vec::new();
    for action in &actions.recommended {
        let task = by_id
            .get(action.task_id.as_str())
            .ok_or("portfolio task missing")?;
        if slots.len() >= request.max_tasks {
            unplaced.push(UnplacedTask {
                task_id: action.task_id.clone(),
                reason: "Task count limit reached in portfolio proposal".into(),
            });
            continue;
        }
        let duration = match task.estimated_minutes {
            Some(value) if value > 0 => value,
            Some(_) => {
                unplaced.push(UnplacedTask {
                    task_id: action.task_id.clone(),
                    reason: "Invalid task duration".into(),
                });
                continue;
            }
            None if request.include_unsized => match request.default_minutes {
                Some(value) => value,
                None => {
                    unplaced.push(UnplacedTask {
                        task_id: action.task_id.clone(),
                        reason: "Unsized task needs a positive default duration".into(),
                    });
                    continue;
                }
            },
            None => {
                unplaced.push(UnplacedTask {
                    task_id: action.task_id.clone(),
                    reason: "Unsized task excluded from automatic portfolio scheduling".into(),
                });
                continue;
            }
        };
        let duration_ms = duration
            .checked_mul(60_000)
            .ok_or("task duration overflow")?;
        let earliest = task
            .earliest_start_at_ms
            .unwrap_or(request.starts_at_ms)
            .max(request.starts_at_ms);
        // The same shared rule as the standard planner and the approval gate
        // (`orbit_planner::deadline_bound_at`): a deadline already missed when the
        // window opens cannot be met, so it stops limiting the block end; a
        // deadline still ahead keeps binding.
        let deadline = orbit_planner::deadline_bound_at(task.deadline_at_ms, request.starts_at_ms);
        let missed_deadline = task.deadline_at_ms.is_some() && deadline.is_none();
        let mut placement = None;
        let mut deadline_blocked = false;
        for (index, window) in windows.iter().enumerate() {
            let starts_at_ms = window.starts_at_ms.max(earliest);
            let ends_at_ms = starts_at_ms
                .checked_add(duration_ms)
                .ok_or("slot overflow")?;
            if ends_at_ms <= window.ends_at_ms {
                if deadline.is_some_and(|limit| ends_at_ms > limit) {
                    deadline_blocked = true;
                    continue;
                }
                placement = Some((index, starts_at_ms, ends_at_ms));
                break;
            }
        }
        let Some((index, start, end)) = placement else {
            unplaced.push(UnplacedTask {
                task_id: action.task_id.clone(),
                reason: if deadline_blocked {
                    format!(
                        "Its deadline falls before this {duration}-minute block could end; move the deadline later or choose an earlier window"
                    )
                } else {
                    format!("No uninterrupted {duration}-minute opening after earliest start")
                },
            });
            continue;
        };
        let old = windows.remove(index);
        if start > old.starts_at_ms {
            let remaining = (start - old.starts_at_ms) / 60_000;
            if remaining > 0 {
                windows.push(FreeWindow {
                    starts_at_ms: old.starts_at_ms,
                    ends_at_ms: start,
                    minutes: remaining,
                });
            }
        }
        if old.ends_at_ms > end {
            let remaining = (old.ends_at_ms - end) / 60_000;
            if remaining > 0 {
                windows.push(FreeWindow {
                    starts_at_ms: end,
                    ends_at_ms: old.ends_at_ms,
                    minutes: remaining,
                });
            }
        }
        windows.sort_by_key(|window| window.starts_at_ms);
        let mut explanation = vec![
            format!(
                "Portfolio action #{} fits an uninterrupted {duration}-minute window",
                action.rank
            ),
            format!(
                "Immediately unblocks {} accepted task(s)",
                action.immediately_unblocked
            ),
        ];
        if action.other_project_downstream > 0 {
            explanation.push(format!(
                "Downstream impact includes {} task(s) in other projects",
                action.other_project_downstream
            ));
        }
        if action.overdue {
            explanation.push("Overdue accepted deadline".into());
        }
        if missed_deadline {
            explanation.push(
                "Deadline has already passed; it no longer limits where this block ends".into(),
            );
        }
        slots.push(PlannedSlot {
            task_id: task.id.clone(),
            title: task.title.clone(),
            project_id: task.project_id.clone(),
            starts_at_ms: start,
            ends_at_ms: end,
            minutes: duration,
            ranking: action.rank,
            explanation,
        });
    }
    // Blocked and deferred work should not silently disappear from a planning
    // proposal. Preserve reasons in the existing planning brief read model;
    // only actionable portfolio candidates are explicitly unplaced here.
    let allocated_minutes = slots
        .iter()
        .try_fold(0i64, |sum, slot| sum.checked_add(slot.minutes))
        .ok_or("allocated time overflow")?;
    let mut warnings = Vec::new();
    if !unplaced.is_empty() {
        warnings.push("Some actionable portfolio work could not fit today's free windows".into());
    }
    if slots.is_empty() && !actions.recommended.is_empty() {
        warnings.push("No portfolio action fits the current planning window".into());
    }
    if request.project_id.is_some() {
        warnings.push(
            "Cross-project leverage is aggregate-only; foreign task details are not shown".into(),
        );
    }
    base.allocated_minutes = allocated_minutes;
    base.slots = slots;
    base.free_windows = windows;
    base.unplaced = unplaced;
    base.warnings = warnings;
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, project: &str, minutes: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: format!("Task {id}"),
            status: "ready".into(),
            project_id: Some(project.into()),
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn edge(t: &str, p: &str) -> TaskDependencyEdge {
        TaskDependencyEdge {
            task_id: t.into(),
            prerequisite_id: p.into(),
        }
    }
    fn req() -> SchedulePlanRequest {
        SchedulePlanRequest {
            starts_at_ms: 0,
            ends_at_ms: 120 * 60_000,
            max_tasks: 2,
            project_id: None,
            default_minutes: Some(30),
            include_unsized: false,
        }
    }
    fn block(start: i64, end: i64) -> ScheduleRecord {
        ScheduleRecord {
            id: format!("meeting-{start}"),
            title: "Busy".into(),
            starts_at_ms: start,
            ends_at_ms: end,
            duration_minutes: ((end - start) / 60_000) as u32,
            flexibility: "fixed".into(),
            target_kind: "none".into(),
            target_id: None,
        }
    }
    #[test]
    fn prioritizes_cross_project_unlock_over_alphabetical_order() {
        let tasks = [
            task("aaa", "platform", Some(40)),
            task("zzz", "platform", Some(40)),
            task("child", "web", Some(30)),
            task("second-child", "mobile", Some(30)),
        ];
        let report = plan_portfolio_schedule(
            2,
            &tasks,
            &[edge("child", "zzz"), edge("second-child", "zzz")],
            &[],
            &req(),
        )
        .unwrap();
        assert_eq!(report.slots[0].task_id, "zzz");
        assert_eq!(report.slots[0].ranking, 1);
        assert!(
            report.slots[0]
                .explanation
                .iter()
                .any(|r| r.contains("other projects"))
        );
        assert_eq!(report.allocated_minutes, 80);
    }
    #[test]
    fn never_double_books_existing_meeting() {
        let tasks = [task("a", "p", Some(40)), task("b", "p", Some(40))];
        let report =
            plan_portfolio_schedule(1, &tasks, &[], &[block(40 * 60_000, 80 * 60_000)], &req())
                .unwrap();
        assert_eq!(report.slots.len(), 2);
        assert_eq!(report.slots[0].starts_at_ms, 0);
        assert_eq!(report.slots[1].starts_at_ms, 80 * 60_000);
        assert_eq!(report.occupied_minutes, 40);
        assert_eq!(report.allocated_minutes, 80);
    }
    #[test]
    fn impossible_slot_is_reported_not_split() {
        let tasks = [task("large", "p", Some(70))];
        let report =
            plan_portfolio_schedule(1, &tasks, &[], &[block(50 * 60_000, 70 * 60_000)], &req())
                .unwrap();
        assert!(report.slots.is_empty());
        assert!(report.unplaced[0].reason.contains("uninterrupted"));
    }
    #[test]
    fn project_scoped_proposal_never_names_foreign_tasks() {
        let tasks = [
            task("local", "p", Some(20)),
            task("secret-foreign", "q", Some(20)),
        ];
        let mut request = req();
        request.project_id = Some("p".into());
        let report =
            plan_portfolio_schedule(1, &tasks, &[edge("secret-foreign", "local")], &[], &request)
                .unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("secret-foreign"));
        assert_eq!(report.slots.len(), 1);
    }
    #[test]
    fn an_overdue_deadline_is_still_placed_and_says_why() {
        let mut request = req();
        request.starts_at_ms = 60 * 60_000;
        request.ends_at_ms = 180 * 60_000;
        let mut late = task("late", "p", Some(30));
        // The deadline passed before the window opens: no block in the window
        // can meet it, so it must not stop the proposal (D-01's rule, applied to
        // portfolio ordering too).
        late.deadline_at_ms = Some(0);
        let report = plan_portfolio_schedule(1, &[late], &[], &[], &request).unwrap();
        assert_eq!(report.slots.len(), 1);
        assert!(
            report.slots[0]
                .explanation
                .iter()
                .any(|line| line.contains("Deadline has already passed")),
            "got {:?}",
            report.slots[0].explanation
        );
    }
    #[test]
    fn missing_estimate_is_explicitly_excluded() {
        let tasks = [task("unsized", "p", None)];
        let mut request = req();
        let report = plan_portfolio_schedule(1, &tasks, &[], &[], &request).unwrap();
        assert!(report.slots.is_empty());
        assert!(report.unplaced[0].reason.contains("Unsized"));
        request.include_unsized = true;
        assert_eq!(
            plan_portfolio_schedule(1, &tasks, &[], &[], &request)
                .unwrap()
                .slots[0]
                .minutes,
            30
        );
    }
    #[test]
    fn duplicate_identity_and_cycle_fail_closed() {
        let a = task("a", "p", Some(10));
        assert!(plan_portfolio_schedule(1, &[a.clone(), a], &[], &[], &req()).is_err());
        let tasks = [task("a", "p", Some(10)), task("b", "p", Some(10))];
        assert!(
            plan_portfolio_schedule(1, &tasks, &[edge("a", "b"), edge("b", "a")], &[], &req())
                .is_err()
        );
    }
    #[test]
    fn input_order_does_not_affect_proposal() {
        let mut tasks = vec![task("b", "p", Some(15)), task("a", "p", Some(15))];
        let mut edges = vec![];
        let first = plan_portfolio_schedule(1, &tasks, &edges, &[], &req()).unwrap();
        tasks.reverse();
        edges.reverse();
        let second = plan_portfolio_schedule(1, &tasks, &edges, &[], &req()).unwrap();
        assert_eq!(first, second);
    }
    #[test]
    fn zero_task_limit_is_a_valid_empty_proposal() {
        let tasks = [task("a", "p", Some(30))];
        let mut request = req();
        request.max_tasks = 0;
        let result = plan_portfolio_schedule(1, &tasks, &[], &[], &request).unwrap();
        assert!(result.slots.is_empty());
        assert_eq!(result.available_minutes, 120);
    }
}
