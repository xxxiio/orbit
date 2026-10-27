//! Deterministic, read-only time-window allocation based on accepted task and
//! schedule projections. No calendar reservations or task edits are performed.
use crate::{
    authority_schedule::ScheduleRecord,
    daily_planning::{DayPlanRequest, PlanningDisposition, plan_day},
    domain_commands::TaskRecord,
    task_dependencies::TaskDependencyEdge,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulePlanRequest {
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub max_tasks: usize,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub default_minutes: Option<i64>,
    #[serde(default)]
    pub include_unsized: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreeWindow {
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub minutes: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedSlot {
    pub task_id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub minutes: i64,
    pub ranking: usize,
    pub explanation: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnplacedTask {
    pub task_id: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulePlan {
    pub task_revision: i64,
    pub schedule_revision: i64,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub available_minutes: i64,
    pub allocated_minutes: i64,
    pub occupied_minutes: i64,
    pub free_windows: Vec<FreeWindow>,
    pub slots: Vec<PlannedSlot>,
    pub unplaced: Vec<UnplacedTask>,
    pub warnings: Vec<String>,
}

/// Merge overlapping accepted schedule intervals. The planner treats every
/// block as busy, regardless of target or flexibility, to avoid double-booking.
/// Sub-minute tails are deliberately not counted as usable capacity.
fn free_windows(
    start: i64,
    end: i64,
    blocks: &[ScheduleRecord],
) -> Result<(Vec<FreeWindow>, i64), &'static str> {
    let mut busy = Vec::new();
    for block in blocks {
        if block.ends_at_ms <= block.starts_at_ms {
            return Err("invalid accepted schedule interval");
        }
        if block.ends_at_ms <= start || block.starts_at_ms >= end {
            continue;
        }
        busy.push((block.starts_at_ms.max(start), block.ends_at_ms.min(end)));
    }
    busy.sort_unstable();
    let mut union: Vec<(i64, i64)> = Vec::new();
    for (s, e) in busy {
        if let Some(last) = union.last_mut()
            && s <= last.1
        {
            last.1 = last.1.max(e);
            continue;
        }
        union.push((s, e));
    }
    let occupied_ms: i64 = union
        .iter()
        .try_fold(0i64, |sum, (s, e)| sum.checked_add(e - s))
        .ok_or("busy capacity overflow")?;
    let mut windows = Vec::new();
    let mut cursor = start;
    for (s, e) in union {
        if s > cursor {
            let minutes = (s - cursor) / 60_000;
            if minutes > 0 {
                windows.push(FreeWindow {
                    starts_at_ms: cursor,
                    ends_at_ms: cursor + minutes * 60_000,
                    minutes,
                });
            }
        }
        cursor = e;
    }
    if cursor < end {
        let minutes = (end - cursor) / 60_000;
        if minutes > 0 {
            windows.push(FreeWindow {
                starts_at_ms: cursor,
                ends_at_ms: cursor + minutes * 60_000,
                minutes,
            });
        }
    }
    Ok((windows, occupied_ms / 60_000))
}

/// Place the existing deterministic next-action order into real free windows.
/// Duration is indivisible: a task requiring 60 minutes never occupies two
/// disjoint 30-minute windows. Existing task-targeted blocks remain busy.
pub fn plan_schedule(
    task_revision: i64,
    schedule_revision: i64,
    tasks: &[TaskRecord],
    edges: &[TaskDependencyEdge],
    blocks: &[ScheduleRecord],
    request: &SchedulePlanRequest,
) -> Result<SchedulePlan, &'static str> {
    if task_revision < 0
        || schedule_revision < 0
        || request.starts_at_ms < 0
        || request.ends_at_ms <= request.starts_at_ms
    {
        return Err("invalid schedule planning interval");
    }
    let span = request
        .ends_at_ms
        .checked_sub(request.starts_at_ms)
        .ok_or("time overflow")?;
    if span > 7 * 86_400_000 {
        return Err("schedule horizon exceeds seven days");
    }
    if request.max_tasks > 10_000 {
        return Err("task limit exceeds safety bound");
    }
    if request
        .default_minutes
        .is_some_and(|x| x <= 0 || x > 10_080)
    {
        return Err("invalid default duration");
    }
    let (mut windows, occupied_minutes) =
        free_windows(request.starts_at_ms, request.ends_at_ms, blocks)?;
    let available_minutes = windows
        .iter()
        .try_fold(0i64, |sum, w| sum.checked_add(w.minutes))
        .ok_or("capacity overflow")?;
    let day = plan_day(
        task_revision,
        tasks,
        edges,
        &DayPlanRequest {
            now_ms: request.starts_at_ms,
            available_minutes,
            max_tasks: request.max_tasks,
            project_id: request.project_id.clone(),
            default_minutes: request.default_minutes,
            include_unsized: request.include_unsized,
        },
    )?;
    let mut slots = Vec::new();
    let mut unplaced = Vec::new();
    let mut selected = BTreeSet::new();
    let by_id: BTreeMap<&str, &TaskRecord> =
        tasks.iter().map(|task| (task.id.as_str(), task)).collect();
    let mut ranked: Vec<_> = day
        .tasks
        .iter()
        .filter(|task| {
            task.rank.is_some() && task.disposition != PlanningDisposition::OutsideProject
        })
        .collect();
    ranked.sort_by_key(|task| task.rank);
    let had_actionable = !ranked.is_empty();
    for candidate in ranked {
        if slots.len() >= request.max_tasks {
            unplaced.push(UnplacedTask {
                task_id: candidate.task_id.clone(),
                reason: "Task limit reached".into(),
            });
            continue;
        }
        let Some(duration) = candidate.estimated_minutes.or(request.default_minutes) else {
            unplaced.push(UnplacedTask {
                task_id: candidate.task_id.clone(),
                reason: "No duration estimate; an unsized task cannot reserve a time window".into(),
            });
            continue;
        };
        if duration <= 0 {
            unplaced.push(UnplacedTask {
                task_id: candidate.task_id.clone(),
                reason: "Invalid task estimate".into(),
            });
            continue;
        }
        let task = by_id
            .get(candidate.task_id.as_str())
            .ok_or("missing ranked task")?;
        let earliest = task
            .earliest_start_at_ms
            .unwrap_or(request.starts_at_ms)
            .max(request.starts_at_ms);
        // The same rule the approval gate applies (see
        // `orbit_planner::deadline_bound_at`): a deadline that was already missed
        // when this window opens cannot be met by any block in it, so it stops
        // limiting where the block ends; the slot says so. A deadline still ahead
        // of the window keeps binding, so this placement is always reviewable.
        let deadline = orbit_planner::deadline_bound_at(task.deadline_at_ms, request.starts_at_ms);
        let missed_deadline = task.deadline_at_ms.is_some() && deadline.is_none();
        let required_ms = duration.checked_mul(60_000).ok_or("duration overflow")?;
        let mut placement = None;
        let mut deadline_blocked = false;
        for (index, window) in windows.iter().enumerate() {
            let start = window.starts_at_ms.max(earliest);
            let end = start.checked_add(required_ms).ok_or("slot time overflow")?;
            if end <= window.ends_at_ms {
                if deadline.is_some_and(|limit| end > limit) {
                    deadline_blocked = true;
                    continue;
                }
                placement = Some((index, start, end));
                break;
            }
        }
        if let Some((index, start, end)) = placement {
            let previous = windows.remove(index);
            if previous.starts_at_ms < start {
                let minutes = (start - previous.starts_at_ms) / 60_000;
                if minutes > 0 {
                    windows.insert(
                        index,
                        FreeWindow {
                            starts_at_ms: previous.starts_at_ms,
                            ends_at_ms: start,
                            minutes,
                        },
                    );
                }
            }
            let tail = (end, previous.ends_at_ms);
            if tail.1 > tail.0 {
                let minutes = (tail.1 - tail.0) / 60_000;
                if minutes > 0 {
                    windows.push(FreeWindow {
                        starts_at_ms: tail.0,
                        ends_at_ms: tail.1,
                        minutes,
                    });
                }
            }
            windows.sort_by_key(|w| w.starts_at_ms);
            selected.insert(candidate.task_id.clone());
            let mut explanation =
                vec![format!("Fits an uninterrupted {}-minute opening", duration)];
            if missed_deadline {
                explanation.push(
                    "Deadline has already passed; it no longer limits where this block ends".into(),
                );
            }
            slots.push(PlannedSlot {
                task_id: candidate.task_id.clone(),
                title: candidate.title.clone(),
                project_id: candidate.project_id.clone(),
                starts_at_ms: start,
                ends_at_ms: end,
                minutes: duration,
                ranking: candidate.rank.unwrap_or_default(),
                explanation,
            });
        } else if deadline_blocked {
            unplaced.push(UnplacedTask {
                task_id: candidate.task_id.clone(),
                reason: format!(
                    "Its deadline falls before this {duration}-minute block could end; move the deadline later or choose an earlier window"
                ),
            });
        } else {
            unplaced.push(UnplacedTask {
                task_id: candidate.task_id.clone(),
                reason: format!(
                    "No uninterrupted {}-minute window after earliest start",
                    duration
                ),
            });
        }
    }
    for candidate in day.tasks.iter().filter(|task| {
        task.rank.is_none() && task.disposition != PlanningDisposition::OutsideProject
    }) {
        unplaced.push(UnplacedTask {
            task_id: candidate.task_id.clone(),
            reason: candidate.explanation.join("; "),
        });
    }
    let allocated_minutes = slots.iter().map(|slot| slot.minutes).sum();
    let mut warnings = day.warnings;
    if !unplaced.is_empty() {
        warnings.push("Review unplaced tasks and their explanations".into());
    }
    if selected.is_empty() && had_actionable {
        warnings.push("No actionable task fits a free schedule window".into());
    }
    Ok(SchedulePlan {
        task_revision,
        schedule_revision,
        starts_at_ms: request.starts_at_ms,
        ends_at_ms: request.ends_at_ms,
        available_minutes,
        allocated_minutes,
        occupied_minutes,
        free_windows: windows,
        slots,
        unplaced,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, minutes: Option<i64>, start: Option<i64>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: "ready".into(),
            project_id: None,
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: start,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn task_due(id: &str, minutes: Option<i64>, deadline: i64) -> TaskRecord {
        TaskRecord {
            deadline_at_ms: Some(deadline),
            ..task(id, minutes, None)
        }
    }
    fn block(id: &str, start: i64, end: i64) -> ScheduleRecord {
        ScheduleRecord {
            id: id.into(),
            title: id.into(),
            starts_at_ms: start,
            ends_at_ms: end,
            duration_minutes: ((end - start) / 60_000) as u32,
            flexibility: "fixed".into(),
            target_kind: "none".into(),
            target_id: None,
        }
    }
    fn request() -> SchedulePlanRequest {
        SchedulePlanRequest {
            // The fixture window is 01:00..04:01: the leading free interval is
            // 61 minutes, which cannot hold a 90-minute task.
            starts_at_ms: 60 * 60_000,
            ends_at_ms: 241 * 60_000,
            max_tasks: 5,
            project_id: None,
            default_minutes: None,
            include_unsized: false,
        }
    }
    #[test]
    fn rejects_invalid_intervals_and_week_long_horizons() {
        let mut r = request();
        r.ends_at_ms = r.starts_at_ms;
        assert!(plan_schedule(1, 1, &[], &[], &[], &r).is_err());
        r.ends_at_ms = r.starts_at_ms + 8 * 86_400_000;
        assert!(plan_schedule(1, 1, &[], &[], &[], &r).is_err());
    }
    #[test]
    fn overlapping_blocks_do_not_double_subtract_capacity() {
        let r = request();
        let blocks = [
            block("one", 61 * 60_000, 121 * 60_000),
            block("two", 91 * 60_000, 151 * 60_000),
        ];
        let plan = plan_schedule(2, 2, &[], &[], &blocks, &r).unwrap();
        assert_eq!(plan.occupied_minutes, 90);
        assert_eq!(plan.available_minutes, 91);
        assert_eq!(plan.free_windows.len(), 2);
    }
    #[test]
    fn never_splits_a_task_across_busy_windows() {
        let r = request();
        let blocks = [block("meeting", 121 * 60_000, 181 * 60_000)];
        let plan = plan_schedule(2, 2, &[task("long", Some(90), None)], &[], &blocks, &r).unwrap();
        assert!(plan.slots.is_empty());
        assert_eq!(plan.unplaced.len(), 1);
        assert!(plan.unplaced[0].reason.contains("uninterrupted"));
    }
    #[test]
    fn uses_free_intervals_in_priority_order_without_overlaps() {
        let r = request();
        let blocks = [block("meeting", 121 * 60_000, 181 * 60_000)];
        let tasks = [
            task("first", Some(60), None),
            task("second", Some(30), None),
            task("third", Some(30), None),
        ];
        let plan = plan_schedule(3, 3, &tasks, &[], &blocks, &r).unwrap();
        assert_eq!(plan.slots.len(), 3);
        for slot in &plan.slots {
            assert!(slot.ends_at_ms <= 121 * 60_000 || slot.starts_at_ms >= 181 * 60_000);
        }
        for (i, a) in plan.slots.iter().enumerate() {
            for b in plan.slots.iter().skip(i + 1) {
                assert!(a.ends_at_ms <= b.starts_at_ms || b.ends_at_ms <= a.starts_at_ms);
            }
        }
        assert_eq!(plan.allocated_minutes, 120);
    }
    #[test]
    fn earliest_start_is_respected_in_real_time() {
        let r = request();
        let tasks = [task("later", Some(40), Some(201 * 60_000))];
        let plan = plan_schedule(4, 4, &tasks, &[], &[], &r).unwrap();
        assert!(plan.slots.is_empty());
        assert!(plan.unplaced.iter().any(|t| t.task_id == "later"));
    }
    #[test]
    fn unsized_tasks_are_reported_but_not_placed() {
        let r = request();
        let plan = plan_schedule(1, 1, &[task("unsized", None, None)], &[], &[], &r).unwrap();
        assert!(plan.slots.is_empty());
        assert_eq!(plan.unplaced.len(), 1);
    }
    /// D-01: the window's own proposal must be reviewable. A task whose deadline
    /// was already missed when the window opens is still placed — late work is
    /// the point of planning it — and says why the deadline is not limiting it.
    /// The approval gate's side of the same rule is covered in
    /// `planning_approval`, which reviews exactly this placement.
    #[test]
    fn an_overdue_deadline_is_placed_with_an_explanation() {
        let r = request();
        let overdue = r.starts_at_ms - 50 * 3_600_000;
        let tasks = [task_due("late", Some(30), overdue)];
        let plan = plan_schedule(1, 1, &tasks, &[], &[], &r).unwrap();

        assert_eq!(plan.slots.len(), 1);
        assert_eq!(plan.slots[0].task_id, "late");
        assert!(
            plan.slots[0]
                .explanation
                .iter()
                .any(|line| line.contains("Deadline has already passed")),
            "the slot must explain the missed deadline, got {:?}",
            plan.slots[0].explanation
        );
    }

    /// The other half of the same rule: a deadline still ahead of the window is
    /// never scheduled after it, and the rejection names the task and the reason
    /// instead of proposing a block the approval gate would refuse.
    #[test]
    fn a_deadline_still_ahead_is_named_instead_of_breached() {
        let r = request();
        let deadline = r.starts_at_ms + 20 * 60_000;
        let tasks = [task_due("due-soon", Some(30), deadline)];
        let plan = plan_schedule(1, 1, &tasks, &[], &[], &r).unwrap();

        assert!(plan.slots.is_empty());
        assert_eq!(plan.unplaced.len(), 1);
        assert_eq!(plan.unplaced[0].task_id, "due-soon");
        assert!(
            plan.unplaced[0].reason.contains("deadline"),
            "the reason must name the deadline, got {}",
            plan.unplaced[0].reason
        );
    }

    #[test]
    fn invalid_busy_intervals_fail_closed() {
        let r = request();
        let result = plan_schedule(1, 1, &[], &[], &[block("bad", 120_000, 60_000)], &r);
        assert!(result.is_err());
    }
    #[test]
    fn results_do_not_depend_on_schedule_input_order() {
        let r = request();
        let a = block("a", 100 * 60_000, 120 * 60_000);
        let b = block("b", 140 * 60_000, 160 * 60_000);
        let left = plan_schedule(
            1,
            1,
            &[task("one", Some(30), None)],
            &[],
            &[a.clone(), b.clone()],
            &r,
        )
        .unwrap();
        let right = plan_schedule(1, 1, &[task("one", Some(30), None)], &[], &[b, a], &r).unwrap();
        assert_eq!(left, right);
    }
}

#[cfg(test)]
mod integration_contract_tests {
    use super::*;
    fn task(id: &str, status: &str, minutes: Option<i64>, project: Option<&str>) -> TaskRecord {
        TaskRecord {
            id: id.into(),
            title: id.into(),
            status: status.into(),
            project_id: project.map(str::to_string),
            milestone_id: None,
            estimated_minutes: minutes,
            earliest_start_at_ms: None,
            deadline_at_ms: None,
            description: String::new(),
            priority: None,
            labels: Vec::new(),
        }
    }
    fn request() -> SchedulePlanRequest {
        SchedulePlanRequest {
            starts_at_ms: 0,
            ends_at_ms: 180 * 60_000,
            max_tasks: 10,
            project_id: None,
            default_minutes: None,
            include_unsized: false,
        }
    }
    fn busy(id: &str, start: i64, end: i64) -> ScheduleRecord {
        ScheduleRecord {
            id: id.into(),
            title: id.into(),
            starts_at_ms: start,
            ends_at_ms: end,
            duration_minutes: ((end - start) / 60_000) as u32,
            flexibility: "fixed".into(),
            target_kind: "task".into(),
            target_id: None,
        }
    }
    #[test]
    fn fully_busy_day_reports_no_capacity() {
        let plan = plan_schedule(
            1,
            1,
            &[task("a", "ready", Some(30), None)],
            &[],
            &[busy("meeting", 0, 180 * 60_000)],
            &request(),
        )
        .unwrap();
        assert_eq!(plan.available_minutes, 0);
        assert_eq!(plan.allocated_minutes, 0);
        assert!(plan.slots.is_empty());
        assert_eq!(plan.unplaced.len(), 1);
    }
    #[test]
    fn blocks_preexisting_task_reservations_too() {
        let plan = plan_schedule(
            1,
            1,
            &[task("a", "ready", Some(70), None)],
            &[],
            &[busy("task-block", 30 * 60_000, 120 * 60_000)],
            &request(),
        )
        .unwrap();
        assert!(plan.slots.is_empty());
        assert_eq!(plan.occupied_minutes, 90);
    }
    #[test]
    fn project_filter_is_respected() {
        let mut req = request();
        req.project_id = Some("one".into());
        let tasks = [
            task("a", "ready", Some(30), Some("one")),
            task("b", "ready", Some(30), Some("two")),
        ];
        let plan = plan_schedule(1, 1, &tasks, &[], &[], &req).unwrap();
        assert_eq!(plan.slots.len(), 1);
        assert_eq!(plan.slots[0].task_id, "a");
    }
    #[test]
    fn blocked_task_is_never_scheduled() {
        let tasks = [
            task("a", "ready", Some(30), None),
            task("b", "ready", Some(30), None),
        ];
        let edges = [TaskDependencyEdge {
            task_id: "b".into(),
            prerequisite_id: "a".into(),
        }];
        let plan = plan_schedule(1, 1, &tasks, &edges, &[], &request()).unwrap();
        assert_eq!(plan.slots.len(), 1);
        assert_eq!(plan.slots[0].task_id, "a");
        assert!(plan.unplaced.iter().any(|u| u.task_id == "b"));
    }
    #[test]
    fn default_estimate_places_unsized_task() {
        let mut req = request();
        req.default_minutes = Some(25);
        let plan = plan_schedule(1, 1, &[task("a", "ready", None, None)], &[], &[], &req).unwrap();
        assert_eq!(plan.slots.len(), 1);
        assert_eq!(plan.allocated_minutes, 25);
    }
    #[test]
    fn zero_task_limit_keeps_calendar_unmodified() {
        let mut req = request();
        req.max_tasks = 0;
        let plan =
            plan_schedule(1, 1, &[task("a", "ready", Some(30), None)], &[], &[], &req).unwrap();
        assert!(plan.slots.is_empty());
        assert_eq!(plan.available_minutes, 180);
    }
    #[test]
    fn rejects_cycle_even_when_calendar_has_no_free_slots() {
        let tasks = [
            task("a", "ready", Some(30), None),
            task("b", "ready", Some(30), None),
        ];
        let edges = [
            TaskDependencyEdge {
                task_id: "a".into(),
                prerequisite_id: "b".into(),
            },
            TaskDependencyEdge {
                task_id: "b".into(),
                prerequisite_id: "a".into(),
            },
        ];
        assert!(plan_schedule(1, 1, &tasks, &edges, &[], &request()).is_err());
    }
    #[test]
    fn disjoint_busy_intervals_preserve_exact_free_capacity() {
        let r = request();
        let blocks = [
            busy("one", 10 * 60_000, 20 * 60_000),
            busy("two", 40 * 60_000, 60 * 60_000),
            busy("three", 80 * 60_000, 100 * 60_000),
        ];
        let plan = plan_schedule(1, 1, &[], &[], &blocks, &r).unwrap();
        assert_eq!(plan.occupied_minutes, 50);
        assert_eq!(plan.available_minutes, 130);
        assert_eq!(plan.free_windows.len(), 4);
    }
}
